use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use directories::ProjectDirs;

pub const APP_QUALIFIER: &str = "com";
pub const APP_ORGANIZATION: &str = "torii";
pub const APP_NAME: &str = "torii";

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl AppPaths {
    pub fn from_system() -> Result<Self> {
        let project_dirs = ProjectDirs::from(APP_QUALIFIER, APP_ORGANIZATION, APP_NAME)
            .ok_or_else(|| anyhow!("failed to resolve project directories"))?;

        Self::from_base_dirs(
            project_dirs.config_dir().to_path_buf(),
            project_dirs.data_dir().to_path_buf(),
            project_dirs.cache_dir().to_path_buf(),
        )
    }

    pub fn from_base_dirs(config_dir: PathBuf, data_dir: PathBuf, cache_dir: PathBuf) -> Result<Self> {
        let paths = Self {
            config_dir,
            data_dir,
            cache_dir,
        };
        paths.ensure_dirs()?;
        Ok(paths)
    }

    pub fn from_test_base(base_dir: &std::path::Path) -> Result<Self> {
        Self::from_base_dirs(
            base_dir.join("config"),
            base_dir.join("data"),
            base_dir.join("cache"),
        )
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.config_dir).with_context(|| {
            format!("failed to create config dir {}", self.config_dir.display())
        })?;
        std::fs::create_dir_all(&self.data_dir)
            .with_context(|| format!("failed to create data dir {}", self.data_dir.display()))?;
        std::fs::create_dir_all(&self.cache_dir)
            .with_context(|| format!("failed to create cache dir {}", self.cache_dir.display()))?;
        std::fs::create_dir_all(self.blobs_dir())
            .with_context(|| format!("failed to create blobs dir {}", self.blobs_dir().display()))?;
        std::fs::create_dir_all(self.blobs_temp_dir()).with_context(|| {
            format!(
                "failed to create blobs temp dir {}",
                self.blobs_temp_dir().display()
            )
        })?;
        Ok(())
    }

    pub fn sqlite_path(&self) -> PathBuf {
        self.data_dir.join("torii.sqlite3")
    }

    pub fn blobs_dir(&self) -> PathBuf {
        self.data_dir.join("blobs")
    }

    pub fn blobs_temp_dir(&self) -> PathBuf {
        self.blobs_dir().join(".tmp")
    }
}
