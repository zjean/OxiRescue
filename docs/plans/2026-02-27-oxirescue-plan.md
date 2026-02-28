# OxiRescue Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a standalone TUI-first disaster recovery tool that can reconstruct a normal filesystem from OxiCloud's deduplicated blob store + PostgreSQL metadata.

**Architecture:** Standalone Rust binary with three operating modes (Live/Offline/Bare). Uses a trait-based metadata backend (`MetadataSource`) so TUI, FUSE, and export code works identically against PostgreSQL or SQLite. Blob store reader is a thin layer over the `{prefix}/{hash}.blob` layout.

**Tech Stack:** Rust, clap, ratatui + crossterm, fuser, sqlx (PostgreSQL), rusqlite, sha2, infer, indicatif, tokio

**Design doc:** `docs/plans/2026-02-27-oxirescue-design.md`

---

## Phase 1: Project Scaffold + Blob Store + Dump Command

> Bare mode works end-to-end after this phase.

### Task 1: Initialize project

**Files:**
- Create: `oxirescue/Cargo.toml`
- Create: `oxirescue/src/main.rs`
- Create: `oxirescue/src/cli.rs`

**Step 1: Create project directory and Cargo.toml**

```bash
mkdir -p /Users/janwiebe/prive/oxirescue/src
```

```toml
# oxirescue/Cargo.toml
[package]
name = "oxirescue"
version = "0.1.0"
edition = "2024"
description = "Standalone disaster recovery tool for OxiCloud"

[dependencies]
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
anyhow = "1"
```

**Step 2: Write minimal main.rs with clap subcommands skeleton**

```rust
// src/main.rs
use clap::Parser;

mod cli;

fn main() -> anyhow::Result<()> {
    let args = cli::Cli::parse();
    match args.command {
        cli::Command::Dump { blobs, output, classify, copy, verify, dry_run, min_size } => {
            println!("dump: blobs={blobs:?} output={output:?} classify={classify}");
        }
        cli::Command::ExportMetadata { db, output } => {
            println!("export-metadata: db={db} output={output:?}");
        }
        cli::Command::Mount { db, meta, blobs, mountpoint } => {
            println!("mount: mountpoint={mountpoint:?}");
        }
        cli::Command::Tui { db, meta, blobs } => {
            println!("tui: blobs={blobs:?}");
        }
    }
    Ok(())
}
```

```rust
// src/cli.rs
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "oxirescue", about = "Disaster recovery tool for OxiCloud")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Extract all blobs to a directory (works without database)
    Dump {
        /// Path to the .blobs directory
        #[arg(long)]
        blobs: PathBuf,
        /// Output directory for recovered files
        #[arg(long)]
        output: PathBuf,
        /// Group files by MIME type
        #[arg(long, default_value_t = false)]
        classify: bool,
        /// Force copy instead of hard-link
        #[arg(long, default_value_t = false)]
        copy: bool,
        /// Re-hash every blob and report corrupted files
        #[arg(long, default_value_t = false)]
        verify: bool,
        /// Show what would be extracted without doing it
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Skip blobs smaller than this (e.g. "1KB", "1MB")
        #[arg(long)]
        min_size: Option<String>,
    },
    /// Export PostgreSQL metadata to a portable SQLite file
    ExportMetadata {
        /// PostgreSQL connection string
        #[arg(long)]
        db: String,
        /// Output SQLite file path
        #[arg(long)]
        output: PathBuf,
    },
    /// Mount the OxiCloud filesystem as read-only FUSE
    Mount {
        /// PostgreSQL connection string (live mode)
        #[arg(long)]
        db: Option<String>,
        /// Path to exported SQLite metadata (offline mode)
        #[arg(long)]
        meta: Option<PathBuf>,
        /// Path to the .blobs directory
        #[arg(long)]
        blobs: PathBuf,
        /// Directory to mount on
        mountpoint: PathBuf,
    },
    /// Launch interactive TUI
    Tui {
        /// PostgreSQL connection string (live mode)
        #[arg(long)]
        db: Option<String>,
        /// Path to exported SQLite metadata (offline mode)
        #[arg(long)]
        meta: Option<PathBuf>,
        /// Path to the .blobs directory
        #[arg(long)]
        blobs: PathBuf,
    },
}
```

**Step 3: Verify it compiles and help text works**

Run: `cd /Users/janwiebe/prive/oxirescue && cargo build`
Expected: builds successfully

Run: `cargo run -- --help`
Expected: shows subcommands dump, export-metadata, mount, tui

Run: `cargo run -- dump --help`
Expected: shows dump flags

**Step 4: Commit**

```bash
git init && git add -A && git commit -m "feat: project scaffold with clap CLI skeleton"
```

---

### Task 2: Blob store reader

**Files:**
- Create: `oxirescue/src/blob/mod.rs`
- Create: `oxirescue/src/blob/store.rs`
- Create: `oxirescue/src/blob/hasher.rs`
- Modify: `oxirescue/src/main.rs` (add `mod blob;`)

**Step 1: Write tests for blob store**

Create `oxirescue/tests/blob_store_test.rs`:

```rust
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// Helper: create a fake blob store with known files
fn create_test_blob_store(dir: &TempDir) -> PathBuf {
    let blobs = dir.path().join(".blobs");
    // Create prefix dir "a1"
    fs::create_dir_all(blobs.join("a1")).unwrap();
    // Create a blob file
    let hash = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2";
    fs::write(blobs.join("a1").join(format!("{hash}.blob")), b"hello world").unwrap();
    // Create prefix dir "ff" with another blob
    fs::create_dir_all(blobs.join("ff")).unwrap();
    let hash2 = "ff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
    fs::write(blobs.join("ff").join(format!("{hash2}.blob")), b"test data 123").unwrap();
    blobs
}

#[test]
fn test_iter_blobs_finds_all_blobs() {
    let dir = TempDir::new().unwrap();
    let blobs_path = create_test_blob_store(&dir);
    let store = oxirescue::blob::BlobStore::new(&blobs_path).unwrap();
    let entries: Vec<_> = store.iter_blobs().collect();
    assert_eq!(entries.len(), 2);
}

#[test]
fn test_blob_path_for_hash() {
    let dir = TempDir::new().unwrap();
    let blobs_path = create_test_blob_store(&dir);
    let store = oxirescue::blob::BlobStore::new(&blobs_path).unwrap();
    let hash = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2";
    let path = store.blob_path(hash);
    assert!(path.exists());
}

#[test]
fn test_read_blob() {
    let dir = TempDir::new().unwrap();
    let blobs_path = create_test_blob_store(&dir);
    let store = oxirescue::blob::BlobStore::new(&blobs_path).unwrap();
    let hash = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2";
    let data = store.read_blob(hash).unwrap();
    assert_eq!(data, b"hello world");
}

#[test]
fn test_verify_blob_integrity() {
    let dir = TempDir::new().unwrap();
    let blobs_path = dir.path().join(".blobs");
    fs::create_dir_all(blobs_path.join("2c")).unwrap();
    // SHA-256 of "hello" = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
    let hash = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    fs::write(blobs_path.join("2c").join(format!("{hash}.blob")), b"hello").unwrap();
    let store = oxirescue::blob::BlobStore::new(&blobs_path).unwrap();
    assert!(store.verify_blob(hash).unwrap());
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --test blob_store_test`
Expected: FAIL — module `blob` not found

**Step 3: Implement blob store**

Add `tempfile` to `Cargo.toml` dev-dependencies and `sha2`, `hex` to dependencies:

```toml
[dependencies]
# ... existing ...
sha2 = "0.10"
hex = "0.4"

[dev-dependencies]
tempfile = "3"
```

```rust
// src/blob/mod.rs
mod store;
mod hasher;

pub use store::{BlobStore, BlobEntry};
pub use hasher::verify_hash;
```

