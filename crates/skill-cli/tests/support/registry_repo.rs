#![allow(dead_code)]

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

use skill_core::util::slugify;

use super::skills::{skill_by_name, skill_markdown, test_skills};
use super::{init_repo, write_file};

pub struct RegistryFixture {
    pub skills_repo_dir: PathBuf,
    pub repo_dir: PathBuf,
    pub url: String,
    pub registry_id: String,
    pub skills_repo_url: String,
}

impl RegistryFixture {
    pub fn build(root: &Path, name: &str, include_skills: bool) -> Result<Self> {
        let skills_repo = root.join(format!("skills-repo-{name}"));
        let registry_repo = root.join(format!("registry-repo-{name}"));
        let skills_repo_url = format!("file://{}", skills_repo.display());

        if include_skills {
            fs::create_dir_all(&skills_repo)?;
            let skills = test_skills();
            for skill in &skills {
                let skill_dir = skills_repo.join("skills").join(skill.name);
                fs::create_dir_all(&skill_dir)?;
                write_file(&skill_dir.join("SKILL.md"), &skill_markdown(skill))?;
            }
            let commit = init_repo(&skills_repo, "Add sample skills")?;

            fs::create_dir_all(registry_repo.join("skills/acme"))?;
            for skill in &skills {
                let skill_json = serde_json::json!({
                    "namespace": "acme",
                    "name": skill.name,
                    "description": skill.description,
                    "repo_url": skills_repo_url,
                    "path": format!("skills/{}", skill.name),
                    "tags": skill.tags,
                    "latest": { "version": skill.version, "commit": commit },
                    "versions": [
                        {
                            "version": skill.version,
                            "commit": commit
                        }
                    ]
                });
                write_file(
                    &registry_repo
                        .join("skills/acme")
                        .join(format!("{}.json", skill.name)),
                    &serde_json::to_string_pretty(&skill_json)?,
                )?;
            }
        } else {
            write_file(&registry_repo.join("README.md"), "empty registry")?;
        }

        let _ = init_repo(&registry_repo, "Add registry metadata")?;

        let url = format!("file://{}", registry_repo.display());
        let registry_id = slugify(&url);
        Ok(Self {
            skills_repo_dir: skills_repo,
            repo_dir: registry_repo,
            url,
            registry_id,
            skills_repo_url,
        })
    }

    pub fn add_skill_entry(&self, name: &str, description: &str) -> Result<()> {
        let skills_dir = self.repo_dir.join("skills").join("acme");
        fs::create_dir_all(&skills_dir)?;
        let entry = serde_json::json!({
            "namespace": "acme",
            "name": name,
            "description": description,
            "repo_url": self.skills_repo_url,
            "path": format!("skills/{name}"),
            "tags": ["test"],
            "latest": { "version": "9.9.9", "commit": "deadbeef" },
            "versions": [
                { "version": "9.9.9", "commit": "deadbeef" }
            ]
        });
        write_file(
            &skills_dir.join(format!("{name}.json")),
            &serde_json::to_string_pretty(&entry)?,
        )?;
        let _ = git_commit_all(&self.repo_dir, "Add registry entry")?;
        Ok(())
    }

    pub fn update_skill_version(&self, name: &str, version: &str, body: &str) -> Result<String> {
        let base =
            skill_by_name(name).ok_or_else(|| anyhow::anyhow!("missing test skill {name}"))?;
        let skill_dir = self.skills_repo_dir.join("skills").join(name);
        fs::create_dir_all(&skill_dir)?;
        let contents =
            skill_markdown_dynamic(base.name, base.description, version, base.tags, body);
        write_file(&skill_dir.join("SKILL.md"), &contents)?;
        let commit = git_commit_all(&self.skills_repo_dir, "Update skill")?;
        self.update_registry_entry(name, version, &commit)?;
        Ok(commit)
    }

    fn update_registry_entry(&self, name: &str, version: &str, commit: &str) -> Result<()> {
        let path = self
            .repo_dir
            .join("skills")
            .join("acme")
            .join(format!("{name}.json"));
        let data = fs::read_to_string(&path)?;
        let mut value: serde_json::Value = serde_json::from_str(&data)?;
        value["latest"] = serde_json::json!({ "version": version, "commit": commit });
        let versions = value["versions"]
            .as_array_mut()
            .ok_or_else(|| anyhow::anyhow!("registry entry missing versions array"))?;
        versions.push(serde_json::json!({
            "version": version,
            "commit": commit
        }));
        write_file(&path, &serde_json::to_string_pretty(&value)?)?;
        let _ = git_commit_all(&self.repo_dir, "Update registry entry")?;
        Ok(())
    }
}

fn skill_markdown_dynamic(
    name: &str,
    description: &str,
    version: &str,
    tags: &[&str],
    body: &str,
) -> String {
    let tags = tags.join(", ");
    let title = name
        .split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        r#"---
name: {name}
description: {description}
metadata:
  version: {version}
  tags: [{tags}]
  namespace: acme
---

# {title}

{body}
"#,
        name = name,
        description = description,
        version = version,
        tags = tags,
        title = title,
        body = body,
    )
}

fn git_commit_all(repo_dir: &Path, message: &str) -> Result<String> {
    let _ = run_git(&["add", "."], repo_dir)?;
    let _ = run_git(&["commit", "-m", message, "-q"], repo_dir)?;
    run_git(&["rev-parse", "HEAD"], repo_dir)
}

fn run_git(args: &[&str], cwd: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
