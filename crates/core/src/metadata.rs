use std::path::Path;

use anyhow::Result;
use rusqlite::{params, Connection};

#[derive(Debug, Clone)]
pub struct FileRecord {
    pub path: String,
    pub mtime: i64,
    pub size: u64,
    pub hash: Option<String>,
    pub indexed_at: i64,
}

pub struct MetadataStore {
    conn: Connection,
}

impl MetadataStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY,
                mtime INTEGER NOT NULL,
                size INTEGER NOT NULL,
                hash TEXT,
                indexed_at INTEGER NOT NULL
            );
            ",
        )?;

        Ok(Self { conn })
    }

    pub fn all_records(&self) -> Result<Vec<FileRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, mtime, size, hash, indexed_at FROM files")?;
        let rows = stmt.query_map([], |row| {
            Ok(FileRecord {
                path: row.get(0)?,
                mtime: row.get(1)?,
                size: row.get(2)?,
                hash: row.get(3)?,
                indexed_at: row.get(4)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_record(&self, path: &str) -> Result<Option<FileRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, mtime, size, hash, indexed_at FROM files WHERE path = ?1 LIMIT 1",
        )?;
        let mut rows = stmt.query(params![path])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(FileRecord {
                path: row.get(0)?,
                mtime: row.get(1)?,
                size: row.get(2)?,
                hash: row.get(3)?,
                indexed_at: row.get(4)?,
            }));
        }
        Ok(None)
    }

    pub fn upsert_many(&mut self, records: &[FileRecord]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO files(path, mtime, size, hash, indexed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(path) DO UPDATE SET
                 mtime = excluded.mtime,
                 size = excluded.size,
                 hash = excluded.hash,
                 indexed_at = excluded.indexed_at",
            )?;
            for rec in records {
                stmt.execute(params![
                    rec.path,
                    rec.mtime,
                    rec.size,
                    rec.hash,
                    rec.indexed_at
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove_many(&mut self, paths: &[String]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare("DELETE FROM files WHERE path = ?1")?;
            for p in paths {
                stmt.execute(params![p])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{FileRecord, MetadataStore};

    #[test]
    fn upsert_and_read_records() {
        let db_path = std::env::temp_dir().join(format!(
            "lupa_meta_test_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch should be available")
                .as_nanos()
        ));

        let mut store = MetadataStore::open(&db_path).expect("should open sqlite metadata store");
        store
            .upsert_many(&[FileRecord {
                path: "a.txt".to_string(),
                mtime: 10,
                size: 12,
                hash: Some("abc".to_string()),
                indexed_at: 99,
            }])
            .expect("should upsert metadata record");

        let rows = store.all_records().expect("should read metadata records");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "a.txt");

        let _ = std::fs::remove_file(db_path);
    }
}
