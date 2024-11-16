use colored::*;
use console::{style, Term};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use std::time::Duration;
use tabled::settings::Style;
use tabled::{Table, Tabled};

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
