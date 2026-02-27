use anyhow::{bail, Result};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use super::hasher::verify_hash;

/// A single blob found while iterating the blob store.
pub struct BlobEntry {
    pub hash: String,
    pub path: PathBuf,
    pub size: u64,
}

/// Read-only access to an OxiCloud content-addressed blob store.
///
/// Layout: `{blob_root}/{hash[0..2]}/{hash}.blob`
pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    /// Create a new `BlobStore` rooted at `blob_root`.
    ///
    /// Returns an error if `blob_root` does not exist or is not a directory.
    pub fn new(blob_root: &Path) -> Result<Self> {
        if !blob_root.exists() {
            bail!("blob root does not exist: {}", blob_root.display());
        }
        if !blob_root.is_dir() {
            bail!("blob root is not a directory: {}", blob_root.display());
        }
        Ok(Self {
            root: blob_root.to_path_buf(),
        })
    }

    /// Resolve the filesystem path for a blob with the given `hash`.
    pub fn blob_path(&self, hash: &str) -> PathBuf {
        let prefix = &hash[..2];
        self.root.join(prefix).join(format!("{hash}.blob"))
    }

    /// Read the entire blob with the given `hash` into memory.
    pub fn read_blob(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self.blob_path(hash);
        Ok(fs::read(&path)?)
    }

    /// Read the first `n` bytes of the blob with the given `hash`.
    ///
    /// If the blob is smaller than `n` bytes, all bytes are returned.
    pub fn read_blob_head(&self, hash: &str, n: usize) -> Result<Vec<u8>> {
        let path = self.blob_path(hash);
        let mut file = File::open(&path)?;
        let mut buf = vec![0u8; n];
        let mut total = 0usize;
        loop {
            let read = file.read(&mut buf[total..])?;
            if read == 0 {
                break;
            }
            total += read;
            if total >= n {
                break;
            }
        }
        buf.truncate(total);
        Ok(buf)
    }

    /// Re-hash the blob and compare against its filename hash.
    ///
    /// Returns `true` if the content matches the expected hash, `false` otherwise.
    pub fn verify_blob(&self, hash: &str) -> Result<bool> {
        let path = self.blob_path(hash);
        verify_hash(&path, hash)
    }

    /// Walk all prefix directories (`00`–`ff`) and yield every `*.blob` file found.
    pub fn iter_blobs(&self) -> impl Iterator<Item = BlobEntry> {
        let root = self.root.clone();
        // Collect all valid two-hex-char prefix dirs
        let prefix_dirs: Vec<PathBuf> = (0u8..=255)
            .map(|b| root.join(format!("{b:02x}")))
            .filter(|p| p.is_dir())
            .collect();

        prefix_dirs.into_iter().flat_map(|dir| {
            // Read directory entries, silently skip unreadable entries
            let entries: Vec<BlobEntry> = match fs::read_dir(&dir) {
                Err(_) => vec![],
                Ok(rd) => rd
                    .filter_map(|entry| entry.ok())
                    .filter_map(|entry| {
                        let path = entry.path();
                        // Must be a file with .blob extension
                        if !path.is_file() {
                            return None;
                        }
                        if path.extension()?.to_str()? != "blob" {
                            return None;
                        }
                        let hash = path.file_stem()?.to_str()?.to_owned();
                        let size = entry.metadata().ok()?.len();
                        Some(BlobEntry { hash, path, size })
                    })
                    .collect(),
            };
            entries
        })
    }
}
