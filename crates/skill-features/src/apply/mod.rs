use anyhow::{Result, anyhow};
use clap::Args;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::str::FromStr;

mod agents;
mod git_tracking;
mod ui;

use self::agents::{AgentTarget, Scope, TargetKey, detect_agents, targets_for_agents};
use self::git_tracking::{GitTrackingManager, TrackingPreference};
use skill_core::git;
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
    pub source_id: String,
    pub name: String,
}

impl SkillKey {
    pub fn label(&self) -> String {
        let source_id = &self.source_id;
        let name = &self.name;
        format!("{source_id}/{name}")
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
    pub tracking: HashMap<SkillKey, TrackingPreference>,
}

#[derive(Clone, Debug, Default)]
struct TrackingContext {
    by_target: HashMap<TargetKey, TargetTracking>,
}

#[derive(Clone, Debug)]
struct TargetTracking {
    repo_root: Option<PathBuf>,
    initial: HashMap<SkillKey, TrackingPreference>,
}

impl TrackingContext {
    fn for_target(&self, target: &TargetKey) -> Option<&TargetTracking> {
        self.by_target.get(target)
    }
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
                source_id: entry.source_id,
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
    let tracking = build_tracking_context(&repo_root, &targets, &skills);
    let selection = select_apply(output, &targets, &skills, &applied, &tracking, args)?;
    let Some(selection) = selection else {
        output.line("Canceled apply")?;
        return Ok(());
    };
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
    if selection.targets.is_empty()
        || (selection.skills.is_empty()
            && matches!(intent, ApplyIntent::ApplyOnly | ApplyIntent::UnapplyOnly))
    {
        output.line("No targets or skills selected")?;
        return Ok(());
    }

    let results = apply_selection(intent, &selection, &targets, &skills, &tracking)?;

    output.line("")?;
    if args.unapply {
        output.line(format!("{} Unapply Results", "=".repeat(5)))?;
    } else {
        output.line(format!("{} Apply Results", "=".repeat(5)))?;
    }
    output.line("")?;

    if results.added.is_empty()
        && results.removed.is_empty()
        && results.skipped.is_empty()
        && results.failed.is_empty()
        && results.tracking_changes.is_empty()
        && results.tracking_failed.is_empty()
    {
        output.line("No actions taken")?;
        return Ok(());
    }

    if !results.added.is_empty() {
        output.line("Added:")?;
        for entry in results.added {
            output.line(format!(
                "  [+] {} -> {}",
                entry.skill.label(),
                entry.target.label()
            ))?;
        }
        output.line("")?;
    }

    if !results.removed.is_empty() {
        output.line("Removed:")?;
        for entry in results.removed {
            output.line(format!(
                "  [-] {} from {}",
                entry.skill.label(),
                entry.target.label()
            ))?;
        }
        output.line("")?;
    }

    if !results.failed.is_empty() {
        output.line("Failed:")?;
        for entry in results.failed {
            output.line(format!(
                "  [!] {} on {}: {}",
                entry.action.skill.label(),
                entry.action.target.label(),
                entry.reason
            ))?;
        }
        output.line("")?;
    }

    if !results.tracking_changes.is_empty() {
        output.line("Git Tracking:")?;
        for entry in results.tracking_changes {
            output.line(format!(
                "  [{}] {} -> {} ({})",
                entry.kind.symbol(),
                entry.action.skill.label(),
                entry.repo_relative_path,
                entry.kind.label()
            ))?;
        }
        output.line("")?;
    }

    if !results.tracking_failed.is_empty() {
        output.line("Git Tracking Failed:")?;
        for entry in results.tracking_failed {
            output.line(format!(
                "  [!] {} on {}: {}",
                entry.action.skill.label(),
                entry.action.target.label(),
                entry.reason
            ))?;
        }
        output.line("")?;
    }

