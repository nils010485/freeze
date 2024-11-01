use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;

pub fn format_size(size: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;

    if size >= GB {
        format!("{:.2} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.2} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.2} KB", size as f64 / KB as f64)
    } else {
        format!("{} B", size)
    }
}

pub fn create_progress_bar(len: u64) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("#>-")
    );
    pb
}

pub fn print_snapshot_info(path: &Path, date: &str, size: i64, checksum: &str) {
    println!("{}", style("Snapshot Details:").cyan().bold());
    println!("Path: {}", style(path.display()).green());
    println!("Date: {}", style(date).yellow());
    println!("Size: {}", style(format_size(size)).blue());
    println!("Checksum: {}", style(&checksum[..8]).magenta());  // Affiche les 8 premiers caractères
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

