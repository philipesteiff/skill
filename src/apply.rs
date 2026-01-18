use anyhow::Result;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use crate::agents::{AgentTarget, TargetKey, detect_agents, targets_for_agents};
use crate::lockfile;
use crate::output::Output;
use crate::paths::Paths;
use crate::ui;
use crate::util::{copy_dir_recursive, ensure_dir};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SkillKey {
    pub namespace: String,
    pub name: String,
}

impl SkillKey {
    pub fn label(&self) -> String {
        let namespace = &self.namespace;
        let name = &self.name;
        format!("{namespace}/{name}")
    }
}

#[derive(Clone, Debug)]
pub struct ApplySkill {
    pub key: SkillKey,
    pub source_dir: PathBuf,
    pub source_exists: bool,
}

#[derive(Clone, Debug)]
pub struct ApplySelection {
    pub targets: Vec<TargetKey>,
    pub skills: Vec<SkillKey>,
}

pub fn apply_installed(paths: &Paths, output: &mut impl Output) -> Result<()> {
    let lockfile = lockfile::load(paths)?;
    if lockfile.skills.is_empty() {
        output.line("No installed skills to apply")?;
        return Ok(());
    }

    let repo_root = env::current_dir()?;
    let agents = detect_agents(&repo_root)?;
    let targets = targets_for_agents(&agents);
    if targets.is_empty() {
        output.line("No supported agent targets detected")?;
        return Ok(());
    }

    let skills = lockfile
        .skills
        .into_iter()
        .map(|entry| {
            let source_dir = PathBuf::from(entry.install_dir);
            let key = SkillKey {
                namespace: entry.namespace,
                name: entry.name,
            };
            let source_exists = source_dir.exists();
            ApplySkill {
                key,
                source_dir,
                source_exists,
            }
        })
        .collect::<Vec<_>>();

    let applied = compute_applied(&skills, &targets);
    let selection = ui::apply::run_apply_ui(&targets, &skills, &applied)?;
    let Some(selection) = selection else {
        output.line("Canceled apply")?;
        return Ok(());
    };
    if selection.targets.is_empty() || selection.skills.is_empty() {
        output.line("No targets or skills selected")?;
        return Ok(());
    }

    let summary = build_summary(&selection, &targets, &skills);
    output.line("Planned apply:")?;
    for line in summary {
        output.line(line)?;
    }

    let results = apply_selection(&selection, &targets, &skills)?;
    output.line("Apply results:")?;
    if results.added.is_empty() && results.skipped.is_empty() && results.failed.is_empty() {
        output.line("- No actions taken")?;
    }
    for entry in results.added {
        let skill = entry.skill.label();
        let target = entry.target.label();
        output.line(format!("- Added {skill} to {target}"))?;
    }
    for entry in results.skipped {
        let skill = entry.skill.label();
        let target = entry.target.label();
        output.line(format!("- Skipped {skill} on {target} (already applied)"))?;
    }
    for entry in results.failed {
        let skill = entry.action.skill.label();
        let target = entry.action.target.label();
        let reason = entry.reason;
        output.line(format!("- Failed {skill} on {target} ({reason})"))?;
    }

    Ok(())
}

fn compute_applied(
    skills: &[ApplySkill],
    targets: &[AgentTarget],
) -> HashMap<(SkillKey, TargetKey), bool> {
    let mut applied = HashMap::new();
    for skill in skills {
        for target in targets {
            let dest = dest_dir(target, &skill.key);
            applied.insert((skill.key.clone(), target.key.clone()), dest.exists());
        }
    }
    applied
}

fn dest_dir(target: &AgentTarget, skill: &SkillKey) -> PathBuf {
    target.base_dir.join(&skill.namespace).join(&skill.name)
}

fn build_summary(
    selection: &ApplySelection,
    targets: &[AgentTarget],
    skills: &[ApplySkill],
) -> Vec<String> {
    let target_map = targets
        .iter()
        .map(|target| (target.key.clone(), target))
        .collect::<HashMap<_, _>>();
    let skill_map = skills
        .iter()
        .map(|skill| (skill.key.clone(), skill))
        .collect::<HashMap<_, _>>();
    let mut lines = Vec::new();
    let skill_labels = selection
        .skills
        .iter()
        .filter_map(|key| skill_map.get(key))
        .map(|skill| skill.key.label())
        .collect::<Vec<_>>();
    let target_labels = selection
        .targets
        .iter()
        .filter_map(|key| target_map.get(key))
        .map(|target| {
            let label = &target.label;
            let dir = target.base_dir.display();
            format!("{label} -> {dir}")
        })
        .collect::<Vec<_>>();
    let skills_summary = skill_labels.join(", ");
    let targets_summary = target_labels.join(", ");
    lines.push(format!("- Skills: {skills_summary}"));
    lines.push(format!("- Targets: {targets_summary}"));
    lines
}

struct ApplyAction {
    skill: SkillKey,
    target: TargetKey,
}

struct FailedAction {
    action: ApplyAction,
    reason: String,
}

struct ApplyResults {
    added: Vec<ApplyAction>,
    skipped: Vec<ApplyAction>,
    failed: Vec<FailedAction>,
}

fn apply_selection(
    selection: &ApplySelection,
    targets: &[AgentTarget],
    skills: &[ApplySkill],
) -> Result<ApplyResults> {
    let target_map = targets
        .iter()
        .map(|target| (target.key.clone(), target))
        .collect::<HashMap<_, _>>();
    let skill_map = skills
        .iter()
        .map(|skill| (skill.key.clone(), skill))
        .collect::<HashMap<_, _>>();
    let mut results = ApplyResults {
        added: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
    };
    for skill_key in &selection.skills {
        let Some(skill) = skill_map.get(skill_key) else {
            continue;
        };
        for target_key in &selection.targets {
            let Some(target) = target_map.get(target_key) else {
                continue;
            };
            let dest = dest_dir(target, &skill.key);
            let action = ApplyAction {
                skill: skill.key.clone(),
                target: target.key.clone(),
            };
            if dest.exists() {
                results.skipped.push(action);
                continue;
            }
            if !skill.source_exists {
                results.failed.push(FailedAction {
                    action,
                    reason: "missing source directory".to_string(),
                });
                continue;
            }
            ensure_dir(&target.base_dir)?;
            if let Some(parent) = dest.parent() {
                ensure_dir(parent)?;
            }
            match copy_dir_recursive(&skill.source_dir, &dest) {
                Ok(()) => results.added.push(action),
                Err(err) => results.failed.push(FailedAction {
                    action,
                    reason: err.to_string(),
                }),
            }
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::{AgentId, Scope};

    #[test]
    fn computes_applied_status() {
        let temp = tempfile::tempdir().expect("temp dir");
        let skill = ApplySkill {
            key: SkillKey {
                namespace: "acme".to_string(),
                name: "echo".to_string(),
            },
            source_dir: temp.path().join("src"),
            source_exists: false,
        };
        let target = AgentTarget {
            key: TargetKey {
                agent: AgentId::Cursor,
                scope: Scope::Project,
            },
            label: "Cursor (project)".to_string(),
            short: "cu:p".to_string(),
            base_dir: temp.path().join("dest"),
            detected: true,
            enabled: true,
            default_selected: true,
        };
        let applied = compute_applied(&[skill.clone()], &[target.clone()]);
        assert_eq!(
            applied
                .get(&(skill.key.clone(), target.key.clone()))
                .copied(),
            Some(false)
        );
    }
}