```rust
// src/blob/hasher.rs
use sha2::{Sha256, Digest};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// Compute SHA-256 hash of a file, returns hex string.
pub fn hash_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1024 * 1024]; // 1 MB buffer
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Verify a blob file matches its expected hash.
pub fn verify_hash(path: &Path, expected_hash: &str) -> io::Result<bool> {
    let actual = hash_file(path)?;
    Ok(actual == expected_hash)
}
```

```rust
// src/blob/store.rs
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

/// A discovered blob in the store.
pub struct BlobEntry {
    pub hash: String,
    pub path: PathBuf,
    pub size: u64,
}

/// Read-only access to the OxiCloud blob store layout.
pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    pub fn new(blob_root: &Path) -> Result<Self> {
        anyhow::ensure!(blob_root.is_dir(), "Blob root does not exist: {}", blob_root.display());
        Ok(Self { root: blob_root.to_path_buf() })
    }

    /// Resolve the filesystem path for a blob hash.
    /// Layout: {root}/{hash[0..2]}/{hash}.blob
    pub fn blob_path(&self, hash: &str) -> PathBuf {
        let prefix = &hash[..2];
        self.root.join(prefix).join(format!("{hash}.blob"))
    }

    /// Read a blob's content entirely into memory.
    pub fn read_blob(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self.blob_path(hash);
        fs::read(&path).with_context(|| format!("Failed to read blob {hash}"))
    }

    /// Read the first N bytes of a blob (for MIME detection).
    pub fn read_blob_head(&self, hash: &str, n: usize) -> Result<Vec<u8>> {
        use std::io::Read;
        let path = self.blob_path(hash);
        let mut f = fs::File::open(&path)
            .with_context(|| format!("Failed to open blob {hash}"))?;
        let mut buf = vec![0u8; n];
        let read = f.read(&mut buf)?;
        buf.truncate(read);
        Ok(buf)
    }

    /// Verify a blob's integrity by re-hashing.
    pub fn verify_blob(&self, hash: &str) -> Result<bool> {
        let path = self.blob_path(hash);
        super::hasher::verify_hash(&path, hash).map_err(Into::into)
    }

    /// Iterate all blobs in the store.
    pub fn iter_blobs(&self) -> impl Iterator<Item = BlobEntry> {
        let mut entries = Vec::new();
        // Walk prefix dirs 00-ff
        if let Ok(prefixes) = fs::read_dir(&self.root) {
            for prefix_entry in prefixes.flatten() {
                let prefix_path = prefix_entry.path();
                if !prefix_path.is_dir() { continue; }
                if let Ok(blobs) = fs::read_dir(&prefix_path) {
                    for blob_entry in blobs.flatten() {
                        let path = blob_entry.path();
                        if path.extension().and_then(|e| e.to_str()) != Some("blob") {
                            continue;
                        }
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            let size = blob_entry.metadata().map(|m| m.len()).unwrap_or(0);
                            entries.push(BlobEntry {
                                hash: stem.to_string(),
                                path,
                                size,
                            });
                        }
                    }
                }
            }
        }
        entries.into_iter()
    }
}
```

Update `src/main.rs` — add `pub mod blob;` and make it a lib+bin:

```rust
// src/lib.rs
pub mod blob;
```

Update `src/main.rs`:
```rust
use clap::Parser;

mod cli;

fn main() -> anyhow::Result<()> {
    let args = cli::Cli::parse();
    match args.command {
        cli::Command::Dump { blobs, output, classify, copy, verify, dry_run, min_size } => {
            println!("dump: blobs={blobs:?} output={output:?} classify={classify}");
        }
        cli::Command::ExportMetadata { db, output } => {
            println!("export-metadata: db={db} output={output:?}");
        }
        cli::Command::Mount { db, meta, blobs, mountpoint } => {
            println!("mount: mountpoint={mountpoint:?}");
        }
        cli::Command::Tui { db, meta, blobs } => {
            println!("tui: blobs={blobs:?}");
        }
    }
    Ok(())
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --test blob_store_test`
Expected: all 4 tests pass

**Step 5: Commit**

```bash
git add -A && git commit -m "feat: blob store reader with hash verification"
```

---

### Task 3: MIME classifier

**Files:**
- Create: `oxirescue/src/blob/classifier.rs`
- Modify: `oxirescue/src/blob/mod.rs` (add pub use)

**Step 1: Write tests**

Create `oxirescue/tests/classifier_test.rs`:

```rust
use oxirescue::blob::classifier::{classify_mime, MimeCategory};

#[test]
fn test_classify_jpeg() {
    // JPEG magic bytes: FF D8 FF
    let head = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
    let (cat, ext) = classify_mime(head);
    assert_eq!(cat, MimeCategory::Images);
    assert_eq!(ext, "jpg");
}

#[test]
fn test_classify_pdf() {
    let head = b"%PDF-1.4 fake header";
    let (cat, ext) = classify_mime(head);
    assert_eq!(cat, MimeCategory::Documents);
    assert_eq!(ext, "pdf");
}

#[test]
fn test_classify_unknown() {
    let head = &[0x00, 0x01, 0x02, 0x03];
    let (cat, ext) = classify_mime(head);
    assert_eq!(cat, MimeCategory::Unknown);
    assert_eq!(ext, "bin");
}

#[test]
fn test_classify_png() {
    // PNG magic: 89 50 4E 47 0D 0A 1A 0A
    let head = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    let (cat, ext) = classify_mime(head);
    assert_eq!(cat, MimeCategory::Images);
    assert_eq!(ext, "png");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --test classifier_test`
Expected: FAIL

**Step 3: Implement classifier**

Add to `Cargo.toml`:
```toml
infer = "0.16"
```

```rust
// src/blob/classifier.rs
use infer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MimeCategory {
    Images,
    Documents,
    Video,
    Audio,
    Unknown,
}

impl MimeCategory {
    pub fn dir_name(&self) -> &'static str {
        match self {
            Self::Images => "images",
            Self::Documents => "documents",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for MimeCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.dir_name())
    }
}

/// Classify file content by its magic bytes.
/// Returns (category, file extension).
pub fn classify_mime(head: &[u8]) -> (MimeCategory, &'static str) {
    match infer::get(head) {
        Some(kind) => {
            let mime = kind.mime_type();
            let ext = kind.extension();
            let cat = if mime.starts_with("image/") {
                MimeCategory::Images
            } else if mime.starts_with("video/") {
                MimeCategory::Video
            } else if mime.starts_with("audio/") {
                MimeCategory::Audio
            } else if is_document_mime(mime) {
                MimeCategory::Documents
            } else {
                MimeCategory::Unknown
            };
            (cat, ext)
        }
        None => (MimeCategory::Unknown, "bin"),
    }
}

fn is_document_mime(mime: &str) -> bool {
    matches!(mime,
        "application/pdf"
        | "application/msword"
        | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        | "application/vnd.openxmlformats-officedocument.presentationml.presentation"
        | "application/vnd.ms-excel"
        | "application/vnd.ms-powerpoint"
        | "application/vnd.oasis.opendocument.text"
        | "application/vnd.oasis.opendocument.spreadsheet"
        | "application/vnd.oasis.opendocument.presentation"
        | "application/rtf"
        | "application/epub+zip"
        | "text/plain"
        | "text/csv"
        | "text/html"
        | "text/xml"
        | "application/xml"
        | "application/json"
    )
}
```

Update `src/blob/mod.rs`:
```rust
mod store;
mod hasher;
pub mod classifier;

pub use store::{BlobStore, BlobEntry};
pub use hasher::verify_hash;
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --test classifier_test`
Expected: all 4 tests pass

**Step 5: Commit**

```bash
git add -A && git commit -m "feat: MIME classifier using magic bytes"
```

---

### Task 4: Dump command (bare mode)

**Files:**
- Create: `oxirescue/src/dump/mod.rs`
- Create: `oxirescue/src/dump/recover.rs`
- Modify: `oxirescue/src/lib.rs` (add `pub mod dump;`)
- Modify: `oxirescue/src/main.rs` (wire up dump command)

**Step 1: Write integration test**

Create `oxirescue/tests/dump_test.rs`:

