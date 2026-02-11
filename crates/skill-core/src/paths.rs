use anyhow::{Result, anyhow};
use std::env;
use std::path::PathBuf;

use crate::util::slugify;

#[derive(Debug, Clone)]
pub struct Paths {
    base: PathBuf,
}

impl Paths {
    pub fn new() -> Result<Self> {
        let base = if let Ok(val) = env::var("SKILLS_HOME") {
            PathBuf::from(val)
        } else if let Ok(home) = env::var("HOME") {
            PathBuf::from(home).join(".skill")
        } else {
            return Err(anyhow!("HOME is not set"));
        };
        Ok(Self { base })
    }

    pub fn ensure_base_dirs(&self) -> Result<()> {
        crate::util::ensure_dir(&self.base)?;
        crate::util::ensure_dir(&self.base.join("sources"))?;
        crate::util::ensure_dir(&self.base.join("cache"))?;
        crate::util::ensure_dir(&self.cache_dir())?;
        crate::util::ensure_dir(&self.installed_dir())?;
        Ok(())
    }

    pub fn config_path(&self) -> PathBuf {
        self.base.join("config.json")
    }

    pub fn lock_path(&self) -> PathBuf {
        self.base.join("lock.json")
    }

    pub fn applied_index_path(&self) -> PathBuf {
        self.base.join("applied.json")
    }

    pub fn sources_dir(&self) -> PathBuf {
        self.base.join("sources")
    }

    pub fn source_dir(&self, id: &str) -> PathBuf {
        self.sources_dir().join(id)
    }

    pub fn source_index_path(&self, id: &str) -> PathBuf {
        self.source_dir(id).join("index.sqlite")
    }

    pub fn source_head_path(&self, id: &str) -> PathBuf {
        self.source_dir(id).join("head.txt")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.base.join("cache").join("repos")
    }

    pub fn cache_repo_path(&self, repo_url: &str) -> PathBuf {
        let slug = slugify(repo_url);
        self.cache_dir().join(format!("{}.git", slug))
    }

    pub fn installed_dir(&self) -> PathBuf {
        self.base.join("installed")
    }
}
