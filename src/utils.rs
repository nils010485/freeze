use crate::db::Database;
use crate::snapshot::Snapshot;
use anyhow::Result;
use colored::*;
use console::{style, Term};
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tabled::settings::Style;
use tabled::{Table, Tabled};
use walkdir::WalkDir;
#[derive(Tabled)]
struct SnapshotDisplay {
    #[tabled(rename = "📁 Path")]
    path: String,
    #[tabled(rename = "📅 Date")]
    date: String,
    #[tabled(rename = "💾 Size")]
    size: String,
    #[tabled(rename = "🔐 Checksum")]
    checksum: String,
}

pub fn print_header(text: &str) {
    let term = Term::stdout();
    let (_, width) = term.size();
    let width = width as usize;
    println!("\n{}", "═".repeat(width).bright_blue());
    println!("{}", style(text).cyan().bold());
    println!("{}\n", "═".repeat(width).bright_blue());
}

pub fn print_snapshot_info(path: &Path, date: &str, size: i64, checksum: &str) {
    let snapshot = SnapshotDisplay {
        path: path.display().to_string(),
        date: date.to_string(),
        size: format_size(size),
        checksum: checksum[..8].to_string(),
    };

    let table = Table::new(vec![snapshot]).with(Style::modern()).to_string();

    println!("{}", table);
}

pub fn create_progress_bar(len: u64) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.gradient(red,yellow,green)}] {percent}% {msg}")
            .unwrap()
            .progress_chars("█▓▒░"),
    );
    pb.enable_steady_tick(Duration::from_millis(120));
    pb
}

pub fn format_size(size: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let size = size as f64;
    if size >= GB {
        format!("{:.2} GB", size / GB)
    } else if size >= MB {
        format!("{:.2} MB", size / MB)
    } else if size >= KB {
        format!("{:.2} KB", size / KB)
    } else {
        format!("{:.0} B", size)
    }
}

pub fn is_binary(content: &[u8]) -> bool {
    content.iter().take(512).any(|&byte| byte == 0)
}

pub fn validate_path<P: AsRef<Path>>(path: P) -> anyhow::Result<()> {
    let path = path.as_ref();
    if !path.exists() {
        anyhow::bail!("Path does not exist: {}", path.display());
    }
    Ok(())
}

pub fn check_path(path: &str, db: &Database) -> Result<()> {
    let path = PathBuf::from(path).canonicalize()?;

    if path.is_file() {
        check_single_file(&path, db)?;
    } else {
        check_directory(&path, db)?;
    }
    Ok(())
}
fn check_single_file(path: &Path, db: &Database) -> Result<()> {
    let content = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    let current_checksum = format!("{:x}", hasher.finalize());

    let snapshots = db.get_snapshots_for_path(path)?;

    if snapshots.is_empty() {
        println!(
            "{} {} {}",
            style("❌").red(),
            style(path.display()).cyan(),
            style("(No snapshot found)").red()
        );
        return Ok(());
    }

    let latest_snapshot = &snapshots[0];
    if latest_snapshot.checksum == current_checksum {
        println!(
            "{} {} {}",
            style("✅").green(),
            style(path.display()).cyan(),
            style("(Up to date)").green()
        );
    } else {
        println!(
            "{} {} {}",
            style("⚠️").yellow(),
            style(path.display()).cyan(),
            style("(Modified since last snapshot)").yellow()
        );
    }

    Ok(())
}
fn check_directory(dir: &Path, db: &Database) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .unwrap(),
    );

    let all_snapshots = db.list_directory_snapshots(dir)?;
    let snapshot_map: HashMap<String, String> = all_snapshots
        .into_iter()
        .map(|(path, _, _, checksum)| (path.to_string_lossy().to_string(), checksum))
        .collect();

    let mut files_checked = 0;
    let mut files_modified = 0;
    let mut files_new = 0;

    let walker = WalkDir::new(dir).into_iter();
    for entry in walker.filter_entry(|e| !Snapshot::is_excluded(e.path())) {
        let entry = entry?;
        if entry.file_type().is_file() {
            pb.set_message(format!("Checking {}", entry.path().display()));

            let path = entry.path();
            let content = fs::read(path)?;
            let mut hasher = Sha256::new();
            hasher.update(&content);
            let current_checksum = format!("{:x}", hasher.finalize());

            let path_str = path.to_string_lossy().to_string();
            match snapshot_map.get(&path_str) {
                Some(saved_checksum) => {
                    files_checked += 1;
                    if &current_checksum != saved_checksum {
                        files_modified += 1;
                        println!(
                            "{} {} {}",
                            style("⚠️").yellow(),
                            style(path.display()).cyan(),
                            style("(Modified)").yellow()
                        );
                    }
                }
                None => {
                    files_new += 1;
                    println!(
                        "{} {} {}",
                        style("❌").red(),
                        style(path.display()).cyan(),
                        style("(New file)").red()
                    );
                }
            }
        }
    }

    pb.finish_and_clear();

    println!("\n{}", style("Summary:").cyan().bold());
    println!("Files checked: {}", style(files_checked).green());
    println!("Modified files: {}", style(files_modified).yellow());
    println!("New files: {}", style(files_new).red());

    Ok(())
}
