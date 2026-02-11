use anyhow::{Context, Result, anyhow};
use skill_core::git;
use skill_core::util::ensure_dir;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const MANAGED_START: &str = "# >>> skill managed git tracking >>>";
pub const MANAGED_END: &str = "# <<< skill managed git tracking <<<";
const TRACKED_PREFIX: &str = "# tracked: ";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackingPreference {
    Tracked,
    NotTracked,
}

#[derive(Debug)]
pub struct GitTrackingManager {
    repo_root: PathBuf,
    exclude_path: PathBuf,
    document: ExcludeDocument,
    dirty: bool,
}

impl GitTrackingManager {
    pub fn load(repo_root: &Path) -> Result<Self> {
        let exclude_path = git::git_path(repo_root, "info/exclude")?;
        let contents = fs::read_to_string(&exclude_path).unwrap_or_default();
        Ok(Self {
            repo_root: repo_root.to_path_buf(),
            exclude_path,
            document: ExcludeDocument::parse(&contents),
            dirty: false,
        })
    }

    pub fn repo_relative_path(&self, full_path: &Path) -> Result<String> {
        repo_relative_path(&self.repo_root, full_path)
    }

    pub fn preference_for_path(&self, repo_relative_path: &str) -> Result<TrackingPreference> {
        // Default is not tracked (ignored) unless explicitly marked tracked.
        if self.document.tracked_entries.contains(repo_relative_path) {
            return Ok(TrackingPreference::Tracked);
        }
        Ok(TrackingPreference::NotTracked)
    }

    pub fn set_preference(
        &mut self,
        repo_relative_path: &str,
        preference: TrackingPreference,
    ) -> Result<bool> {
        let changed = match preference {
            TrackingPreference::Tracked => self.document.set_tracked(repo_relative_path),
            TrackingPreference::NotTracked => self.document.set_not_tracked(repo_relative_path),
        };
        if changed {
            self.dirty = true;
        }
        self.save_if_dirty()?;
        Ok(changed)
    }

    pub fn remove_managed_entry(&mut self, repo_relative_path: &str) -> Result<bool> {
        let changed = self.document.remove_managed_entry(repo_relative_path);
        if changed {
            self.dirty = true;
        }
        self.save_if_dirty()?;
        Ok(changed)
    }

    pub fn is_path_tracked(&self, repo_relative_path: &str) -> Result<bool> {
        git::is_path_tracked(&self.repo_root, Path::new(repo_relative_path))
    }

    fn save_if_dirty(&mut self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        if let Some(parent) = self.exclude_path.parent() {
            ensure_dir(parent)?;
        }
        fs::write(&self.exclude_path, self.document.render())
            .with_context(|| format!("write {}", self.exclude_path.display()))?;
        self.dirty = false;
        Ok(())
    }
}

fn repo_relative_path(repo_root: &Path, full_path: &Path) -> Result<String> {
    let rel = full_path.strip_prefix(repo_root).with_context(|| {
        format!(
            "path {} is outside repository root {}",
            full_path.display(),
            repo_root.display()
        )
    })?;
    if rel.as_os_str().is_empty() {
        return Err(anyhow!("path resolves to repository root"));
    }

    let mut parts = Vec::new();
    for component in rel.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(anyhow!(
                    "path {} contains parent segment outside managed scope",
                    full_path.display()
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(anyhow!(
                    "path {} has unsupported absolute segment",
                    full_path.display()
                ));
            }
        }
    }

    if parts.is_empty() {
        return Err(anyhow!("path resolves to repository root"));
    }

    Ok(parts.join("/"))
}

#[derive(Debug, Clone)]
struct ExcludeDocument {
    external_lines: Vec<String>,
    tracked_entries: BTreeSet<String>,
    managed_entries: BTreeSet<String>,
}

impl ExcludeDocument {
    fn parse(contents: &str) -> Self {
        let lines = contents
            .lines()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>();

        let Some(start_idx) = lines.iter().position(|line| line.trim() == MANAGED_START) else {
            return Self {
                external_lines: lines,
                tracked_entries: BTreeSet::new(),
                managed_entries: BTreeSet::new(),
            };
        };
        let Some(end_rel) = lines
            .iter()
            .skip(start_idx + 1)
            .position(|line| line.trim() == MANAGED_END)
        else {
            return Self {
                external_lines: lines,
                tracked_entries: BTreeSet::new(),
                managed_entries: BTreeSet::new(),
            };
        };
        let end_idx = start_idx + 1 + end_rel;

        let mut managed_entries = BTreeSet::new();
        let mut tracked_entries = BTreeSet::new();
        for line in &lines[start_idx + 1..end_idx] {
            let value = line.trim();
            if value.starts_with(TRACKED_PREFIX) {
                let path = value.trim_start_matches(TRACKED_PREFIX).trim();
                if !path.is_empty() {
                    tracked_entries.insert(path.to_string());
                }
                continue;
            }
            if value.is_empty() || value.starts_with('#') {
                continue;
            }
            managed_entries.insert(value.to_string());
        }

        let mut external_lines = Vec::new();
        external_lines.extend(lines[..start_idx].iter().cloned());
        external_lines.extend(lines[end_idx + 1..].iter().cloned());

        Self {
            external_lines,
            tracked_entries,
            managed_entries,
        }
    }

