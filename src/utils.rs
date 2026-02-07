/*!
Utility functions and UI components for the freeze application.

This module provides helper functions for formatting, validation,
and user interface elements like progress bars and tables.
*/

use crate::db::Database;
use crate::snapshot::Snapshot;
use anyhow::Result;
use colored::*;
use console::{style, Term};
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tabled::settings::{object::Columns, Modify, Style, Width};
use tabled::{Table, Tabled};
use walkdir::WalkDir;

#[derive(Tabled)]
struct SnapshotDisplay {
    #[tabled(rename = "Date")]
    date: String,
    #[tabled(rename = "Path")]
    path: String,
    #[tabled(rename = "Size")]
    size: String,
    #[tabled(rename = "Checksum")]
    checksum: String,
}

/// Allows the user to interactively select a snapshot from a list.
///
/// If there's only one snapshot, returns it immediately.
/// If there are multiple, displays them and prompts for selection.
///
/// # Arguments
///
/// * `snapshots` - Slice of available snapshots
///
/// # Returns
///
/// Reference to the selected snapshot
///
/// # Errors
///
/// Returns an error if no snapshots are available or if user input is invalid.
pub fn select_snapshot(snapshots: &[Snapshot]) -> Result<&Snapshot> {
    if snapshots.is_empty() {
        anyhow::bail!("No snapshots available");
    }

    if snapshots.len() == 1 {
        return Ok(&snapshots[0]);
    }

    println!("\nAvailable snapshots:");
    for (i, snapshot) in snapshots.iter().enumerate() {
        println!(
            "{}. {} ({}) - Checksum: {}",
            i + 1,
            format_date(&snapshot.date),
            format_size(snapshot.size),
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

    Ok(&snapshots[selection - 1])
}

/// Prints a formatted header with the given text.
///
/// Displays a stylized header with horizontal lines matching the terminal width.
///
/// # Arguments
///
/// * `text` - The header text to display
pub fn print_header(text: &str) {
    let term = Term::stdout();
    let (_, width) = term.size();
    let width = width as usize;
    println!("\n{}", "═".repeat(width).bright_blue());
    println!("{}", style(text).cyan().bold());
    println!("{}\n", "═".repeat(width).bright_blue());
}

/// Prints snapshot information in a table format.
///
/// Displays all snapshots with their path, date, size, and checksum
/// in a formatted table.
///
/// # Arguments
///
/// * `snapshots` - Slice of tuples containing (path, date, size, checksum)
pub fn print_snapshot_info(snapshots: &[(PathBuf, String, i64, String)]) {
    let snapshot_displays: Vec<SnapshotDisplay> = snapshots
        .iter()
        .map(|(path, date, size, checksum)| SnapshotDisplay {
            date: format_date(date),
            path: path.display().to_string(),
            size: format_size(*size),
            checksum: checksum.get(..8).unwrap_or(checksum).to_string(),
        })
        .collect();

    let term = Term::stdout();
    let (_, width) = term.size();
    let width = width as usize;

    let table = Table::new(snapshot_displays)
        .with(Style::rounded())
        .with(Modify::new(Columns::new(1..2)).with(Width::wrap(width.saturating_sub(50))))
        .to_string();

    println!("{}", table);
}

/// Prints snapshot information with pagination support.
///
/// Displays snapshots in pages of 10 items. If no page is specified,
/// displays all snapshots. Shows navigation hints for multiple pages.
///
/// # Arguments
///
/// * `snapshots` - Slice of tuples containing (path, date, size, checksum)
/// * `page` - Optional page number (1-indexed, 10 items per page)
pub fn print_snapshot_info_paginated(
    snapshots: &[(PathBuf, String, i64, String)],
    page: Option<u32>,
) {
    const ITEMS_PER_PAGE: usize = 10;

    let total_snapshots = snapshots.len();

    if page.is_none() {
        print_snapshot_info(snapshots);
        return;
    }

    let total_pages = total_snapshots.div_ceil(ITEMS_PER_PAGE);
    let page_num = page.unwrap() as usize;

    if page_num == 0 || page_num > total_pages {
        println!(
            "{}",
            style(format!(
                "Invalid page number. Must be between 1 and {}.",
                total_pages
            ))
            .red()
        );
        return;
    }

    let start_index = (page_num - 1) * ITEMS_PER_PAGE;
    let end_index = std::cmp::min(start_index + ITEMS_PER_PAGE, total_snapshots);

    let page_snapshots = &snapshots[start_index..end_index];

    let snapshot_displays: Vec<SnapshotDisplay> = page_snapshots
        .iter()
        .map(|(path, date, size, checksum)| SnapshotDisplay {
            date: format_date(date),
            path: path.display().to_string(),
            size: format_size(*size),
            checksum: checksum.get(..8).unwrap_or(checksum).to_string(),
        })
        .collect();

    let term = Term::stdout();
    let (_, width) = term.size();
    let width = width as usize;

    let table = Table::new(snapshot_displays)
        .with(Style::rounded())
        .with(Modify::new(Columns::new(1..2)).with(Width::wrap(width.saturating_sub(50))))
        .to_string();

    println!("{}", table);

    println!("{}", style("─".repeat(width.min(50))).dim());
    println!(
        "{} {} {} {} {}",
        style("Page:").cyan(),
        style(page_num).yellow(),
        style("of").cyan(),
        style(total_pages).yellow(),
        style(format!("({} items)", total_snapshots)).dim()
    );

    if total_pages > 1 {
        let navigation = if page_num == 1 {
            format!("Next: --page {}", page_num + 1)
        } else if page_num == total_pages {
            format!("Previous: --page {}", page_num - 1)
        } else {
            format!(
                "Previous: --page {} | Next: --page {}",
                page_num - 1,
                page_num + 1
            )
        };

        println!("{}", style(navigation).dim());
    }
}

/// Creates a styled progress bar.
///
/// # Arguments
///
/// * `len` - Maximum value of the progress bar
///
/// # Returns
///
/// A configured progress bar with gradient styling and steady tick
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

/// Formats a byte count into a human-readable size string.
///
/// # Arguments
///
/// * `size` - Size in bytes
///
/// # Returns
///
/// Formatted string with appropriate unit (B, KB, MB, or GB)
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

/// Formats an RFC3339 date string into a more readable format.
///
/// # Arguments
///
/// * `date` - RFC3339 date string
///
/// # Returns
///
/// Formatted date string (YYYY-MM-DD HH:MM)
pub fn format_date(date: &str) -> String {
    if date.len() >= 16 {
        date[..16].replace('T', " ")
    } else {
        date.to_string()
    }
}

/// Detects if content contains binary data.
///
/// Checks the first 512 bytes for null bytes, which indicates binary content.
///
/// # Arguments
///
/// * `content` - Byte slice to check
///
/// # Returns
///
/// `true` if null bytes are found, `false` otherwise
pub fn is_binary(content: &[u8]) -> bool {
    content.iter().take(512).any(|&byte| byte == 0)
}

/// Validates that a path exists.
///
/// # Arguments
///
/// * `path` - Path to validate
///
/// # Errors
///
/// Returns an error if the path does not exist.
pub fn validate_path<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    if !path.exists() {
        anyhow::bail!("Path does not exist: {}", path.display());
    }
    Ok(())
}

/// Checks if files have changed since their last snapshot.
///
/// For files, compares the current checksum with the stored one.
/// For directories, checks all files within.
///
/// # Arguments
///
/// * `path` - Path to check
/// * `db` - Database connection to retrieve snapshots from
///
/// # Errors
///
/// Returns an error if path canonicalization or file operations fail.
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
        ProgressStyle::default_spinner().template("{spinner:.green} [{elapsed_precise}] {msg}")?,
    );

    let all_snapshots = db.list_directory_snapshots(dir)?;
    let snapshot_map: HashMap<String, String> = all_snapshots
        .into_iter()
        .map(|(path, _, _, checksum)| (path.display().to_string(), checksum))
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

            let path_str = path.display().to_string();
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

