use anyhow::{Result, anyhow};
use std::collections::HashSet;
use std::path::Path;
use tempfile::tempdir;

use crate::config::{Config, RegistryConfig};
use crate::git;
use crate::lockfile::{self, LockedSkill};
use crate::paths::Paths;
use crate::refs::{self, Selector};
use crate::registry;
use crate::skills;
use crate::tui;
use crate::util::{copy_dir_recursive, is_hexish, remove_dir_if_exists, short_sha};

#[derive(Debug, Clone)]
struct InstallTarget {
    namespace: String,
    expected_name: Option<String>,
    repo_url: String,
    path: String,
    commit: String,
    version: Option<String>,
    requested: String,
    registry_id: Option<String>,
}

#[derive(Debug, Clone)]
struct RepoSkill {
    name: String,
    description: String,
    path: String,
}

pub fn install_reference(
    paths: &Paths,
    config: &Config,
    reference: &str,
    pick: bool,
    registry: Option<&str>,
) -> Result<()> {
    let parsed = refs::parse_reference(reference);
    let requested = parsed.selector.requested_string();
    let base = parsed.base;

    if refs::is_git_url(&base) {
        let (url, path_opt) = refs::split_git_url(&base);
        let namespace = namespace_from_repo(&url)?;
        let targets = resolve_repo_targets(
            paths,
            &url,
            path_opt,
            &parsed.selector,
            &requested,
            pick,
            namespace,
        )?;
        for target in targets {
            install_target(paths, &target)?;
        }
        return Ok(());
    }

    let segments = refs::split_segments(&base);
    if segments.len() < 2 {
        return Err(anyhow!("invalid reference: {}", reference));
    }
    let namespace = segments.first().cloned().unwrap_or_default();
    let name = segments.last().cloned().unwrap_or_default();

    if let Some(target) = resolve_registry_target(
        paths,
        config,
        registry,
        &namespace,
        &name,
        &parsed.selector,
        &requested,
    )? {
        install_target(paths, &target)?;
        return Ok(());
    }

    let owner = segments[0].clone();
    let repo = segments[1].clone();
    let path_segments: Vec<String> = segments.into_iter().skip(2).collect();
    let path_opt = if path_segments.is_empty() {
        None
    } else {
        Some(path_segments.join("/"))
    };
    let repo_url = format!("https://github.com/{}/{}.git", owner, repo);
    let namespace = owner;
    let targets = resolve_repo_targets(
        paths,
        &repo_url,
        path_opt,
        &parsed.selector,
        &requested,
        pick,
        namespace,
    )?;
    for target in targets {
        install_target(paths, &target)?;
    }

    Ok(())
}

pub fn upgrade_latest(paths: &Paths, config: &Config) -> Result<()> {
    let lock = lockfile::load(paths)?;
    if lock.skills.is_empty() {
        println!("No installed skills");
        return Ok(());
    }

    for registry in &config.registries {
        registry::sync_registry(paths, registry)?;
    }

    let mut updated = 0;
    for entry in lock.skills {
        if entry.requested != "@latest" {
            continue;
        }
        if let Some(registry_id) = &entry.registry_id {
            let registry = config
                .registries
                .iter()
                .find(|reg| &reg.id == registry_id)
                .ok_or_else(|| anyhow!("missing registry: {}", registry_id))?;
            let target = resolve_latest_from_registry(
                paths,
                registry,
                &entry.namespace,
                &entry.name,
                &entry.requested,
            )?;
            if target.commit == entry.resolved_commit {
                continue;
            }
            install_target(paths, &target)?;
            updated += 1;
        } else {
            let commit = git::ls_remote_head(&entry.repo_url)?;
            if commit == entry.resolved_commit {
                continue;
            }
            let target = InstallTarget {
                namespace: entry.namespace.clone(),
                expected_name: Some(entry.name.clone()),
                repo_url: entry.repo_url.clone(),
                path: entry.path.clone(),
                commit,
                version: entry.resolved_version.clone(),
                requested: entry.requested.clone(),
                registry_id: None,
            };
            install_target(paths, &target)?;
            updated += 1;
        }
    }

    if updated == 0 {
        println!("All @latest skills are up to date");
    } else {
        println!("Updated {} skill(s)", updated);
    }

    Ok(())
}

pub fn remove_skill(paths: &Paths, reference: &str) -> Result<()> {
    let parsed = refs::parse_reference(reference);
    let segments = refs::split_segments(&parsed.base);
    if segments.len() < 2 {
        return Err(anyhow!("invalid reference: {}", reference));
    }
    let namespace = segments.first().cloned().unwrap_or_default();
    let name = segments.last().cloned().unwrap_or_default();

    let mut lock = lockfile::load(paths)?;
    let entry = lockfile::remove(&mut lock, &namespace, &name)
        .ok_or_else(|| anyhow!("skill not installed: {}/{}", namespace, name))?;

    remove_dir_if_exists(Path::new(&entry.install_dir))?;
    lockfile::save(paths, &lock)?;
    println!("Removed {}/{}", namespace, name);
    Ok(())
}

