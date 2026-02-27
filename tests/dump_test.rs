use oxirescue::dump::recover::dump_blobs;
use std::fs;
use tempfile::TempDir;

/// Helper: create a fake blob file at the correct layout path inside a TempDir.
fn write_fake_blob(root: &TempDir, hash: &str, content: &[u8]) -> std::path::PathBuf {
    let prefix = &hash[..2];
    let dir = root.path().join(prefix);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{hash}.blob"));
    fs::write(&path, content).unwrap();
    path
}

fn make_blobs_dir() -> TempDir {
    let blobs = TempDir::new().unwrap();

    // PNG blob: starts with PNG magic bytes + zero padding
    let mut png_content = vec![0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    png_content.extend_from_slice(&[0u8; 100]);
    write_fake_blob(
        &blobs,
        "8900000000000000000000000000000000000000000000000000000000000001",
        &png_content,
    );

    // PDF blob: starts with %PDF magic bytes
    write_fake_blob(
        &blobs,
        "2500000000000000000000000000000000000000000000000000000000000002",
        b"%PDF-1.4 fake",
    );

    // Unknown blob: 3 bytes, no recognizable magic
    write_fake_blob(
        &blobs,
        "0000000000000000000000000000000000000000000000000000000000000003",
        &[0x00u8, 0x01, 0x02],
    );

    blobs
}

#[test]
fn test_dump_flat() {
    let blobs = make_blobs_dir();
    let output = TempDir::new().unwrap();

    let stats = dump_blobs(
        blobs.path(),
        output.path(),
        false, // classify
        true,  // force_copy
        false, // verify
        false, // dry_run
        None,  // min_size
    )
    .unwrap();

    assert_eq!(stats.total_blobs, 3, "should dump exactly 3 blobs");

    // All files should be in the output root (flat)
    let entries: Vec<_> = fs::read_dir(output.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(entries.len(), 3, "output root should contain exactly 3 files");

    // Verify no subdirectories were created
    for entry in &entries {
        assert!(
            entry.path().is_file(),
            "all entries should be files in flat mode, found: {:?}",
            entry.path()
        );
    }
}

#[test]
fn test_dump_classified() {
    let blobs = make_blobs_dir();
    let output = TempDir::new().unwrap();

    let stats = dump_blobs(
        blobs.path(),
        output.path(),
        true,  // classify
        true,  // force_copy
        false, // verify
        false, // dry_run
        None,  // min_size
    )
    .unwrap();

    assert_eq!(stats.total_blobs, 3, "should dump exactly 3 blobs");

    // images/ dir should exist (PNG)
    let images_dir = output.path().join("images");
    assert!(images_dir.exists(), "images/ directory should exist");
    assert!(images_dir.is_dir(), "images/ should be a directory");

    // documents/ dir should exist (PDF)
    let documents_dir = output.path().join("documents");
    assert!(documents_dir.exists(), "documents/ directory should exist");
    assert!(documents_dir.is_dir(), "documents/ should be a directory");

    // unknown/ dir should exist
    let unknown_dir = output.path().join("unknown");
    assert!(unknown_dir.exists(), "unknown/ directory should exist");
    assert!(unknown_dir.is_dir(), "unknown/ should be a directory");

    // Verify one file in each category
    let image_files: Vec<_> = fs::read_dir(&images_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(image_files.len(), 1, "images/ should contain 1 file (PNG)");

    let doc_files: Vec<_> = fs::read_dir(&documents_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(doc_files.len(), 1, "documents/ should contain 1 file (PDF)");

    let unknown_files: Vec<_> = fs::read_dir(&unknown_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(unknown_files.len(), 1, "unknown/ should contain 1 file");
}

#[test]
fn test_dump_dry_run_creates_nothing() {
    let blobs = make_blobs_dir();
    let output_parent = TempDir::new().unwrap();
    let output = output_parent.path().join("should_not_exist");

    let stats = dump_blobs(
        blobs.path(),
        &output,
        false, // classify
        true,  // force_copy
        false, // verify
        true,  // dry_run
        None,  // min_size
    )
    .unwrap();

    // Stats should still count the blobs (dry run reports what would happen)
    assert_eq!(stats.total_blobs, 3, "dry_run should still count 3 blobs");

    // Output directory should NOT have been created
    assert!(
        !output.exists(),
        "dry_run should not create the output directory"
    );
}

#[test]
fn test_dump_min_size_filter() {
    let blobs = make_blobs_dir();
    let output = TempDir::new().unwrap();

    // min_size = 10 bytes; the unknown blob is only 3 bytes, should be skipped
    let stats = dump_blobs(
        blobs.path(),
        output.path(),
        false, // classify
        true,  // force_copy
        false, // verify
        false, // dry_run
        Some(10), // min_size: 10 bytes
    )
    .unwrap();

    assert_eq!(
        stats.skipped, 1,
        "exactly 1 blob should be skipped (the 3-byte unknown blob)"
    );
    assert_eq!(
        stats.total_blobs, 2,
        "exactly 2 blobs should be dumped (PNG and PDF are larger than 10 bytes)"
    );

    // Verify only 2 files in output
    let entries: Vec<_> = fs::read_dir(output.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(entries.len(), 2, "output root should contain exactly 2 files");
}
