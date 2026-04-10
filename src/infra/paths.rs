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

        let paths = Self {
            config_dir: project_dirs.config_dir().to_path_buf(),
            data_dir: project_dirs.data_dir().to_path_buf(),
            cache_dir: project_dirs.cache_dir().to_path_buf(),
        };

        paths.ensure_dirs()?;
        Ok(paths)
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.config_dir).with_context(|| {
            format!("failed to create config dir {}", self.config_dir.display())
        })?;
        std::fs::create_dir_all(&self.data_dir)
            .with_context(|| format!("failed to create data dir {}", self.data_dir.display()))?;
        std::fs::create_dir_all(&self.cache_dir)
            .with_context(|| format!("failed to create cache dir {}", self.cache_dir.display()))?;
        Ok(())
    }

    pub fn ui_preferences_path(&self) -> PathBuf {
        self.config_dir.join("ui_preferences.json")
    }
}