```rust
use std::fs;
use tempfile::TempDir;

fn create_test_blobs(dir: &TempDir) -> std::path::PathBuf {
    let blobs = dir.path().join(".blobs");
    // PNG-like blob
    fs::create_dir_all(blobs.join("89")).unwrap();
    let png_hash = "8900000000000000000000000000000000000000000000000000000000000001";
    let mut png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    png_data.extend_from_slice(&[0u8; 100]); // pad
    fs::write(blobs.join("89").join(format!("{png_hash}.blob")), &png_data).unwrap();

    // PDF-like blob
    fs::create_dir_all(blobs.join("25")).unwrap();
    let pdf_hash = "2500000000000000000000000000000000000000000000000000000000000002";
    fs::write(blobs.join("25").join(format!("{pdf_hash}.blob")), b"%PDF-1.4 fake").unwrap();

    // Unknown blob
    fs::create_dir_all(blobs.join("00")).unwrap();
    let unk_hash = "0000000000000000000000000000000000000000000000000000000000000003";
    fs::write(blobs.join("00").join(format!("{unk_hash}.blob")), &[0x00, 0x01, 0x02]).unwrap();

    blobs
}

#[test]
fn test_dump_flat() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let blobs_path = create_test_blobs(&src);
    let output = dst.path().join("recovered");

    let stats = oxirescue::dump::recover::dump_blobs(
        &blobs_path,
        &output,
        false, // classify
        true,  // copy (no hard-link in tests, different fs)
        false, // verify
        false, // dry_run
        None,  // min_size
    ).unwrap();

    assert_eq!(stats.total_blobs, 3);
    // All files should be in output root (flat)
    let files: Vec<_> = fs::read_dir(&output).unwrap().flatten().collect();
    assert_eq!(files.len(), 3);
}

#[test]
fn test_dump_classified() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let blobs_path = create_test_blobs(&src);
    let output = dst.path().join("recovered");

    let stats = oxirescue::dump::recover::dump_blobs(
        &blobs_path,
        &output,
        true,  // classify
        true,  // copy
        false,
        false,
        None,
    ).unwrap();

    assert_eq!(stats.total_blobs, 3);
    assert!(output.join("images").is_dir());
    assert!(output.join("documents").is_dir());
    assert!(output.join("unknown").is_dir());
}

#[test]
fn test_dump_dry_run_creates_nothing() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let blobs_path = create_test_blobs(&src);
    let output = dst.path().join("recovered");

    let stats = oxirescue::dump::recover::dump_blobs(
        &blobs_path,
        &output,
        false,
        true,
        false,
        true, // dry_run
        None,
    ).unwrap();

    assert_eq!(stats.total_blobs, 3);
    assert!(!output.exists());
}

#[test]
fn test_dump_min_size_filter() {
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    let blobs_path = create_test_blobs(&src);
    let output = dst.path().join("recovered");

    let stats = oxirescue::dump::recover::dump_blobs(
        &blobs_path,
        &output,
        false,
        true,
        false,
        false,
        Some(10), // min 10 bytes — the 3-byte "unknown" blob should be skipped
    ).unwrap();

    assert_eq!(stats.total_blobs, 2);
    assert_eq!(stats.skipped, 1);
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --test dump_test`
Expected: FAIL — module `dump` not found

**Step 3: Implement dump**

```rust
// src/dump/mod.rs
pub mod recover;
```

```rust
// src/dump/recover.rs
use std::fs;
use std::path::Path;
use anyhow::{Context, Result};
use crate::blob::BlobStore;
use crate::blob::classifier::{classify_mime, MimeCategory};

pub struct DumpStats {
    pub total_blobs: u64,
    pub total_bytes: u64,
    pub skipped: u64,
    pub corrupted: u64,
    pub by_category: std::collections::HashMap<String, (u64, u64)>, // category -> (count, bytes)
}

pub fn dump_blobs(
    blobs_path: &Path,
    output: &Path,
    classify: bool,
    force_copy: bool,
    verify: bool,
    dry_run: bool,
    min_size: Option<u64>,
) -> Result<DumpStats> {
    let store = BlobStore::new(blobs_path)?;
    let min_size = min_size.unwrap_or(0);

    let mut stats = DumpStats {
        total_blobs: 0,
        total_bytes: 0,
        skipped: 0,
        corrupted: 0,
        by_category: std::collections::HashMap::new(),
    };

    let entries: Vec<_> = store.iter_blobs().collect();

    for entry in &entries {
        // Size filter
        if entry.size < min_size {
            stats.skipped += 1;
            continue;
        }

        // Integrity check
        if verify {
            if !store.verify_blob(&entry.hash).unwrap_or(false) {
                stats.corrupted += 1;
                continue;
            }
        }

        // Classify
        let head = store.read_blob_head(&entry.hash, 8192).unwrap_or_default();
        let (category, ext) = classify_mime(&head);

        let filename = format!("{}.{}", entry.hash, ext);
        let dest = if classify {
            output.join(category.dir_name()).join(&filename)
        } else {
            output.join(&filename)
        };

        stats.total_blobs += 1;
        stats.total_bytes += entry.size;
        let cat_entry = stats.by_category
            .entry(category.dir_name().to_string())
            .or_insert((0, 0));
        cat_entry.0 += 1;
        cat_entry.1 += entry.size;

        if dry_run {
            continue;
        }

        // Ensure parent dir exists
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create dir {}", parent.display()))?;
        }

        // Try hard-link first, fall back to copy
        if force_copy {
            fs::copy(&entry.path, &dest)
                .with_context(|| format!("Failed to copy blob {}", entry.hash))?;
        } else {
            match fs::hard_link(&entry.path, &dest) {
                Ok(()) => {}
                Err(_) => {
                    fs::copy(&entry.path, &dest)
                        .with_context(|| format!("Failed to copy blob {}", entry.hash))?;
                }
            }
        }
    }

    Ok(stats)
}
```

Update `src/lib.rs`:
```rust
pub mod blob;
pub mod dump;
```

Wire up in `src/main.rs` — replace the `Dump` match arm:
```rust
cli::Command::Dump { blobs, output, classify, copy, verify, dry_run, min_size } => {
    let min_bytes = min_size.map(|s| parse_size(&s)).transpose()?;
    let stats = oxirescue::dump::recover::dump_blobs(
        &blobs, &output, classify, copy, verify, dry_run, min_bytes,
    )?;
    println!("Recovered: {} blobs ({} bytes)", stats.total_blobs, stats.total_bytes);
    if stats.skipped > 0 { println!("Skipped: {}", stats.skipped); }
    if stats.corrupted > 0 { println!("Corrupted: {}", stats.corrupted); }
    for (cat, (count, bytes)) in &stats.by_category {
        println!("  {cat}: {count} files ({bytes} bytes)");
    }
}
```

Add a `parse_size` helper in `main.rs`:
```rust
fn parse_size(s: &str) -> anyhow::Result<u64> {
    let s = s.trim().to_uppercase();
    if let Some(num) = s.strip_suffix("GB") {
        Ok(num.trim().parse::<u64>()? * 1024 * 1024 * 1024)
    } else if let Some(num) = s.strip_suffix("MB") {
        Ok(num.trim().parse::<u64>()? * 1024 * 1024)
    } else if let Some(num) = s.strip_suffix("KB") {
        Ok(num.trim().parse::<u64>()? * 1024)
    } else {
        Ok(s.parse::<u64>()?)
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --test dump_test`
Expected: all 4 tests pass

**Step 5: Manual smoke test against real OxiCloud blobs (if available)**

Run: `cargo run -- dump --blobs /path/to/storage/.blobs --output /tmp/oxirescue-test --classify --dry-run`
Expected: prints stats without creating files

**Step 6: Commit**

```bash
git add -A && git commit -m "feat: dump command for bare-mode blob recovery"
```

---

## Phase 2: Metadata Backend + Export

> After this phase: `export-metadata` works, shared `MetadataSource` trait powers TUI/FUSE later.

