use anyhow::{Result, anyhow};
use std::path::Path;

use crate::config::SourceConfig;
use crate::git;
use crate::output::Output;
use crate::paths::Paths;
use crate::skills;
use crate::source_index::{self, IndexedSkill};
use crate::util::read_to_string;

#[derive(Debug, Clone)]
pub struct InvalidSkill {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone)]
pub struct SourceScan {
    pub skills: Vec<IndexedSkill>,
    pub invalid_skills: Vec<InvalidSkill>,
}

pub fn ensure_index(
    paths: &Paths,
    source: &SourceConfig,
    output: &mut impl Output,
) -> Result<String> {
    let source_dir = paths.source_dir(&source.id);
    crate::util::ensure_dir(&source_dir)?;

    output.line(format!("Resolving source {}", source.url))?;
    let head = git::ls_remote_head(&source.url)?;

    let mirror_path = paths.cache_repo_path(&source.url);
    output.line("Preparing local mirror cache")?;
    git::ensure_mirror(&source.url, &mirror_path)?;
    output.line("Fetching latest commit")?;
    git::fetch_commit(&mirror_path, &head)?;

    let head_path = paths.source_head_path(&source.id);
    let previous = read_to_string(&head_path).unwrap_or_default();
    let index_path = paths.source_index_path(&source.id);
    if previous.trim() == head && index_path.exists() {
        return Ok(head);
    }

    let scan = scan_repo_skills(&mirror_path, &head)?;
    for invalid in &scan.invalid_skills {
        output.line(format!(
            "Invalid skill at {} ({})",
            invalid.path, invalid.error
        ))?;
    }
    if scan.skills.is_empty() {
        return Err(anyhow!("no valid SKILL.md found in source"));
    }

    source_index::rebuild_index(paths, &source.id, &scan.skills, output)?;
    std::fs::write(head_path, &head)?;
    Ok(head)
}

fn scan_repo_skills(mirror_path: &Path, head_commit: &str) -> Result<SourceScan> {
    let files = git::list_files(mirror_path, head_commit)?;
    let mut skills = Vec::new();
    let mut invalid_skills = Vec::new();
    let head_date =
        git::commit_date_short(mirror_path, head_commit).unwrap_or_else(|_| "unknown".to_string());

    for file in files {
        if !file.ends_with("SKILL.md") {
            continue;
        }
        let path = Path::new(&file);
        let dir = match path.parent() {
            Some(dir) => dir,
            None => continue,
        };
        let dir_name = match dir.file_name().and_then(|name| name.to_str()) {
            Some(name) => name,
            None => continue,
        };
        let contents = match git::show_file(mirror_path, head_commit, &file) {
            Ok(contents) => contents,
            Err(err) => {
                invalid_skills.push(InvalidSkill {
                    path: dir.to_string_lossy().to_string(),
                    error: err.to_string(),
                });
                continue;
            }
        };

        match skills::parse_skill_details(&contents, dir_name) {
            Ok(details) => {
                let skill_path = dir.to_string_lossy().to_string();
                let (commit, updated_at) =
                    match git::last_commit_and_date(mirror_path, head_commit, &skill_path) {
                        Ok((commit, date)) => (commit, date),
                        Err(_) => (head_commit.to_string(), head_date.clone()),
                    };
                skills.push(IndexedSkill {
                    name: details.name,
                    description: details.description,
                    tags: details.tags,
                    path: skill_path,
                    updated_at,
                    commit,
                    version: details.version,
                });
            }
            Err(err) => {
                invalid_skills.push(InvalidSkill {
                    path: dir.to_string_lossy().to_string(),
                    error: err.to_string(),
                });
            }
        }
    }

    Ok(SourceScan {
        skills,
        invalid_skills,
    })
}
