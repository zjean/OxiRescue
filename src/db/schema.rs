#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
}

#[derive(Debug, Clone)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub user_id: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub id: String,
    pub name: String,
    pub folder_id: Option<String>,
    pub user_id: String,
    pub blob_hash: String,
    pub size: u64,
    pub mime_type: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BlobRecord {
    pub hash: String,
    pub size: u64,
    pub ref_count: i32,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StorageStats {
    pub user_count: u64,
    pub file_count: u64,
    pub folder_count: u64,
    pub unique_blobs: u64,
    pub logical_bytes: u64,
    pub physical_bytes: u64,
}

/// Unified read-only metadata access — implemented for both PostgreSQL and SQLite.
pub trait MetadataSource: Send + Sync {
    fn stats(&self) -> anyhow::Result<StorageStats>;
    fn list_users(&self) -> anyhow::Result<Vec<User>>;
    fn list_folders_for_user(&self, user_id: &str) -> anyhow::Result<Vec<Folder>>;
    fn list_files_in_folder(
        &self,
        user_id: &str,
        folder_id: Option<&str>,
    ) -> anyhow::Result<Vec<FileEntry>>;
    fn get_root_folders(&self, user_id: &str) -> anyhow::Result<Vec<Folder>>;
    fn get_subfolders(&self, folder_id: &str) -> anyhow::Result<Vec<Folder>>;
    fn search_files(&self, user_id: &str, query: &str) -> anyhow::Result<Vec<FileEntry>>;
    fn get_blob_record(&self, hash: &str) -> anyhow::Result<Option<BlobRecord>>;
    fn get_all_blobs(&self) -> anyhow::Result<Vec<BlobRecord>>;
    fn get_all_files(&self) -> anyhow::Result<Vec<FileEntry>>;
    fn get_all_folders(&self) -> anyhow::Result<Vec<Folder>>;
    fn user_stats(&self, user_id: &str) -> anyhow::Result<(u64, u64)>; // (file_count, total_bytes)
}