### Task 5: MetadataSource trait + schema types

**Files:**
- Create: `oxirescue/src/db/mod.rs`
- Create: `oxirescue/src/db/schema.rs`
- Modify: `oxirescue/src/lib.rs`

**Step 1: Define shared types and trait**

```rust
// src/db/schema.rs
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub display_name: String, // same as username if no display name
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
    fn list_files_in_folder(&self, user_id: &str, folder_id: Option<&str>) -> anyhow::Result<Vec<FileEntry>>;
    fn get_root_folders(&self, user_id: &str) -> anyhow::Result<Vec<Folder>>;
    fn get_subfolders(&self, folder_id: &str) -> anyhow::Result<Vec<Folder>>;
    fn search_files(&self, user_id: &str, query: &str) -> anyhow::Result<Vec<FileEntry>>;
    fn get_blob_record(&self, hash: &str) -> anyhow::Result<Option<BlobRecord>>;
    fn get_all_blobs(&self) -> anyhow::Result<Vec<BlobRecord>>;
    fn get_all_files(&self) -> anyhow::Result<Vec<FileEntry>>;
    fn get_all_folders(&self) -> anyhow::Result<Vec<Folder>>;
    fn user_stats(&self, user_id: &str) -> anyhow::Result<(u64, u64)>; // (file_count, total_bytes)
}
```

```rust
// src/db/mod.rs
pub mod schema;

pub use schema::*;
```

Update `src/lib.rs`:
```rust
pub mod blob;
pub mod db;
pub mod dump;
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: success

**Step 3: Commit**

```bash
git add -A && git commit -m "feat: MetadataSource trait and shared schema types"
```

---

### Task 6: PostgreSQL metadata reader

**Files:**
- Create: `oxirescue/src/db/postgres.rs`
- Modify: `oxirescue/src/db/mod.rs`
- Modify: `oxirescue/Cargo.toml` (add sqlx)

**Step 1: Add sqlx dependency**

```toml
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "chrono"] }
```

**Step 2: Implement PostgreSQL reader**

```rust
// src/db/postgres.rs
use anyhow::{Context, Result};
use sqlx::PgPool;
use super::schema::*;

pub struct PgMetadata {
    pool: PgPool,
}

impl PgMetadata {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPool::connect(database_url)
            .await
            .context("Failed to connect to PostgreSQL")?;
        Ok(Self { pool })
    }
}

impl MetadataSource for PgMetadata {
    fn stats(&self) -> Result<StorageStats> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let user_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM auth.users")
                    .fetch_one(&self.pool).await?;
                let file_count: (i64,) = sqlx::query_as(
                    "SELECT COUNT(*) FROM storage.files WHERE NOT is_trashed"
                ).fetch_one(&self.pool).await?;
                let folder_count: (i64,) = sqlx::query_as(
                    "SELECT COUNT(*) FROM storage.folders WHERE NOT is_trashed"
                ).fetch_one(&self.pool).await?;
                let blob_stats: (i64, i64) = sqlx::query_as(
                    "SELECT COUNT(*), COALESCE(SUM(size), 0) FROM storage.blobs"
                ).fetch_one(&self.pool).await?;
                let logical: (i64,) = sqlx::query_as(
                    "SELECT COALESCE(SUM(size), 0) FROM storage.files WHERE NOT is_trashed"
                ).fetch_one(&self.pool).await?;
                Ok(StorageStats {
                    user_count: user_count.0 as u64,
                    file_count: file_count.0 as u64,
                    folder_count: folder_count.0 as u64,
                    unique_blobs: blob_stats.0 as u64,
                    physical_bytes: blob_stats.1 as u64,
                    logical_bytes: logical.0 as u64,
                })
            })
        })
    }

    fn list_users(&self) -> Result<Vec<User>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows: Vec<(String, String, String, String)> = sqlx::query_as(
                    "SELECT id, username, username, role::text FROM auth.users ORDER BY username"
                ).fetch_all(&self.pool).await?;
                Ok(rows.into_iter().map(|(id, username, display_name, role)| User {
                    id, username, display_name, role,
                }).collect())
            })
        })
    }

    fn list_folders_for_user(&self, user_id: &str) -> Result<Vec<Folder>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows: Vec<(String, String, Option<String>, String, String)> = sqlx::query_as(
                    "SELECT id::text, name, parent_id::text, user_id, path \
                     FROM storage.folders WHERE user_id = $1 AND NOT is_trashed \
                     ORDER BY path"
                ).fetch_all(&self.pool).bind(user_id).await?;
                Ok(rows.into_iter().map(|(id, name, parent_id, user_id, path)| Folder {
                    id, name, parent_id, user_id, path,
                }).collect())
            })
        })
    }

    fn list_files_in_folder(&self, user_id: &str, folder_id: Option<&str>) -> Result<Vec<FileEntry>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows: Vec<(String, String, Option<String>, String, String, i64, String, Option<String>, Option<String>)> =
                    if let Some(fid) = folder_id {
                        sqlx::query_as(
                            "SELECT id::text, name, folder_id::text, user_id, blob_hash, size, mime_type, \
                             created_at::text, updated_at::text \
                             FROM storage.files WHERE user_id = $1 AND folder_id = $2::uuid AND NOT is_trashed \
                             ORDER BY name"
                        ).bind(user_id).bind(fid).fetch_all(&self.pool).await?
                    } else {
                        sqlx::query_as(
                            "SELECT id::text, name, folder_id::text, user_id, blob_hash, size, mime_type, \
                             created_at::text, updated_at::text \
                             FROM storage.files WHERE user_id = $1 AND folder_id IS NULL AND NOT is_trashed \
                             ORDER BY name"
                        ).bind(user_id).fetch_all(&self.pool).await?
                    };
                Ok(rows.into_iter().map(|(id, name, folder_id, user_id, blob_hash, size, mime_type, created_at, updated_at)| FileEntry {
                    id, name, folder_id, user_id, blob_hash, size: size as u64, mime_type, created_at, updated_at,
                }).collect())
            })
        })
    }

    fn get_root_folders(&self, user_id: &str) -> Result<Vec<Folder>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows: Vec<(String, String, Option<String>, String, String)> = sqlx::query_as(
                    "SELECT id::text, name, parent_id::text, user_id, path \
                     FROM storage.folders WHERE user_id = $1 AND parent_id IS NULL AND NOT is_trashed \
                     ORDER BY name"
                ).bind(user_id).fetch_all(&self.pool).await?;
                Ok(rows.into_iter().map(|(id, name, parent_id, user_id, path)| Folder {
                    id, name, parent_id, user_id, path,
                }).collect())
            })
        })
    }

    fn get_subfolders(&self, folder_id: &str) -> Result<Vec<Folder>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows: Vec<(String, String, Option<String>, String, String)> = sqlx::query_as(
                    "SELECT id::text, name, parent_id::text, user_id, path \
                     FROM storage.folders WHERE parent_id = $1::uuid AND NOT is_trashed \
                     ORDER BY name"
                ).bind(folder_id).fetch_all(&self.pool).await?;
                Ok(rows.into_iter().map(|(id, name, parent_id, user_id, path)| Folder {
                    id, name, parent_id, user_id, path,
                }).collect())
            })
        })
    }

    fn search_files(&self, user_id: &str, query: &str) -> Result<Vec<FileEntry>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let pattern = format!("%{query}%");
                let rows: Vec<(String, String, Option<String>, String, String, i64, String, Option<String>, Option<String>)> =
                    sqlx::query_as(
                        "SELECT id::text, name, folder_id::text, user_id, blob_hash, size, mime_type, \
                         created_at::text, updated_at::text \
                         FROM storage.files WHERE user_id = $1 AND name ILIKE $2 AND NOT is_trashed \
                         ORDER BY name LIMIT 100"
                    ).bind(user_id).bind(&pattern).fetch_all(&self.pool).await?;
                Ok(rows.into_iter().map(|(id, name, folder_id, user_id, blob_hash, size, mime_type, created_at, updated_at)| FileEntry {
                    id, name, folder_id, user_id, blob_hash, size: size as u64, mime_type, created_at, updated_at,
                }).collect())
            })
        })
    }

    fn get_blob_record(&self, hash: &str) -> Result<Option<BlobRecord>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let row: Option<(String, i64, i32, Option<String>)> = sqlx::query_as(
                    "SELECT hash, size, ref_count, content_type FROM storage.blobs WHERE hash = $1"
                ).bind(hash).fetch_optional(&self.pool).await?;
                Ok(row.map(|(hash, size, ref_count, content_type)| BlobRecord {
                    hash, size: size as u64, ref_count, content_type,
                }))
            })
        })
    }

    fn get_all_blobs(&self) -> Result<Vec<BlobRecord>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows: Vec<(String, i64, i32, Option<String>)> = sqlx::query_as(
                    "SELECT hash, size, ref_count, content_type FROM storage.blobs"
                ).fetch_all(&self.pool).await?;
                Ok(rows.into_iter().map(|(hash, size, ref_count, content_type)| BlobRecord {
                    hash, size: size as u64, ref_count, content_type,
                }).collect())
            })
        })
    }

    fn get_all_files(&self) -> Result<Vec<FileEntry>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows: Vec<(String, String, Option<String>, String, String, i64, String, Option<String>, Option<String>)> =
                    sqlx::query_as(
                        "SELECT id::text, name, folder_id::text, user_id, blob_hash, size, mime_type, \
                         created_at::text, updated_at::text FROM storage.files"
                    ).fetch_all(&self.pool).await?;
                Ok(rows.into_iter().map(|(id, name, folder_id, user_id, blob_hash, size, mime_type, created_at, updated_at)| FileEntry {
                    id, name, folder_id, user_id, blob_hash, size: size as u64, mime_type, created_at, updated_at,
                }).collect())
            })
        })
    }

    fn get_all_folders(&self) -> Result<Vec<Folder>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows: Vec<(String, String, Option<String>, String, String)> = sqlx::query_as(
                    "SELECT id::text, name, parent_id::text, user_id, path FROM storage.folders"
                ).fetch_all(&self.pool).await?;
                Ok(rows.into_iter().map(|(id, name, parent_id, user_id, path)| Folder {
                    id, name, parent_id, user_id, path,
                }).collect())
            })
        })
    }

    fn user_stats(&self, user_id: &str) -> Result<(u64, u64)> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let row: (i64, i64) = sqlx::query_as(
                    "SELECT COUNT(*), COALESCE(SUM(size), 0) \
                     FROM storage.files WHERE user_id = $1 AND NOT is_trashed"
                ).bind(user_id).fetch_one(&self.pool).await?;
                Ok((row.0 as u64, row.1 as u64))
            })
        })
    }
}
```

Update `src/db/mod.rs`:
```rust
pub mod schema;
pub mod postgres;