fn is_checksum(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn compare(first: &str, second: &str, db: &Database) -> Result<()> {
    let (left_content, left_name, left_is_binary) = resolve_content(first, second, db)?;
    let (right_content, right_name, right_is_binary) = resolve_content(second, first, db)?;

    if left_is_binary || right_is_binary {
        print_binary_diff(&left_name, &right_name, &left_content, &right_content);
        return Ok(());
    }

    let left_str = String::from_utf8_lossy(&left_content);
    let right_str = String::from_utf8_lossy(&right_content);

    let diff = compute_unified_diff(&left_name, &right_name, &left_str, &right_str);

    if diff.is_empty() {
        println!("{}", style("✅ Files are identical").green().bold());
    } else {
        print!("{}", diff);
    }

    Ok(())
}

/// Inspects the evolution of a file across snapshots.
///
/// Displays a timeline of changes including date, size, and diff stats.
pub fn inspect_file(path: &Path, db: &Database) -> Result<()> {
    let mut snapshots = db.get_snapshots_for_path(path)?;

    if snapshots.is_empty() {
        println!(
            "{} {}",
            style("No snapshots found for:").yellow(),
            style(path.display()).cyan()
        );
        return Ok(());
    }

    // Sort by date ascending (oldest to newest)
    snapshots.sort_by(|a, b| a.date.cmp(&b.date));

    println!(
        "{} {}",
        style("Evolution of:").cyan().bold(),
        style(path.display()).green()
    );
    println!("{}", style("─".repeat(50)).dim());

    let mut prev_snapshot: Option<&Snapshot> = None;

    for (i, snapshot) in snapshots.iter().enumerate() {
        let size_formatted = format_size(snapshot.size);
        let date_short = snapshot.date.split('T').next().unwrap_or(&snapshot.date);
        let time_short = snapshot
            .date
            .split('T')
            .nth(1)
            .unwrap_or("")
            .split('.')
            .next()
            .unwrap_or("");

        let header = format!(
            "{} {} {} {}",
            style(format!("#{}", i + 1)).dim(),
            style(format!("{} {}", date_short, time_short)).cyan(),
            style(&snapshot.checksum[..8]).yellow(),
            style(size_formatted).blue()
        );

        if let Some(prev) = prev_snapshot {
            // Compare with previous
            if prev.checksum == snapshot.checksum {
                println!("{} {}", header, style("(No change)").dim());
            } else {
                // Calculate diff stats
                let prev_content = prev.get_decompressed_content()?;
                let curr_content = snapshot.get_decompressed_content()?;

                if is_binary(&curr_content) || is_binary(&prev_content) {
                    println!("{} {}", header, style("(Binary changed)").yellow());
                } else {
                    let prev_str = String::from_utf8_lossy(&prev_content);
                    let curr_str_copy = String::from_utf8_lossy(&curr_content);

                    let diff = TextDiff::from_lines(&prev_str, &curr_str_copy);
                    let mut added = 0;
                    let mut removed = 0;
                    let mut diff_lines: Vec<String> = Vec::new();

                    for change in diff.iter_all_changes() {
                        match change.tag() {
                            ChangeTag::Delete => {
                                removed += 1;
                                diff_lines.push(format!("  {}", style(change.to_string()).red()));
                            }
                            ChangeTag::Insert => {
                                added += 1;
                                diff_lines.push(format!("  {}", style(change.to_string()).green()));
                            }
                            ChangeTag::Equal => {
                                diff_lines.push(format!("  {}", style(change.to_string()).dim()));
                            }
                        }
                    }

                    let mut stats = String::new();
                    if added > 0 {
                        stats.push_str(&format!("+{} ", added));
                    }
                    if removed > 0 {
                        stats.push_str(&format!("-{} ", removed));
                    }

                    let stats_colored = if added > removed {
                        style(stats).green()
                    } else {
                        style(stats).red()
                    };

                    println!("{} {}", header, stats_colored);

                    let mut lines_printed = 0;
                    const MAX_LINES: usize = 50;

                    for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
                        if lines_printed >= MAX_LINES {
                            println!("{}", style("  ... (diff truncated)").dim());
                            break;
                        }

                        if idx > 0 {
                            println!("{}", style("  ...").dim());
                        }

                        for op in group {
                            for change in diff.iter_inline_changes(op) {
                                if lines_printed >= MAX_LINES {
                                    break;
                                }

                                let (sign, s) = match change.tag() {
                                    ChangeTag::Delete => ("-", style(change.to_string()).red()),
                                    ChangeTag::Insert => ("+", style(change.to_string()).green()),
                                    ChangeTag::Equal => (" ", style(change.to_string()).dim()),
                                };
                                print!("  {}{}", sign, s);
                                lines_printed += 1;
                            }
                        }
                    }
                }
            }
        } else {
            // First snapshot
            println!("{} {}", header, style("(Initial)").green());

            if let Ok(content) = snapshot.peek_decompressed_content(1024)
                && !is_binary(&content)
                && let Ok(text) = String::from_utf8(content)
            {
                let lines: Vec<&str> = text.lines().take(5).collect();
                for line in lines {
                    println!("  {}", style(line).dim());
                }
                if text.lines().count() > 5 {
                    println!("  {}", style("...").dim());
                }
            }
        }

        prev_snapshot = Some(snapshot);
    }
    println!("{}", style("─".repeat(50)).dim());

    Ok(())
}

