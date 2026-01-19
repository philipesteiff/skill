use anyhow::Result;
use clap::Args;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::str::FromStr;

mod agents;
mod ui;

use self::agents::{AgentTarget, TargetKey, detect_agents, targets_for_agents};
use skill_core::lockfile;
use skill_core::output::Output;
use skill_core::paths::Paths;
use skill_core::util::ensure_dir;

#[derive(Args, Clone, Debug)]
pub struct ApplyArgs {
    #[arg(long, value_delimiter = ',')]
    pub targets: Vec<String>,
    #[arg(long, value_delimiter = ',')]
    pub skills: Vec<String>,
    #[arg(long)]
    pub all_targets: bool,
    #[arg(long)]
    pub all_skills: bool,
    #[arg(long)]
    pub no_tui: bool,
    #[arg(long)]
    pub unapply: bool,
}

pub fn run(paths: &Paths, args: ApplyArgs) -> Result<()> {
    paths.ensure_base_dirs()?;
    let mut ui = skill_core::ui::log::LogUi::new("skill apply")?;
    let result = apply_installed(paths, &mut ui, &args);
    let finish = ui.finish();
    result?;
    finish?;
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct SkillKey {
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
struct ApplySkill {
    pub key: SkillKey,
    pub source_dir: PathBuf,
    pub source_exists: bool,
}

#[derive(Clone, Debug)]
struct ApplySelection {
    pub targets: Vec<TargetKey>,
    pub skills: Vec<SkillKey>,
}

fn apply_installed(paths: &Paths, output: &mut impl Output, args: &ApplyArgs) -> Result<()> {
    let lockfile = lockfile::load(paths)?;
    if lockfile.skills.is_empty() {
        let message = if args.unapply {
            "No installed skills to unapply"
        } else {
            "No installed skills to apply"
        };
        output.line(message)?;
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
    let selection = select_apply(output, &targets, &skills, &applied, args)?;
    let Some(selection) = selection else {
        output.line("Canceled apply")?;
        return Ok(());
    };
    if selection.targets.is_empty() || selection.skills.is_empty() {
        output.line("No targets or skills selected")?;
        return Ok(());
    }

    let summary = build_summary(&selection, &targets, &skills);
    if args.unapply {
        output.line("Planned unapply:")?;
    } else {
        output.line("Planned apply:")?;
    }
    for line in summary {
        output.line(line)?;
    }

    let intent = if args.unapply {
        ApplyIntent::UnapplyOnly
    } else if args.no_tui
        || !args.targets.is_empty()
        || !args.skills.is_empty()
        || args.all_targets
        || args.all_skills
    {
        ApplyIntent::ApplyOnly
    } else {
        ApplyIntent::DesiredState
    };
    let results = apply_selection(intent, &selection, &targets, &skills)?;
    if args.unapply {
        output.line("Unapply results:")?;
    } else {
        output.line("Apply results:")?;
    }
    if results.added.is_empty()
        && results.removed.is_empty()
        && results.skipped.is_empty()
        && results.failed.is_empty()
    {
        output.line("- No actions taken")?;
    }
    for entry in results.added {
        let skill = entry.skill.label();
        let target = entry.target.label();
        output.line(format!("- Added {skill} to {target}"))?;
    }
    for entry in results.removed {
        let skill = entry.skill.label();
        let target = entry.target.label();
        output.line(format!("- Removed {skill} from {target}"))?;
    }
    for entry in results.skipped {
        let skill = entry.skill.label();
        let target = entry.target.label();
        let reason = if args.unapply {
            "not applied"
        } else {
            "already applied"
        };
        output.line(format!("- Skipped {skill} on {target} ({reason})"))?;
    }
    for entry in results.failed {
        let skill = entry.action.skill.label();
        let target = entry.action.target.label();
        let reason = entry.reason;
        output.line(format!("- Failed {skill} on {target} ({reason})"))?;
    }

    Ok(())
}

fn select_apply(
    output: &mut impl Output,
    targets: &[AgentTarget],
    skills: &[ApplySkill],
    applied: &HashMap<(SkillKey, TargetKey), bool>,
    args: &ApplyArgs,
) -> Result<Option<ApplySelection>> {
    let wants_cli = args.no_tui
        || !args.targets.is_empty()
        || !args.skills.is_empty()
        || args.all_targets
        || args.all_skills
        || !std::io::stdout().is_terminal();
    if wants_cli {
        return select_from_args(output, targets, skills, args).map(Some);
    }
    ui::run_apply_ui(targets, skills, applied)
}

fn select_from_args(
    _output: &mut impl Output,
    targets: &[AgentTarget],
    skills: &[ApplySkill],
    args: &ApplyArgs,
) -> Result<ApplySelection> {
    let targets = if args.all_targets {
        targets.iter().map(|target| target.key.clone()).collect()
    } else if args.targets.is_empty() {
        return Err(anyhow::anyhow!(
            "no targets provided; pass --targets, --all-targets, or run in TUI"
        ));
    } else {
        args.targets
            .iter()
            .map(|value| parse_target(value, targets))
            .collect::<Result<Vec<_>>>()?
    };

    let skills = if args.all_skills {
        skills.iter().map(|skill| skill.key.clone()).collect()
    } else if args.skills.is_empty() {
        return Err(anyhow::anyhow!(
            "no skills provided; pass --skills, --all-skills, or run in TUI"
        ));
    } else {
        args.skills
            .iter()
            .map(|value| parse_skill(value, skills))
            .collect::<Result<Vec<_>>>()?
    };

    Ok(ApplySelection { targets, skills })
}

fn parse_target(value: &str, targets: &[AgentTarget]) -> Result<TargetKey> {
    let key = TargetKey::from_str(value)?;
    if !targets.iter().any(|target| target.key == key) {
        return Err(anyhow::anyhow!("unknown target: {value}"));
    }
    Ok(key)
}

fn parse_skill(value: &str, skills: &[ApplySkill]) -> Result<SkillKey> {
    let parts: Vec<&str> = value.split('/').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!("invalid skill reference: {}", value));
    }
    let key = SkillKey {
        namespace: parts[0].to_string(),
        name: parts[1].to_string(),
    };
    if !skills.iter().any(|skill| skill.key == key) {
        return Err(anyhow::anyhow!("unknown skill: {value}"));
    }
    Ok(key)
}

fn compute_applied(
    skills: &[ApplySkill],
    targets: &[AgentTarget],
) -> HashMap<(SkillKey, TargetKey), bool> {
    let mut applied = HashMap::new();
    for skill in skills {
        for target in targets {
            let dest = dest_dir(target, &skill.key);
            let is_applied = symlink_matches(&skill.source_dir, &dest).unwrap_or(false);
            applied.insert((skill.key.clone(), target.key.clone()), is_applied);
        }
    }
    applied
}

fn dest_dir(target: &AgentTarget, skill: &SkillKey) -> PathBuf {
    target
        .base_dir
        .join(format!("{}__{}", skill.namespace, skill.name))
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
    removed: Vec<ApplyAction>,
    skipped: Vec<ApplyAction>,
    failed: Vec<FailedAction>,
}

enum ApplyIntent {
    ApplyOnly,
    UnapplyOnly,
    DesiredState,
}

enum ActionMode {
    Apply,
    Unapply,
}

fn apply_selection(
    intent: ApplyIntent,
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
        removed: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
    };
    let selected_skills = selection
        .skills
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    let skills_iter: Vec<_> = match intent {
        ApplyIntent::ApplyOnly | ApplyIntent::UnapplyOnly => selection.skills.clone(),
        ApplyIntent::DesiredState => skills.iter().map(|skill| skill.key.clone()).collect(),
    };

    for skill_key in &skills_iter {
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

            let mode = match intent {
                ApplyIntent::ApplyOnly => ActionMode::Apply,
                ApplyIntent::UnapplyOnly => ActionMode::Unapply,
                ApplyIntent::DesiredState => {
                    if selected_skills.contains(&skill.key) {
                        ActionMode::Apply
                    } else {
                        ActionMode::Unapply
                    }
                }
            };

            match mode {
                ActionMode::Apply => {
                    if dest.exists() {
                        let is_managed = symlink_matches(&skill.source_dir, &dest).unwrap_or(false);
                        if is_managed {
                            results.skipped.push(action);
                        } else {
                            results.failed.push(FailedAction {
                                action,
                                reason: "destination exists and is not a managed symlink"
                                    .to_string(),
                            });
                        }
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
                    match create_symlink_dir(&skill.source_dir, &dest) {
                        Ok(()) => results.added.push(action),
                        Err(err) => results.failed.push(FailedAction {
                            action,
                            reason: err.to_string(),
                        }),
                    }
                }
                ActionMode::Unapply => {
                    if !dest.exists() {
                        results.skipped.push(action);
                        continue;
                    }
                    match remove_applied(&skill.source_dir, &dest) {
                        Ok(RemoveOutcome::Removed) => results.removed.push(action),
                        Ok(RemoveOutcome::Skipped(reason)) => {
                            results.failed.push(FailedAction { action, reason })
                        }
                        Err(err) => results.failed.push(FailedAction {
                            action,
                            reason: err.to_string(),
                        }),
                    }
                }
            }
        }
    }
    Ok(results)
}