pub fn list_installed(paths: &Paths) -> Result<()> {
    let lock = lockfile::load(paths)?;
    if lock.skills.is_empty() {
        println!("No installed skills");
        return Ok(());
    }

    for entry in lock.skills {
        let version = entry.resolved_version.as_deref().unwrap_or("latest");
        let commit = short_sha(&entry.resolved_commit);
        println!(
            "{}/{} {} ({})",
            entry.namespace, entry.name, version, commit
        );
    }
    Ok(())
}

fn resolve_registry_target(
    paths: &Paths,
    config: &Config,
    registry_selector: Option<&str>,
    namespace: &str,
    name: &str,
    selector: &Selector,
    requested: &str,
) -> Result<Option<InstallTarget>> {
    let mut matches = Vec::new();
    let mut selector_matched = false;
    for registry in &config.registries {
        if let Some(selector) = registry_selector {
            if registry.id != selector && registry.url != selector {
                continue;
            }
            selector_matched = true;
        }
        if let Some(row) = registry::find_by_namespace_name(paths, registry, namespace, name)? {
            matches.push((registry.clone(), row));
        }
    }

    if let Some(selector) = registry_selector
        && !selector_matched
    {
        return Err(anyhow!("registry not found: {}", selector));
    }

    if matches.is_empty() {
        return Ok(None);
    }
    if matches.len() > 1 {
        return Err(anyhow!(
            "reference matches multiple registries; pass --registry"
        ));
    }

    let (registry, row) = matches.remove(0);
    let (commit, version) = resolve_commit_from_registry(paths, &registry, selector, &row)?;

    Ok(Some(InstallTarget {
        namespace: row.namespace,
        expected_name: Some(row.name),
        repo_url: row.repo_url,
        path: row.path,
        commit,
        version,
        requested: requested.to_string(),
        registry_id: Some(registry.id),
    }))
}

fn resolve_latest_from_registry(
    paths: &Paths,
    registry: &RegistryConfig,
    namespace: &str,
    name: &str,
    requested: &str,
) -> Result<InstallTarget> {
    let row = registry::find_by_namespace_name(paths, registry, namespace, name)?
        .ok_or_else(|| anyhow!("registry entry not found for {}/{}", namespace, name))?;
    let selector = Selector::Latest;
    let (commit, version) = resolve_commit_from_registry(paths, registry, &selector, &row)?;

    Ok(InstallTarget {
        namespace: row.namespace,
        expected_name: Some(row.name),
        repo_url: row.repo_url,
        path: row.path,
        commit,
        version,
        requested: requested.to_string(),
        registry_id: Some(registry.id.clone()),
    })
}

fn resolve_commit_from_registry(
    paths: &Paths,
    registry: &RegistryConfig,
    selector: &Selector,
    row: &registry::RegistryRow,
) -> Result<(String, Option<String>)> {
    match selector {
        Selector::Latest | Selector::None => {
            let commit = row
                .latest_commit
                .clone()
                .ok_or_else(|| anyhow!("registry entry has no latest commit"))?;
            Ok((commit, row.latest_version.clone()))
        }
        Selector::Version(version) => {
            let skill = registry::load_skill_file(paths, registry, &row.namespace, &row.name)?
                .ok_or_else(|| {
                    anyhow!("registry file missing for {}/{}", row.namespace, row.name)
                })?;
            let entry = skill
                .versions
                .iter()
                .find(|entry| entry.version == *version)
                .ok_or_else(|| anyhow!("version {} not found in registry", version))?;
            Ok((entry.commit.clone(), Some(entry.version.clone())))
        }
    }
}

