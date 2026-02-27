use anyhow::Result;
use sqlx::PgPool;

use super::schema::{
    BlobRecord, FileEntry, Folder, MetadataSource, StorageStats, User,
};

pub struct PgMetadata {
    pool: PgPool,
}

impl PgMetadata {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPool::connect(database_url).await?;
        Ok(Self { pool })
    }
}

impl MetadataSource for PgMetadata {
    fn stats(&self) -> Result<StorageStats> {
        let pool = self.pool.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let user_count: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM auth.users")
                        .fetch_one(&pool)
                        .await?;

                let file_count: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM storage.files WHERE NOT is_trashed")
                        .fetch_one(&pool)
                        .await?;

                let folder_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM storage.folders WHERE NOT is_trashed",
                )
                .fetch_one(&pool)
                .await?;

                let unique_blobs: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM storage.blobs")
                        .fetch_one(&pool)
                        .await?;

                let logical_bytes: i64 = sqlx::query_scalar(
                    "SELECT COALESCE(SUM(size), 0) FROM storage.files WHERE NOT is_trashed",
                )
                .fetch_one(&pool)
                .await?;

                let physical_bytes: i64 =
                    sqlx::query_scalar("SELECT COALESCE(SUM(size), 0) FROM storage.blobs")
                        .fetch_one(&pool)
                        .await?;

