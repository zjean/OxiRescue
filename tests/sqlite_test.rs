use oxirescue::db::sqlite::SqliteMetadata;
use std::path::PathBuf;
use tempfile::TempDir;

fn create_test_db(dir: &TempDir) -> PathBuf {
    let db_path = dir.path().join("test.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    SqliteMetadata::init_schema(&conn).unwrap();
    conn.execute(
        "INSERT INTO users (id, username, display_name, role) VALUES ('u1', 'alice', 'Alice', 'user')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO folders (id, name, parent_id, user_id, path) VALUES ('f1', 'Documents', NULL, 'u1', 'Documents')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO folders (id, name, parent_id, user_id, path) VALUES ('f2', 'Work', 'f1', 'u1', 'Documents/Work')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO blobs (hash, size, ref_count, content_type) VALUES ('aabbccdd', 1024, 1, 'text/plain')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO files (id, name, folder_id, user_id, blob_hash, size, mime_type, created_at, updated_at) VALUES ('file1', 'notes.txt', 'f2', 'u1', 'aabbccdd', 1024, 'text/plain', '2025-01-01', '2025-01-01')",
        [],
    )
    .unwrap();
    db_path
}

#[test]
fn test_sqlite_stats() {
    use oxirescue::db::schema::MetadataSource;
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let meta = SqliteMetadata::open(&db_path).unwrap();
    let stats = meta.stats().unwrap();
    assert_eq!(stats.user_count, 1);
    assert_eq!(stats.file_count, 1);
    assert_eq!(stats.folder_count, 2);
    assert_eq!(stats.unique_blobs, 1);
}

#[test]
fn test_sqlite_list_users() {
    use oxirescue::db::schema::MetadataSource;
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let meta = SqliteMetadata::open(&db_path).unwrap();
    let users = meta.list_users().unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].username, "alice");
}

#[test]
fn test_sqlite_folder_hierarchy() {
    use oxirescue::db::schema::MetadataSource;
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let meta = SqliteMetadata::open(&db_path).unwrap();

    let roots = meta.get_root_folders("u1").unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].name, "Documents");

    let subs = meta.get_subfolders("f1").unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].name, "Work");
}

#[test]
fn test_sqlite_list_files() {
    use oxirescue::db::schema::MetadataSource;
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let meta = SqliteMetadata::open(&db_path).unwrap();

    let files = meta.list_files_in_folder("u1", Some("f2")).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "notes.txt");
    assert_eq!(files[0].blob_hash, "aabbccdd");
}