pub use schema::*;
```

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: success (no test for PG since it requires a running database)

**Step 4: Commit**

```bash
git add -A && git commit -m "feat: PostgreSQL metadata reader"
```

---

### Task 7: SQLite metadata reader + export command

**Files:**
- Create: `oxirescue/src/db/sqlite.rs`
- Create: `oxirescue/src/export/mod.rs`
- Create: `oxirescue/src/export/metadata.rs`
- Modify: `oxirescue/src/db/mod.rs`
- Modify: `oxirescue/src/lib.rs`
- Modify: `oxirescue/src/main.rs`

**Step 1: Add rusqlite dependency**

```toml
rusqlite = { version = "0.32", features = ["bundled"] }
```

**Step 2: Write test for SQLite round-trip**

Create `oxirescue/tests/sqlite_test.rs`:

```rust
use tempfile::TempDir;
use oxirescue::db::schema::*;
use oxirescue::db::sqlite::SqliteMetadata;

fn create_test_db(dir: &TempDir) -> std::path::PathBuf {
    let db_path = dir.path().join("test.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    SqliteMetadata::init_schema(&conn).unwrap();

    conn.execute("INSERT INTO users (id, username, display_name, role) VALUES ('u1', 'alice', 'Alice', 'user')", []).unwrap();
    conn.execute("INSERT INTO folders (id, name, parent_id, user_id, path) VALUES ('f1', 'Documents', NULL, 'u1', 'Documents')", []).unwrap();
    conn.execute("INSERT INTO folders (id, name, parent_id, user_id, path) VALUES ('f2', 'Work', 'f1', 'u1', 'Documents/Work')", []).unwrap();
    conn.execute("INSERT INTO blobs (hash, size, ref_count, content_type) VALUES ('aabbccdd', 1024, 1, 'text/plain')", []).unwrap();
    conn.execute(
        "INSERT INTO files (id, name, folder_id, user_id, blob_hash, size, mime_type, created_at, updated_at) \
         VALUES ('file1', 'notes.txt', 'f2', 'u1', 'aabbccdd', 1024, 'text/plain', '2025-01-01', '2025-01-01')",
        [],
    ).unwrap();
    db_path
}

#[test]
fn test_sqlite_stats() {
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
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let meta = SqliteMetadata::open(&db_path).unwrap();
    let users = meta.list_users().unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].username, "alice");
}

#[test]
fn test_sqlite_folder_hierarchy() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let meta = SqliteMetadata::open(&db_path).unwrap();

    let roots = meta.get_root_folders("u1").unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].name, "Documents");

    let subs = meta.get_subfolders(&roots[0].id).unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].name, "Work");
}

#[test]
fn test_sqlite_list_files() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let meta = SqliteMetadata::open(&db_path).unwrap();
    let files = meta.list_files_in_folder("u1", Some("f2")).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "notes.txt");
    assert_eq!(files[0].blob_hash, "aabbccdd");
}
```

**Step 3: Run tests to verify they fail**

Run: `cargo test --test sqlite_test`
Expected: FAIL

**Step 4: Implement SQLite reader**

```rust
// src/db/sqlite.rs
use anyhow::{Context, Result};
use rusqlite::Connection;
use super::schema::*;

pub struct SqliteMetadata {
    conn: Connection,
}

impl SqliteMetadata {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open SQLite db: {}", path.display()))?;
        Ok(Self { conn })
    }

    pub fn init_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL,
                display_name TEXT NOT NULL,
                role TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS folders (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                parent_id TEXT,
                user_id TEXT NOT NULL,
                path TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS files (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                folder_id TEXT,
                user_id TEXT NOT NULL,
                blob_hash TEXT NOT NULL,
                size INTEGER NOT NULL,
                mime_type TEXT NOT NULL,
                created_at TEXT,
                updated_at TEXT
            );
            CREATE TABLE IF NOT EXISTS blobs (
                hash TEXT PRIMARY KEY,
                size INTEGER NOT NULL,
                ref_count INTEGER NOT NULL,
                content_type TEXT
            );
            CREATE TABLE IF NOT EXISTS shares (
                id TEXT PRIMARY KEY,
                item_id TEXT NOT NULL,
                item_type TEXT NOT NULL,
                token TEXT NOT NULL,
                permissions_read INTEGER NOT NULL DEFAULT 1,
                expires_at INTEGER,
                created_by TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_folders_user ON folders(user_id);
            CREATE INDEX IF NOT EXISTS idx_folders_parent ON folders(parent_id);
            CREATE INDEX IF NOT EXISTS idx_files_user ON files(user_id);
            CREATE INDEX IF NOT EXISTS idx_files_folder ON files(folder_id);
            CREATE INDEX IF NOT EXISTS idx_files_blob ON files(blob_hash);"
        )?;
        Ok(())
    }
}

