use std::{
    collections::HashSet,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, anyhow};

use crate::infra::paths::AppPaths;

pub mod cleanup;
pub mod writer;

pub const DEFAULT_PREVIEW_BYTES: usize = 4096;

#[derive(Debug, Clone)]
pub struct BlobMetadata {
    pub blob_id: String,
    pub hash: String,
    pub size_bytes: u64,
    pub media_type: Option<String>,
    pub preview: Vec<u8>,
    pub preview_truncated: bool,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BlobStore {
    root_dir: PathBuf,
    temp_dir: PathBuf,
    preview_bytes: usize,
}

impl BlobStore {
    pub fn new(paths: &AppPaths) -> Result<Self> {
        let root_dir = paths.blobs_dir();
        let temp_dir = paths.blobs_temp_dir();
        std::fs::create_dir_all(&root_dir)
            .with_context(|| format!("failed to create {}", root_dir.display()))?;
        std::fs::create_dir_all(&temp_dir)
            .with_context(|| format!("failed to create {}", temp_dir.display()))?;

        Ok(Self {
            root_dir,
            temp_dir,
            preview_bytes: DEFAULT_PREVIEW_BYTES,
        })
    }

    pub fn with_preview_bytes(mut self, preview_bytes: usize) -> Self {
        self.preview_bytes = preview_bytes;
        self
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn temp_dir(&self) -> &Path {
        &self.temp_dir
    }

    pub fn write_bytes(&self, bytes: &[u8], media_type: Option<&str>) -> Result<BlobMetadata> {
        writer::write_atomic(
            &self.root_dir,
            &self.temp_dir,
            bytes,
            media_type,
            self.preview_bytes,
        )
    }

    pub fn exists(&self, hash: &str) -> bool {
        self.path_for_hash(hash).exists()
    }

    pub fn path_for_hash(&self, hash: &str) -> PathBuf {
        self.root_dir.join(hash)
    }

    pub fn read_preview(&self, hash: &str, limit: usize) -> Result<Vec<u8>> {
        let limit = limit.min(self.preview_bytes.max(1));
        let path = self.path_for_hash(hash);
        if !path.exists() {
            return Err(anyhow!("blob {} does not exist", hash));
        }

        let mut file =
            File::open(&path).with_context(|| format!("failed to open blob {}", path.display()))?;
        let mut buf = vec![0_u8; limit];
        let read = file
            .read(&mut buf)
            .with_context(|| format!("failed to read blob {}", path.display()))?;
        buf.truncate(read);
        Ok(buf)
    }

    pub fn read_all(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self.path_for_hash(hash);
        std::fs::read(&path).with_context(|| format!("failed to read blob {}", path.display()))
    }

    pub fn cleanup_stale_temp_files(&self, older_than: Duration) -> Result<usize> {
        cleanup::cleanup_stale_temp_files(&self.temp_dir, older_than)
    }

    pub fn cleanup_orphan_blobs(&self, referenced_hashes: &HashSet<String>) -> Result<usize> {
        cleanup::cleanup_orphan_blobs(&self.root_dir, referenced_hashes)
    }
}