fn resolve_repo_targets(
    paths: &Paths,
    repo_url: &str,
    path_opt: Option<String>,
    selector: &Selector,
    requested: &str,
    pick: bool,
    namespace: String,
) -> Result<Vec<InstallTarget>> {
    let commit = resolve_direct_commit(repo_url, selector)?;
    let mirror_path = paths.cache_repo_path(repo_url);
    git::ensure_mirror(repo_url, &mirror_path)?;
    git::fetch_commit(&mirror_path, &commit)?;

    if let Some(path) = path_opt {
        return Ok(vec![InstallTarget {
            namespace,
            expected_name: None,
            repo_url: repo_url.to_string(),
            path,
            commit,
            version: None,
            requested: requested.to_string(),
            registry_id: None,
        }]);
    }

    let skills = scan_repo_skills(&mirror_path, &commit)?;
    if skills.is_empty() {
        return Err(anyhow!("no SKILL.md found in repo"));
    }

    if skills.len() == 1 {
        let skill = &skills[0];
        return Ok(vec![InstallTarget {
            namespace,
            expected_name: Some(skill.name.clone()),
            repo_url: repo_url.to_string(),
            path: skill.path.clone(),
            commit,
            version: None,
            requested: requested.to_string(),
            registry_id: None,
        }]);
    }

    if pick {
        let items: Vec<String> = skills
            .iter()
            .map(|skill| format!("{} - {}", skill.name, skill.description))
            .collect();
        let choice = tui::pick_from_list("Select a skill", &items)?;
        let Some(idx) = choice else {
            println!("Install cancelled");
            return Ok(Vec::new());
        };
        let skill = &skills[idx];
        return Ok(vec![InstallTarget {
            namespace,
            expected_name: Some(skill.name.clone()),
            repo_url: repo_url.to_string(),
            path: skill.path.clone(),
            commit,
            version: None,
            requested: requested.to_string(),
            registry_id: None,
        }]);
    }

    let mut targets = Vec::new();
    for skill in skills {
        targets.push(InstallTarget {
            namespace: namespace.clone(),
            expected_name: Some(skill.name),
            repo_url: repo_url.to_string(),
            path: skill.path,
            commit: commit.clone(),
            version: None,
            requested: requested.to_string(),
            registry_id: None,
        });
    }
    Ok(targets)
}

fn resolve_direct_commit(repo_url: &str, selector: &Selector) -> Result<String> {
    match selector {
        Selector::Latest | Selector::None => git::ls_remote_head(repo_url),
        Selector::Version(value) => {
            if is_hexish(value) {
                Ok(value.to_string())
            } else {
                Err(anyhow!(
                    "version selectors require a registry or a commit SHA"
                ))
            }
        }
    }
}

fn namespace_from_repo(repo_url: &str) -> Result<String> {
    if let Some((owner, _repo)) = crate::util::parse_github_slug(repo_url) {
        return Ok(owner);
    }
    Err(anyhow!("unsupported repo URL: {}", repo_url))
}

fn scan_repo_skills(mirror_path: &Path, commit: &str) -> Result<Vec<RepoSkill>> {
    let files = git::list_files(mirror_path, commit)?;
    let mut seen = HashSet::new();
    let mut skills = Vec::new();

    for file in files {
        if !file.ends_with("SKILL.md") {
            continue;
        }
        let path = Path::new(&file);
        let dir = match path.parent() {
            Some(dir) => dir,
            None => continue,
        };
        let dir_str = dir.to_string_lossy().to_string();
        if dir_str.is_empty() || !seen.insert(dir_str.clone()) {
            continue;
        }
        let contents = git::show_file(mirror_path, commit, &file)?;
        let dir_name = dir.file_name().and_then(|name| name.to_str()).unwrap_or("");
        let summary = skills::parse_skill_summary(&contents, dir_name)?;
        skills.push(RepoSkill {
            name: summary.name,
            description: summary.description,
            path: dir_str,
        });
    }

    Ok(skills)
}

fn install_target(paths: &Paths, target: &InstallTarget) -> Result<()> {
    if target.path.trim().is_empty() {
        return Err(anyhow!("missing skill path"));
    }
    let mirror_path = paths.cache_repo_path(&target.repo_url);
    git::ensure_mirror(&target.repo_url, &mirror_path)?;
    git::fetch_commit(&mirror_path, &target.commit)?;

    let temp = tempdir()?;
    git::archive_path(&mirror_path, &target.commit, &target.path, temp.path())?;

    let skill_dir = temp.path().join(&target.path);
    let spec = skills::read_skill_spec(&skill_dir)?;

    if let Some(expected) = &target.expected_name
        && &spec.name != expected
    {
        return Err(anyhow!(
            "skill name mismatch: expected {}, found {}",
            expected,
            spec.name
        ));
    }

    let resolved_version = target.version.clone().or(spec.version.clone());
    let version_label = resolved_version.clone().unwrap_or_else(|| {
        if target.requested == "@latest" {
            "latest".to_string()
        } else {
            target.requested.trim_start_matches('@').to_string()
        }
    });

    let skill_root = paths
        .installed_dir()
        .join(&target.namespace)
        .join(&spec.name);
    remove_dir_if_exists(&skill_root)?;
    let install_dir = skill_root.join(&version_label);
    copy_dir_recursive(&skill_dir, &install_dir)?;

    let mut lock = lockfile::load(paths)?;
    let entry = LockedSkill {
        namespace: target.namespace.clone(),
        name: spec.name,
        requested: target.requested.clone(),
        resolved_version,
        resolved_commit: target.commit.clone(),
        repo_url: target.repo_url.clone(),
        path: target.path.clone(),
        install_dir: install_dir.to_string_lossy().to_string(),
        registry_id: target.registry_id.clone(),
    };
    lockfile::upsert(&mut lock, entry);
    lockfile::save(paths, &lock)?;

    println!(
        "Installed {} (commit {})",
        install_dir.display(),
        short_sha(&target.commit)
    );
    Ok(())
}