impl MetadataSource for SqliteMetadata {
    fn stats(&self) -> Result<StorageStats> {
        let user_count: u64 = self.conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
        let file_count: u64 = self.conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
        let folder_count: u64 = self.conn.query_row("SELECT COUNT(*) FROM folders", [], |r| r.get(0))?;
        let unique_blobs: u64 = self.conn.query_row("SELECT COUNT(*) FROM blobs", [], |r| r.get(0))?;
        let physical_bytes: u64 = self.conn.query_row("SELECT COALESCE(SUM(size), 0) FROM blobs", [], |r| r.get(0))?;
        let logical_bytes: u64 = self.conn.query_row("SELECT COALESCE(SUM(size), 0) FROM files", [], |r| r.get(0))?;
        Ok(StorageStats { user_count, file_count, folder_count, unique_blobs, physical_bytes, logical_bytes })
    }

    fn list_users(&self) -> Result<Vec<User>> {
        let mut stmt = self.conn.prepare("SELECT id, username, display_name, role FROM users ORDER BY username")?;
        let rows = stmt.query_map([], |r| Ok(User {
            id: r.get(0)?, username: r.get(1)?, display_name: r.get(2)?, role: r.get(3)?,
        }))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn list_folders_for_user(&self, user_id: &str) -> Result<Vec<Folder>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, parent_id, user_id, path FROM folders WHERE user_id = ?1 ORDER BY path"
        )?;
        let rows = stmt.query_map([user_id], |r| Ok(Folder {
            id: r.get(0)?, name: r.get(1)?, parent_id: r.get(2)?, user_id: r.get(3)?, path: r.get(4)?,
        }))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn list_files_in_folder(&self, user_id: &str, folder_id: Option<&str>) -> Result<Vec<FileEntry>> {
        let mut entries = Vec::new();
        if let Some(fid) = folder_id {
            let mut stmt = self.conn.prepare(
                "SELECT id, name, folder_id, user_id, blob_hash, size, mime_type, created_at, updated_at \
                 FROM files WHERE user_id = ?1 AND folder_id = ?2 ORDER BY name"
            )?;
            let rows = stmt.query_map(rusqlite::params![user_id, fid], |r| Ok(FileEntry {
                id: r.get(0)?, name: r.get(1)?, folder_id: r.get(2)?, user_id: r.get(3)?,
                blob_hash: r.get(4)?, size: r.get::<_, i64>(5)? as u64,
                mime_type: r.get(6)?, created_at: r.get(7)?, updated_at: r.get(8)?,
            }))?;
            entries = rows.collect::<Result<Vec<_>, _>>()?;
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, name, folder_id, user_id, blob_hash, size, mime_type, created_at, updated_at \
                 FROM files WHERE user_id = ?1 AND folder_id IS NULL ORDER BY name"
            )?;
            let rows = stmt.query_map([user_id], |r| Ok(FileEntry {
                id: r.get(0)?, name: r.get(1)?, folder_id: r.get(2)?, user_id: r.get(3)?,
                blob_hash: r.get(4)?, size: r.get::<_, i64>(5)? as u64,
                mime_type: r.get(6)?, created_at: r.get(7)?, updated_at: r.get(8)?,
            }))?;
            entries = rows.collect::<Result<Vec<_>, _>>()?;
        }
        Ok(entries)
    }

    fn get_root_folders(&self, user_id: &str) -> Result<Vec<Folder>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, parent_id, user_id, path FROM folders \
             WHERE user_id = ?1 AND parent_id IS NULL ORDER BY name"
        )?;
        let rows = stmt.query_map([user_id], |r| Ok(Folder {
            id: r.get(0)?, name: r.get(1)?, parent_id: r.get(2)?, user_id: r.get(3)?, path: r.get(4)?,
        }))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn get_subfolders(&self, folder_id: &str) -> Result<Vec<Folder>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, parent_id, user_id, path FROM folders \
             WHERE parent_id = ?1 ORDER BY name"
        )?;
        let rows = stmt.query_map([folder_id], |r| Ok(Folder {
            id: r.get(0)?, name: r.get(1)?, parent_id: r.get(2)?, user_id: r.get(3)?, path: r.get(4)?,
        }))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn search_files(&self, user_id: &str, query: &str) -> Result<Vec<FileEntry>> {
        let pattern = format!("%{query}%");
        let mut stmt = self.conn.prepare(
            "SELECT id, name, folder_id, user_id, blob_hash, size, mime_type, created_at, updated_at \
             FROM files WHERE user_id = ?1 AND name LIKE ?2 ORDER BY name LIMIT 100"
        )?;
        let rows = stmt.query_map(rusqlite::params![user_id, pattern], |r| Ok(FileEntry {
            id: r.get(0)?, name: r.get(1)?, folder_id: r.get(2)?, user_id: r.get(3)?,
            blob_hash: r.get(4)?, size: r.get::<_, i64>(5)? as u64,
            mime_type: r.get(6)?, created_at: r.get(7)?, updated_at: r.get(8)?,
        }))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn get_blob_record(&self, hash: &str) -> Result<Option<BlobRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT hash, size, ref_count, content_type FROM blobs WHERE hash = ?1"
        )?;
        let mut rows = stmt.query_map([hash], |r| Ok(BlobRecord {
            hash: r.get(0)?, size: r.get::<_, i64>(1)? as u64,
            ref_count: r.get(2)?, content_type: r.get(3)?,
        }))?;
        Ok(rows.next().transpose()?)
    }

    fn get_all_blobs(&self) -> Result<Vec<BlobRecord>> {
        let mut stmt = self.conn.prepare("SELECT hash, size, ref_count, content_type FROM blobs")?;
        let rows = stmt.query_map([], |r| Ok(BlobRecord {
            hash: r.get(0)?, size: r.get::<_, i64>(1)? as u64,
            ref_count: r.get(2)?, content_type: r.get(3)?,
        }))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn get_all_files(&self) -> Result<Vec<FileEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, folder_id, user_id, blob_hash, size, mime_type, created_at, updated_at FROM files"
        )?;
        let rows = stmt.query_map([], |r| Ok(FileEntry {
            id: r.get(0)?, name: r.get(1)?, folder_id: r.get(2)?, user_id: r.get(3)?,
            blob_hash: r.get(4)?, size: r.get::<_, i64>(5)? as u64,
            mime_type: r.get(6)?, created_at: r.get(7)?, updated_at: r.get(8)?,
        }))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn get_all_folders(&self) -> Result<Vec<Folder>> {
        let mut stmt = self.conn.prepare("SELECT id, name, parent_id, user_id, path FROM folders")?;
        let rows = stmt.query_map([], |r| Ok(Folder {
            id: r.get(0)?, name: r.get(1)?, parent_id: r.get(2)?, user_id: r.get(3)?, path: r.get(4)?,
        }))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn user_stats(&self, user_id: &str) -> Result<(u64, u64)> {
        let row: (i64, i64) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(size), 0) FROM files WHERE user_id = ?1",
            [user_id], |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok((row.0 as u64, row.1 as u64))
    }
}
```

**Step 5: Implement export command**

```rust
// src/export/mod.rs
pub mod metadata;
```

```rust
// src/export/metadata.rs
use anyhow::{Context, Result};
use std::path::Path;
use crate::db::schema::MetadataSource;
use crate::db::sqlite::SqliteMetadata;