    Ok(())
}

fn select_apply(
    output: &mut impl Output,
    targets: &[AgentTarget],
    skills: &[ApplySkill],
    applied: &HashMap<(SkillKey, TargetKey), bool>,
    tracking: &TrackingContext,
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
    ui::run_apply_ui(targets, skills, applied, tracking, args.unapply)
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
        return Err(anyhow!(
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
        return Err(anyhow!(
            "no skills provided; pass --skills, --all-skills, or run in TUI"
        ));
    } else {
        args.skills
            .iter()
            .map(|value| parse_skill(value, skills))
            .collect::<Result<Vec<_>>>()?
    };

    Ok(ApplySelection {
        targets,
        skills,
        tracking: HashMap::new(),
    })
}

fn parse_target(value: &str, targets: &[AgentTarget]) -> Result<TargetKey> {
    let key = TargetKey::from_str(value)?;
    if !targets.iter().any(|target| target.key == key) {
        return Err(anyhow!("unknown target: {value}"));
    }
    Ok(key)
}

fn parse_skill(value: &str, skills: &[ApplySkill]) -> Result<SkillKey> {
    let parts: Vec<&str> = value.split('/').collect();
    if parts.len() != 2 {
        return Err(anyhow!("invalid skill reference: {}", value));
    }
    let key = SkillKey {
        source_id: parts[0].to_string(),
        name: parts[1].to_string(),
    };
    if !skills.iter().any(|skill| skill.key == key) {
        return Err(anyhow!("unknown skill: {value}"));
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
            let is_applied = is_symlink(&dest).unwrap_or(false);
            applied.insert((skill.key.clone(), target.key.clone()), is_applied);
        }
    }
    applied
}

fn build_tracking_context(
    cwd: &std::path::Path,
    targets: &[AgentTarget],
    skills: &[ApplySkill],
) -> TrackingContext {
    let repo_root = git::repo_root(cwd).ok();
    let manager = repo_root
        .as_ref()
        .and_then(|root| GitTrackingManager::load(root).ok());
    let mut by_target = HashMap::new();

    for target in targets {
        let Some(repo_root) = manager.as_ref().map(GitTrackingManager::repo_root) else {
            by_target.insert(
                target.key.clone(),
                TargetTracking {
                    repo_root: None,
                    initial: HashMap::new(),
                },
            );
            continue;
        };

        if target.key.scope != Scope::Project || !target.base_dir.starts_with(repo_root) {
            by_target.insert(
                target.key.clone(),
                TargetTracking {
                    repo_root: None,
                    initial: HashMap::new(),
                },
            );
            continue;
        }

        let mut initial = HashMap::new();
        if let Some(manager) = manager.as_ref() {
            for skill in skills {
                let dest = dest_dir(target, &skill.key);
                let preference = manager
                    .repo_relative_path(&dest)
                    .ok()
                    .and_then(|path| manager.preference_for_path(&path).ok())
                    .unwrap_or(TrackingPreference::Tracked);
                initial.insert(skill.key.clone(), preference);
            }
        }

        by_target.insert(
            target.key.clone(),
            TargetTracking {
                repo_root: Some(repo_root.to_path_buf()),
                initial,
            },
        );
    }

    TrackingContext { by_target }
}

fn dest_dir(target: &AgentTarget, skill: &SkillKey) -> PathBuf {
    target
        .base_dir
        .join(format!("{}__{}", skill.source_id, skill.name))
}

#[derive(Clone)]
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
    tracking_changes: Vec<TrackingChange>,
    tracking_failed: Vec<TrackingFailedAction>,
}

struct TrackingChange {
    action: ApplyAction,
    repo_relative_path: String,
    kind: TrackingChangeKind,
}

struct TrackingFailedAction {
    action: ApplyAction,
    reason: String,
}

#[derive(Clone, Copy)]
enum TrackingChangeKind {
    Tracked,
    NotTracked,
    Cleared,
}

