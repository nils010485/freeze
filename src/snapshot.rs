// snapshot.rs
use crate::db::Database;
use anyhow::Result;
use chrono::Local;
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Sha256, Digest};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct Snapshot {
    pub(crate) path: PathBuf,
    pub(crate) content: Vec<u8>,
    pub(crate) checksum: String,
    pub(crate) date: String,
    pub(crate) size: i64,
}
impl Snapshot {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().canonicalize()?;
        let content = fs::read(&path)?;
        let mut hasher = Sha256::new();
        hasher.update(&content);
        let checksum = format!("{:x}", hasher.finalize());
        let date = Local::now().to_rfc3339();
        let size = content.len() as i64;

        Ok(Snapshot {
            path,
            content,
            checksum,
            date,
            size,
        })
    }

    pub fn save_recursive<P: AsRef<Path>>(path: P, db: &Database) -> Result<()> {
        let path = path.as_ref();

        if path.is_file() {
            return Self::save_file(path, db);
        }

        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")?
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

    fn save_file<P: AsRef<Path>>(path: P, db: &Database) -> Result<()> {
        let snapshot = Self::new(path)?;
        db.save_snapshot(&snapshot)?;
        Ok(())
    }

    pub fn restore<P: AsRef<Path>>(path: P, db: &Database) -> Result<()> {
        let path = path.as_ref();

        // Si c'est un fichier, utiliser la restauration simple
        if path.is_file() {
            return Self::restore_single(path, db);
        }

        // Pour un dossier, restauration récursive
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")?
        );

        // Récupérer tous les snapshots qui commencent par le chemin du dossier
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
            println!("{}. {} ({}) - Checksum: {}",
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

        let selection = input.trim().parse::<usize>()
            .map_err(|_| anyhow::anyhow!("Invalid selection"))?;

        if selection < 1 || selection > snapshots.len() {
            anyhow::bail!("Invalid selection: {}", selection);
        }

        Self::restore_snapshot(&snapshots[selection - 1], path)
    }

    fn restore_snapshot(snapshot: &Snapshot, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &snapshot.content)?;
        Ok(())
    }

    fn is_excluded(path: &Path) -> bool {
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
                },
                "extension" => {
                    if let Some(ext) = path.extension() {
                        if ext.to_string_lossy() == pattern.trim_start_matches('.') {
                            return true;
                        }
                    }
                },
                "file" => {
                    if let Some(file_name) = path.file_name() {
                        if file_name.to_string_lossy() == pattern {
                            return true;
                        }
                    }
                },
                _ => continue,
            }
        }
        false
    }
}
