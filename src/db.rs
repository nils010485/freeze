/*!
Database operations for the freeze application.

This module provides the `Database` struct which handles all SQLite database
operations including snapshot persistence, retrieval, and exclusion management.
*/

use crate::snapshot::Snapshot;
use anyhow::Result;
use console::style;
use rusqlite::{params, Connection};
use std::fs;
use std::path::{Path, PathBuf};

/// Database connection wrapper for freeze snapshot storage.
///
/// Handles all persistence operations for snapshots and exclusions using SQLite.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Clears all snapshots for a specific directory and its subdirectories.
    ///
    /// # Arguments
    ///
    /// * `dir` - The directory path to clear snapshots for
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn clear_directory_snapshots<P: AsRef<Path>>(&self, dir: P) -> Result<()> {
        let dir_pattern = format!("{}/%", dir.as_ref().to_string_lossy());
        let dir_path = dir.as_ref().display().to_string();

        let count = self.conn.execute(
            "DELETE FROM snapshots WHERE path LIKE ? OR path = ?",
            params![dir_pattern, dir_path],
        )?;

        if count == 0 {
            println!(
                "{}",
                style("No snapshots found in this directory.").yellow()
            );
        } else {
            self.cleanup_orphaned_files()?;
            println!(
                "{} {} {}",
                style("Cleared").green(),
                style(count).cyan(),
                style(if count == 1 { "snapshot" } else { "snapshots" }).green()
            );
        }
        Ok(())
    }
    /// Removes storage files that are no longer referenced by any snapshot.
    ///
    /// This is a private method used internally to clean up unused storage files.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the storage directory or removing files fails.
    fn cleanup_orphaned_files(&self) -> Result<()> {
        let mut stmt = self
            .conn
            .prepare("SELECT content_path FROM snapshots GROUP BY content_path")?;

        let used_files: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<_, _>>()?;

        let storage_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
            .join(".freeze/storage");

        for entry in fs::read_dir(storage_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !used_files.contains(&path.display().to_string()) {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }
    /// Searches for snapshots by path pattern.
    ///
    /// # Arguments
    ///
    /// * `pattern` - The search pattern to match against snapshot paths
    ///
    /// # Returns
    ///
    /// A vector of tuples containing (path, date, size, checksum) for matching snapshots
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn search_snapshots(&self, pattern: &str) -> Result<Vec<(PathBuf, String, i64, String)>> {
        let search_pattern = format!("%{}%", pattern);
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT path, date, size, checksum
             FROM snapshots
             WHERE path LIKE ?
             ORDER BY date DESC",
        )?;

        let snapshot_iter = stmt.query_map(params![search_pattern], |row| {
            Ok((
                PathBuf::from(row.get::<_, String>(0)?),
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        let mut snapshots = Vec::new();
        for snapshot in snapshot_iter {
            snapshots.push(snapshot?);
        }
        Ok(snapshots)
    }
    /// Lists all snapshots within a specific directory.
    ///
    /// # Arguments
    ///
    /// * `dir` - The directory path to list snapshots for
    ///
    /// # Returns
    ///
    /// A vector of tuples containing (path, date, size, checksum) for snapshots in the directory
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn list_directory_snapshots<P: AsRef<Path>>(
        &self,
        dir: P,
    ) -> Result<Vec<(PathBuf, String, i64, String)>> {
        let dir_pattern = format!("{}/%", dir.as_ref().to_string_lossy());
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT path, date, size, checksum FROM snapshots
             WHERE path LIKE ? OR path = ?
             ORDER BY path, date DESC",
        )?;

        let snapshot_iter = stmt.query_map(
            params![dir_pattern, dir.as_ref().display().to_string()],
            |row| {
                Ok((
                    PathBuf::from(row.get::<_, String>(0)?),
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )?;

        let mut snapshots = Vec::new();
        for snapshot in snapshot_iter {
            snapshots.push(snapshot?);
        }
        Ok(snapshots)
    }

    /// Clears all snapshots for a specific path.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to clear snapshots for
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn clear_snapshots<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path_str = path.as_ref().display().to_string();
        let deleted = self
            .conn
            .execute("DELETE FROM snapshots WHERE path = ?", params![path_str])?;

        if deleted > 0 {
            self.cleanup_orphaned_files()?; // Nettoyage ajouté ici
        }
        Ok(())
    }
    /// Creates a new database connection and initializes the schema.
    ///
    /// # Returns
    ///
    /// A new `Database` instance with initialized schema
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The home directory cannot be determined
    /// - The data directory cannot be created
    /// - The database cannot be opened
    /// - The schema cannot be initialized
    pub fn new() -> Result<Self> {
        let data_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
            .join(".freeze");
        std::fs::create_dir_all(&data_dir)?;

        let db_path = data_dir.join("data.sql");
        let conn = Connection::open(db_path)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS snapshots (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL,
                content_path TEXT NOT NULL,
                checksum TEXT NOT NULL,
                date TEXT NOT NULL,
                size INTEGER NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS exclusions (
                id INTEGER PRIMARY KEY,
                pattern TEXT NOT NULL,
                type TEXT NOT NULL
            )",
            [],
        )?;

        Ok(Database { conn })
    }

    /// Saves a snapshot to the database.
    ///
    /// # Arguments
    ///
    /// * `snapshot` - Reference to the snapshot to save
    ///
    /// # Errors
    ///
    /// Returns an error if the database insert operation fails.
    pub fn save_snapshot(&self, snapshot: &Snapshot) -> Result<()> {
        self.conn.execute(
            "INSERT INTO snapshots (path, content_path, checksum, date, size) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                snapshot.path.to_string_lossy(),
                snapshot.content_path.to_string_lossy(),
                snapshot.checksum,
                snapshot.date,
                snapshot.size,
            ],
        )?;
        Ok(())
    }

    /// Retrieves all snapshots for a specific path.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to retrieve snapshots for
    ///
    /// # Returns
    ///
    /// A vector of `Snapshot` instances for the given path, ordered by date descending
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn get_snapshots_for_path<P: AsRef<Path>>(&self, path: P) -> Result<Vec<Snapshot>> {
        let path_str = path.as_ref().display().to_string();
        let mut stmt = self.conn.prepare(
            "SELECT path, content_path, checksum, date, size FROM snapshots WHERE path = ? ORDER BY date DESC"
        )?;

        let snapshot_iter = stmt.query_map(params![path_str], |row| {
            Ok(Snapshot {
                path: PathBuf::from(row.get::<_, String>(0)?),
                content_path: PathBuf::from(row.get::<_, String>(1)?),
                checksum: row.get(2)?,
                date: row.get(3)?,
                size: row.get(4)?,
            })
        })?;

        let mut snapshots = Vec::new();
        for snapshot in snapshot_iter {
            snapshots.push(snapshot?);
        }
        Ok(snapshots)
    }

    /// Lists all snapshots in the database.
    ///
    /// # Returns
    ///
    /// A vector of tuples containing (path, date, size, checksum) for all snapshots
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn list_all_snapshots(&self) -> Result<Vec<(PathBuf, String, i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT path, date, size, checksum FROM snapshots ORDER BY date DESC",
        )?;

        let snapshot_iter = stmt.query_map([], |row| {
            Ok((
                PathBuf::from(row.get::<_, String>(0)?),
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        let mut snapshots = Vec::new();
        for snapshot in snapshot_iter {
            snapshots.push(snapshot?);
        }
        Ok(snapshots)
    }

    /// Lists all snapshots in the current working directory.
    ///
    /// # Arguments
    ///
    /// * `current_dir` - The current directory path
    ///
    /// # Returns
    ///
    /// A vector of tuples containing (path, date, size, checksum) for snapshots
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn list_current_directory_snapshots<P: AsRef<Path>>(
        &self,
        current_dir: P,
    ) -> Result<Vec<(PathBuf, String, i64, String)>> {
        let path_pattern = format!("{}/%", current_dir.as_ref().to_string_lossy());
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT path, date, size, checksum FROM snapshots WHERE path LIKE ? ORDER BY date DESC"
        )?;

        let snapshot_iter = stmt.query_map(params![path_pattern], |row| {
            Ok((
                PathBuf::from(row.get::<_, String>(0)?),
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        let mut snapshots = Vec::new();
        for snapshot in snapshot_iter {
            snapshots.push(snapshot?);
        }
        Ok(snapshots)
    }

    /// Clears all snapshots from the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn clear_all_snapshots(&self) -> Result<()> {
        let count = self.conn.execute("DELETE FROM snapshots", [])?;
        if count > 0 {
            self.cleanup_orphaned_files()?;
        }
        Ok(())
    }

    /// Adds an exclusion pattern to the database.
    ///
    /// # Arguments
    ///
    /// * `pattern` - The pattern to exclude (e.g., ".git", "node_modules")
    /// * `exclusion_type` - The type of exclusion ("directory", "extension", or "file")
    ///
    /// # Errors
    ///
    /// Returns an error if the database insert operation fails.
    pub fn add_exclusion(&self, pattern: &str, exclusion_type: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO exclusions (pattern, type) VALUES (?1, ?2)",
            params![pattern, exclusion_type],
        )?;
        Ok(())
    }

    /// Removes an exclusion pattern from the database.
    ///
    /// # Arguments
    ///
    /// * `pattern` - The pattern to remove
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete operation fails.
    pub fn remove_exclusion(&self, pattern: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM exclusions WHERE pattern = ?", params![pattern])?;
        Ok(())
    }

    /// Lists all exclusion patterns.
    ///
    /// # Returns
    ///
    /// A vector of tuples containing (pattern, exclusion_type)
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn list_exclusions(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT pattern, type FROM exclusions ORDER BY type, pattern")?;

        let exclusion_iter = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut exclusions = Vec::new();
        for exclusion in exclusion_iter {
            exclusions.push(exclusion?);
        }
        Ok(exclusions)
    }

    /// Gets all exclusion patterns (alias for list_exclusions).
    ///
    /// # Returns
    ///
    /// A vector of tuples containing (pattern, exclusion_type)
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn get_exclusions(&self) -> Result<Vec<(String, String)>> {
        self.list_exclusions()
    }
}
