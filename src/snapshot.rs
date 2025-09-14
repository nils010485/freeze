// snapshot.rs
use crate::db::Database;
use anyhow::{Context, Result};
use chrono::Local;
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zstd::stream::{encode_all, decode_all};

#[derive(Debug)]
pub struct Snapshot {
    pub path: PathBuf,
    pub content_path: PathBuf,
    pub checksum: String,
    pub date: String,
    pub size: i64,
}

impl Snapshot {
    /// Create a new snapshot for a file
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

    /// Save a file or directory recursively
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

    /// Save a single file to the database
    fn save_file<P: AsRef<Path>>(path: P, db: &Database) -> Result<()> {
        let snapshot = Self::new(path)?;
        db.save_snapshot(&snapshot)?;
        Ok(())
    }

    /// Restore a file or directory from snapshots
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

    /// Restore a single file from snapshot
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

    /// Perform the actual file restoration
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

    /// Check if a path should be excluded
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
                    if let Some(ext) = path.extension() {
                        if ext.to_string_lossy() == pattern.trim_start_matches('.') {
                            return true;
                        }
                    }
                }
                "file" => {
                    if let Some(file_name) = path.file_name() {
                        if file_name.to_string_lossy() == pattern {
                            return true;
                        }
                    }
                }
                _ => continue,
            }
        }
        false
    }

    /// Calculate SHA256 checksum of a file in chunks
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

    /// Get the storage directory path
    fn get_storage_dir() -> Result<PathBuf> {
        let data_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
            .join(".freeze/storage");
        Ok(data_dir)
    }

    /// Clean up any orphaned temporary files
    pub fn cleanup_temp_files() -> Result<()> {
        let storage_dir = Self::get_storage_dir()?;
        if !storage_dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&storage_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("tmp") {
                if let Err(e) = fs::remove_file(&path) {
                    eprintln!("Warning: Failed to remove temp file {:?}: {}", path, e);
                }
            }
        }
        Ok(())
    }

    /// Compress and copy file with temp file
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
        
        // Read the source file
        let file_data = fs::read(src)?;
        
        // Compress the data
        let compressed_data = encode_all(&file_data[..], 3)?; // Compression level 3
        
        // Write compressed data to temp file
        fs::write(&temp_path, compressed_data)?;
        
        // Atomic rename
        fs::rename(&temp_path, dest)?;
        Ok(())
    }

    /// Decompress and copy file with temp file
    fn decompress_and_copy<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dest: Q) -> Result<()> {
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
        
        // Read the compressed file
        let compressed_data = fs::read(src)?;
        
        // Decompress the data
        let decompressed_data = decode_all(&compressed_data[..])?;
        
        // Write decompressed data to temp file
        fs::write(&temp_path, decompressed_data)?;
        
        // Atomic rename
        fs::rename(&temp_path, dest)?;
        Ok(())
    }
}