    fn render(&self) -> String {
        let mut lines = self.external_lines.clone();

        if !self.tracked_entries.is_empty() || !self.managed_entries.is_empty() {
            if !lines.is_empty() && !lines.last().is_some_and(|line| line.is_empty()) {
                lines.push(String::new());
            }
            lines.push(MANAGED_START.to_string());
            lines.extend(
                self.tracked_entries
                    .iter()
                    .map(|path| format!("{TRACKED_PREFIX}{path}")),
            );
            lines.extend(self.managed_entries.iter().cloned());
            lines.push(MANAGED_END.to_string());
        }

        let mut output = lines.join("\n");
        output.push('\n');
        output
    }

    fn set_not_tracked(&mut self, repo_relative_path: &str) -> bool {
        let removed_tracked = self.tracked_entries.remove(repo_relative_path);
        let inserted_managed = self.managed_entries.insert(repo_relative_path.to_string());
        removed_tracked || inserted_managed
    }

    fn set_tracked(&mut self, repo_relative_path: &str) -> bool {
        let removed_managed = self.managed_entries.remove(repo_relative_path);
        let inserted_tracked = self.tracked_entries.insert(repo_relative_path.to_string());
        removed_managed || inserted_tracked
    }

    fn remove_managed_entry(&mut self, repo_relative_path: &str) -> bool {
        let removed_managed = self.managed_entries.remove(repo_relative_path);
        let removed_tracked = self.tracked_entries.remove(repo_relative_path);
        removed_managed || removed_tracked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn when_managed_entry_added_should_preserve_existing_exclude_lines() {
        let initial = [
            "# File patterns to ignore; see `git help ignore` for more information.",
            "*.tmp",
            "",
        ]
        .join("\n");
        let mut document = ExcludeDocument::parse(&initial);

        let changed = document.set_not_tracked(".codex/skills/acme__echo-skill");

        assert!(changed);
        let rendered = document.render();
        assert!(rendered.contains("*.tmp"));
        assert!(rendered.contains(MANAGED_START));
        assert!(rendered.contains(".codex/skills/acme__echo-skill"));
        assert!(rendered.contains(MANAGED_END));
    }

    #[test]
    fn when_managed_entry_removed_should_keep_other_entries() {
        let initial = [
            "# keep",
            MANAGED_START,
            ".codex/skills/acme__echo-skill",
            ".codex/skills/acme__notes-skill",
            MANAGED_END,
            "# trailing",
        ]
        .join("\n");
        let mut document = ExcludeDocument::parse(&initial);

        let changed = document.remove_managed_entry(".codex/skills/acme__echo-skill");

        assert!(changed);
        let rendered = document.render();
        assert!(rendered.contains(".codex/skills/acme__notes-skill"));
        assert!(!rendered.contains(".codex/skills/acme__echo-skill\n"));
        assert!(rendered.contains("# keep"));
        assert!(rendered.contains("# trailing"));
    }

    #[test]
    fn when_setting_same_state_twice_should_be_idempotent() {
        let mut document = ExcludeDocument::parse("");

        let first = document.set_not_tracked(".codex/skills/acme__echo-skill");
        let once = document.render();
        let second = document.set_not_tracked(".codex/skills/acme__echo-skill");
        let twice = document.render();

        assert!(first);
        assert!(!second);
        assert_eq!(once, twice);
    }

    #[test]
    fn when_setting_tracked_should_render_as_comment_line() {
        let mut document = ExcludeDocument::parse("");

        let changed = document.set_tracked(".codex/skills/acme__echo-skill");

        assert!(changed);
        let rendered = document.render();
        assert!(rendered.contains(MANAGED_START));
        assert!(rendered.contains("# tracked: .codex/skills/acme__echo-skill"));
        // Tracked entries should not appear as ignore patterns.
        assert!(!rendered.contains("\n.codex/skills/acme__echo-skill\n"));
    }

    #[test]
    fn when_managed_entry_removed_should_remove_tracked_entry_too() {
        let initial = [
            MANAGED_START,
            "# tracked: .codex/skills/acme__echo-skill",
            MANAGED_END,
        ]
        .join("\n");
        let mut document = ExcludeDocument::parse(&initial);

        let changed = document.remove_managed_entry(".codex/skills/acme__echo-skill");

        assert!(changed);
        let rendered = document.render();
        assert!(!rendered.contains("# tracked: .codex/skills/acme__echo-skill"));
    }
}
