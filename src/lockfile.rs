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
    pub namespace: String,
    pub name: String,
    pub requested: String,
    pub resolved_version: Option<String>,
    pub resolved_commit: String,
    pub repo_url: String,
    pub path: String,
    pub install_dir: String,
    pub registry_id: Option<String>,
}

pub fn load(paths: &Paths) -> Result<Lockfile> {
    let path = paths.lock_path();
    if !path.exists() {
        return Ok(Lockfile::default());
    }
    let data = fs::read_to_string(&path)?;
    let lockfile = serde_json::from_str(&data)?;
    Ok(lockfile)
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
        .find(|skill| skill.namespace == entry.namespace && skill.name == entry.name)
    {
        *existing = entry;
        return;
    }
    lockfile.skills.push(entry);
}

pub fn remove(lockfile: &mut Lockfile, namespace: &str, name: &str) -> Option<LockedSkill> {
    if let Some(idx) = lockfile
        .skills
        .iter()
        .position(|skill| skill.namespace == namespace && skill.name == name)
    {
        return Some(lockfile.skills.remove(idx));
    }
    None
}
