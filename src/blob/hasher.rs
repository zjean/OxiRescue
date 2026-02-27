use anyhow::Result;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

const BUFFER_SIZE: usize = 1024 * 1024; // 1 MB

/// Compute SHA-256 of the file at `path`, returned as a lowercase hex string.
pub fn compute_hash(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; BUFFER_SIZE];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Return `true` if the SHA-256 of the file at `path` matches `expected_hash`.
pub fn verify_hash(path: &Path, expected_hash: &str) -> Result<bool> {
    let actual = compute_hash(path)?;
    Ok(actual == expected_hash)
}
