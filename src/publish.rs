use anyhow::{Result, anyhow};
use chrono::Utc;
use reqwest::blocking::Client;
use serde_json::json;
use std::env;
use std::path::Path;
use walkdir::WalkDir;

use crate::config::RegistryConfig;
use crate::git;
use crate::output::Output;
use crate::paths::Paths;
use crate::registry::{self, RegistryLatest, RegistrySkillFile, RegistryVersion};
use crate::skills::SkillSpec;
use crate::util::{parse_github_slug, short_sha};

pub fn publish(
    paths: &Paths,
    registry: &RegistryConfig,
    dry_run: bool,
    output: &mut impl Output,
) -> Result<()> {
    registry::sync_registry(paths, registry, output)?;
    let registry_repo = paths.registry_repo_dir(&registry.id);

    let status = git::status_porcelain(&registry_repo)?;
    if !status.trim().is_empty() {
        return Err(anyhow!(
            "registry repo has uncommitted changes; clean it before publishing"
        ));
    }

    let cwd = env::current_dir()?;
    let repo_root = git::repo_root(&cwd)?;
    let head = git::repo_head(&repo_root)?;
    let origin_url = git::remote_url(&repo_root)?;
    let repo_url = crate::util::normalize_github_url(&origin_url).unwrap_or(origin_url);

    let default_namespace = parse_github_slug(&repo_url)
        .map(|(owner, _)| owner)
        .unwrap_or_else(|| "unknown".to_string());

    let skills = find_skills(&repo_root)?;
    if skills.is_empty() {
        return Err(anyhow!("no SKILL.md files found in repo"));
    }

    output.line(format!("Publishing {} skill(s)...", skills.len()))?;
    let published_at = Utc::now().to_rfc3339();

    for skill in skills {
        let namespace = skill
            .namespace
            .as_deref()
            .unwrap_or(&default_namespace)
            .to_string();
        if namespace == "unknown" {
            return Err(anyhow!(
                "unable to infer namespace; set metadata.namespace in SKILL.md"
            ));
        }
        let version = skill.version.clone().unwrap_or_else(|| short_sha(&head));
        let rel_path = skill_path_relative(&repo_root, &skill.path)?;

        let mut entry = registry::load_skill_file(paths, registry, &namespace, &skill.name)?
            .unwrap_or_else(|| RegistrySkillFile {
                namespace: namespace.clone(),
                name: skill.name.clone(),
                description: skill.description.clone(),
                repo_url: repo_url.clone(),
                path: rel_path.clone(),
                tags: skill.tags.clone(),
                latest: None,
                versions: Vec::new(),
            });

        entry.namespace = namespace.clone();
        entry.name = skill.name.clone();
        entry.description = skill.description.clone();
        entry.repo_url = repo_url.clone();
        entry.path = rel_path.clone();
        entry.tags = skill.tags.clone();
        entry.latest = Some(RegistryLatest {
            version: version.clone(),
            commit: head.clone(),
        });

        if let Some(existing) = entry
            .versions
            .iter_mut()
            .find(|item| item.version == version)
        {
            existing.commit = head.clone();
            existing.published_at = published_at.clone();
        } else {
            entry.versions.push(RegistryVersion {
                version: version.clone(),
                commit: head.clone(),
                published_at: published_at.clone(),
            });
        }

        registry::write_skill_file(paths, registry, &entry)?;
        output.line(format!("- {}/{} @{}", namespace, skill.name, version))?;
    }

    let diff = git::diff(&registry_repo)?;
    if diff.trim().is_empty() {
        output.line("No metadata changes to publish")?;
        return Ok(());
    }

    if dry_run {
        output.line("--- Registry diff (dry-run) ---")?;
        for line in diff.lines() {
            output.line(line)?;
        }
        return Ok(());
    }

    let branch = format!("skill/publish-{}", Utc::now().format("%Y%m%d%H%M%S"));
    git::checkout_new_branch(&registry_repo, &branch)?;
    git::add_all(&registry_repo)?;
    git::commit(&registry_repo, "Update skills metadata")?;
    git::push_branch(&registry_repo, &branch)?;

    let base = git::default_branch(&registry_repo)?;
    let pr_url = create_pull_request(&registry.url, &branch, &base)?;
    output.line(format!("Opened PR: {}", pr_url))?;

    Ok(())
}

fn find_skills(repo_root: &Path) -> Result<Vec<SkillSpec>> {
    let mut skills = Vec::new();
    let walker = WalkDir::new(repo_root).into_iter();

    for entry in walker.filter_entry(|entry| !is_ignored(entry.path())) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name() != "SKILL.md" {
            continue;
        }
        let dir = entry
            .path()
            .parent()
            .ok_or_else(|| anyhow!("invalid SKILL.md path"))?;
        let spec = crate::skills::read_skill_spec(dir)?;
        skills.push(spec);
    }

    Ok(skills)
}

fn is_ignored(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
        return name == ".git" || name == "target";
    }
    false
}

fn skill_path_relative(repo_root: &Path, skill_path: &Path) -> Result<String> {
    let rel = skill_path
        .strip_prefix(repo_root)
        .map_err(|_| anyhow!("skill path is outside repo"))?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

fn create_pull_request(repo_url: &str, branch: &str, base: &str) -> Result<String> {
    let token = env::var("GITHUB_TOKEN")
        .or_else(|_| env::var("GH_TOKEN"))
        .map_err(|_| anyhow!("GITHUB_TOKEN or GH_TOKEN is required to create a PR"))?;
    let (owner, repo) =
        parse_github_slug(repo_url).ok_or_else(|| anyhow!("unable to parse GitHub repo URL"))?;

    let client = Client::new();
    let api_url = format!("https://api.github.com/repos/{}/{}/pulls", owner, repo);
    let body = json!({
        "title": "Update skills metadata",
        "head": branch,
        "base": base,
        "body": "Automated skill metadata update."
    });

    let resp = client
        .post(api_url)
        .bearer_auth(token)
        .header("User-Agent", "skill-cli")
        .json(&body)
        .send()?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(anyhow!("GitHub API error: {} {}", status, text));
    }

    let value: serde_json::Value = resp.json()?;
    let url = value
        .get("html_url")
        .and_then(|val| val.as_str())
        .ok_or_else(|| anyhow!("missing PR URL in response"))?;
    Ok(url.to_string())
}
