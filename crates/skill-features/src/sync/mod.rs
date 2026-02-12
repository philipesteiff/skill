use anyhow::{Result, anyhow};
use clap::Args;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::applied_index::{AppliedEntry, AppliedIndex};
use skill_core::config::{self, SelectionConfig};
use skill_core::installer::{InstallTarget, Installer};
use skill_core::lockfile;
use skill_core::output::Output;
use skill_core::paths::Paths;
use skill_core::progress::Reporter;
use skill_core::source;
use skill_core::source_index::{self, IndexedSkill};
use skill_core::ui::log::LogUi;
use skill_core::util::{copy_dir_recursive, remove_dir_if_exists};

#[derive(Args, Clone, Debug)]
pub struct SyncArgs {
    pub source: Option<String>,
}

pub fn run(paths: &Paths, args: SyncArgs) -> Result<()> {
    paths.ensure_base_dirs()?;
    let mut config = config::load(paths)?;
    match args.source {
        Some(input) => {
            let (source_cfg, created) = config::resolve_source(&mut config, &input)?;
            if created {
                config::save(paths, &config)?;
            }
            sync_one_source(paths, &source_cfg, &format!("skill sync {input}"), false)
        }
        None => sync_all_sources(paths, &config.sources),
    }
}

fn sync_all_sources(paths: &Paths, sources: &[config::SourceConfig]) -> Result<()> {
    if sources.is_empty() {
        return Err(anyhow!(
            "no configured sources; add one with `skill browse <repo>`"
        ));
    }
    let total = sources.len();
    let mut succeeded = 0usize;
    let mut failed = Vec::new();
    for source_cfg in sources {
        let context = format!("skill sync @{}", source_cfg.id);
        match sync_one_source(paths, source_cfg, &context, true) {
            Ok(()) => {
                succeeded += 1;
            }
            Err(err) => {
                eprintln!("source @{} failed: {err}", source_cfg.id);
                failed.push(source_cfg.id.clone());
            }
        }
    }

    if failed.is_empty() {
        let mut reporter = Reporter::new()?;
        reporter.set_context("skill sync")?;
        reporter.finish(format!(
            "Sync all complete (sources total {total}, succeeded {succeeded}, failed 0)"
        ))?;
        return Ok(());
    }

    let failed_count = failed.len();
    let failed_sources = failed
        .iter()
        .map(|id| format!("@{id}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut reporter = Reporter::new()?;
    reporter.set_context("skill sync")?;
    reporter.line(format!("Failed sources: {failed_sources}"))?;
    reporter.finish(format!(
        "Sync all completed with failures (sources total {total}, succeeded {succeeded}, failed {failed_count})"
    ))?;
    Err(anyhow!(
        "sync failed for {failed_count} source(s): {failed_sources}"
    ))
}

fn sync_one_source(
    paths: &Paths,
    source_cfg: &config::SourceConfig,
    sync_context: &str,
    allow_empty_selection: bool,
) -> Result<()> {
    let mut log_ui = LogUi::new(sync_context.to_string())?;
    let result = sync_one_source_inner(
        paths,
        source_cfg,
        sync_context,
        allow_empty_selection,
        &mut log_ui,
    );
    let finish = log_ui.finish();
    finish?;
    result
}

fn sync_one_source_inner(
    paths: &Paths,
    source_cfg: &config::SourceConfig,
    sync_context: &str,
    allow_empty_selection: bool,
    output: &mut impl Output,
) -> Result<()> {
    source::ensure_index(paths, source_cfg, output)?;

    let skills = source_index::list_all(paths, &source_cfg.id)?;
    let (desired, missing) = select_skills(&skills, &source_cfg.selection);
    if desired.is_empty() {
        if allow_empty_selection {
            output.line("Skipped source: no selected skills")?;
            return Ok(());
        }
        return Err(anyhow!(
            "no skills selected; run skill browse to choose skills"
        ));
    }
    if !missing.is_empty() {
        output.line(format!(
            "Warning: {} selected skill(s) not found in source",
            missing.len()
        ))?;
        for path in missing {
            output.line(format!("Missing: {path}"))?;
        }
    }

    let lock = lockfile::load(paths)?;
    let installed = lock
        .skills
        .into_iter()
        .filter(|entry| entry.source_id == source_cfg.id)
        .map(|entry| (entry.name.clone(), entry))
        .collect::<HashMap<_, _>>();

    let mut targets = Vec::new();
    let mut installed_count = 0;
    let mut updated_count = 0;
    let mut skipped_count = 0;

    for skill in desired {
        match installed.get(&skill.name) {
            Some(existing)
                if existing
                    .content_hash
                    .as_deref()
                    .is_some_and(|hash| hash == skill.content_hash) =>
            {
                skipped_count += 1;
            }
            Some(_) => {
                updated_count += 1;
                targets.push(to_target(source_cfg, &skill));
            }
            None => {
                installed_count += 1;
                targets.push(to_target(source_cfg, &skill));
            }
        }
    }

    if targets.is_empty() {
        let mut reporter = Reporter::new()?;
        reporter.set_context(sync_context)?;
        reporter.finish(format!("Up to date (skipped {skipped_count})"))?;
        return Ok(());
    }

    let refresh_targets = targets.clone();
    let installer = Installer::new(paths);
    let mut reporter = Reporter::new()?;
    reporter.set_context(sync_context)?;
    installer.install_targets(targets, &mut reporter)?;
    let refresh_summary = match refresh_applied(paths, &mut reporter, &refresh_targets) {
        Ok(summary) => summary,
        Err(err) => {
            reporter.line(format!("Warning: failed to refresh applied copies: {err}"))?;
            RefreshSummary::default()
        }
    };
    let refreshed_note = if refresh_summary.refreshed > 0 {
        format!(", refreshed {}", refresh_summary.refreshed)
    } else {
        String::new()
    };
    reporter.finish(format!(
        "Sync complete (installed {installed_count}, updated {updated_count}, skipped {skipped_count}{refreshed_note})"
    ))?;

    Ok(())
}

#[derive(Default)]
struct RefreshSummary {
    refreshed: usize,
    missing: usize,
}

fn refresh_applied(
    paths: &Paths,
    reporter: &mut Reporter,
    targets: &[InstallTarget],
) -> Result<RefreshSummary> {
    if targets.is_empty() {
        return Ok(RefreshSummary::default());
    }
    let mut applied_index = match AppliedIndex::load(paths) {
        Ok(index) => index,
        Err(err) => {
            reporter.line(format!(
                "Warning: failed to read applied index; skipping refresh: {err}"
            ))?;
            return Ok(RefreshSummary::default());
        }
    };

    let lock = lockfile::load(paths)?;
    let lock_map = lock
        .skills
        .into_iter()
        .map(|entry| ((entry.source_id.clone(), entry.name.clone()), entry))
        .collect::<HashMap<_, _>>();

    let mut summary = RefreshSummary::default();

    for target in targets {
        let Some(name) = target.expected_name.as_deref() else {
            continue;
        };
        let key = (target.source_id.clone(), name.to_string());
        let Some(locked) = lock_map.get(&key) else {
            reporter.line(format!(
                "Warning: missing lockfile entry for {}/{}",
                target.source_id, name
            ))?;
            continue;
        };
        let entries = applied_index.entries_for_skill(&target.source_id, name);
        if entries.is_empty() {
            continue;
        }
        for entry in entries {
            if !entry.target_dir.exists() {
                summary.missing += 1;
                reporter.line(format!(
                    "Skipped missing applied target: {}",
                    entry.target_dir.display()
                ))?;
                applied_index.remove_target(&entry.target_dir);
                continue;
            }
            if is_symlink(&entry.target_dir)? {
                reporter.line(format!(
                    "Skipped symlinked applied target: {}",
                    entry.target_dir.display()
                ))?;
                continue;
            }
            if !entry.target_dir.is_dir() {
                reporter.line(format!(
                    "Skipped non-directory applied target: {}",
                    entry.target_dir.display()
                ))?;
                continue;
            }

            remove_dir_if_exists(&entry.target_dir)?;
            copy_dir_recursive(Path::new(&locked.install_dir), &entry.target_dir)?;
            applied_index.upsert(build_refresh_entry(&entry, locked));
            summary.refreshed += 1;
        }
    }

    applied_index.save(paths)?;
    if summary.refreshed > 0 {
        reporter.line(format!("Refreshed applied copies: {}", summary.refreshed))?;
    }
    Ok(summary)
}

fn build_refresh_entry(entry: &AppliedEntry, locked: &lockfile::LockedSkill) -> AppliedEntry {
    AppliedEntry {
        source_id: entry.source_id.clone(),
        name: entry.name.clone(),
        target_dir: entry.target_dir.clone(),
        install_dir: Path::new(&locked.install_dir).to_path_buf(),
        resolved_commit: locked.resolved_commit.clone(),
        content_hash: locked.content_hash.clone(),
        updated_at: locked.updated_at.clone(),
    }
}

fn is_symlink(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let metadata = std::fs::symlink_metadata(path)?;
    Ok(metadata.file_type().is_symlink())
}

fn select_skills(
    skills: &[IndexedSkill],
    selection: &SelectionConfig,
) -> (Vec<IndexedSkill>, Vec<String>) {
    match selection {
        SelectionConfig::All => (skills.to_vec(), Vec::new()),
        SelectionConfig::List { skills: selected } => {
            if selected.is_empty() {
                return (Vec::new(), Vec::new());
            }
            let selected_set: HashSet<&String> = selected.iter().collect();
            let desired = skills
                .iter()
                .filter(|skill| selected_set.contains(&skill.path))
                .cloned()
                .collect::<Vec<_>>();
            let available: HashSet<&String> = skills.iter().map(|skill| &skill.path).collect();
            let missing = selected
                .iter()
                .filter(|path| !available.contains(path))
                .cloned()
                .collect();
            (desired, missing)
        }
    }
}

fn to_target(source_cfg: &config::SourceConfig, skill: &IndexedSkill) -> InstallTarget {
    InstallTarget {
        source_id: source_cfg.id.clone(),
        repo_url: source_cfg.url.clone(),
        path: skill.path.clone(),
        commit: skill.commit.clone(),
        content_hash: Some(skill.content_hash.clone()),
        expected_name: Some(skill.name.clone()),
        version: skill.version.clone(),
        updated_at: Some(skill.updated_at.clone()),
    }
}
