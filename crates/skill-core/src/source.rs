use anyhow::{Result, anyhow};
use std::collections::HashMap;
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
    if previous.trim() == head && index_path.exists() && source_index::is_compatible(&index_path)? {
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
    validate_unique_names(&scan.skills)?;

    source_index::rebuild_index(paths, &source.id, &scan.skills, output)?;
    std::fs::write(head_path, &head)?;
    Ok(head)
}

fn validate_unique_names(skills: &[IndexedSkill]) -> Result<()> {
    let mut by_name: HashMap<&str, &str> = HashMap::new();
    for skill in skills {
        if let Some(existing_path) = by_name.get(skill.name.as_str()) {
            if *existing_path != skill.path.as_str() {
                return Err(anyhow!(
                    "source contains duplicate skill name '{}' at '{}' and '{}'; skill names must be unique per source",
                    skill.name,
                    existing_path,
                    skill.path
                ));
            }
            continue;
        }
        by_name.insert(skill.name.as_str(), skill.path.as_str());
    }
    Ok(())
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
        let mut skill_path = dir.to_string_lossy().to_string();
        if skill_path.is_empty() {
            skill_path = ".".to_string();
        }
        let is_root_skill = skill_path == ".";
        let dir_name = dir.file_name().and_then(|name| name.to_str());
        let contents = match git::show_file(mirror_path, head_commit, &file) {
            Ok(contents) => contents,
            Err(err) => {
                invalid_skills.push(InvalidSkill {
                    path: skill_path.clone(),
                    error: err.to_string(),
                });
                continue;
            }
        };

        match skills::parse_skill_details_with_options(&contents, dir_name, !is_root_skill) {
            Ok(details) => {
                let content_hash = match git::object_hash(mirror_path, head_commit, &skill_path) {
                    Ok(hash) => hash,
                    Err(_) => head_commit.to_string(),
                };
                // Use the SKILL.md path for the "last updated" stamp.
                let (_, updated_at) =
                    match git::last_commit_and_date(mirror_path, head_commit, &file) {
                        Ok((commit, date)) => (commit, date),
                        Err(_) => (head_commit.to_string(), head_date.clone()),
                    };
                skills.push(IndexedSkill {
                    name: details.name,
                    description: details.description,
                    tags: details.tags,
                    path: skill_path,
                    updated_at,
                    commit: head_commit.to_string(),
                    content_hash,
                    version: details.version,
                });
            }
            Err(err) => {
                invalid_skills.push(InvalidSkill {
                    path: skill_path,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn indexed_skill(name: &str, path: &str) -> IndexedSkill {
        IndexedSkill {
            name: name.to_string(),
            description: String::new(),
            tags: Vec::new(),
            path: path.to_string(),
            updated_at: "2026-01-01".to_string(),
            commit: "deadbeef".to_string(),
            content_hash: "hash".to_string(),
            version: None,
        }
    }

    #[test]
    fn when_source_has_duplicate_skill_names_should_error() {
        let skills = vec![
            indexed_skill("echo", "skills/echo"),
            indexed_skill("echo", "other/echo"),
        ];

        let error = validate_unique_names(&skills).expect_err("expected duplicate-name failure");
        let message = error.to_string();
        assert!(message.contains("duplicate skill name 'echo'"));
        assert!(message.contains("skills/echo"));
        assert!(message.contains("other/echo"));
    }

    #[test]
    fn when_source_skill_names_are_unique_should_validate() {
        let skills = vec![
            indexed_skill("echo", "skills/echo"),
            indexed_skill("sum", "skills/sum"),
        ];

        validate_unique_names(&skills).expect("unique names should validate");
    }
}
