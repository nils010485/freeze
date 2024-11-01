// cli.rs
use clap::{Parser, Subcommand};
use anyhow::Result;
use crate::db::Database;
use crate::snapshot::Snapshot;
use crate::utils;
use console::style;
use std::env;
use std::path::PathBuf;

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
    /// Manage exclusions
    Exclusion {
        #[command(subcommand)]
        action: ExclusionCommands,
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
        Commands::Save { path } => {
            let path = PathBuf::from(path).canonicalize()?;  // Convertit en chemin absolu
            utils::validate_path(&path)?;

            println!("{} {}",
                     style("Saving:").cyan().bold(),
                     style(path.display()).green()
            );

            let pb = utils::create_progress_bar(1);
            pb.set_message("Creating snapshot...");

            Snapshot::save_recursive(&path, &db)?;

            pb.finish_with_message("Snapshot created successfully!");
            Ok(())
        }

        Commands::Restore { path } => {
            let path = PathBuf::from(path).canonicalize()?;  // Convertit en chemin absolu
            println!("{} {}",
                     style("Restoring:").cyan().bold(),
                     style(path.display()).green()
            );

            Snapshot::restore(&path, &db)?;
            println!("{}", style("Restore completed successfully!").green().bold());
            Ok(())
        }

        Commands::Ls => {
            let snapshots = db.list_all_snapshots()?;
            if snapshots.is_empty() {
                println!("{}", style("No snapshots found.").yellow());
                return Ok(());
            }

            println!("{}", style("All snapshots:").cyan().bold());
            for (path, date, size, checksum) in snapshots {  // Ajout du checksum
                println!("\n{}", style("→").cyan());
                utils::print_snapshot_info(
                    &path,
                    &date,
                    size,
                    &checksum
                );
            }
            Ok(())
        }

        Commands::Cls => {
            let current_dir = env::current_dir()?;
            let snapshots = db.list_current_directory_snapshots(&current_dir)?;

            if snapshots.is_empty() {
                println!("{}",
                         style(format!("No snapshots found in {}.", current_dir.display()))
                             .yellow()
                );
                return Ok(());
            }

            println!("{} {}",
                     style("Snapshots in current directory:").cyan().bold(),
                     style(current_dir.display()).green()
            );

            for (path, date, size, checksum) in snapshots {  // Ajout du checksum
                println!("\n{}", style("→").cyan());
                utils::print_snapshot_info(
                    &path,
                    &date,
                    size,
                    &checksum
                );
            }
            Ok(())
        }

        Commands::Clear { all, path } => {
            if all {
                println!("{}", style("Clearing all snapshots...").yellow());
                db.clear_all_snapshots()?;
                println!("{}", style("All snapshots cleared!").green());
            } else if let Some(path) = path {
                let path = PathBuf::from(path);
                println!("{} {}",
                         style("Clearing snapshots for:").yellow(),
                         style(path.display()).green()
                );
                db.clear_snapshots(path)?;
                println!("{}", style("Snapshots cleared!").green());
            }
            Ok(())
        }

        Commands::Exclusion { action } => {
            match action {
                ExclusionCommands::Add { pattern, exclusion_type } => {
                    db.add_exclusion(&pattern, exclusion_type.as_str())?;
                    println!("{} {} ({})",
                             style("Added exclusion:").green(),
                             style(&pattern).yellow(),
                             style(exclusion_type.as_str()).cyan()
                    );
                }
                ExclusionCommands::Remove { pattern } => {
                    db.remove_exclusion(&pattern)?;
                    println!("{} {}",
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
                        println!("{} {} ({})",
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
