use std::{
    fs::OpenOptions,
    io::{Read, Write},
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
    mut reader: impl Read,
    media_type: Option<&str>,
    preview_bytes: usize,
) -> Result<BlobMetadata> {
    std::fs::create_dir_all(root_dir)
        .with_context(|| format!("failed to create {}", root_dir.display()))?;
    std::fs::create_dir_all(temp_dir)
        .with_context(|| format!("failed to create {}", temp_dir.display()))?;

    let preview_limit = preview_bytes.max(1);
    let temp_file_name = format!("{}.tmp", Uuid::now_v7());
    let temp_path = temp_dir.join(temp_file_name);
    let mut hasher = blake3::Hasher::new();
    let mut preview = Vec::with_capacity(preview_limit);
    let mut size_bytes = 0_u64;

    {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .with_context(|| format!("failed to create temp blob {}", temp_path.display()))?;
        let mut buf = vec![0_u8; 16 * 1024];

        loop {
            let read = reader
                .read(&mut buf)
                .context("failed to read blob source")?;
            if read == 0 {
                break;
            }

            let chunk = &buf[..read];
            file.write_all(chunk)
                .with_context(|| format!("failed to write temp blob {}", temp_path.display()))?;
            hasher.update(chunk);
            size_bytes += read as u64;

            if preview.len() < preview_limit {
                let remaining = preview_limit - preview.len();
                preview.extend_from_slice(&chunk[..read.min(remaining)]);
            }
        }

        file.sync_all()
            .with_context(|| format!("failed to fsync temp blob {}", temp_path.display()))?;
    }

    let hash = hasher.finalize().to_hex().to_string();
    let final_path = root_dir.join(&hash);
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

    Ok(BlobMetadata {
        blob_id: hash.clone(),
        hash,
        size_bytes,
        media_type: media_type.map(ToOwned::to_owned),
        preview_truncated: size_bytes > preview.len() as u64,
        preview,
        path: final_path,
    })
}

#[allow(dead_code)]
fn _blob_path(root_dir: &Path, hash: &str) -> PathBuf {
    root_dir.join(hash)
}
