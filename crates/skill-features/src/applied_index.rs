use anyhow::{Context, Result};
use serde_json::{Value, json};
use skill_core::paths::Paths;
use std::fs;
use std::path::{Path, PathBuf};

const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedEntry {
    pub source_id: String,
    pub name: String,
    pub target_dir: PathBuf,
    pub install_dir: PathBuf,
    pub resolved_commit: String,
    pub content_hash: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug)]
pub struct AppliedIndex {
    version: u32,
    entries: Vec<AppliedEntry>,
    dirty: bool,
}

impl Default for AppliedIndex {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            entries: Vec::new(),
            dirty: false,
        }
    }
}

impl AppliedIndex {
    pub fn load(paths: &Paths) -> Result<Self> {
        load_from_path(&paths.applied_index_path())
    }

    pub fn save(&mut self, paths: &Paths) -> Result<()> {
        save_to_path(self, &paths.applied_index_path())
    }

    pub fn is_target_managed(&self, target_dir: &Path) -> bool {
        self.entry_for_target(target_dir).is_some()
    }

    pub fn entry_for_target(&self, target_dir: &Path) -> Option<&AppliedEntry> {
        self.entries
            .iter()
            .find(|entry| entry.target_dir == target_dir)
    }

    pub fn entries_for_skill(&self, source_id: &str, name: &str) -> Vec<AppliedEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.source_id == source_id && entry.name == name)
            .cloned()
            .collect()
    }

    pub fn upsert(&mut self, entry: AppliedEntry) -> bool {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|existing| existing.target_dir == entry.target_dir)
        {
            if *existing == entry {
                return false;
            }
            *existing = entry;
            self.dirty = true;
            return true;
        }
        self.entries.push(entry);
        self.dirty = true;
        true
    }

    pub fn remove_target(&mut self, target_dir: &Path) -> bool {
        if let Some(idx) = self
            .entries
            .iter()
            .position(|entry| entry.target_dir == target_dir)
        {
            self.entries.remove(idx);
            self.dirty = true;
            return true;
        }
        false
    }
}

fn load_from_path(path: &Path) -> Result<AppliedIndex> {
    if !path.exists() {
        return Ok(AppliedIndex::default());
    }
    let data = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let value: Value = serde_json::from_str(&data)?;
    let version = value
        .get("version")
        .and_then(|value| value.as_u64())
        .map(|value| value as u32)
        .unwrap_or(CURRENT_VERSION);
    let entries = value
        .get("entries")
        .and_then(|value| value.as_array())
        .map(|entries| parse_entries(entries))
        .unwrap_or_default();
    Ok(AppliedIndex {
        version,
        entries,
        dirty: false,
    })
}

fn parse_entries(entries: &[Value]) -> Vec<AppliedEntry> {
    let mut parsed = Vec::new();
    for entry in entries {
        let Some(source_id) = entry.get("source_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(name) = entry.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(target_dir) = entry.get("target_dir").and_then(Value::as_str) else {
            continue;
        };
        let Some(install_dir) = entry.get("install_dir").and_then(Value::as_str) else {
            continue;
        };
        let Some(resolved_commit) = entry.get("resolved_commit").and_then(Value::as_str) else {
            continue;
        };
        let content_hash = entry
            .get("content_hash")
            .and_then(Value::as_str)
            .map(str::to_string);
        let updated_at = entry
            .get("updated_at")
            .and_then(Value::as_str)
            .map(str::to_string);
        parsed.push(AppliedEntry {
            source_id: source_id.to_string(),
            name: name.to_string(),
            target_dir: PathBuf::from(target_dir),
            install_dir: PathBuf::from(install_dir),
            resolved_commit: resolved_commit.to_string(),
            content_hash,
            updated_at,
        });
    }
    parsed
}

fn save_to_path(index: &mut AppliedIndex, path: &Path) -> Result<()> {
    if !index.dirty {
        return Ok(());
    }
    let entries = index.entries.iter().map(entry_to_value).collect::<Vec<_>>();
    let value = json!({
        "version": index.version,
        "entries": entries,
    });
    fs::write(path, serde_json::to_string_pretty(&value)?)
        .with_context(|| format!("write {}", path.display()))?;
    index.dirty = false;
    Ok(())
}

fn entry_to_value(entry: &AppliedEntry) -> Value {
    json!({
        "source_id": entry.source_id,
        "name": entry.name,
        "target_dir": entry.target_dir.to_string_lossy(),
        "install_dir": entry.install_dir.to_string_lossy(),
        "resolved_commit": entry.resolved_commit,
        "content_hash": entry.content_hash,
        "updated_at": entry.updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn when_index_missing_should_load_empty() -> Result<()> {
        let temp = tempdir()?;
        let index = load_from_path(&temp.path().join("applied.json"))?;

        assert_eq!(index.version, CURRENT_VERSION);
        assert!(index.entries.is_empty());

        Ok(())
    }

    #[test]
    fn when_upserting_entry_should_roundtrip() -> Result<()> {
        let temp = tempdir()?;
        let path = temp.path().join("applied.json");
        let mut index = AppliedIndex::default();
        let entry = AppliedEntry {
            source_id: "acme".to_string(),
            name: "echo".to_string(),
            target_dir: temp.path().join("target").join("acme__echo"),
            install_dir: temp.path().join("install").join("acme__echo"),
            resolved_commit: "deadbeef".to_string(),
            content_hash: Some("hash".to_string()),
            updated_at: Some("2024-01-01".to_string()),
        };

        assert!(index.upsert(entry.clone()));
        save_to_path(&mut index, &path)?;

        let loaded = load_from_path(&path)?;
        assert_eq!(loaded.entries, vec![entry]);

        Ok(())
    }

    #[test]
    fn when_removing_entry_should_update_index() -> Result<()> {
        let temp = tempdir()?;
        let path = temp.path().join("applied.json");
        let mut index = AppliedIndex::default();
        let entry = AppliedEntry {
            source_id: "acme".to_string(),
            name: "echo".to_string(),
            target_dir: temp.path().join("target").join("acme__echo"),
            install_dir: temp.path().join("install").join("acme__echo"),
            resolved_commit: "deadbeef".to_string(),
            content_hash: None,
            updated_at: None,
        };

        index.upsert(entry.clone());
        index.remove_target(&entry.target_dir);
        save_to_path(&mut index, &path)?;

        let loaded = load_from_path(&path)?;
        assert!(loaded.entries.is_empty());

        Ok(())
    }
}
