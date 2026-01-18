use anyhow::{Result, anyhow};
use std::collections::HashSet;
use std::path::Path;
use tempfile::tempdir;

use crate::config::{Config, RegistryConfig};
use crate::git;
use crate::lockfile::{self, LockedSkill};
use crate::manifest;
use crate::output::Output;
use crate::paths::Paths;
use crate::progress::{QueuedSkill, Reporter, SkillUpdate};
use crate::refs::{self, Selector};
use crate::registry;
use crate::skills;
use crate::util::{copy_dir_recursive, ensure_dir, is_hexish, remove_dir_if_exists, short_sha};

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

#[derive(Debug, Clone)]
struct InvalidSkill {
    path: String,
    error: String,
}

struct RepoScan {
    skills: Vec<RepoSkill>,
    invalid_skills: Vec<InvalidSkill>,
}

fn install_targets(
    paths: &Paths,
    reporter: &mut Reporter,
    targets: Vec<InstallTarget>,
) -> Result<()> {
    if targets.is_empty() {
        return Ok(());
    }
    let queue = targets
        .iter()
        .map(|target| QueuedSkill {
            name: target
                .expected_name
                .clone()
                .unwrap_or_else(|| fallback_skill_name(&target.path)),
            version: target.version.clone(),
            requested: target.requested.clone(),
            source: target.repo_url.clone(),
            commit: target.commit.clone(),
        })
        .collect();
    reporter.queue_skills(queue)?;

    for (index, target) in targets.iter().enumerate() {
        reporter.begin_skill(index)?;
        if let Err(err) = install_target(paths, reporter, target) {
            let _ = reporter.fail_active_skill(err.to_string());
            return Err(err);
        }
        reporter.finish_skill()?;
    }

    Ok(())
}

fn fallback_skill_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("skill")
        .to_string()
}

pub fn install_reference(
    paths: &Paths,
    config: &Config,
    reference: &str,
    pick: bool,
    registry: Option<&str>,
) -> Result<()> {
    let mut reporter = Reporter::new()?;
    reporter.set_context(format!("skill install {reference}"))?;
    install_reference_with_reporter(paths, config, reference, pick, registry, &mut reporter)?;
    reporter.finish("Done")?;
    Ok(())
}

pub fn install_manifest(
    paths: &Paths,
    config: &Config,
    manifest_path: &Path,
    pick: bool,
    registry: Option<&str>,
) -> Result<()> {
    let mut reporter = Reporter::new()?;
    reporter.set_context("skill install".to_string())?;
    reporter.step(format!("Loading {}", manifest_path.display()))?;
    let dependencies = manifest::load_dependencies(manifest_path)?;

    for dependency in dependencies {
        reporter.step(format!(
            "Installing dependency {} ({})",
            dependency.name, dependency.reference
        ))?;
        let dependency_registry = dependency.registry.as_deref().or(registry);
        if let Err(err) = install_reference_with_reporter(
            paths,
            config,
            &dependency.reference,
            pick,
            dependency_registry,
            &mut reporter,
        ) {
            return Err(anyhow!(
                "failed to install dependency {}: {}",
                dependency.name,
                err
            ));
        }
    }

    reporter.finish("Installed skills from skills.toml")?;
    Ok(())
}

fn install_reference_with_reporter(
    paths: &Paths,
    config: &Config,
    reference: &str,
    pick: bool,
    registry: Option<&str>,
    reporter: &mut Reporter,
) -> Result<()> {
    reporter.step(format!("Parsing reference: {reference}"))?;

    let parsed = refs::parse_reference(reference);
    let requested = parsed.selector.requested_string();
    let base = parsed.base;

    if refs::is_git_url(&base) {
        reporter.step("Resolving git URL")?;
        let (url, path_opt) = refs::split_git_url(&base);
        let namespace = namespace_from_repo(&url)?;
        let targets = resolve_repo_targets(
            paths,
            reporter,
            &url,
            path_opt,
            &parsed.selector,
            &requested,
            pick,
            namespace,
        )?;
        install_targets(paths, reporter, targets)?;
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
        reporter,
        registry,
        &namespace,
        &name,
        &parsed.selector,
        &requested,
    )? {
        install_targets(paths, reporter, vec![target])?;
        return Ok(());
    }

    reporter.step("Falling back to GitHub shorthand")?;
    let owner = segments[0].clone();
    let repo = segments[1].clone();
    let path_segments: Vec<String> = segments.into_iter().skip(2).collect();
    let path_opt = if path_segments.is_empty() {
        None
    } else {
        Some(path_segments.join("/"))
    };
    let repo_url = format!("https://github.com/{owner}/{repo}.git");
    let namespace = owner;
    let targets = resolve_repo_targets(
        paths,
        reporter,
        &repo_url,
        path_opt,
        &parsed.selector,
        &requested,
        pick,
        namespace,
    )?;
    install_targets(paths, reporter, targets)?;

    Ok(())
}

