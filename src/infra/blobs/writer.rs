use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use uuid::Uuid;

use super::BlobMetadata;

pub fn write_atomic(
    root_dir: &Path,
    temp_dir: &Path,
    bytes: &[u8],
    media_type: Option<&str>,
    preview_bytes: usize,
) -> Result<BlobMetadata> {
    std::fs::create_dir_all(root_dir)
        .with_context(|| format!("failed to create {}", root_dir.display()))?;
    std::fs::create_dir_all(temp_dir)
        .with_context(|| format!("failed to create {}", temp_dir.display()))?;

    let hash = blake3::hash(bytes).to_hex().to_string();
    let final_path = root_dir.join(&hash);
    let temp_file_name = format!("{}.tmp", Uuid::now_v7());
    let temp_path = temp_dir.join(temp_file_name);

    {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .with_context(|| format!("failed to create temp blob {}", temp_path.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("failed to write temp blob {}", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to fsync temp blob {}", temp_path.display()))?;
    }

    if !final_path.exists() {
        std::fs::rename(&temp_path, &final_path).with_context(|| {
            format!(
                "failed to atomically move blob {} -> {}",
                temp_path.display(),
                final_path.display()
            )
        })?;
    } else {
        std::fs::remove_file(&temp_path)
            .with_context(|| format!("failed to remove stale temp {}", temp_path.display()))?;
    }

    let preview_len = bytes.len().min(preview_bytes.max(1));
    let preview = bytes[..preview_len].to_vec();

    Ok(BlobMetadata {
        blob_id: hash.clone(),
        hash,
        size_bytes: bytes.len() as u64,
        media_type: media_type.map(ToOwned::to_owned),
        preview,
        preview_truncated: bytes.len() > preview_len,
        path: final_path,
    })
}

pub fn write_from_reader(
    root_dir: &Path,
    temp_dir: &Path,
    mut reader: impl std::io::Read,
    media_type: Option<&str>,
    preview_bytes: usize,
) -> Result<BlobMetadata> {
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .context("failed to read blob source")?;
    write_atomic(root_dir, temp_dir, &bytes, media_type, preview_bytes)
}

#[allow(dead_code)]
fn _blob_path(root_dir: &Path, hash: &str) -> PathBuf {
    root_dir.join(hash)
}