                Ok(StorageStats {
                    user_count: user_count as u64,
                    file_count: file_count as u64,
                    folder_count: folder_count as u64,
                    unique_blobs: unique_blobs as u64,
                    logical_bytes: logical_bytes as u64,
                    physical_bytes: physical_bytes as u64,
                })
            })
        })
    }

    fn list_users(&self) -> Result<Vec<User>> {
        let pool = self.pool.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let rows = sqlx::query_as::<_, (String, String, String, String)>(
                    "SELECT id, username, username, role::text FROM auth.users ORDER BY username",
                )
                .fetch_all(&pool)
                .await?;

                Ok(rows
                    .into_iter()
                    .map(|(id, username, display_name, role)| User {
                        id,
                        username,
                        display_name,
                        role,
                    })
                    .collect())
            })
        })
    }

    fn list_folders_for_user(&self, user_id: &str) -> Result<Vec<Folder>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let rows = sqlx::query_as::<_, (String, String, Option<String>, String, String)>(
                    "SELECT id::text, name, parent_id::text, user_id, path \
                     FROM storage.folders \
                     WHERE user_id = $1 AND NOT is_trashed \
                     ORDER BY path",
                )
                .bind(&user_id)
                .fetch_all(&pool)
                .await?;

                Ok(rows
                    .into_iter()
                    .map(|(id, name, parent_id, user_id, path)| Folder {
                        id,
                        name,
                        parent_id,
                        user_id,
                        path,
                    })
                    .collect())
            })
        })
    }

    fn list_files_in_folder(
        &self,
        user_id: &str,
        folder_id: Option<&str>,
    ) -> Result<Vec<FileEntry>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let folder_id = folder_id.map(|s| s.to_string());
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let rows = if let Some(fid) = &folder_id {
                    sqlx::query_as::<
                        _,
                        (
                            String,
                            String,
                            Option<String>,
                            String,
                            String,
                            i64,
                            String,
                            Option<String>,
                            Option<String>,
                        ),
                    >(
                        "SELECT id::text, name, folder_id::text, user_id, blob_hash, size, \
                         mime_type, \
                         created_at::text, updated_at::text \
                         FROM storage.files \
                         WHERE user_id = $1 AND folder_id = $2::uuid AND NOT is_trashed \
                         ORDER BY name",
                    )
                    .bind(&user_id)
                    .bind(fid)
                    .fetch_all(&pool)
                    .await?
                } else {
                    sqlx::query_as::<
                        _,
                        (
                            String,
                            String,
                            Option<String>,
                            String,
                            String,
                            i64,
                            String,
                            Option<String>,
                            Option<String>,
                        ),
                    >(
                        "SELECT id::text, name, folder_id::text, user_id, blob_hash, size, \
                         mime_type, \
                         created_at::text, updated_at::text \
                         FROM storage.files \
                         WHERE user_id = $1 AND folder_id IS NULL AND NOT is_trashed \
                         ORDER BY name",
                    )
                    .bind(&user_id)
                    .fetch_all(&pool)
                    .await?
                };

                Ok(rows
                    .into_iter()
                    .map(
                        |(id, name, folder_id, user_id, blob_hash, size, mime_type, created_at, updated_at)| {
                            FileEntry {
                                id,
                                name,
                                folder_id,
                                user_id,
                                blob_hash,
                                size: size as u64,
                                mime_type,
                                created_at,
                                updated_at,
                            }
                        },
                    )
                    .collect())
            })
        })
    }

    fn get_root_folders(&self, user_id: &str) -> Result<Vec<Folder>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let rows = sqlx::query_as::<_, (String, String, Option<String>, String, String)>(
                    "SELECT id::text, name, parent_id::text, user_id, path \
                     FROM storage.folders \
                     WHERE user_id = $1 AND parent_id IS NULL AND NOT is_trashed \
                     ORDER BY name",
                )
                .bind(&user_id)
                .fetch_all(&pool)
                .await?;

                Ok(rows
                    .into_iter()
                    .map(|(id, name, parent_id, user_id, path)| Folder {
                        id,
                        name,
                        parent_id,
                        user_id,
                        path,
                    })
                    .collect())
            })
        })
    }

    fn get_subfolders(&self, folder_id: &str) -> Result<Vec<Folder>> {
        let pool = self.pool.clone();
        let folder_id = folder_id.to_string();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let rows = sqlx::query_as::<_, (String, String, Option<String>, String, String)>(
                    "SELECT id::text, name, parent_id::text, user_id, path \
                     FROM storage.folders \
                     WHERE parent_id = $1::uuid AND NOT is_trashed \
                     ORDER BY name",
                )
                .bind(&folder_id)
                .fetch_all(&pool)
                .await?;

                Ok(rows
                    .into_iter()
                    .map(|(id, name, parent_id, user_id, path)| Folder {
                        id,
                        name,
                        parent_id,
                        user_id,
                        path,
                    })
                    .collect())
            })
        })
    }

    fn search_files(&self, user_id: &str, query: &str) -> Result<Vec<FileEntry>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let pattern = format!("%{}%", query);
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let rows = sqlx::query_as::<
                    _,
                    (
                        String,
                        String,
                        Option<String>,
                        String,
                        String,
                        i64,
                        String,
                        Option<String>,
                        Option<String>,
                    ),
                >(
                    "SELECT id::text, name, folder_id::text, user_id, blob_hash, size, \
                     mime_type, \
                     created_at::text, updated_at::text \
                     FROM storage.files \
                     WHERE user_id = $1 AND name ILIKE $2 AND NOT is_trashed \
                     ORDER BY name",
                )
                .bind(&user_id)
                .bind(&pattern)
                .fetch_all(&pool)
                .await?;

                Ok(rows
                    .into_iter()
                    .map(
                        |(id, name, folder_id, user_id, blob_hash, size, mime_type, created_at, updated_at)| {
                            FileEntry {
                                id,
                                name,
                                folder_id,
                                user_id,
                                blob_hash,
                                size: size as u64,
                                mime_type,
                                created_at,
                                updated_at,
                            }
                        },
                    )
                    .collect())
            })
        })
    }

    fn get_blob_record(&self, hash: &str) -> Result<Option<BlobRecord>> {
        let pool = self.pool.clone();
        let hash = hash.to_string();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let row = sqlx::query_as::<_, (String, i64, i32, Option<String>)>(
                    "SELECT hash, size, ref_count, content_type \
                     FROM storage.blobs \
                     WHERE hash = $1",
                )
                .bind(&hash)
                .fetch_optional(&pool)
                .await?;

                Ok(row.map(|(hash, size, ref_count, content_type)| BlobRecord {
                    hash,
                    size: size as u64,
                    ref_count,
                    content_type,
                }))
            })
        })
    }

    fn get_all_blobs(&self) -> Result<Vec<BlobRecord>> {
        let pool = self.pool.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let rows = sqlx::query_as::<_, (String, i64, i32, Option<String>)>(
                    "SELECT hash, size, ref_count, content_type \
                     FROM storage.blobs \
                     ORDER BY hash",
                )
                .fetch_all(&pool)
                .await?;

                Ok(rows
                    .into_iter()
                    .map(|(hash, size, ref_count, content_type)| BlobRecord {
                        hash,
                        size: size as u64,
                        ref_count,
                        content_type,
                    })
                    .collect())
            })
        })
    }

    fn get_all_files(&self) -> Result<Vec<FileEntry>> {
        let pool = self.pool.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let rows = sqlx::query_as::<
                    _,
                    (
                        String,
                        String,
                        Option<String>,
                        String,
                        String,
                        i64,
                        String,
                        Option<String>,
                        Option<String>,
                    ),
                >(
                    "SELECT id::text, name, folder_id::text, user_id, blob_hash, size, \
                     mime_type, \
                     created_at::text, updated_at::text \
                     FROM storage.files \
                     WHERE NOT is_trashed \
                     ORDER BY user_id, name",
                )
                .fetch_all(&pool)
                .await?;

                Ok(rows
                    .into_iter()
                    .map(
                        |(id, name, folder_id, user_id, blob_hash, size, mime_type, created_at, updated_at)| {
                            FileEntry {
                                id,
                                name,
                                folder_id,
                                user_id,
                                blob_hash,
                                size: size as u64,
                                mime_type,
                                created_at,
                                updated_at,
                            }
                        },
                    )
                    .collect())
            })
        })
    }

    fn get_all_folders(&self) -> Result<Vec<Folder>> {
        let pool = self.pool.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let rows = sqlx::query_as::<_, (String, String, Option<String>, String, String)>(
                    "SELECT id::text, name, parent_id::text, user_id, path \
                     FROM storage.folders \
                     WHERE NOT is_trashed \
                     ORDER BY user_id, path",
                )
                .fetch_all(&pool)
                .await?;

                Ok(rows
                    .into_iter()
                    .map(|(id, name, parent_id, user_id, path)| Folder {
                        id,
                        name,
                        parent_id,
                        user_id,
                        path,
                    })
                    .collect())
            })
        })
    }

    fn user_stats(&self, user_id: &str) -> Result<(u64, u64)> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let file_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM storage.files WHERE user_id = $1 AND NOT is_trashed",
                )
                .bind(&user_id)
                .fetch_one(&pool)
                .await?;

                let total_bytes: i64 = sqlx::query_scalar(
                    "SELECT COALESCE(SUM(size), 0) FROM storage.files \
                     WHERE user_id = $1 AND NOT is_trashed",
                )
                .bind(&user_id)
                .fetch_one(&pool)
                .await?;

                Ok((file_count as u64, total_bytes as u64))
            })
        })
    }
}
