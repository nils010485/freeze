/*!
Snapshot management for the freeze application.

This module provides the `Snapshot` struct which represents a file snapshot
with associated metadata and methods for creating, restoring, and managing snapshots.
*/

use crate::db::Database;
use anyhow::{Context, Result};
use chrono::Local;
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Represents a file snapshot with metadata.
///
/// Contains information about a snapshot including the original path, storage location,
/// checksum for integrity verification, timestamp, and file size.
#[derive(Debug)]
pub struct Snapshot {
    /// Original path of the snapshotted file
    pub path: PathBuf,
    /// Path where the compressed content is stored
    pub content_path: PathBuf,
    /// SHA256 checksum of the original file
    pub checksum: String,
    /// RFC3339 timestamp when the snapshot was created
    pub date: String,
    /// Size of the original file in bytes
    pub size: i64,
}

impl Snapshot {
    /// Creates a new snapshot for a file.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to snapshot
    ///
    /// # Returns
    ///
    /// A new `Snapshot` instance with calculated checksum and compressed content
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path cannot be canonicalized
    /// - The path is not a file
    /// - The file cannot be read
    /// - The checksum cannot be calculated
    /// - The storage directory cannot be created
    /// - The file cannot be compressed
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path
            .as_ref()
            .canonicalize()
            .with_context(|| format!("Failed to canonicalize path: {}", path.as_ref().display()))?;

        if !path.is_file() {
            anyhow::bail!("Path is not a file: {}", path.display());
        }

        // Calculate checksum in chunks to avoid memory issues (BEFORE compression)
        let checksum = Self::calculate_checksum(&path)?;

        // Get file metadata for size
        let metadata = fs::metadata(&path)?;
        let size = metadata.len() as i64;

        // Prepare storage directory
        let storage_dir = Self::get_storage_dir()?;
        fs::create_dir_all(&storage_dir)?;

        // Create content path based on checksum with .zstd extension
        let content_path = storage_dir.join(format!("{}.zstd", checksum));

        // Compress and copy file to storage if not already there (deduplication)
        if !content_path.exists() {
            Self::compress_and_copy(&path, &content_path)?;
        }

