use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::Mutex;

use super::schema::*;

pub struct SqliteMetadata {
    conn: Mutex<Connection>,
}

impl SqliteMetadata {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn init_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS users (
                id           TEXT PRIMARY KEY,
                username     TEXT NOT NULL,
                display_name TEXT NOT NULL,
                role         TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS folders (
                id        TEXT PRIMARY KEY,
                name      TEXT NOT NULL,
                parent_id TEXT,
                user_id   TEXT NOT NULL,
                path      TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS files (
                id         TEXT PRIMARY KEY,
                name       TEXT NOT NULL,
                folder_id  TEXT,
                user_id    TEXT NOT NULL,
                blob_hash  TEXT NOT NULL,
                size       INTEGER NOT NULL,
                mime_type  TEXT NOT NULL,
                created_at TEXT,
                updated_at TEXT
            );

            CREATE TABLE IF NOT EXISTS blobs (
                hash         TEXT PRIMARY KEY,
                size         INTEGER NOT NULL,
                ref_count    INTEGER NOT NULL,
                content_type TEXT
            );

            CREATE TABLE IF NOT EXISTS shares (
                id               TEXT PRIMARY KEY,
                item_id          TEXT NOT NULL,
                item_type        TEXT NOT NULL,
                token            TEXT NOT NULL,
                permissions_read INTEGER NOT NULL DEFAULT 1,
                expires_at       INTEGER,
                created_by       TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_folders_user_id   ON folders(user_id);
            CREATE INDEX IF NOT EXISTS idx_folders_parent_id ON folders(parent_id);
            CREATE INDEX IF NOT EXISTS idx_files_user_id     ON files(user_id);
            CREATE INDEX IF NOT EXISTS idx_files_folder_id   ON files(folder_id);
            CREATE INDEX IF NOT EXISTS idx_files_blob_hash   ON files(blob_hash);
            ",
        )?;
        Ok(())
    }
}

impl MetadataSource for SqliteMetadata {
    fn stats(&self) -> Result<StorageStats> {
        let conn = self.conn.lock().unwrap();
        let user_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
        let file_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
        let folder_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM folders", [], |r| r.get(0))?;
        let unique_blobs: i64 =
            conn.query_row("SELECT COUNT(*) FROM blobs", [], |r| r.get(0))?;
        let logical_bytes: i64 =
            conn.query_row("SELECT COALESCE(SUM(size), 0) FROM files", [], |r| r.get(0))?;
        let physical_bytes: i64 =
            conn.query_row("SELECT COALESCE(SUM(size), 0) FROM blobs", [], |r| r.get(0))?;

        Ok(StorageStats {
            user_count: user_count as u64,
            file_count: file_count as u64,
            folder_count: folder_count as u64,
            unique_blobs: unique_blobs as u64,
            logical_bytes: logical_bytes as u64,
            physical_bytes: physical_bytes as u64,
        })
    }

