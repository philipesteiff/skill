use anyhow::{Result, anyhow};
use clap::Args;
use std::collections::HashSet;

use skill_core::config::{self, SelectionConfig};
use skill_core::installer::{InstallTarget, Installer};
use skill_core::paths::Paths;
use skill_core::progress::Reporter;
use skill_core::source;
use skill_core::source_index::{self, IndexedSkill};
use skill_core::ui::browse::{BrowseItem, BrowseSelection};
use skill_core::ui::log::LogUi;

#[derive(Args, Clone, Debug)]
pub struct BrowseArgs {
    pub source: String,
    #[arg(long)]
    pub search: Option<String>,
    #[arg(long, value_delimiter = ',')]
    pub tags: Vec<String>,
}

pub fn run(paths: &Paths, args: BrowseArgs) -> Result<()> {
    paths.ensure_base_dirs()?;
    let mut config = config::load(paths)?;
    let input = args.source;
    let (mut source_cfg, created) = config::resolve_source(&mut config, &input)?;
    if created {
        config::save(paths, &config)?;
    }

    let mut log_ui = LogUi::new(format!("skill browse {input}"))?;
    let result = source::ensure_index(paths, &source_cfg, &mut log_ui);
    let finish = log_ui.finish();
    let _head = result?;
    finish?;

    let all_skills = load_skills(paths, &source_cfg.id, None)?;
    let mut filtered = load_skills(paths, &source_cfg.id, args.search.as_deref())?;
    if !args.tags.is_empty() {
        filtered = filter_by_tags(filtered, &args.tags);
    }
    if filtered.is_empty() {
        return Err(anyhow!("no skills found for selection"));
    }

    let items = filtered
        .iter()
        .map(|skill| BrowseItem {
            name: skill.name.clone(),
            description: skill.description.clone(),
            updated_at: skill.updated_at.clone(),
            tags: skill.tags.clone(),
            path: skill.path.clone(),
        })
        .collect::<Vec<_>>();

    let (initial_selection, initial_all) = selection_from_config(&source_cfg.selection);
    let selection = skill_core::ui::browse::run_browse_ui(
        format!("Browse {}", source_cfg.id),
        &items,
        &initial_selection,
        initial_all,
        args.search.as_deref(),
    )?;

    let selection = match selection {
        BrowseSelection::Cancel => {
            return Ok(());
        }
        BrowseSelection::All => {
            source_cfg.selection = SelectionConfig::All;
            SelectionConfig::All
        }
        BrowseSelection::List(paths) => {
            source_cfg.selection = SelectionConfig::List { skills: paths };
            source_cfg.selection.clone()
        }
    };

    config::update_source(&mut config, &source_cfg)?;
    config::save(paths, &config)?;

    let selected = match selection {
        SelectionConfig::All => all_skills,
        SelectionConfig::List { skills: selected } => {
            let selected: HashSet<String> = selected.into_iter().collect();
            all_skills
                .into_iter()
                .filter(|skill| selected.contains(&skill.path))
                .collect()
        }
    };

    if selected.is_empty() {
        return Err(anyhow!("no skills selected"));
    }

    let targets = selected
        .iter()
        .map(|skill| InstallTarget {
            source_id: source_cfg.id.clone(),
            repo_url: source_cfg.url.clone(),
            path: skill.path.clone(),
            commit: skill.commit.clone(),
            expected_name: Some(skill.name.clone()),
            version: skill.version.clone(),
            updated_at: Some(skill.updated_at.clone()),
        })
        .collect::<Vec<_>>();

    let installer = Installer::new(paths);
    let mut reporter = Reporter::new()?;
    reporter.set_context("skill browse")?;
    installer.install_targets(targets, &mut reporter)?;
    reporter.finish("Done")?;

    Ok(())
}

fn load_skills(paths: &Paths, source_id: &str, search: Option<&str>) -> Result<Vec<IndexedSkill>> {
    match search {
        Some(query) if !query.trim().is_empty() => source_index::search(paths, source_id, query),
        _ => source_index::list_all(paths, source_id),
    }
}

fn filter_by_tags(skills: Vec<IndexedSkill>, tags: &[String]) -> Vec<IndexedSkill> {
    let tags = tags
        .iter()
        .map(|tag| tag.to_lowercase())
        .collect::<Vec<_>>();
    skills
        .into_iter()
        .filter(|skill| {
            tags.iter().all(|tag| {
                skill
                    .tags
                    .iter()
                    .any(|skill_tag| skill_tag.to_lowercase() == *tag)
            })
        })
        .collect()
}

fn selection_from_config(selection: &SelectionConfig) -> (HashSet<String>, bool) {
    match selection {
        SelectionConfig::All => (HashSet::new(), true),
        SelectionConfig::List { skills } => (skills.iter().cloned().collect(), false),
    }
}