fn resolve_content(arg: &str, other_arg: &str, db: &Database) -> Result<(Vec<u8>, String, bool)> {
    if is_checksum(arg) {
        if let Some(snapshot) = db.get_snapshot_by_checksum(arg)? {
            let content = snapshot.get_decompressed_content()?;
            let is_bin = is_binary(&content);
            return Ok((
                content,
                format!("snapshot:{}:{}", snapshot.path.display(), &arg[..8]),
                is_bin,
            ));
        }
        anyhow::bail!("Checksum not found: {}", &arg[..8]);
    }

    let path = PathBuf::from(arg);
    if path.exists() {
        let content = fs::read(&path)?;
        let is_bin = is_binary(&content);
        let name = if is_checksum(other_arg) {
            format!("{}:{}", path.display(), &other_arg[..8])
        } else {
            path.display().to_string()
        };
        return Ok((content, name, is_bin));
    }

    anyhow::bail!("Path or checksum not found: {}", arg);
}

use similar::{ChangeTag, TextDiff};

fn compute_unified_diff(old_name: &str, new_name: &str, old: &str, new: &str) -> String {
    let mut result = String::new();

    result.push_str(&style(format!("--- {}\n", old_name)).red().to_string());
    result.push_str(&style(format!("+++ {}\n", new_name)).green().to_string());

    let diff = TextDiff::from_lines(old, new);

    for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
        if idx > 0 {
            result.push_str(&style("@@ ... @@\n").yellow().bold().to_string());
        }
        for op in group {
            for change in diff.iter_inline_changes(op) {
                let (sign, s) = match change.tag() {
                    ChangeTag::Delete => ("-", style(change.to_string()).red()),
                    ChangeTag::Insert => ("+", style(change.to_string()).green()),
                    ChangeTag::Equal => (" ", style(change.to_string()).dim()),
                };
                result.push_str(&format!("{}{}", sign, s));
            }
        }
    }

    result
}