/// Export all metadata from a live MetadataSource (PostgreSQL) to a SQLite file.
pub fn export_to_sqlite(source: &dyn MetadataSource, output: &Path) -> Result<()> {
    let conn = rusqlite::Connection::open(output)
        .with_context(|| format!("Failed to create SQLite file: {}", output.display()))?;

    SqliteMetadata::init_schema(&conn)?;

    // Export users
    let users = source.list_users()?;
    let mut stmt = conn.prepare(
        "INSERT INTO users (id, username, display_name, role) VALUES (?1, ?2, ?3, ?4)"
    )?;
    for u in &users {
        stmt.execute(rusqlite::params![u.id, u.username, u.display_name, u.role])?;
    }
    drop(stmt);

    // Export folders
    let folders = source.get_all_folders()?;
    let mut stmt = conn.prepare(
        "INSERT INTO folders (id, name, parent_id, user_id, path) VALUES (?1, ?2, ?3, ?4, ?5)"
    )?;
    for f in &folders {
        stmt.execute(rusqlite::params![f.id, f.name, f.parent_id, f.user_id, f.path])?;
    }
    drop(stmt);

    // Export files
    let files = source.get_all_files()?;
    let mut stmt = conn.prepare(
        "INSERT INTO files (id, name, folder_id, user_id, blob_hash, size, mime_type, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    )?;
    for f in &files {
        stmt.execute(rusqlite::params![
            f.id, f.name, f.folder_id, f.user_id, f.blob_hash, f.size as i64, f.mime_type, f.created_at, f.updated_at
        ])?;
    }
    drop(stmt);

    // Export blobs
    let blobs = source.get_all_blobs()?;
    let mut stmt = conn.prepare(
        "INSERT INTO blobs (hash, size, ref_count, content_type) VALUES (?1, ?2, ?3, ?4)"
    )?;
    for b in &blobs {
        stmt.execute(rusqlite::params![b.hash, b.size as i64, b.ref_count, b.content_type])?;
    }
    drop(stmt);

    println!("Exported: {} users, {} folders, {} files, {} blobs",
        users.len(), folders.len(), files.len(), blobs.len());
    Ok(())
}
```

Update `src/lib.rs`:
```rust
pub mod blob;
pub mod db;
pub mod dump;
pub mod export;
```

Wire `export-metadata` in `main.rs`:
```rust
cli::Command::ExportMetadata { db, output } => {
    let rt = tokio::runtime::Runtime::new()?;
    let pg = rt.block_on(oxirescue::db::postgres::PgMetadata::connect(&db))?;
    oxirescue::export::metadata::export_to_sqlite(&pg, &output)?;
}
```

**Step 6: Run SQLite tests**

Run: `cargo test --test sqlite_test`
Expected: all 4 tests pass

**Step 7: Commit**

```bash
git add -A && git commit -m "feat: SQLite metadata reader and PG-to-SQLite export"
```

---

## Phase 3: TUI

> Interactive dashboard and dual-pane browser after this phase.

### Task 8: TUI scaffold with dashboard screen

**Files:**
- Create: `oxirescue/src/tui/mod.rs`
- Create: `oxirescue/src/tui/app.rs`
- Create: `oxirescue/src/tui/dashboard.rs`
- Modify: `oxirescue/src/lib.rs`
- Modify: `oxirescue/src/main.rs`
- Modify: `oxirescue/Cargo.toml`

**Step 1: Add TUI dependencies**

```toml
ratatui = "0.29"
crossterm = "0.28"
```

**Step 2: Implement app state machine**

```rust
// src/tui/app.rs
use crate::blob::BlobStore;
use crate::db::schema::*;

pub enum Screen {
    Dashboard,
    Browser { user_id: String, user_name: String },
}

pub struct App {
    pub meta: Box<dyn MetadataSource>,
    pub blobs: BlobStore,
    pub screen: Screen,
    pub stats: Option<StorageStats>,
    pub users: Vec<(User, u64, u64)>, // user, file_count, total_bytes
    pub selected_user: usize,
    pub should_quit: bool,
}

impl App {
    pub fn new(meta: Box<dyn MetadataSource>, blobs: BlobStore) -> Self {
        Self {
            meta, blobs,
            screen: Screen::Dashboard,
            stats: None,
            users: Vec::new(),
            selected_user: 0,
            should_quit: false,
        }
    }

    pub fn load_dashboard(&mut self) -> anyhow::Result<()> {
        self.stats = Some(self.meta.stats()?);
        let users = self.meta.list_users()?;
        self.users = users.into_iter().map(|u| {
            let (count, bytes) = self.meta.user_stats(&u.id).unwrap_or((0, 0));
            (u, count, bytes)
        }).collect();
        Ok(())
    }
}
```

**Step 3: Implement dashboard rendering**

```rust
// src/tui/dashboard.rs
use ratatui::prelude::*;
use ratatui::widgets::*;
use crate::tui::app::App;

pub fn render_dashboard(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // title
            Constraint::Length(8),  // stats
            Constraint::Min(5),    // user list
            Constraint::Length(3), // help bar
        ])
        .split(f.area());

    // Title
    let title = Paragraph::new(" OxiRescue")
        .style(Style::default().fg(Color::Cyan).bold());
    f.render_widget(title, chunks[0]);

    // Stats
    if let Some(stats) = &app.stats {
        let dedup_pct = if stats.logical_bytes > 0 {
            ((stats.logical_bytes as f64 - stats.physical_bytes as f64) / stats.logical_bytes as f64) * 100.0
        } else { 0.0 };

        let stats_text = vec![
            Line::from(vec![
                Span::raw("  Users: "), Span::styled(format!("{}", stats.user_count), Style::default().fg(Color::Yellow)),
                Span::raw("          Total files: "), Span::styled(format!("{}", stats.file_count), Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::raw("  Folders: "), Span::styled(format!("{}", stats.folder_count), Style::default().fg(Color::Yellow)),
                Span::raw("        Unique blobs: "), Span::styled(format!("{}", stats.unique_blobs), Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::raw("  Logical: "), Span::styled(format_bytes(stats.logical_bytes), Style::default().fg(Color::Green)),
                Span::raw("   Physical: "), Span::styled(format_bytes(stats.physical_bytes), Style::default().fg(Color::Green)),
            ]),
            Line::from(vec![
                Span::raw("  Dedup savings: "), Span::styled(format!("{dedup_pct:.1}%"), Style::default().fg(Color::Magenta)),
            ]),
        ];
        let stats_block = Paragraph::new(stats_text)
            .block(Block::default().borders(Borders::ALL).title(" Storage Overview "));
        f.render_widget(stats_block, chunks[1]);
    }

    // User list
    let items: Vec<ListItem> = app.users.iter().enumerate().map(|(i, (u, count, bytes))| {
        let style = if i == app.selected_user {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default()
        };
        ListItem::new(Line::from(vec![
            Span::raw(format!("  {:<20} {:>8} files   {:>10}", u.username, count, format_bytes(*bytes))),
        ])).style(style)
    }).collect();
    let user_list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Users "));
    f.render_widget(user_list, chunks[2]);

    // Help bar
    let help = Paragraph::new(" [Enter] Browse user  [e] Export all  [q] Quit")
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(help, chunks[3]);
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
```

**Step 4: Implement TUI module and main event loop**

```rust
// src/tui/mod.rs
pub mod app;
pub mod dashboard;

use std::io;
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::prelude::*;
use app::{App, Screen};

pub fn run_tui(mut app: App) -> anyhow::Result<()> {
    app.load_dashboard()?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| {
            match &app.screen {
                Screen::Dashboard => dashboard::render_dashboard(f, &app),
                Screen::Browser { .. } => {
                    // Phase 3 Task 9 will implement this
                    let msg = ratatui::widgets::Paragraph::new("Browser — not yet implemented");
                    f.render_widget(msg, f.area());
                }
            }
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match &app.screen {
                    Screen::Dashboard => match key.code {
                        KeyCode::Char('q') => { app.should_quit = true; }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if app.selected_user > 0 { app.selected_user -= 1; }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if app.selected_user + 1 < app.users.len() { app.selected_user += 1; }
                        }
                        KeyCode::Enter => {
                            if let Some((user, _, _)) = app.users.get(app.selected_user) {
                                app.screen = Screen::Browser {
                                    user_id: user.id.clone(),
                                    user_name: user.username.clone(),
                                };
                            }
                        }
                        _ => {}
                    },
                    Screen::Browser { .. } => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            app.screen = Screen::Dashboard;
                        }
                        _ => {}
                    },
                }
            }
        }

        if app.should_quit { break; }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
