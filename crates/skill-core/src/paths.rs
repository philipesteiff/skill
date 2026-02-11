use anyhow::{Result, anyhow};
use sha2::{Digest, Sha256};
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Paths {
    base: PathBuf,
}

impl Paths {
    pub fn from_base(base: PathBuf) -> Self {
        Self { base }
    }

    pub fn new() -> Result<Self> {
        let base = if let Ok(val) = env::var("SKILLS_HOME") {
            PathBuf::from(val)
        } else if let Ok(home) = env::var("HOME") {
            PathBuf::from(home).join(".skill")
        } else {
            return Err(anyhow!("HOME is not set"));
        };
        Ok(Self::from_base(base))
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
        let normalized = normalized_cache_url(repo_url);
        let key = sha256_hex(&normalized);
        self.cache_dir().join(format!("{key}.git"))
    }

    pub fn installed_dir(&self) -> PathBuf {
        self.base.join("installed")
    }
}

fn normalized_cache_url(repo_url: &str) -> String {
    if let Some(normalized) = crate::util::normalize_github_url(repo_url) {
        return normalized;
    }
    repo_url.trim().to_string()
}

fn sha256_hex(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn when_cache_repo_paths_have_slug_collision_inputs_should_still_be_unique() {
        let paths = Paths::from_base(PathBuf::from("/tmp/skill-test"));
        let first = paths.cache_repo_path("file:///tmp/repo-a");
        let second = paths.cache_repo_path("file:///tmp/repo_a");

        assert_ne!(first, second);
    }

    #[test]
    fn when_cache_repo_urls_are_equivalent_github_forms_should_share_path() {
        let paths = Paths::from_base(PathBuf::from("/tmp/skill-test"));
        let first = paths.cache_repo_path("https://github.com/acme/skills");
        let second = paths.cache_repo_path("https://github.com/acme/skills.git");

        assert_eq!(first, second);
        assert_eq!(
            first.extension().and_then(|ext| ext.to_str()),
            Some("git"),
            "cache path should keep .git suffix"
        );
        assert_eq!(
            first.parent(),
            Some(Path::new("/tmp/skill-test/cache/repos"))
        );
    }
}