fn print_binary_diff(left_name: &str, right_name: &str, left: &[u8], right: &[u8]) {
    println!("{}", style("Binary files differ").yellow().bold());
    println!("\n{}:", style(left_name).cyan());
    println!("  Size: {}", format_size(left.len() as i64));
    println!("\n{}:", style(right_name).cyan());
    println!("  Size: {}", format_size(right.len() as i64));

    if left.len() != right.len() {
        let diff = (right.len() as i64) - (left.len() as i64);
        if diff > 0 {
            println!(
                "\n{} +{} bytes",
                style("Size difference:").yellow(),
                style(format!("+{}", diff)).green()
            );
        } else {
            println!(
                "\n{} {} bytes",
                style("Size difference:").yellow(),
                style(diff).red()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn test_format_size_kb() {
        let result = format_size(2048);
        assert!(result.contains("KB"));
    }

    #[test]
    fn test_format_size_mb() {
        let result = format_size(2_000_000);
        assert!(result.contains("MB"));
    }

    #[test]
    fn test_format_size_gb() {
        let result = format_size(2_000_000_000);
        assert!(result.contains("GB"));
    }

    #[test]
    fn test_is_binary_with_text() {
        let content = b"Hello, world!";
        assert!(!is_binary(content));
    }

    #[test]
    fn test_is_binary_with_binary() {
        let content = b"Hello\0world";
        assert!(is_binary(content));
    }

    #[test]
    fn test_is_binary_empty() {
        let content = b"";
        assert!(!is_binary(content));
    }
}