impl TrackingChangeKind {
    fn symbol(self) -> &'static str {
        match self {
            Self::Tracked => "g+",
            Self::NotTracked => "g-",
            Self::Cleared => "gc",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Tracked => "tracked",
            Self::NotTracked => "not tracked",
            Self::Cleared => "cleared",
        }
    }
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
    tracking: &TrackingContext,
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
        tracking_changes: Vec::new(),
        tracking_failed: Vec::new(),
    };
    let selected_skills = selection.skills.iter().cloned().collect::<HashSet<_>>();
    let skills_iter: Vec<_> = match intent {
        ApplyIntent::ApplyOnly | ApplyIntent::UnapplyOnly => selection.skills.clone(),
        ApplyIntent::DesiredState => skills.iter().map(|skill| skill.key.clone()).collect(),
    };
    let mut tracking_managers = HashMap::<PathBuf, GitTrackingManager>::new();

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
            let tracking_pref = selection.tracking.get(&skill.key).copied();

            match mode {
                ActionMode::Apply => {
                    let mut apply_completed = false;
                    if dest.exists() {
                        if is_symlink(&dest).unwrap_or(false) {
                            if symlink_matches(&skill.source_dir, &dest).unwrap_or(false) {
                                results.skipped.push(action.clone());
                                apply_completed = true;
                            } else {
                                fs::remove_file(&dest)?;
                            }
                        } else {
                            results.failed.push(FailedAction {
                                action,
                                reason: "destination exists and is not a managed symlink"
                                    .to_string(),
                            });
                            continue;
                        }
                    }
                    if !skill.source_exists && !apply_completed {
                        results.failed.push(FailedAction {
                            action,
                            reason: "missing source directory".to_string(),
                        });
                        continue;
                    }
                    if !apply_completed {
                        ensure_dir(&target.base_dir)?;
                        if let Some(parent) = dest.parent() {
                            ensure_dir(parent)?;
                        }
                        match create_symlink_dir(&skill.source_dir, &dest) {
                            Ok(()) => {
                                results.added.push(action.clone());
                                apply_completed = true;
                            }
                            Err(err) => {
                                results.failed.push(FailedAction {
                                    action,
                                    reason: err.to_string(),
                                });
                                continue;
                            }
                        }
                    }
                    if apply_completed {
                        apply_tracking_preference(
                            &mut results,
                            &action,
                            &dest,
                            target_key,
                            tracking_pref,
                            tracking,
                            &mut tracking_managers,
                        );
                    }
                }
                ActionMode::Unapply => {
                    let unapply_completed = if !dest.exists() {
                        results.skipped.push(action.clone());
                        true
                    } else {
                        match remove_applied(&skill.source_dir, &dest) {
                            Ok(RemoveOutcome::Removed) => {
                                results.removed.push(action.clone());
                                true
                            }
                            Ok(RemoveOutcome::Skipped(reason)) => {
                                results.failed.push(FailedAction { action, reason });
                                continue;
                            }
                            Err(err) => {
                                results.failed.push(FailedAction {
                                    action,
                                    reason: err.to_string(),
                                });
                                continue;
                            }
                        }
                    };
                    if unapply_completed {
                        clear_tracking_preference(
                            &mut results,
                            &action,
                            &dest,
                            target_key,
                            tracking,
                            &mut tracking_managers,
                        );
                    }
                }
            }
        }
    }
    Ok(results)
}

