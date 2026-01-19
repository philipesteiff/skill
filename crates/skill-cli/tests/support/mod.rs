use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

use skill_core::config::{Config, RegistryConfig};
use skill_core::output::Output;
use skill_core::registry;
use skill_core::util::slugify;

mod skills;
pub use skills::{TestSkill, skill_by_name};
use skills::{skill_markdown, test_skills};
mod git_repo;
pub use git_repo::GitRepoBuilder;

#[derive(Default)]
struct TestOutput {
    lines: Vec<String>,
}

impl Output for TestOutput {
    fn line(&mut self, message: impl Into<String>) -> Result<()> {
        self.lines.push(message.into());
        Ok(())
    }
}

pub struct Playground {
    root: TempDir,
    pub skills_home: PathBuf,
    pub skills_repo_url: String,
    pub registry_id: String,
    pub skills_commit: String,
}

pub struct ManifestBuilder {
    lines: Vec<String>,
}

impl ManifestBuilder {
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    pub fn add_ref(mut self, name: &str, reference: &str) -> Self {
        self.lines.push(format!("{name} = \"{reference}\""));
        self
    }

    pub fn write_to(self, dir: &Path) -> Result<PathBuf> {
        let mut contents = String::from("[dependencies]\n");
        for line in self.lines {
            contents.push_str(&line);
            contents.push('\n');
        }
        let path = dir.join("skills.toml");
        write_file(&path, &contents)?;
        Ok(path)
    }
}

impl Playground {
    pub fn new() -> Result<Self> {
        let root = TempDir::new().context("create temp dir")?;
        let work_dir = root.path().join("work");
        let skills_repo = work_dir.join("skills-repo");
        let registry_repo = work_dir.join("skills-registry");
        let skills_home = work_dir.join("home");

        fs::create_dir_all(&skills_home)?;
        let skills = test_skills();
        for skill in &skills {
            let skill_dir = skills_repo.join("skills").join(skill.name);
            fs::create_dir_all(&skill_dir)?;
            write_file(&skill_dir.join("SKILL.md"), &skill_markdown(skill))?;
        }

        let skills_commit = init_repo(&skills_repo, "Add sample skills")?;
        let skills_repo_url = format!("file://{}", skills_repo.display());

        fs::create_dir_all(registry_repo.join("skills/acme"))?;
        let published_at = "2024-01-01T00:00:00Z";
        for skill in &skills {
            let skill_json = serde_json::json!({
                "namespace": "acme",
                "name": skill.name,
                "description": skill.description,
                "repo_url": skills_repo_url,
                "path": format!("skills/{}", skill.name),
                "tags": skill.tags,
                "latest": { "version": skill.version, "commit": skills_commit },
                "versions": [
                    {
                        "version": skill.version,
                        "commit": skills_commit,
                        "published_at": published_at
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

        init_repo(&registry_repo, "Add registry metadata")?;

        let registry_url = format!("file://{}", registry_repo.display());
        let mut output = TestOutput::default();
        let registry_id = prepare_registry(&skills_home, &registry_url, root.path(), &mut output)?;

        Ok(Self {
            root,
            skills_home,
            skills_repo_url,
            registry_id,
            skills_commit,
        })
    }

    pub fn root(&self) -> &Path {
        self.root.path()
    }
}

pub fn write_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn run_git<I, S>(args: I, cwd: &Path) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .context("run git")?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn init_repo(repo_dir: &Path, message: &str) -> Result<String> {
    run_git(["init", "-q"], repo_dir)?;
    run_git(["config", "user.email", "playground@example.com"], repo_dir)?;
    run_git(["config", "user.name", "Playground"], repo_dir)?;
    run_git(["add", "."], repo_dir)?;
    run_git(["commit", "-m", message, "-q"], repo_dir)?;
    run_git(["rev-parse", "HEAD"], repo_dir)
}

pub fn run_skill(
    args: &[&str],
    skills_home: &Path,
    cwd: Option<&Path>,
) -> Result<std::process::Output> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_skill"));
    cmd.args(args);
    cmd.env("SKILLS_HOME", skills_home);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    cmd.output().context("run skill")
}

pub fn run_skill_with_env(
    args: &[&str],
    skills_home: &Path,
    cwd: Option<&Path>,
    envs: &[(String, String)],
) -> Result<std::process::Output> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_skill"));
    cmd.args(args);
    cmd.env("SKILLS_HOME", skills_home);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    cmd.output().context("run skill")
}

fn prepare_registry(
    skills_home: &Path,
    registry_url: &str,
    cwd: &Path,
    output: &mut TestOutput,
) -> Result<String> {
    let registry_id = slugify(registry_url);
    let registry_home = skills_home.join("registry").join(&registry_id);
    let registry_clone = registry_home.join("repo");

    fs::create_dir_all(&registry_home)?;
    run_git(
        [
            "clone",
            "-q",
            registry_url,
            registry_clone.to_str().unwrap(),
        ],
        cwd,
    )?;

    let registry_config = RegistryConfig {
        id: registry_id.clone(),
        url: registry_url.to_string(),
    };
    let index_path = registry_home.join("index.sqlite");
    registry::rebuild_index(&registry_config, &registry_clone, &index_path, output)?;

    let config = Config {
        registries: vec![registry_config],
    };
    let config_path = skills_home.join("config.json");
    fs::write(config_path, serde_json::to_string_pretty(&config)?)?;

    Ok(registry_id)
}
