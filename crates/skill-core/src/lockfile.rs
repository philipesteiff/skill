use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;

use crate::paths::Paths;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Lockfile {
    pub skills: Vec<LockedSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedSkill {
    pub source_id: String,
    pub name: String,
    pub resolved_version: Option<String>,
    pub resolved_commit: String,
    pub content_hash: Option<String>,
    pub path: String,
    pub install_dir: String,
    pub updated_at: Option<String>,
}

pub fn load(paths: &Paths) -> Result<Lockfile> {
    let path = paths.lock_path();
    if !path.exists() {
        return Ok(Lockfile::default());
    }
    let data = fs::read_to_string(&path)?;
    if let Ok(lockfile) = serde_json::from_str::<Lockfile>(&data) {
        return Ok(lockfile);
    }
    let legacy: LegacyLockfile = serde_json::from_str(&data)?;
    Ok(legacy.into())
}

pub fn save(paths: &Paths, lockfile: &Lockfile) -> Result<()> {
    let path = paths.lock_path();
    let data = serde_json::to_string_pretty(lockfile)?;
    fs::write(path, data)?;
    Ok(())
}

pub fn upsert(lockfile: &mut Lockfile, entry: LockedSkill) {
    if let Some(existing) = lockfile
        .skills
        .iter_mut()
        .find(|skill| skill.source_id == entry.source_id && skill.name == entry.name)
    {
        *existing = entry;
        return;
    }
    lockfile.skills.push(entry);
}

pub fn remove(lockfile: &mut Lockfile, source_id: &str, name: &str) -> Option<LockedSkill> {
    if let Some(idx) = lockfile
        .skills
        .iter()
        .position(|skill| skill.source_id == source_id && skill.name == name)
    {
        return Some(lockfile.skills.remove(idx));
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LegacyLockfile {
    #[serde(default)]
    skills: Vec<LegacyLockedSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyLockedSkill {
    namespace: String,
    name: String,
    #[serde(default)]
    resolved_version: Option<String>,
    resolved_commit: String,
    path: String,
    install_dir: String,
}

impl From<LegacyLockfile> for Lockfile {
    fn from(legacy: LegacyLockfile) -> Self {
        let skills = legacy
            .skills
            .into_iter()
            .map(|entry| LockedSkill {
                source_id: entry.namespace,
                name: entry.name,
                resolved_version: entry.resolved_version,
                resolved_commit: entry.resolved_commit,
                content_hash: None,
                path: entry.path,
                install_dir: entry.install_dir,
                updated_at: None,
            })
            .collect();
        Lockfile { skills }
    }
}