fn apply_tracking_preference(
    results: &mut ApplyResults,
    action: &ApplyAction,
    dest: &std::path::Path,
    target_key: &TargetKey,
    preference: Option<TrackingPreference>,
    tracking: &TrackingContext,
    managers: &mut HashMap<PathBuf, GitTrackingManager>,
) {
    let Some(preference) = preference else {
        return;
    };
    let Some(target_tracking) = tracking.for_target(target_key) else {
        return;
    };
    let Some(repo_root) = target_tracking.repo_root.as_ref() else {
        return;
    };

    let manager = manager_for_repo(repo_root, managers, results, action);
    let Some(manager) = manager else {
        return;
    };

    let repo_relative_path = match manager.repo_relative_path(dest) {
        Ok(path) => path,
        Err(err) => {
            results.tracking_failed.push(TrackingFailedAction {
                action: action.clone(),
                reason: err.to_string(),
            });
            return;
        }
    };

    if matches!(preference, TrackingPreference::NotTracked) {
        match manager.is_path_tracked(&repo_relative_path) {
            Ok(true) => {
                results.tracking_failed.push(TrackingFailedAction {
                    action: action.clone(),
                    reason: format!(
                        "{repo_relative_path} is already tracked by Git; run `git rm --cached -- {repo_relative_path}` first"
                    ),
                });
                return;
            }
            Ok(false) => {}
            Err(err) => {
                results.tracking_failed.push(TrackingFailedAction {
                    action: action.clone(),
                    reason: err.to_string(),
                });
                return;
            }
        }
    }

    match manager.set_preference(&repo_relative_path, preference) {
        Ok(true) => {
            let kind = match preference {
                TrackingPreference::Tracked => TrackingChangeKind::Tracked,
                TrackingPreference::NotTracked => TrackingChangeKind::NotTracked,
            };
            results.tracking_changes.push(TrackingChange {
                action: action.clone(),
                repo_relative_path,
                kind,
            });
        }
        Ok(false) => {}
        Err(err) => {
            results.tracking_failed.push(TrackingFailedAction {
                action: action.clone(),
                reason: err.to_string(),
            });
        }
    }
}

fn clear_tracking_preference(
    results: &mut ApplyResults,
    action: &ApplyAction,
    dest: &std::path::Path,
    target_key: &TargetKey,
    tracking: &TrackingContext,
    managers: &mut HashMap<PathBuf, GitTrackingManager>,
) {
    let Some(target_tracking) = tracking.for_target(target_key) else {
        return;
    };
    let Some(repo_root) = target_tracking.repo_root.as_ref() else {
        return;
    };
    let manager = manager_for_repo(repo_root, managers, results, action);
    let Some(manager) = manager else {
        return;
    };

    let repo_relative_path = match manager.repo_relative_path(dest) {
        Ok(path) => path,
        Err(err) => {
            results.tracking_failed.push(TrackingFailedAction {
                action: action.clone(),
                reason: err.to_string(),
            });
            return;
        }
    };

    match manager.remove_managed_entry(&repo_relative_path) {
        Ok(true) => {
            results.tracking_changes.push(TrackingChange {
                action: action.clone(),
                repo_relative_path,
                kind: TrackingChangeKind::Cleared,
            });
        }
        Ok(false) => {}
        Err(err) => {
            results.tracking_failed.push(TrackingFailedAction {
                action: action.clone(),
                reason: err.to_string(),
            });
        }
    }
}

fn manager_for_repo<'a>(
    repo_root: &PathBuf,
    managers: &'a mut HashMap<PathBuf, GitTrackingManager>,
    results: &mut ApplyResults,
    action: &ApplyAction,
) -> Option<&'a mut GitTrackingManager> {
    if !managers.contains_key(repo_root) {
        match GitTrackingManager::load(repo_root) {
            Ok(manager) => {
                managers.insert(repo_root.clone(), manager);
            }
            Err(err) => {
                results.tracking_failed.push(TrackingFailedAction {
                    action: action.clone(),
                    reason: err.to_string(),
                });
                return None;
            }
        }
    }
    managers.get_mut(repo_root)
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
        Err(anyhow!("symlinked apply is not supported on this platform"))
    }
}

enum RemoveOutcome {
    Removed,
    Skipped(String),
}

fn remove_applied(_source: &std::path::Path, dest: &std::path::Path) -> Result<RemoveOutcome> {
    let metadata = fs::symlink_metadata(dest)?;
    if !metadata.file_type().is_symlink() {
        return Ok(RemoveOutcome::Skipped("not a managed symlink".to_string()));
    }
    fs::remove_file(dest)?;
    Ok(RemoveOutcome::Removed)
}