pub fn upgrade_latest(paths: &Paths, config: &Config) -> Result<()> {
    let lock = lockfile::load(paths)?;
    let mut reporter = Reporter::new()?;
    reporter.set_context("skill upgrade")?;

    if lock.skills.is_empty() {
        reporter.finish("No installed skills")?;
        return Ok(());
    }

    for registry in &config.registries {
        registry::sync_registry(paths, registry, &mut reporter)?;
    }

    let mut targets = Vec::new();
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
            targets.push(target);
        } else {
            let commit = git::ls_remote_head(&entry.repo_url)?;
            if commit == entry.resolved_commit {
                continue;
            }
            targets.push(InstallTarget {
                namespace: entry.namespace.clone(),
                expected_name: Some(entry.name.clone()),
                repo_url: entry.repo_url.clone(),
                path: entry.path.clone(),
                commit,
                version: entry.resolved_version.clone(),
                requested: entry.requested.clone(),
                registry_id: None,
            });
        }
    }

    if targets.is_empty() {
        reporter.finish("All @latest skills are up to date")?;
        return Ok(());
    }

    let updated = targets.len();
    install_targets(paths, &mut reporter, targets)?;
    reporter.finish(format!("Updated {updated} skill(s)"))?;
    Ok(())
}

pub fn remove_skill(paths: &Paths, reference: &str, output: &mut impl Output) -> Result<()> {
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
    output.line(format!("Removed {namespace}/{name}"))?;
    Ok(())
}

pub fn remove_all_skills(paths: &Paths, output: &mut impl Output) -> Result<()> {
    let mut lock = lockfile::load(paths)?;
    if lock.skills.is_empty() {
        remove_dir_if_exists(&paths.installed_dir())?;
        ensure_dir(&paths.installed_dir())?;
        output.line("No installed skills")?;
        return Ok(());
    }

    let count = lock.skills.len();
    for entry in &lock.skills {
        remove_dir_if_exists(Path::new(&entry.install_dir))?;
    }
    lock.skills.clear();
    lockfile::save(paths, &lock)?;
    remove_dir_if_exists(&paths.installed_dir())?;
    ensure_dir(&paths.installed_dir())?;
    output.line(format!("Removed {count} skill(s)"))?;
    Ok(())
}

pub fn list_installed(paths: &Paths, output: &mut impl Output) -> Result<()> {
    let lock = lockfile::load(paths)?;
    if lock.skills.is_empty() {
        output.line("No installed skills")?;
        return Ok(());
    }

    for entry in lock.skills {
        let version = entry.resolved_version.as_deref().unwrap_or("latest");
        let commit = short_sha(&entry.resolved_commit);
        output.line(format!(
            "{}/{} {} ({})",
            entry.namespace, entry.name, version, commit
        ))?;
    }
    Ok(())
}

