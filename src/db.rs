use crate::snapshot::Snapshot;
use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};

pub struct Database {
    conn: Connection,
}

impl Database {
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
                content BLOB NOT NULL,
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

    pub fn save_snapshot(&self, snapshot: &Snapshot) -> Result<()> {
        self.conn.execute(
            "INSERT INTO snapshots (path, content, checksum, date, size) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                snapshot.path.to_string_lossy().to_string(),
                snapshot.content,
                snapshot.checksum,
                snapshot.date,
                snapshot.size,
            ],
        )?;
        Ok(())
    }

    pub fn get_snapshots_for_path<P: AsRef<Path>>(&self, path: P) -> Result<Vec<Snapshot>> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let mut stmt = self.conn.prepare(
            "SELECT path, content, checksum, date, size FROM snapshots WHERE path = ? ORDER BY date DESC"
        )?;

        let snapshot_iter = stmt.query_map(params![path_str], |row| {
            Ok(Snapshot {
                path: PathBuf::from(row.get::<_, String>(0)?),
                content: row.get(1)?,
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

    pub fn list_all_snapshots(&self) -> Result<Vec<(PathBuf, String, i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT path, date, size, checksum FROM snapshots ORDER BY date DESC"
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

    pub fn list_current_directory_snapshots<P: AsRef<Path>>(&self, current_dir: P)
                                                            -> Result<Vec<(PathBuf, String, i64, String)>>
    {
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


    pub fn clear_snapshots<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        self.conn.execute(
            "DELETE FROM snapshots WHERE path = ?",
            params![path_str],
        )?;
        Ok(())
    }

    pub fn clear_all_snapshots(&self) -> Result<()> {
        self.conn.execute("DELETE FROM snapshots", [])?;
        Ok(())
    }

    pub fn add_exclusion(&self, pattern: &str, exclusion_type: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO exclusions (pattern, type) VALUES (?1, ?2)",
            params![pattern, exclusion_type],
        )?;
        Ok(())
    }

    pub fn remove_exclusion(&self, pattern: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM exclusions WHERE pattern = ?",
            params![pattern],
        )?;
        Ok(())
    }

    pub fn list_exclusions(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT pattern, type FROM exclusions ORDER BY type, pattern"
        )?;

        let exclusion_iter = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
            ))
        })?;

        let mut exclusions = Vec::new();
        for exclusion in exclusion_iter {
            exclusions.push(exclusion?);
        }
        Ok(exclusions)
    }

    pub fn get_exclusions(&self) -> Result<Vec<(String, String)>> {
        self.list_exclusions()
    }
}

