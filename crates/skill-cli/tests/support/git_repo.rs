#![allow(dead_code)]

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

use super::skills::{TestSkill, skill_markdown};
use super::{init_repo, write_file};

struct GitRepoSkill {
    pub path: String,
    pub skill: TestSkill,
}

pub struct GitRepoBuilder {
    root: PathBuf,
    owner: String,
    repo: String,
    skills: Vec<GitRepoSkill>,
    files: Vec<(String, String)>,
}

pub struct GitRepoFixture {
    pub owner: String,
    pub repo: String,
    pub url: String,
    pub commit: String,
    git_config: PathBuf,
}

impl GitRepoFixture {
    pub fn shorthand(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    pub fn reference_with_path(&self, path: &str) -> String {
        format!("{}#{}", self.url, path)
    }

    pub fn reference_with_path_and_commit(&self, path: &str) -> String {
        format!("{}#{}@{}", self.url, path, self.commit)
    }

    pub fn git_env(&self) -> Vec<(String, String)> {
        vec![
            ("GIT_CONFIG_NOSYSTEM".to_string(), "1".to_string()),
            (
                "GIT_CONFIG_GLOBAL".to_string(),
                self.git_config.to_string_lossy().to_string(),
            ),
        ]
    }
}

impl GitRepoBuilder {
    pub fn new(root: &Path, owner: &str, repo: &str) -> Self {
        Self {
            root: root.to_path_buf(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            skills: Vec::new(),
            files: Vec::new(),
        }
    }

    pub fn add_skill(mut self, path: &str, skill: TestSkill) -> Self {
        self.skills.push(GitRepoSkill {
            path: path.to_string(),
            skill,
        });
        self
    }

    pub fn add_root_skill(mut self, skill: TestSkill) -> Self {
        self.skills.push(GitRepoSkill {
            path: String::new(),
            skill,
        });
        self
    }

    pub fn add_file(mut self, path: &str, contents: &str) -> Self {
        self.files.push((path.to_string(), contents.to_string()));
        self
    }

    pub fn build(self) -> Result<GitRepoFixture> {
        let repo_dir = self.root.join(format!("{}-{}", self.owner, self.repo));
        fs::create_dir_all(&repo_dir)?;

        for skill in &self.skills {
            let skill_dir = if skill.path.is_empty() {
                repo_dir.clone()
            } else {
                repo_dir.join(&skill.path)
            };
            fs::create_dir_all(&skill_dir)?;
            write_file(&skill_dir.join("SKILL.md"), &skill_markdown(&skill.skill))?;
        }

        for (path, contents) in &self.files {
            write_file(&repo_dir.join(path), contents)?;
        }

        let commit = init_repo(&repo_dir, "Add fixtures")?;
        let url = format!("https://github.com/{}/{}.git", self.owner, self.repo);
        let git_config = repo_dir.join(".gitconfig");
        write_git_config(&git_config, &url, &repo_dir)?;

        Ok(GitRepoFixture {
            owner: self.owner,
            repo: self.repo,
            url,
            commit,
            git_config,
        })
    }
}

fn write_git_config(path: &Path, url: &str, repo_dir: &Path) -> Result<()> {
    let file_url = format!("file://{}", repo_dir.display());
    let contents = format!(
        r#"[url "{file_url}"]
    insteadOf = {url}
"#
    );
    write_file(path, &contents)
}
