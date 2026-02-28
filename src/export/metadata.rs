use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::Path;

use crate::db::schema::MetadataSource;
use crate::db::sqlite::SqliteMetadata;

pub fn export_to_sqlite(source: &dyn MetadataSource, output: &Path) -> Result<()> {
    let conn = Connection::open(output)?;
    SqliteMetadata::init_schema(&conn)?;

    // Export users
    let users = source.list_users()?;
    let user_count = users.len();
    for u in users {
        conn.execute(
            "INSERT OR REPLACE INTO users (id, username, display_name, role) \
             VALUES (?1, ?2, ?3, ?4)",
            params![u.id, u.username, u.display_name, u.role],
        )?;
    }

    // Export folders
    let folders = source.get_all_folders()?;
    let folder_count = folders.len();
    for f in folders {
        conn.execute(
            "INSERT OR REPLACE INTO folders (id, name, parent_id, user_id, path) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![f.id, f.name, f.parent_id, f.user_id, f.path],
        )?;
    }

    // Export files
    let files = source.get_all_files()?;
    let file_count = files.len();
    for f in files {
        conn.execute(
            "INSERT OR REPLACE INTO files \
             (id, name, folder_id, user_id, blob_hash, size, mime_type, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                f.id,
                f.name,
                f.folder_id,
                f.user_id,
                f.blob_hash,
                f.size as i64,
                f.mime_type,
                f.created_at,
                f.updated_at
            ],
        )?;
    }

    // Export blobs
    let blobs = source.get_all_blobs()?;
    let blob_count = blobs.len();
    for b in blobs {
        conn.execute(
            "INSERT OR REPLACE INTO blobs (hash, size, ref_count, content_type) \
             VALUES (?1, ?2, ?3, ?4)",
            params![b.hash, b.size as i64, b.ref_count, b.content_type],
        )?;
    }

    println!(
        "Exported: {} users, {} folders, {} files, {} blobs",
        user_count, folder_count, file_count, blob_count
    );

    Ok(())
}
