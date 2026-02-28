use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::blob::BlobStore;
use crate::db::schema::{FileEntry, Folder, MetadataSource};

/// Export a single file from the blob store to `target_dir/filename`.
/// Creates `target_dir` if it does not exist.
pub fn export_file(blobs: &BlobStore, file: &FileEntry, target_dir: &Path) -> Result<()> {
    fs::create_dir_all(target_dir)?;
    let data = blobs.read_blob(&file.blob_hash)?;
    let dest = target_dir.join(&file.name);
    fs::write(&dest, &data)?;
    Ok(())
}

/// Recursively export a folder and all its contents to `target_dir`.
/// Creates `target_dir/<folder.name>/` and recurses.
/// Returns the total number of files exported.
pub fn export_folder(
    blobs: &BlobStore,
    meta: &dyn MetadataSource,
    folder: &Folder,
    target_dir: &Path,
) -> Result<u64> {
    let folder_dest = target_dir.join(&folder.name);
    fs::create_dir_all(&folder_dest)?;

    let mut count: u64 = 0;

    // Export all files in this folder
    let files = meta.list_files_in_folder(&folder.user_id, Some(&folder.id))?;
    for file in &files {
        export_file(blobs, file, &folder_dest)?;
        count += 1;
    }

    // Recurse into subfolders
    let subfolders = meta.get_subfolders(&folder.id)?;
    for subfolder in &subfolders {
        count += export_folder(blobs, meta, subfolder, &folder_dest)?;
    }

    Ok(count)
}
