use crate::blob::BlobStore;
use crate::blob::classifier::classify_mime;
use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::path::Path;

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

    // First pass: count total blobs that will be processed (for the progress bar total).
    let total_for_pb = entries
        .iter()
        .filter(|e| {
            if e.size < min_size {
                return false;
            }
            if verify && !store.verify_blob(&e.hash).unwrap_or(false) {
                return false;
            }
            true
        })
        .count() as u64;

    // Set up progress bar only for non-dry-run runs.
    let pb: Option<ProgressBar> = if dry_run {
        None
    } else {
        let bar = ProgressBar::new(total_for_pb);
        bar.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({bytes_per_sec}) {msg}",
            )
            .unwrap()
            .progress_chars("#>-"),
        );
        Some(bar)
    };

    // Second pass: process blobs.
    for entry in &entries {
        if entry.size < min_size {
            stats.skipped += 1;
            continue;
        }
        if verify && !store.verify_blob(&entry.hash).unwrap_or(false) {
            stats.corrupted += 1;
            continue;
        }

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
        let cat_entry = stats
            .by_category
            .entry(category.dir_name().to_string())
            .or_insert((0, 0));
        cat_entry.0 += 1;
        cat_entry.1 += entry.size;

        if dry_run {
            continue;
        }

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create dir {}", parent.display()))?;
        }

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

        if let Some(ref bar) = pb {
            bar.inc(1);
            bar.set_message(entry.hash.to_string());
        }
    }

    if let Some(ref bar) = pb {
        bar.finish_with_message(format!(
            "Done — {} blobs, {} bytes",
            stats.total_blobs, stats.total_bytes
        ));
    }

    Ok(stats)
}