fn create_symlink_dir(src: &std::path::Path, dest: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dest)?;
        Ok(())
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(src, dest)?;
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(anyhow::anyhow!(
            "symlinked apply is not supported on this platform"
        ))
    }
}

enum RemoveOutcome {
    Removed,
    Skipped(String),
}

fn remove_applied(source: &std::path::Path, dest: &std::path::Path) -> Result<RemoveOutcome> {
    let metadata = fs::symlink_metadata(dest)?;
    if !metadata.file_type().is_symlink() {
        return Ok(RemoveOutcome::Skipped("not a managed symlink".to_string()));
    }
    if !symlink_matches(source, dest)? {
        return Ok(RemoveOutcome::Skipped(
            "link target does not match installed skill".to_string(),
        ));
    }
    fs::remove_file(dest)?;
    Ok(RemoveOutcome::Removed)
}

fn symlink_matches(source: &std::path::Path, dest: &std::path::Path) -> Result<bool> {
    let metadata = fs::symlink_metadata(dest)?;
    if !metadata.file_type().is_symlink() {
        return Ok(false);
    }
    let link_target = fs::read_link(dest)?;
    let resolved_target = if link_target.is_absolute() {
        fs::canonicalize(&link_target)?
    } else {
        let parent = dest
            .parent()
            .ok_or_else(|| anyhow::anyhow!("missing symlink parent"))?;
        fs::canonicalize(parent.join(link_target))?
    };
    let resolved_source = fs::canonicalize(source)?;
    Ok(resolved_target == resolved_source)
}

