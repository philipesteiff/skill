use anyhow::{Result, anyhow};
use clap::Args;
use std::collections::HashSet;

use crate::applied_index::AppliedIndex;
use skill_core::config::{self, SelectionConfig};
use skill_core::installer::{InstallTarget, Installer};
use skill_core::lockfile;
use skill_core::paths::Paths;
use skill_core::progress::Reporter;
use skill_core::source;
use skill_core::source_index::{self, IndexedSkill};
use skill_core::ui::browse::{BrowseItem, BrowseMode, BrowseSelection};
use skill_core::ui::log::LogUi;
use skill_core::util::remove_dir_if_exists;
use std::collections::HashMap;

#[derive(Args, Clone, Debug)]
pub struct BrowseArgs {
    pub source: Option<String>,
    #[arg(long)]
    pub search: Option<String>,
    #[arg(long, value_delimiter = ',')]
    pub tags: Vec<String>,
}

pub fn run(paths: &Paths, args: BrowseArgs) -> Result<()> {
    paths.ensure_base_dirs()?;
    if args.source.is_none() {
        return show_installed(paths, args.search.as_deref());
    }

    let mut config = config::load(paths)?;
    let input = args.source.unwrap_or_default();
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

    let installed_paths = installed_paths(paths, &source_cfg.id)?;
    let items = filtered
        .iter()
        .map(|skill| BrowseItem {
            name: skill.name.clone(),
            description: skill.description.clone(),
            updated_at: skill.updated_at.clone(),
            tags: skill.tags.clone(),
            path: skill.path.clone(),
            installed: installed_paths.contains(&skill.path),
        })
        .collect::<Vec<_>>();

    let available_paths = items
        .iter()
        .map(|item| item.path.clone())
        .collect::<HashSet<_>>();
    let initial_selection = selection_from_config(&source_cfg.selection, &available_paths);
    let selection = skill_core::ui::browse::run_browse_ui(
        format!("Browse {}", source_cfg.id),
        &items,
        &initial_selection,
        args.search.as_deref(),
        BrowseMode::Install,
    )?;

    let selection = match selection {
        BrowseSelection::Cancel => {
            return Ok(());
        }
        BrowseSelection::List(paths) => {
            let selected: HashSet<_> = paths.iter().cloned().collect();
            let all_paths: HashSet<_> = all_skills.iter().map(|skill| skill.path.clone()).collect();
            if selected == all_paths {
                source_cfg.selection = SelectionConfig::All;
            } else {
                source_cfg.selection = SelectionConfig::List { skills: paths };
            }
            source_cfg.selection.clone()
        }
        BrowseSelection::All => SelectionConfig::All,
        BrowseSelection::Delete(_) => return Ok(()),
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
            content_hash: Some(skill.content_hash.clone()),
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

fn show_installed(paths: &Paths, search: Option<&str>) -> Result<()> {
    let mut config = config::load(paths)?;

    loop {
        let lockfile = lockfile::load(paths)?;
        if lockfile.skills.is_empty() {
            return Err(anyhow!("no installed skills"));
        }

        let query = search.unwrap_or("").trim().to_lowercase();
        let mut items = lockfile
            .skills
            .iter()
            .filter(|entry| {
                if query.is_empty() {
                    return true;
                }
                let name = format!("{}/{}", entry.source_id, entry.name).to_lowercase();
                name.contains(&query)
            })
            .map(|entry| BrowseItem {
                name: format!("{}/{}", entry.source_id, entry.name),
                description: String::new(),
                updated_at: entry.updated_at.clone().unwrap_or_default(),
                tags: Vec::new(),
                path: format!("{}/{}", entry.source_id, entry.name),
                installed: true,
            })
            .collect::<Vec<_>>();

        items.sort_by(|a, b| a.name.cmp(&b.name));
        if items.is_empty() {
            return Err(anyhow!("no installed skills matched"));
        }

        let selection = skill_core::ui::browse::run_browse_ui(
            "Installed skills".to_string(),
            &items,
            &HashSet::new(),
            search,
            BrowseMode::Installed,
        )?;

        match selection {
            BrowseSelection::Cancel => return Ok(()),
            BrowseSelection::Delete(entries) => {
                delete_installed(paths, &mut config, entries)?;
            }
            _ => return Ok(()),
        }
    }
}

fn delete_installed(paths: &Paths, config: &mut config::Config, keys: Vec<String>) -> Result<()> {
    if keys.is_empty() {
        return Ok(());
    }

    let mut lockfile = lockfile::load(paths)?;
    let mut removed_paths: HashMap<String, Vec<String>> = HashMap::new();
    let mut removed_skills = Vec::new();

    for key in keys {
        let Some((source_id, name)) = parse_installed_key(&key) else {
            continue;
        };
        if let Some(entry) = lockfile::remove(&mut lockfile, &source_id, &name) {
            remove_dir_if_exists(std::path::Path::new(&entry.install_dir))?;
            removed_paths
                .entry(entry.source_id)
                .or_default()
                .push(entry.path);
            removed_skills.push((source_id, name));
        }
    }

    if removed_paths.is_empty() {
        return Ok(());
    }

    let remaining_paths =
        lockfile
            .skills
            .iter()
            .fold(HashMap::<String, Vec<String>>::new(), |mut acc, entry| {
                acc.entry(entry.source_id.clone())
                    .or_default()
                    .push(entry.path.clone());
                acc
            });

    for source in &mut config.sources {
        let Some(removed) = removed_paths.get(&source.id) else {
            continue;
        };
        let remaining = remaining_paths.get(&source.id).cloned().unwrap_or_default();
        update_selection_after_delete(&mut source.selection, removed, &remaining);
    }

    let mut applied_index = AppliedIndex::load(paths)?;
    for (source_id, name) in removed_skills {
        for entry in applied_index.entries_for_skill(&source_id, &name) {
            remove_dir_if_exists(&entry.target_dir)?;
            applied_index.remove_target(&entry.target_dir);
        }
    }

    lockfile::save(paths, &lockfile)?;
    applied_index.save(paths)?;
    config::save(paths, config)?;
    Ok(())
}

fn update_selection_after_delete(
    selection: &mut SelectionConfig,
    removed_paths: &[String],
    remaining_paths: &[String],
) {
    match selection {
        SelectionConfig::All => {
            *selection = SelectionConfig::List {
                skills: remaining_paths.to_vec(),
            };
        }
        SelectionConfig::List { skills } => {
            skills.retain(|path| !removed_paths.contains(path));
        }
    }
}

fn parse_installed_key(value: &str) -> Option<(String, String)> {
    let (source_id, name) = value.split_once('/')?;
    if source_id.is_empty() || name.is_empty() {
        return None;
    }
    Some((source_id.to_string(), name.to_string()))
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

fn selection_from_config(
    selection: &SelectionConfig,
    available_paths: &HashSet<String>,
) -> HashSet<String> {
    match selection {
        SelectionConfig::All => available_paths.clone(),
        SelectionConfig::List { skills } => skills
            .iter()
            .filter(|skill| available_paths.contains(*skill))
            .cloned()
            .collect(),
    }
}

fn installed_paths(paths: &Paths, source_id: &str) -> Result<HashSet<String>> {
    let lockfile = lockfile::load(paths)?;
    Ok(lockfile
        .skills
        .into_iter()
        .filter(|entry| entry.source_id == source_id)
        .map(|entry| entry.path)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::applied_index::{AppliedEntry, AppliedIndex};
    use skill_core::config::{Config, SourceConfig};
    use skill_core::lockfile::{LockedSkill, Lockfile};
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn build_paths(base: PathBuf) -> Paths {
        Paths::from_base(base)
    }

    #[test]
    fn when_uninstalling_should_remove_applied_targets_and_index_entries() -> Result<()> {
        let temp = tempdir()?;
        let paths = build_paths(temp.path().to_path_buf());
        paths.ensure_base_dirs()?;

        let install_dir = temp.path().join("installed/acme/echo/latest");
        std::fs::create_dir_all(&install_dir)?;
        std::fs::write(install_dir.join("SKILL.md"), "skill")?;

        let lock = Lockfile {
            skills: vec![LockedSkill {
                source_id: "acme".to_string(),
                name: "echo".to_string(),
                resolved_version: None,
                resolved_commit: "deadbeef".to_string(),
                content_hash: Some("hash".to_string()),
                path: "skills/echo".to_string(),
                install_dir: install_dir.to_string_lossy().to_string(),
                updated_at: Some("2026-01-01".to_string()),
            }],
        };
        lockfile::save(&paths, &lock)?;

        let target_dir = temp.path().join("project/.claude/skills/acme__echo");
        std::fs::create_dir_all(&target_dir)?;
        std::fs::write(target_dir.join("SKILL.md"), "applied")?;

        let mut applied_index = AppliedIndex::default();
        applied_index.upsert(AppliedEntry {
            source_id: "acme".to_string(),
            name: "echo".to_string(),
            target_dir: target_dir.clone(),
            install_dir: install_dir.clone(),
            resolved_commit: "deadbeef".to_string(),
            content_hash: Some("hash".to_string()),
            updated_at: Some("2026-01-01".to_string()),
        });
        applied_index.save(&paths)?;

        let mut config = Config {
            sources: vec![SourceConfig {
                id: "acme".to_string(),
                url: "https://github.com/acme/skills.git".to_string(),
                selection: SelectionConfig::All,
            }],
        };

        delete_installed(&paths, &mut config, vec!["acme/echo".to_string()])?;

        let updated_lock = lockfile::load(&paths)?;
        assert!(updated_lock.skills.is_empty());
        assert!(!install_dir.exists());
        assert!(!target_dir.exists());

        let updated_index = AppliedIndex::load(&paths)?;
        assert!(updated_index.entries_for_skill("acme", "echo").is_empty());

        Ok(())
    }
}