```

Wire up in `main.rs`:
```rust
cli::Command::Tui { db, meta, blobs } => {
    let blob_store = oxirescue::blob::BlobStore::new(&blobs)?;
    let metadata: Box<dyn oxirescue::db::schema::MetadataSource> = if let Some(db_url) = db {
        let rt = tokio::runtime::Runtime::new()?;
        Box::new(rt.block_on(oxirescue::db::postgres::PgMetadata::connect(&db_url))?)
    } else if let Some(meta_path) = meta {
        Box::new(oxirescue::db::sqlite::SqliteMetadata::open(&meta_path)?)
    } else {
        anyhow::bail!("Either --db or --meta is required for TUI mode");
    };
    let app = oxirescue::tui::app::App::new(metadata, blob_store);
    oxirescue::tui::run_tui(app)?;
}
```

Update `src/lib.rs`:
```rust
pub mod blob;
pub mod db;
pub mod dump;
pub mod export;
pub mod tui;
```

**Step 5: Verify it compiles**

Run: `cargo build`
Expected: success

**Step 6: Commit**

```bash
git add -A && git commit -m "feat: TUI scaffold with dashboard screen"
```

---

### Task 9: TUI dual-pane browser

**Files:**
- Create: `oxirescue/src/tui/browser.rs`
- Create: `oxirescue/src/tui/preview.rs`
- Create: `oxirescue/src/tui/export.rs`
- Modify: `oxirescue/src/tui/app.rs` (add browser state)
- Modify: `oxirescue/src/tui/mod.rs` (wire browser rendering + keys)

This is the largest task. It implements:
- Left pane: virtual folder tree from metadata
- Right pane: local filesystem target directory
- Bottom pane: file metadata preview
- Selection + copy/export operations

**Step 1: Add browser state to app.rs**

Add to `App`:
```rust
pub struct BrowserState {
    pub user_id: String,
    pub user_name: String,
    pub current_folder_id: Option<String>,
    pub current_path: String,
    pub folders: Vec<Folder>,
    pub files: Vec<FileEntry>,
    pub left_selected: usize,
    pub left_items: Vec<BrowserItem>, // combined folders + files
    pub selected_items: std::collections::HashSet<usize>,
    pub target_dir: std::path::PathBuf,
    pub right_entries: Vec<String>,
    pub right_selected: usize,
    pub active_pane: Pane,
    pub search_query: Option<String>,
}

pub enum BrowserItem {
    ParentDir,
    Folder(Folder),
    File(FileEntry),
}

pub enum Pane {
    Left,
    Right,
}
```

**Step 2: Implement browser rendering in browser.rs**

Render the dual-pane layout with left (virtual FS), right (local FS), and bottom (preview) areas. Use `ratatui::layout::Layout` to split into three vertical sections, then split the top horizontally.

**Step 3: Implement export logic in export.rs**

```rust
// src/tui/export.rs
use std::fs;
use std::path::Path;
use anyhow::Result;
use crate::blob::BlobStore;
use crate::db::schema::*;

/// Copy a single file from blob store to target, preserving name.
pub fn export_file(
    blobs: &BlobStore,
    file: &FileEntry,
    target_dir: &Path,
) -> Result<()> {
    fs::create_dir_all(target_dir)?;
    let dest = target_dir.join(&file.name);
    let data = blobs.read_blob(&file.blob_hash)?;
    fs::write(&dest, &data)?;
    Ok(())
}

/// Recursively export a folder subtree.
pub fn export_folder(
    blobs: &BlobStore,
    meta: &dyn MetadataSource,
    folder: &Folder,
    target_dir: &Path,
) -> Result<u64> {
    let dest = target_dir.join(&folder.name);
    fs::create_dir_all(&dest)?;
    let mut count = 0u64;

    // Export files in this folder
    let files = meta.list_files_in_folder(&folder.user_id, Some(&folder.id))?;
    for f in &files {
        export_file(blobs, f, &dest)?;
        count += 1;
    }

    // Recurse into subfolders
    let subfolders = meta.get_subfolders(&folder.id)?;
    for sf in &subfolders {
        count += export_folder(blobs, meta, sf, &dest)?;
    }

    Ok(count)
}
```

**Step 4: Wire browser into the TUI event loop**

Handle keys: `j`/`k` navigate, `Space` selects, `Enter` opens folder, `c` copies selected, `E` exports subtree, `/` searches, `Tab` switches pane, `q`/`Esc` goes back to dashboard.

**Step 5: Verify it compiles and test manually**

Run: `cargo build`
Expected: success

**Step 6: Commit**

```bash
git add -A && git commit -m "feat: TUI dual-pane browser with export"
```

---

## Phase 4: FUSE Mount

> Read-only FUSE mount works after this phase.

### Task 10: FUSE read-only filesystem

**Files:**
- Create: `oxirescue/src/fuse/mod.rs`
- Create: `oxirescue/src/fuse/mount.rs`
- Modify: `oxirescue/Cargo.toml` (add fuser)
- Modify: `oxirescue/src/lib.rs`
- Modify: `oxirescue/src/main.rs`

**Step 1: Add fuser dependency**

```toml
fuser = "0.15"
libc = "0.2"
```

**Step 2: Implement FUSE filesystem**

The FUSE implementation needs to:
1. On mount, load all users/folders/files from `MetadataSource`
2. Assign inode numbers: inode 1 = root, then sequential for users/folders/files
3. Build an in-memory inode table mapping inode → (type, metadata)
4. Implement `lookup`, `getattr`, `readdir`, `open`, `read`
5. `read` serves blob content from `BlobStore`

Key structures:
```rust
// src/fuse/mount.rs
use fuser::{Filesystem, Request, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::time::{Duration, SystemTime};
use crate::blob::BlobStore;
use crate::db::schema::*;

enum InodeKind {
    Root,
    UserDir { user_id: String },
    Folder { folder: Folder },
    File { file: FileEntry },
    MetaDir,
    MetaFile { name: String, content: Vec<u8> },
}

struct InodeEntry {
    kind: InodeKind,
    children: Vec<(String, u64)>, // name → child inode
}

pub struct OxiFs {
    blobs: BlobStore,
    inodes: HashMap<u64, InodeEntry>,
    ttl: Duration,
}
```

Implement `Filesystem` trait methods:
- `lookup`: find child inode by name in parent's children list
- `getattr`: return file attributes (size, permissions, timestamps)
- `readdir`: list children of a directory inode
- `read`: for file inodes, read from blob store; for meta files, return cached content
- All writes return `EROFS` (read-only filesystem)

**Step 3: Wire mount command in main.rs**

```rust
cli::Command::Mount { db, meta, blobs, mountpoint } => {
    let blob_store = oxirescue::blob::BlobStore::new(&blobs)?;
    let metadata: Box<dyn oxirescue::db::schema::MetadataSource> = /* same as TUI */;
    oxirescue::fuse::mount::mount_filesystem(metadata, blob_store, &mountpoint)?;
}
```

**Step 4: Verify it compiles**

Run: `cargo build`
Expected: success

**Step 5: Manual test**

```bash
mkdir /tmp/oxitest-mount
cargo run -- mount --meta test.db --blobs /path/to/.blobs /tmp/oxitest-mount
# In another terminal:
ls /tmp/oxitest-mount/
# Should show user directories
```

**Step 6: Commit**

```bash
git add -A && git commit -m "feat: read-only FUSE mount"
```

---

## Phase 5: Polish

### Task 11: Progress bars for dump command

Add `indicatif` dependency. Wrap the blob iteration in `dump/recover.rs` with a progress bar showing count and bytes.

### Task 12: Error handling and user-friendly messages

Review all `anyhow::bail!` and `?` propagation. Ensure error messages tell the user what to do (e.g., "Blob directory not found — did you pass the correct --blobs path?").

### Task 13: README

Write a README.md with usage examples for all four modes.

---

## Summary

| Phase | Tasks | What works after |
|-------|-------|-----------------|
| 1 | Tasks 1-4 | CLI skeleton, blob reader, MIME classifier, dump command (bare mode) |
| 2 | Tasks 5-7 | MetadataSource trait, PG reader, SQLite reader, export-metadata |
| 3 | Tasks 8-9 | TUI dashboard + dual-pane browser with export |
| 4 | Task 10 | FUSE read-only mount |
| 5 | Tasks 11-13 | Progress bars, error messages, README |