#[cfg(test)]
mod tests {
    use super::agents::{AgentId, Scope};
    use super::*;
    use std::fs;

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

    #[test]
    fn when_desired_state_unselects_should_unapply() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let base_dir = temp.path().join("targets");
        let selected_src = temp.path().join("selected-src");
        let removed_src = temp.path().join("removed-src");
        fs::create_dir_all(&selected_src)?;
        fs::create_dir_all(&removed_src)?;
        fs::write(selected_src.join("SKILL.md"), "selected")?;
        fs::write(removed_src.join("SKILL.md"), "removed")?;

        let selected_key = SkillKey {
            namespace: "acme".to_string(),
            name: "selected".to_string(),
        };
        let removed_key = SkillKey {
            namespace: "acme".to_string(),
            name: "removed".to_string(),
        };
        let selected_skill = ApplySkill {
            key: selected_key.clone(),
            source_dir: selected_src,
            source_exists: true,
        };
        let removed_skill = ApplySkill {
            key: removed_key.clone(),
            source_dir: removed_src,
            source_exists: true,
        };

        let target = AgentTarget {
            key: TargetKey {
                agent: AgentId::Cursor,
                scope: Scope::Project,
            },
            label: "Cursor (project)".to_string(),
            short: "cu:p".to_string(),
            base_dir: base_dir.clone(),
            detected: true,
            enabled: true,
            default_selected: true,
        };

        let removed_dest = base_dir
            .join(&removed_key.namespace)
            .join(&removed_key.name);
        fs::create_dir_all(&removed_dest)?;
        fs::write(removed_dest.join("SKILL.md"), "old")?;

        let selection = ApplySelection {
            targets: vec![target.key.clone()],
            skills: vec![selected_key.clone()],
        };
        let results = apply_selection(
            ApplyIntent::DesiredState,
            &selection,
            &[target.clone()],
            &[selected_skill, removed_skill],
        )?;

        let selected_dest = base_dir
            .join(&selected_key.namespace)
            .join(&selected_key.name);
        assert!(selected_dest.exists());
        assert!(!removed_dest.exists());
        assert_eq!(results.added.len(), 1);
        assert_eq!(results.removed.len(), 1);

        Ok(())
    }
}
