use oxirescue::blob::{BlobEntry, BlobStore, verify_hash};
use std::fs;
use tempfile::TempDir;

/// Helper: create a fake blob file at the correct layout path inside a TempDir.
/// Returns the path of the written file.
fn write_fake_blob(root: &TempDir, hash: &str, content: &[u8]) -> std::path::PathBuf {
    let prefix = &hash[..2];
    let dir = root.path().join(prefix);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{hash}.blob"));
    fs::write(&path, content).unwrap();
    path
}

// SHA-256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
const HELLO_HASH: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

#[test]
fn test_iter_blobs_finds_all_blobs() {
    let root = TempDir::new().unwrap();

    let hash1 = "aabbccdd1122334455667788990011223344556677889900aabbccddeeff0011";
    let hash2 = "ff00112233445566778899aabbccddeeff00112233445566778899aabbccddee";

    write_fake_blob(&root, hash1, b"content one");
    write_fake_blob(&root, hash2, b"content two");

    let store = BlobStore::new(root.path()).unwrap();
    let mut entries: Vec<BlobEntry> = store.iter_blobs().collect();
    entries.sort_by(|a, b| a.hash.cmp(&b.hash));

    assert_eq!(entries.len(), 2, "should find exactly 2 blobs");

    let hashes: Vec<&str> = entries.iter().map(|e| e.hash.as_str()).collect();
    assert!(hashes.contains(&hash1), "hash1 should be found");
    assert!(hashes.contains(&hash2), "hash2 should be found");

    for entry in &entries {
        assert!(entry.size > 0, "each blob should report a non-zero size");
        assert!(entry.path.exists(), "each blob path should exist");
    }
}

#[test]
fn test_blob_path_for_hash() {
    let root = TempDir::new().unwrap();
    let store = BlobStore::new(root.path()).unwrap();

    let hash = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
    let path = store.blob_path(hash);

    // Expected: {root}/ab/abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890.blob
    let expected = root.path().join("ab").join(format!("{hash}.blob"));
    assert_eq!(
        path, expected,
        "blob_path should resolve to {{root}}/{{prefix}}/{{hash}}.blob"
    );
}

#[test]
fn test_read_blob() {
    let root = TempDir::new().unwrap();
    let content = b"the quick brown fox jumps over the lazy dog";

    let hash = "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899";
    write_fake_blob(&root, hash, content);

    let store = BlobStore::new(root.path()).unwrap();
    let read = store.read_blob(hash).unwrap();
    assert_eq!(read, content, "read_blob should return exact file contents");

    // read_blob_head with fewer bytes than content
    let head = store.read_blob_head(hash, 10).unwrap();
    assert_eq!(
        &head,
        &content[..10],
        "read_blob_head(10) should return first 10 bytes"
    );

    // read_blob_head requesting more than available should return all bytes
    let head_all = store.read_blob_head(hash, 1000).unwrap();
    assert_eq!(
        head_all, content,
        "read_blob_head with n > size should return full content"
    );
}

#[test]
fn test_verify_blob_integrity() {
    let root = TempDir::new().unwrap();

    // HELLO_HASH is the real SHA-256 of b"hello", so verify_blob should return true
    write_fake_blob(&root, HELLO_HASH, b"hello");

    let store = BlobStore::new(root.path()).unwrap();

    let valid = store.verify_blob(HELLO_HASH).unwrap();
    assert!(
        valid,
        "blob whose filename matches its SHA-256 should verify as valid"
    );

    // Now write a blob with wrong content under a known hash => should be invalid
    let bad_hash = "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778800";
    write_fake_blob(
        &root,
        bad_hash,
        b"this content does not match the hash above",
    );
    let invalid = store.verify_blob(bad_hash).unwrap();
    assert!(!invalid, "blob with wrong content should fail verification");

    // verify_hash standalone helper
    let blob_path = store.blob_path(HELLO_HASH);
    assert!(
        verify_hash(&blob_path, HELLO_HASH).unwrap(),
        "verify_hash standalone should return true for matching content"
    );
}
