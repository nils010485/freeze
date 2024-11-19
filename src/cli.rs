// cli.rs
use crate::db::Database;
use crate::snapshot::Snapshot;
use crate::utils;
use crate::utils::check_path;
use crate::utils::print_header;
use anyhow::Result;
use clap::{Parser, Subcommand};
use console::style;
use std::{env, fs};
use std::path::PathBuf;
use crate::utils::{format_size, is_binary};
use std::path::Path;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Save file or directory state
    Save {
        /// Path to save
        path: String,
    },
    /// Export a snapshot to a specified path
    Export {
        /// Path of the snapshot to export
        snapshot_path: String,
        /// Optional export destination (defaults to current directory)
        #[arg(short, long)]
        destination: Option<String>,
    },
    /// View the contents of a snapshot
    View {
        /// Path of the snapshot to view
        snapshot_path: String,
    },
    /// Restore file or directory from snapshot
    Restore {
        /// Path to restore
        path: String,
    },
    /// List all snapshots
    Ls,
    /// List snapshots in current directory
    Cls,
    /// Clear snapshots
    Clear {
        #[arg(long)]
        all: bool,
        path: Option<String>,
    },
    /// Search snapshots by name
    Search {
        /// Name pattern to search
        pattern: String,
    },
    /// Manage exclusions
    Exclusion {
        #[command(subcommand)]
        action: ExclusionCommands,
    },
    /// Check if current version is already saved
    Check {
        /// Path to check
        path: String,
    },
}