fn is_symlink(dest: &std::path::Path) -> Result<bool> {
    if !dest.exists() {
        return Ok(false);
    }
    let metadata = fs::symlink_metadata(dest)?;
    Ok(metadata.file_type().is_symlink())
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
            .ok_or_else(|| anyhow!("missing symlink parent"))?;
        fs::canonicalize(parent.join(link_target))?
    };
    let resolved_source = fs::canonicalize(source)?;
    Ok(resolved_target == resolved_source)
}

#[cfg(test)]
mod tests {
    use super::agents::{AgentId, Scope};
    use super::git_tracking::MANAGED_START;
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    fn run_git<I, S>(args: I, cwd: &Path) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let output = Command::new("git").args(args).current_dir(cwd).output()?;
        if output.status.success() {
            return Ok(());
        }
        Err(anyhow!(
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }

    fn init_repo(repo_dir: &Path) -> Result<()> {
        run_git(["init", "-q"], repo_dir)?;
        run_git(["config", "user.email", "test@example.com"], repo_dir)?;
        run_git(["config", "user.name", "Test User"], repo_dir)?;
        Ok(())
    }

    fn tracking_context_for_target(
        target: &AgentTarget,
        repo_root: Option<PathBuf>,
    ) -> TrackingContext {
        let mut by_target = HashMap::new();
        by_target.insert(
            target.key.clone(),
            TargetTracking {
                repo_root,
                initial: HashMap::new(),
            },
        );
        TrackingContext { by_target }
    }

    #[test]
    fn computes_applied_status() {
        let temp = tempfile::tempdir().expect("temp dir");
        let skill = ApplySkill {
            key: SkillKey {
                source_id: "acme".to_string(),
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
        let applied = compute_applied(std::slice::from_ref(&skill), std::slice::from_ref(&target));
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
        fs::create_dir_all(&base_dir)?;
        let selected_src = temp.path().join("selected-src");
        let removed_src = temp.path().join("removed-src");
        fs::create_dir_all(&selected_src)?;
        fs::create_dir_all(&removed_src)?;
        fs::write(selected_src.join("SKILL.md"), "selected")?;
        fs::write(removed_src.join("SKILL.md"), "removed")?;

        let selected_key = SkillKey {
            source_id: "acme".to_string(),
            name: "selected".to_string(),
        };
        let removed_key = SkillKey {
            source_id: "acme".to_string(),
            name: "removed".to_string(),
        };
        let selected_skill = ApplySkill {
            key: selected_key.clone(),
            source_dir: selected_src,
            source_exists: true,
        };
        let removed_skill = ApplySkill {
            key: removed_key.clone(),
            source_dir: removed_src.clone(),
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

        let removed_dest = base_dir.join("acme__removed");
        create_symlink_dir(&removed_src, &removed_dest)?;

        let selection = ApplySelection {
            targets: vec![target.key.clone()],
            skills: vec![selected_key.clone()],
            tracking: HashMap::new(),
        };
        let results = apply_selection(
            ApplyIntent::DesiredState,
            &selection,
            std::slice::from_ref(&target),
            &[selected_skill, removed_skill],
            &TrackingContext::default(),
        )?;

        let selected_dest = base_dir.join("acme__selected");
        assert!(selected_dest.exists());
        assert!(
            fs::symlink_metadata(&selected_dest)?
                .file_type()
                .is_symlink()
        );
        assert!(!removed_dest.exists());
        assert_eq!(results.added.len(), 1);
        assert_eq!(results.removed.len(), 1);

        Ok(())
    }

    #[test]
    fn when_unapplying_should_remove_mismatched_symlink() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let base_dir = temp.path().join("targets");
        fs::create_dir_all(&base_dir)?;
        let source_dir = temp.path().join("src");
        let other_dir = temp.path().join("other");
        fs::create_dir_all(&source_dir)?;
        fs::create_dir_all(&other_dir)?;
        fs::write(source_dir.join("SKILL.md"), "source")?;
        fs::write(other_dir.join("SKILL.md"), "other")?;

        let skill_key = SkillKey {
            source_id: "acme".to_string(),
            name: "echo".to_string(),
        };
        let skill = ApplySkill {
            key: skill_key.clone(),
            source_dir: source_dir.clone(),
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

        let dest = dest_dir(&target, &skill_key);
        create_symlink_dir(&other_dir, &dest)?;

        let selection = ApplySelection {
            targets: vec![target.key.clone()],
            skills: vec![],
            tracking: HashMap::new(),
        };
        let results = apply_selection(
            ApplyIntent::DesiredState,
            &selection,
            &[target],
            &[skill],
            &TrackingContext::default(),
        )?;

        assert!(!dest.exists());
        assert_eq!(results.removed.len(), 1);

        Ok(())
    }

    #[test]
    fn when_applying_with_not_tracked_should_add_local_exclude_entry() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root)?;
        init_repo(&repo_root)?;

        let source_dir = temp.path().join("source");
        fs::create_dir_all(&source_dir)?;
        fs::write(source_dir.join("SKILL.md"), "source")?;

        let skill_key = SkillKey {
            source_id: "acme".to_string(),
            name: "echo".to_string(),
        };
        let skill = ApplySkill {
            key: skill_key.clone(),
            source_dir: source_dir.clone(),
            source_exists: true,
        };
        let target = AgentTarget {
            key: TargetKey {
                agent: AgentId::Codex,
                scope: Scope::Project,
            },
            label: "Codex (project)".to_string(),
            short: "cdx:p".to_string(),
            base_dir: repo_root.join(".codex/skills"),
            detected: true,
            enabled: true,
            default_selected: true,
        };
        let tracking = tracking_context_for_target(&target, Some(repo_root.clone()));
        let selection = ApplySelection {
            targets: vec![target.key.clone()],
            skills: vec![skill_key.clone()],
            tracking: HashMap::from([(skill_key.clone(), TrackingPreference::NotTracked)]),
        };

        let results = apply_selection(
            ApplyIntent::ApplyOnly,
            &selection,
            std::slice::from_ref(&target),
            std::slice::from_ref(&skill),
            &tracking,
        )?;
        assert!(results.tracking_failed.is_empty());
        assert_eq!(results.tracking_changes.len(), 1);

        let exclude = fs::read_to_string(repo_root.join(".git/info/exclude"))?;
        assert!(exclude.contains(MANAGED_START));
        assert!(exclude.contains(".codex/skills/acme__echo"));

        Ok(())
    }

    #[test]
    fn when_reenabling_tracking_should_remove_local_exclude_entry() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root)?;
        init_repo(&repo_root)?;

        let source_dir = temp.path().join("source");
        fs::create_dir_all(&source_dir)?;
        fs::write(source_dir.join("SKILL.md"), "source")?;

        let skill_key = SkillKey {
            source_id: "acme".to_string(),
            name: "echo".to_string(),
        };
        let skill = ApplySkill {
            key: skill_key.clone(),
            source_dir: source_dir.clone(),
            source_exists: true,
        };
        let target = AgentTarget {
            key: TargetKey {
                agent: AgentId::Codex,
                scope: Scope::Project,
            },
            label: "Codex (project)".to_string(),
            short: "cdx:p".to_string(),
            base_dir: repo_root.join(".codex/skills"),
            detected: true,
            enabled: true,
            default_selected: true,
        };
        let tracking = tracking_context_for_target(&target, Some(repo_root.clone()));

        let not_tracked_selection = ApplySelection {
            targets: vec![target.key.clone()],
            skills: vec![skill_key.clone()],
            tracking: HashMap::from([(skill_key.clone(), TrackingPreference::NotTracked)]),
        };
        apply_selection(
            ApplyIntent::ApplyOnly,
            &not_tracked_selection,
            std::slice::from_ref(&target),
            std::slice::from_ref(&skill),
            &tracking,
        )?;

        let tracked_selection = ApplySelection {
            targets: vec![target.key.clone()],
            skills: vec![skill_key.clone()],
            tracking: HashMap::from([(skill_key.clone(), TrackingPreference::Tracked)]),
        };
        let results = apply_selection(
            ApplyIntent::ApplyOnly,
            &tracked_selection,
            std::slice::from_ref(&target),
            std::slice::from_ref(&skill),
            &tracking,
        )?;
        assert!(results.tracking_failed.is_empty());

        let exclude = fs::read_to_string(repo_root.join(".git/info/exclude"))?;
        assert!(!exclude.contains(".codex/skills/acme__echo"));

        Ok(())
    }

