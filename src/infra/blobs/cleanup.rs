use std::{
    collections::HashSet,
    path::Path,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result};

pub fn cleanup_stale_temp_files(temp_dir: &Path, older_than: Duration) -> Result<usize> {
    if !temp_dir.exists() {
        return Ok(0);
    }

    let now = SystemTime::now();
    let mut removed = 0_usize;

    for entry in std::fs::read_dir(temp_dir)
        .with_context(|| format!("failed to read temp dir {}", temp_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let metadata = entry.metadata()?;
        let modified = metadata.modified().unwrap_or(now);
        let age = now.duration_since(modified).unwrap_or_default();
        if age >= older_than {
            std::fs::remove_file(&path)
                .with_context(|| format!("failed to remove stale temp {}", path.display()))?;
            removed += 1;
        }
    }

    Ok(removed)
}

pub fn cleanup_orphan_blobs(root_dir: &Path, referenced_hashes: &HashSet<String>) -> Result<usize> {
    if !root_dir.exists() {
        return Ok(0);
    }

    let mut removed = 0_usize;
    for entry in std::fs::read_dir(root_dir)
        .with_context(|| format!("failed to read blobs dir {}", root_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.ends_with(".tmp") {
            continue;
        }
        if !referenced_hashes.contains(&file_name) {
            std::fs::remove_file(&path)
                .with_context(|| format!("failed to remove orphan blob {}", path.display()))?;
            removed += 1;
        }
    }

    Ok(removed)
}
