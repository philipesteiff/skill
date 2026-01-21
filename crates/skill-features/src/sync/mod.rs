use anyhow::{Result, anyhow};
use clap::Args;
use std::collections::{HashMap, HashSet};

use skill_core::config::{self, SelectionConfig};
use skill_core::installer::{InstallTarget, Installer};
use skill_core::lockfile;
use skill_core::paths::Paths;
use skill_core::progress::Reporter;
use skill_core::source;
use skill_core::source_index::{self, IndexedSkill};
use skill_core::ui::log::LogUi;

#[derive(Args, Clone, Debug)]
pub struct SyncArgs {
    pub source: String,
}

pub fn run(paths: &Paths, args: SyncArgs) -> Result<()> {
    paths.ensure_base_dirs()?;
    let mut config = config::load(paths)?;
    let input = args.source;
    let (source_cfg, created) = config::resolve_source(&mut config, &input)?;
    if created {
        config::save(paths, &config)?;
    }

    let mut log_ui = LogUi::new(format!("skill sync {input}"))?;
    let result = source::ensure_index(paths, &source_cfg, &mut log_ui);
    let finish = log_ui.finish();
    let _head = result?;
    finish?;

    let skills = source_index::list_all(paths, &source_cfg.id)?;
    let desired = filter_by_selection(&skills, &source_cfg.selection)?;
    if desired.is_empty() {
        return Err(anyhow!(
            "no skills selected; run skill browse to choose skills"
        ));
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
            Some(existing) if existing.resolved_commit == skill.commit => {
                skipped_count += 1;
            }
            Some(_) => {
                updated_count += 1;
                targets.push(to_target(&source_cfg, &skill));
            }
            None => {
                installed_count += 1;
                targets.push(to_target(&source_cfg, &skill));
            }
        }
    }

    if targets.is_empty() {
        let mut reporter = Reporter::new()?;
        reporter.set_context("skill sync")?;
        reporter.finish(format!("Up to date (skipped {skipped_count})"))?;
        return Ok(());
    }

    let installer = Installer::new(paths);
    let mut reporter = Reporter::new()?;
    reporter.set_context("skill sync")?;
    installer.install_targets(targets, &mut reporter)?;
    reporter.finish(format!(
        "Sync complete (installed {installed_count}, updated {updated_count}, skipped {skipped_count})"
    ))?;

    Ok(())
}

fn filter_by_selection(
    skills: &[IndexedSkill],
    selection: &SelectionConfig,
) -> Result<Vec<IndexedSkill>> {
    match selection {
        SelectionConfig::All => Ok(skills.to_vec()),
        SelectionConfig::List { skills: selected } => {
            if selected.is_empty() {
                return Ok(Vec::new());
            }
            let selected: HashSet<&String> = selected.iter().collect();
            Ok(skills
                .iter()
                .filter(|skill| selected.contains(&skill.path))
                .cloned()
                .collect())
        }
    }
}

fn to_target(source_cfg: &config::SourceConfig, skill: &IndexedSkill) -> InstallTarget {
    InstallTarget {
        source_id: source_cfg.id.clone(),
        repo_url: source_cfg.url.clone(),
        path: skill.path.clone(),
        commit: skill.commit.clone(),
        expected_name: Some(skill.name.clone()),
        version: skill.version.clone(),
        updated_at: Some(skill.updated_at.clone()),
    }
}