    #[test]
    fn when_unapplying_should_cleanup_managed_exclude_entry() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root)?;
        init_repo(&repo_root)?;

        let source_dir = temp.path().join("source");
        fs::create_dir_all(&source_dir)?;
        fs::write(source_dir.join("SKILL.md"), "source")?;

        let skill_key = SkillKey {
            source_id: "acme".to_string(),
            name: "echo".to_string(),
        };
        let skill = ApplySkill {
            key: skill_key.clone(),
            source_dir: source_dir.clone(),
            source_exists: true,
        };
        let target = AgentTarget {
            key: TargetKey {
                agent: AgentId::Codex,
                scope: Scope::Project,
            },
            label: "Codex (project)".to_string(),
            short: "cdx:p".to_string(),
            base_dir: repo_root.join(".codex/skills"),
            detected: true,
            enabled: true,
            default_selected: true,
        };
        let tracking = tracking_context_for_target(&target, Some(repo_root.clone()));

        let apply_selection_args = ApplySelection {
            targets: vec![target.key.clone()],
            skills: vec![skill_key.clone()],
            tracking: HashMap::from([(skill_key.clone(), TrackingPreference::NotTracked)]),
        };
        apply_selection(
            ApplyIntent::ApplyOnly,
            &apply_selection_args,
            std::slice::from_ref(&target),
            std::slice::from_ref(&skill),
            &tracking,
        )?;

        let unapply_selection_args = ApplySelection {
            targets: vec![target.key.clone()],
            skills: vec![skill_key.clone()],
            tracking: HashMap::new(),
        };
        let results = apply_selection(
            ApplyIntent::UnapplyOnly,
            &unapply_selection_args,
            std::slice::from_ref(&target),
            std::slice::from_ref(&skill),
            &tracking,
        )?;
        assert!(results.tracking_failed.is_empty());

        let exclude = fs::read_to_string(repo_root.join(".git/info/exclude"))?;
        assert!(!exclude.contains(".codex/skills/acme__echo"));

        Ok(())
    }

    #[test]
    fn when_not_in_git_repo_should_skip_tracking_changes() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let source_dir = temp.path().join("source");
        fs::create_dir_all(&source_dir)?;
        fs::write(source_dir.join("SKILL.md"), "source")?;

        let skill_key = SkillKey {
            source_id: "acme".to_string(),
            name: "echo".to_string(),
        };
        let skill = ApplySkill {
            key: skill_key.clone(),
            source_dir: source_dir.clone(),
            source_exists: true,
        };
        let target = AgentTarget {
            key: TargetKey {
                agent: AgentId::Codex,
                scope: Scope::Project,
            },
            label: "Codex (project)".to_string(),
            short: "cdx:p".to_string(),
            base_dir: temp.path().join(".codex/skills"),
            detected: true,
            enabled: true,
            default_selected: true,
        };
        let tracking = tracking_context_for_target(&target, None);
        let selection = ApplySelection {
            targets: vec![target.key.clone()],
            skills: vec![skill_key.clone()],
            tracking: HashMap::from([(skill_key.clone(), TrackingPreference::NotTracked)]),
        };

        let results = apply_selection(
            ApplyIntent::ApplyOnly,
            &selection,
            std::slice::from_ref(&target),
            std::slice::from_ref(&skill),
            &tracking,
        )?;
        assert!(results.tracking_changes.is_empty());
        assert!(results.tracking_failed.is_empty());

        Ok(())
    }
}