    fn list_users(&self) -> Result<Vec<User>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, username, display_name, role FROM users ORDER BY username",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(User {
                id: r.get(0)?,
                username: r.get(1)?,
                display_name: r.get(2)?,
                role: r.get(3)?,
            })
        })?;
        rows.map(|r| r.map_err(anyhow::Error::from)).collect()
    }

    fn list_folders_for_user(&self, user_id: &str) -> Result<Vec<Folder>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, parent_id, user_id, path \
             FROM folders WHERE user_id = ?1 ORDER BY path",
        )?;
        let rows = stmt.query_map(params![user_id], |r| {
            Ok(Folder {
                id: r.get(0)?,
                name: r.get(1)?,
                parent_id: r.get(2)?,
                user_id: r.get(3)?,
                path: r.get(4)?,
            })
        })?;
        rows.map(|r| r.map_err(anyhow::Error::from)).collect()
    }

    fn list_files_in_folder(
        &self,
        user_id: &str,
        folder_id: Option<&str>,
    ) -> Result<Vec<FileEntry>> {
        let conn = self.conn.lock().unwrap();
        if let Some(fid) = folder_id {
            let mut stmt = conn.prepare(
                "SELECT id, name, folder_id, user_id, blob_hash, size, mime_type, \
                 created_at, updated_at \
                 FROM files WHERE user_id = ?1 AND folder_id = ?2 ORDER BY name",
            )?;
            let rows = stmt.query_map(params![user_id, fid], |r| {
                Ok(FileEntry {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    folder_id: r.get(2)?,
                    user_id: r.get(3)?,
                    blob_hash: r.get(4)?,
                    size: r.get::<_, i64>(5)? as u64,
                    mime_type: r.get(6)?,
                    created_at: r.get(7)?,
                    updated_at: r.get(8)?,
                })
            })?;
            rows.map(|r| r.map_err(anyhow::Error::from)).collect()
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, name, folder_id, user_id, blob_hash, size, mime_type, \
                 created_at, updated_at \
                 FROM files WHERE user_id = ?1 AND folder_id IS NULL ORDER BY name",
            )?;
            let rows = stmt.query_map(params![user_id], |r| {
                Ok(FileEntry {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    folder_id: r.get(2)?,
                    user_id: r.get(3)?,
                    blob_hash: r.get(4)?,
                    size: r.get::<_, i64>(5)? as u64,
                    mime_type: r.get(6)?,
                    created_at: r.get(7)?,
                    updated_at: r.get(8)?,
                })
            })?;
            rows.map(|r| r.map_err(anyhow::Error::from)).collect()
        }
    }

    fn get_root_folders(&self, user_id: &str) -> Result<Vec<Folder>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, parent_id, user_id, path \
             FROM folders WHERE user_id = ?1 AND parent_id IS NULL ORDER BY name",
        )?;
        let rows = stmt.query_map(params![user_id], |r| {
            Ok(Folder {
                id: r.get(0)?,
                name: r.get(1)?,
                parent_id: r.get(2)?,
                user_id: r.get(3)?,
                path: r.get(4)?,
            })
        })?;
        rows.map(|r| r.map_err(anyhow::Error::from)).collect()
    }

    fn get_subfolders(&self, folder_id: &str) -> Result<Vec<Folder>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, parent_id, user_id, path \
             FROM folders WHERE parent_id = ?1 ORDER BY name",
        )?;
        let rows = stmt.query_map(params![folder_id], |r| {
            Ok(Folder {
                id: r.get(0)?,
                name: r.get(1)?,
                parent_id: r.get(2)?,
                user_id: r.get(3)?,
                path: r.get(4)?,
            })
        })?;
        rows.map(|r| r.map_err(anyhow::Error::from)).collect()
    }

    fn search_files(&self, user_id: &str, query: &str) -> Result<Vec<FileEntry>> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(
            "SELECT id, name, folder_id, user_id, blob_hash, size, mime_type, \
             created_at, updated_at \
             FROM files WHERE user_id = ?1 AND name LIKE ?2 ORDER BY name",
        )?;
        let rows = stmt.query_map(params![user_id, pattern], |r| {
            Ok(FileEntry {
                id: r.get(0)?,
                name: r.get(1)?,
                folder_id: r.get(2)?,
                user_id: r.get(3)?,
                blob_hash: r.get(4)?,
                size: r.get::<_, i64>(5)? as u64,
                mime_type: r.get(6)?,
                created_at: r.get(7)?,
                updated_at: r.get(8)?,
            })
        })?;
        rows.map(|r| r.map_err(anyhow::Error::from)).collect()
    }

    fn get_blob_record(&self, hash: &str) -> Result<Option<BlobRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT hash, size, ref_count, content_type FROM blobs WHERE hash = ?1",
        )?;
        let mut rows = stmt.query_map(params![hash], |r| {
            Ok(BlobRecord {
                hash: r.get(0)?,
                size: r.get::<_, i64>(1)? as u64,
                ref_count: r.get(2)?,
                content_type: r.get(3)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    fn get_all_blobs(&self) -> Result<Vec<BlobRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT hash, size, ref_count, content_type FROM blobs ORDER BY hash")?;
        let rows = stmt.query_map([], |r| {
            Ok(BlobRecord {
                hash: r.get(0)?,
                size: r.get::<_, i64>(1)? as u64,
                ref_count: r.get(2)?,
                content_type: r.get(3)?,
            })
        })?;
        rows.map(|r| r.map_err(anyhow::Error::from)).collect()
    }

    fn get_all_files(&self) -> Result<Vec<FileEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, folder_id, user_id, blob_hash, size, mime_type, \
             created_at, updated_at \
             FROM files ORDER BY user_id, name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(FileEntry {
                id: r.get(0)?,
                name: r.get(1)?,
                folder_id: r.get(2)?,
                user_id: r.get(3)?,
                blob_hash: r.get(4)?,
                size: r.get::<_, i64>(5)? as u64,
                mime_type: r.get(6)?,
                created_at: r.get(7)?,
                updated_at: r.get(8)?,
            })
        })?;
        rows.map(|r| r.map_err(anyhow::Error::from)).collect()
    }

    fn get_all_folders(&self) -> Result<Vec<Folder>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, parent_id, user_id, path \
             FROM folders ORDER BY user_id, path",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Folder {
                id: r.get(0)?,
                name: r.get(1)?,
                parent_id: r.get(2)?,
                user_id: r.get(3)?,
                path: r.get(4)?,
            })
        })?;
        rows.map(|r| r.map_err(anyhow::Error::from)).collect()
    }

    fn user_stats(&self, user_id: &str) -> Result<(u64, u64)> {
        let conn = self.conn.lock().unwrap();
        let file_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM files WHERE user_id = ?1",
            params![user_id],
            |r| r.get(0),
        )?;
        let total_bytes: i64 = conn.query_row(
            "SELECT COALESCE(SUM(size), 0) FROM files WHERE user_id = ?1",
            params![user_id],
            |r| r.get(0),
        )?;
        Ok((file_count as u64, total_bytes as u64))
    }
}