        Ok(Snapshot {
            path,
            content_path,
            checksum,
            date: Local::now().to_rfc3339(),
            size,
        })
    }

    /// Saves a file or directory recursively to the database.
    ///
    /// For directories, walks through all files and creates snapshots for each one,
    /// excluding files matching exclusion patterns.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file or directory to save
    /// * `db` - Database connection to store snapshots in
    ///
    /// # Errors
    ///
    /// Returns an error if any file operation or database save fails.
    pub fn save_recursive<P: AsRef<Path>>(path: P, db: &Database) -> Result<()> {
        let path = path.as_ref();

        if path.is_file() {
            return Self::save_file(path, db);
        }

        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")?,
        );

        let walker = WalkDir::new(path).into_iter();
        for entry in walker.filter_entry(|e| !Self::is_excluded(e.path())) {
            let entry = entry?;
            if entry.file_type().is_file() {
                pb.set_message(format!("Processing {}", entry.path().display()));
                Self::save_file(entry.path(), db)?;
            }
        }

        pb.finish_with_message("Done!");
        Ok(())
    }

    /// Saves a single file to the database.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file
    /// * `db` - Database connection
    ///
    /// # Errors
    ///
    /// Returns an error if snapshot creation or database save fails.
    fn save_file<P: AsRef<Path>>(path: P, db: &Database) -> Result<()> {
        let snapshot = Self::new(path)?;
        db.save_snapshot(&snapshot)?;
        Ok(())
    }

    /// Restores a file or directory from snapshots.
    ///
    /// For directories, restores all files that have snapshots.
    /// If multiple snapshots exist for a file, prompts the user to select one.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to restore
    /// * `db` - Database connection to retrieve snapshots from
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No snapshots are found for the path
    /// - File decompression fails
    /// - File writing fails
    pub fn restore<P: AsRef<Path>>(path: P, db: &Database) -> Result<()> {
        let path = path.as_ref();

        if path.is_file() {
            return Self::restore_single(path, db);
        }

        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")?,
        );

        let all_snapshots = db.list_directory_snapshots(path)?;
        if all_snapshots.is_empty() {
            anyhow::bail!("No snapshots found for directory: {}", path.display());
        }

        for (file_path, _, _, _) in all_snapshots {
            pb.set_message(format!("Restoring {}", file_path.display()));
            Self::restore_single(&file_path, db)?;
        }

        pb.finish_with_message("Directory restore completed!");
        Ok(())
    }

    /// Restores a single file from snapshot.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to restore
    /// * `db` - Database connection
    ///
    /// # Errors
    ///
    /// Returns an error if no snapshots are found or restoration fails.
    fn restore_single<P: AsRef<Path>>(path: P, db: &Database) -> Result<()> {
        let path = path.as_ref();
        let snapshots = db.get_snapshots_for_path(path)?;

        if snapshots.is_empty() {
            anyhow::bail!("No snapshots found for {}", path.display());
        }

        if snapshots.len() == 1 {
            return Self::restore_snapshot(&snapshots[0], path);
        }

        println!("\nAvailable snapshots for {}:", path.display());
        for (i, snapshot) in snapshots.iter().enumerate() {
            println!(
                "{}. {} ({}) - Checksum: {}",
                i + 1,
                snapshot.date,
                crate::utils::format_size(snapshot.size),
                &snapshot.checksum[..8]
            );
        }

        let mut input = String::new();
        print!("\nSelect snapshot number (1-{}): ", snapshots.len());
        std::io::stdout().flush()?;
        std::io::stdin().read_line(&mut input)?;

        let selection = input
            .trim()
            .parse::<usize>()
            .map_err(|_| anyhow::anyhow!("Invalid selection"))?;

        if selection < 1 || selection > snapshots.len() {
            anyhow::bail!("Invalid selection: {}", selection);
        }

        Self::restore_snapshot(&snapshots[selection - 1], path)
    }

    /// Performs the actual file restoration from a snapshot.
    ///
    /// Handles both compressed (.zstd) and legacy uncompressed snapshots.
    ///
    /// # Arguments
    ///
    /// * `snapshot` - The snapshot to restore from
    /// * `path` - Destination path for restoration
    ///
    /// # Errors
    ///
    /// Returns an error if decompression or file writing fails.
    fn restore_snapshot(snapshot: &Snapshot, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Check if the file is compressed (has .zstd extension)
        if snapshot.content_path.extension().and_then(|s| s.to_str()) == Some("zstd") {
            Self::decompress_and_copy(&snapshot.content_path, path)?;
        } else {
            // Legacy file (not compressed)
            fs::copy(&snapshot.content_path, path)?;
        }
        Ok(())
    }

    /// Checks if a path should be excluded based on exclusion patterns.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to check
    ///
    /// # Returns
    ///
    /// `true` if the path matches an exclusion pattern, `false` otherwise
    pub fn is_excluded(path: &Path) -> bool {
        let db = match Database::new() {
            Ok(db) => db,
            Err(_) => return false,
        };

        let exclusions = match db.get_exclusions() {
            Ok(excl) => excl,
            Err(_) => return false,
        };

        for (pattern, exc_type) in exclusions {
            match exc_type.as_str() {
                "directory" => {
                    if path.is_dir() && path.to_string_lossy().contains(&pattern) {
                        return true;
                    }
                }
                "extension" => {
                    if let Some(ext) = path.extension()
                        && ext.to_string_lossy() == pattern.trim_start_matches('.')
                    {
                        return true;
                    }
                }
                "file" => {
                    if let Some(file_name) = path.file_name()
                        && file_name.to_string_lossy() == pattern
                    {
                        return true;
                    }
                }
                _ => continue,
            }
        }
        false
    }

    /// Calculates the SHA256 checksum of a file in chunks.
    ///
    /// Uses a 64KB buffer to avoid loading large files entirely into memory.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file
    ///
    /// # Returns
    ///
    /// Hexadecimal string representation of the SHA256 checksum
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened or read.
    fn calculate_checksum<P: AsRef<Path>>(path: P) -> Result<String> {
        let mut file = fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0; 64 * 1024]; // 64KB buffer

        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Gets the storage directory path for compressed files.
    ///
    /// # Returns
    ///
    /// Path to `~/.freeze/storage`
    ///
    /// # Errors
    ///
    /// Returns an error if the home directory cannot be determined.
    fn get_storage_dir() -> Result<PathBuf> {
        let data_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
            .join(".freeze/storage");
        Ok(data_dir)
    }

    /// Cleans up any orphaned temporary files from the storage directory.
    ///
    /// Removes all `.tmp` files that may have been left from interrupted operations.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage directory cannot be read.
    /// Individual file removal failures are logged but don't cause an error.
    pub fn cleanup_temp_files() -> Result<()> {
        let storage_dir = Self::get_storage_dir()?;
        if !storage_dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&storage_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("tmp")
                && let Err(e) = fs::remove_file(&path)
            {
                eprintln!("Warning: Failed to remove temp file {:?}: {}", path, e);
            }
        }
        Ok(())
    }

    /// Compresses a file and copies it to storage using a temporary file.
    ///
    /// Uses zstd compression level 3. Writes to a temporary file first,
    /// then atomically renames to ensure data integrity.
    ///
    /// # Arguments
    ///
    /// * `src` - Source file path
    /// * `dest` - Destination path for the compressed file
    ///
    /// # Errors
    ///
    /// Returns an error if reading, compression, or writing fails.
    fn compress_and_copy<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dest: Q) -> Result<()> {
        let src = src.as_ref();
        let dest = dest.as_ref();

        let temp_path = dest.with_extension("tmp");

        // Ensure temp file is cleaned up on error
        struct TempFileGuard<'a>(&'a Path);
        impl<'a> Drop for TempFileGuard<'a> {
            fn drop(&mut self) {
                let _ = fs::remove_file(self.0);
            }
        }
        let _guard = TempFileGuard(&temp_path);

        let mut source_file = fs::File::open(src)?;

        let dest_file = fs::File::create(&temp_path)?;
        let mut writer = std::io::BufWriter::new(dest_file);

        zstd::stream::copy_encode(&mut source_file, &mut writer, 3)?;

        writer.flush()?;

        fs::rename(&temp_path, dest)?;
        Ok(())
    }

    /// Decompresses a file and copies it to the destination using a temporary file.
    ///
    /// Reads a zstd-compressed file, decompresses it, and writes to a temporary
    /// file first, then atomically renames to ensure data integrity.
    ///
    /// # Arguments
    ///
    /// * `src` - Compressed source file path
    /// * `dest` - Destination path for the decompressed file
    ///
    /// # Errors
    ///
    /// Returns an error if reading, decompression, or writing fails.
    fn decompress_and_copy<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dest: Q) -> Result<()> {
        let src = src.as_ref();
        let dest = dest.as_ref();

        let temp_path = dest.with_extension("tmp");

        struct TempFileGuard<'a>(&'a Path);
        impl<'a> Drop for TempFileGuard<'a> {
            fn drop(&mut self) {
                let _ = fs::remove_file(self.0);
            }
        }
        let _guard = TempFileGuard(&temp_path);

        let mut source_file = fs::File::open(src)?;
        let dest_file = fs::File::create(&temp_path)?;
        let mut writer = std::io::BufWriter::new(dest_file);

        zstd::stream::copy_decode(&mut source_file, &mut writer)?;

        writer.flush()?;

        fs::rename(&temp_path, dest)?;
        Ok(())
    }

    pub fn get_decompressed_content(&self) -> Result<Vec<u8>> {
        let mut source_file = fs::File::open(&self.content_path)?;
        let mut buffer = Vec::new();
        zstd::stream::copy_decode(&mut source_file, &mut buffer)?;
        Ok(buffer)
    }

    /// Reads the first `limit` bytes of decompressed content.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of bytes to read
    ///
    /// # Returns
    ///
    /// A vector containing up to `limit` bytes of decompressed content.
    pub fn peek_decompressed_content(&self, limit: usize) -> Result<Vec<u8>> {
        let mut source_file = fs::File::open(&self.content_path)?;
        let mut decoder = zstd::stream::Decoder::new(&mut source_file)?;
        let mut buffer = vec![0; limit];
        let mut bytes_read = 0;

        while bytes_read < limit {
            match decoder.read(&mut buffer[bytes_read..]) {
                Ok(0) => break, // EOF
                Ok(n) => bytes_read += n,
                Err(e) => return Err(e.into()),
            }
        }

        buffer.truncate(bytes_read);
        Ok(buffer)
    }

    /// Exports the snapshot to a destination path using streaming.
    ///
    /// # Arguments
    ///
    /// * `dest` - Destination path
    ///
    /// # Errors
    ///
    /// Returns an error if reading, decompression, or writing fails.
    pub fn export(&self, dest: &Path) -> Result<()> {
        Self::decompress_and_copy(&self.content_path, dest)
    }
}