#[derive(Subcommand)]
pub enum ExclusionCommands {
    /// Add exclusion pattern
    Add {
        /// Pattern to exclude
        pattern: String,
        /// Type of exclusion (directory, extension, file)
        #[arg(value_enum)]
        exclusion_type: ExclusionType,
    },
    /// Remove exclusion pattern
    Remove {
        /// Pattern to remove
        pattern: String,
    },
    /// List all exclusions
    List,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum ExclusionType {
    Directory,
    Extension,
    File,
}

impl ExclusionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExclusionType::Directory => "directory",
            ExclusionType::Extension => "extension",
            ExclusionType::File => "file",
        }
    }
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let db = Database::new()?;

    match cli.command {
        Commands::Export { snapshot_path, destination } => {
            print_header("📦 Exporting Snapshot");

            // Convert snapshot path to absolute path
            let snapshot_path = PathBuf::from(snapshot_path).canonicalize()?;

            // Get snapshots for the specific path
            let snapshots = db.get_snapshots_for_path(&snapshot_path)?;

            if snapshots.is_empty() {
                println!(
                    "{} {}",
                    style("No snapshots found for:").yellow(),
                    style(snapshot_path.display()).cyan()
                );
                return Ok(());
            }

            // If multiple snapshots, let user choose
            let snapshot = utils::select_snapshot(&snapshots)?;


            // Determine export destination
            // Determine export destination
            let export_path = match destination {
                Some(ref dest) => {
                    let dest_path = PathBuf::from(dest);

                    // If destination is an existing directory, use original filename
                    if dest_path.is_dir() {
                        dest_path.join(snapshot.path.file_name().unwrap_or_else(|| std::ffi::OsStr::new(&snapshot.checksum)))
                    }
                    // If destination contains path separators, treat as a full path
                    else if dest.contains('/') || dest.contains('\\') {
                        let full_path = PathBuf::from(dest);
                        // Create parent directories if they don't exist
                        if let Some(parent) = full_path.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        full_path
                    }
                    // Otherwise, use as filename in current directory
                    else {
                        env::current_dir()?.join(dest)
                    }
                },
                None => env::current_dir()?.join(
                    snapshot.path.file_name().unwrap_or_else(|| std::ffi::OsStr::new(&snapshot.checksum))
                ),
            };


            // Ensure parent directory exists
            if let Some(parent) = export_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Write snapshot content
            fs::write(&export_path, &snapshot.content)?;

            println!(
                "{} {} {} {}",
                style("Exported snapshot:").green(),
                style(snapshot.path.display()).cyan(),
                style("to").green(),
                style(export_path.display()).cyan()
            );

            Ok(())
        }


        Commands::View { snapshot_path } => {
            print_header("👀 Viewing Snapshot");

            // Convert snapshot path to absolute path
            let snapshot_path = PathBuf::from(snapshot_path).canonicalize()?;

            // Get snapshots for the specific path
            let snapshots = db.get_snapshots_for_path(&snapshot_path)?;

            if snapshots.is_empty() {
                println!(
                    "{} {}",
                    style("No snapshots found for:").yellow(),
                    style(snapshot_path.display()).cyan()
                );
                return Ok(());
            }

            // If multiple snapshots, let user choose
            let snapshot = utils::select_snapshot(&snapshots)?;


            // Check if content is binary
            if is_binary(&snapshot.content) {
                println!(
                    "{} {}",
                    style("Binary content detected for:").yellow(),
                    style(snapshot_path.display()).cyan()
                );
                println!("Snapshot details:");
                println!("Path: {}", snapshot.path.display());
                println!("Date: {}", snapshot.date);
                println!("Size: {}", format_size(snapshot.size));
                println!("Checksum: {}", snapshot.checksum);
                return Ok(());
            }

            // Attempt to convert content to UTF-8 string
            match String::from_utf8(snapshot.content.clone()) {
                Ok(content) => {
                    println!("{}", style("Snapshot Content:").cyan().bold());
                    println!("{}", content);
                }
                Err(_) => {
                    println!(
                        "{} {}",
                        style("Unable to display content for:").yellow(),
                        style(snapshot_path.display()).cyan()
                    );
                }
            }

            Ok(())
        }
        Commands::Check { path } => {
            print_header("🔍 Checking Files");
            check_path(&path, &db)?;
            Ok(())
        }
        Commands::Save { path } => {
            print_header("🧊 Freezing File/Directory");
            let path = PathBuf::from(path).canonicalize()?; // Convertit en chemin absolu
            utils::validate_path(&path)?;

            println!(
                "{} {}",
                style("Freezing:").cyan().bold(),
                style(path.display()).green()
            );

            let pb = utils::create_progress_bar(1);
            pb.set_message("Creating snapshot...");

            Snapshot::save_recursive(&path, &db)?;

            pb.finish_with_message("Snapshot created successfully!");
            Ok(())
        }

        Commands::Restore { path } => {
            print_header("♻️  Restoring From Snapshot");
            let path = PathBuf::from(path).canonicalize()?; // Convertit en chemin absolu
            println!(
                "{} {}",
                style("Restoring:").cyan().bold(),
                style(path.display()).green()
            );

            Snapshot::restore(&path, &db)?;
            println!(
                "{}",
                style("Restore completed successfully!").green().bold()
            );
            Ok(())
        }

        Commands::Ls => {
            print_header("📋 All Snapshots");

            let snapshots = db.list_all_snapshots()?;
            if snapshots.is_empty() {
                println!("{}", style("No snapshots found.").yellow());
                return Ok(());
            }

            utils::print_snapshot_info(&snapshots);
            Ok(())
        }

        Commands::Cls => {
            let current_dir = env::current_dir()?;
            let snapshots = db.list_current_directory_snapshots(&current_dir)?;

            if snapshots.is_empty() {
                println!(
                    "{}",
                    style(format!("No snapshots found in {}.", current_dir.display())).yellow()
                );
                return Ok(());
            }

            println!(
                "{} {}",
                style("Snapshots in current directory:").cyan().bold(),
                style(current_dir.display()).green()
            );

            utils::print_snapshot_info(&snapshots);
            Ok(())
        }

        Commands::Clear { all, path } => {
            if all {
                println!("{}", style("Clearing all snapshots...").yellow());
                db.clear_all_snapshots()?;
                println!("{}", style("All snapshots cleared!").green());
            } else {
                let path = path.unwrap_or_else(|| String::from("./"));

                // Convertir en chemin absolu, en utilisant le répertoire courant comme base si nécessaire
                let path = if Path::new(&path).is_absolute() {
                    PathBuf::from(&path)
                } else {
                    env::current_dir()?.join(&path).canonicalize()?
                };

                if path.to_string_lossy() == env::current_dir()?.to_string_lossy() {
                    println!(
                        "{}",
                        style("Clearing snapshots in current directory...").yellow()
                    );
                    db.clear_directory_snapshots(&env::current_dir()?)?;
                } else {
                    println!(
                        "{} {}",
                        style("Clearing snapshots for:").yellow(),
                        style(path.display()).green()
                    );
                    db.clear_snapshots(path)?;
                }
            }
            Ok(())
        }

        Commands::Search { pattern } => {
            let snapshots = db.search_snapshots(&pattern)?;
            if snapshots.is_empty() {
                println!(
                    "{} {}",
                    style("No snapshots found matching:").yellow(),
                    style(&pattern).cyan()
                );
                return Ok(());
            }

            println!(
                "{} {}",
                style("Snapshots matching:").cyan().bold(),
                style(&pattern).green()
            );

            utils::print_snapshot_info(&snapshots);
            Ok(())
        }

        Commands::Exclusion { action } => {
            match action {
                ExclusionCommands::Add {
                    pattern,
                    exclusion_type,
                } => {
                    db.add_exclusion(&pattern, exclusion_type.as_str())?;
                    println!(
                        "{} {} ({})",
                        style("Added exclusion:").green(),
                        style(&pattern).yellow(),
                        style(exclusion_type.as_str()).cyan()
                    );
                }
                ExclusionCommands::Remove { pattern } => {
                    db.remove_exclusion(&pattern)?;
                    println!(
                        "{} {}",
                        style("Removed exclusion:").green(),
                        style(&pattern).yellow()
                    );
                }
                ExclusionCommands::List => {
                    let exclusions = db.list_exclusions()?;
                    if exclusions.is_empty() {
                        println!("{}", style("No exclusions configured.").yellow());
                        return Ok(());
                    }

                    println!("{}", style("Current exclusions:").cyan().bold());
                    for (pattern, exc_type) in exclusions {
                        println!(
                            "{} {} ({})",
                            style("→").cyan(),
                            style(pattern).yellow(),
                            style(exc_type).green()
                        );
                    }
                }
            }
            Ok(())
        }
    }
}