fn resolve_registry_target(
    paths: &Paths,
    config: &Config,
    reporter: &mut Reporter,
    registry_selector: Option<&str>,
    namespace: &str,
    name: &str,
    selector: &Selector,
    requested: &str,
) -> Result<Option<InstallTarget>> {
    reporter.step("Checking registries for a match")?;
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

    reporter.step("Resolving version from registry")?;
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
    reporter: &mut Reporter,
    repo_url: &str,
    path_opt: Option<String>,
    selector: &Selector,
    requested: &str,
    pick: bool,
    namespace: String,
) -> Result<Vec<InstallTarget>> {
    reporter.step("Resolving commit from git")?;
    let commit = resolve_direct_commit(repo_url, selector)?;

    reporter.step("Preparing local mirror cache")?;
    let mirror_path = paths.cache_repo_path(repo_url);
    let repo_url_owned = repo_url.to_string();
    let mirror_path_owned = mirror_path.clone();
    run_with_feedback(reporter, move || {
        git::ensure_mirror(&repo_url_owned, &mirror_path_owned)
    })?;

    reporter.step("Fetching commit into cache (may take a while)")?;
    let mirror_path_owned = mirror_path.clone();
    let commit_owned = commit.clone();
    run_with_feedback(reporter, move || {
        git::fetch_commit(&mirror_path_owned, &commit_owned)
    })?;

    reporter.step("Scanning repo for skills")?;
    let scan = scan_repo_skills(&mirror_path, &commit, path_opt.as_deref())?;
    for invalid in scan.invalid_skills {
        reporter.step(format!(
            "Invalid skill at {} ({})",
            invalid.path, invalid.error
        ))?;
    }
    if scan.skills.is_empty() {
        let scope = path_opt
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .map(|path| format!(" at {}", path))
            .unwrap_or_default();
        return Err(anyhow!("no valid SKILL.md found in repo{}", scope));
    }

    if scan.skills.len() == 1 {
        let skill = &scan.skills[0];
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
        let items: Vec<String> = scan
            .skills
            .iter()
            .map(|skill| format!("{} - {}", skill.name, skill.description))
            .collect();
        let choice = reporter.pick_from_list("Select a skill", &items)?;
        let Some(idx) = choice else {
            reporter.step("Install cancelled")?;
            return Ok(Vec::new());
        };
        let skill = &scan.skills[idx];
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
    for skill in scan.skills {
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

fn scan_repo_skills(mirror_path: &Path, commit: &str, base_path: Option<&str>) -> Result<RepoScan> {
    let files = git::list_files(mirror_path, commit)?;
    let mut seen = HashSet::new();
    let mut skills = Vec::new();
    let mut invalid_skills = Vec::new();
    let base_prefix = base_path
        .map(|path| path.trim_matches('/'))
        .map(|path| path.trim_start_matches("./"))
        .filter(|path| !path.is_empty())
        .map(|path| format!("{path}/"));

    for file in files {
        if !file.ends_with("SKILL.md") {
            continue;
        }
        if let Some(prefix) = &base_prefix
            && !file.starts_with(prefix)
        {
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
        match skills::parse_skill_summary(&contents, dir_name) {
            Ok(summary) => {
                skills.push(RepoSkill {
                    name: summary.name,
                    description: summary.description,
                    path: dir_str,
                });
            }
            Err(err) => {
                invalid_skills.push(InvalidSkill {
                    path: dir_str,
                    error: err.to_string(),
                });
                continue;
            }
        }
    }

    Ok(RepoScan {
        skills,
        invalid_skills,
    })
}

fn install_target(paths: &Paths, reporter: &mut Reporter, target: &InstallTarget) -> Result<()> {
    if target.path.trim().is_empty() {
        return Err(anyhow!("missing skill path"));
    }

    reporter.step("Ensuring mirror cache")?;
    let mirror_path = paths.cache_repo_path(&target.repo_url);
    let repo_url_owned = target.repo_url.clone();
    let mirror_path_owned = mirror_path.clone();
    run_with_feedback(reporter, move || {
        git::ensure_mirror(&repo_url_owned, &mirror_path_owned)
    })?;

    reporter.step("Fetching commit (may take a while)")?;
    let mirror_path_owned = mirror_path.clone();
    let commit_owned = target.commit.clone();
    run_with_feedback(reporter, move || {
        git::fetch_commit(&mirror_path_owned, &commit_owned)
    })?;

    reporter.step("Extracting skill directory")?;
    let temp = tempdir()?;
    git::archive_path(&mirror_path, &target.commit, &target.path, temp.path())?;

    reporter.step("Validating SKILL.md")?;
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
    reporter.update_active_skill(SkillUpdate {
        name: Some(spec.name.clone()),
        version: resolved_version.clone(),
    })?;
    let version_label = resolved_version.clone().unwrap_or_else(|| {
        if target.requested == "@latest" {
            "latest".to_string()
        } else {
            target.requested.trim_start_matches('@').to_string()
        }
    });

    reporter.step("Copying files to skills home")?;
    let skill_root = paths
        .installed_dir()
        .join(&target.namespace)
        .join(&spec.name);
    remove_dir_if_exists(&skill_root)?;
    let install_dir = skill_root.join(&version_label);
    copy_dir_recursive(&skill_dir, &install_dir)?;

    reporter.step("Updating lockfile")?;
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

    reporter.step(format!(
        "Installed {} (commit {})",
        install_dir.display(),
        short_sha(&target.commit)
    ))?;
    Ok(())
}

fn run_with_feedback<F, T>(reporter: &mut Reporter, task: F) -> Result<T>
where
    F: Send + 'static + FnOnce() -> Result<T>,
    T: Send + 'static,
{
    use std::sync::mpsc;
    use std::time::Duration;

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = task();
        let _ = tx.send(result);
    });

    loop {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => reporter.tick()?,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(anyhow!("operation canceled"));
            }
        }
    }
}
