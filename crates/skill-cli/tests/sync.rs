use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod support_sync;
use support_sync::run_skill_with_env;

use skill_core::config::{Config, SelectionConfig, SourceConfig};
use skill_core::lockfile::Lockfile;

fn run_git<I, S>(args: I, cwd: &Path) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
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
    run_git(["config", "user.email", "test@example.com"], repo_dir)?;
    run_git(["config", "user.name", "Test"], repo_dir)?;
    git_commit_all(repo_dir, message)
}

fn git_commit_all(repo_dir: &Path, message: &str) -> Result<String> {
    run_git(["add", "."], repo_dir)?;
    run_git(["commit", "-m", message, "-q"], repo_dir)?;
    run_git(["rev-parse", "HEAD"], repo_dir)
}

fn write_skill(repo_dir: &Path, name: &str, description: &str) -> Result<PathBuf> {
    let skill_dir = repo_dir.join("skills").join(name);
    fs::create_dir_all(&skill_dir)?;
    let contents = format!("---\nname: {name}\ndescription: {description}\n---\n");
    fs::write(skill_dir.join("SKILL.md"), contents)?;
    Ok(skill_dir)
}

fn write_root_skill(repo_dir: &Path, name: &str, description: &str) -> Result<()> {
    let contents = format!("---\nname: {name}\ndescription: {description}\n---\n");
    fs::write(repo_dir.join("SKILL.md"), contents)?;
    Ok(())
}

fn write_config(
    skills_home: &Path,
    source_id: &str,
    url: &str,
    selection: SelectionConfig,
) -> Result<()> {
    let config = Config {
        sources: vec![SourceConfig {
            id: source_id.to_string(),
            url: url.to_string(),
            selection,
        }],
    };
    let contents = serde_json::to_string_pretty(&config)?;
    fs::write(skills_home.join("config.json"), contents)?;
    Ok(())
}

fn read_lock(skills_home: &Path) -> Result<Lockfile> {
    let data = fs::read_to_string(skills_home.join("lock.json"))?;
    Ok(serde_json::from_str(&data)?)
}

#[test]
fn when_syncing_source_with_install_all_should_install_and_update() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;

    let repo_dir = temp.path().join("skills-repo");
    fs::create_dir_all(&repo_dir)?;
    write_skill(&repo_dir, "alpha-skill", "Alpha")?;
    write_skill(&repo_dir, "bravo-skill", "Bravo")?;
    let first_commit = init_repo(&repo_dir, "Initial skills")?;

    let repo_url = format!("file://{}", repo_dir.display());
    write_config(&skills_home, "acme-skills", &repo_url, SelectionConfig::All)?;

    let output = run_skill_with_env(&["sync", "@acme-skills"], &skills_home, None, &[])?;
    assert!(output.status.success());

    let lock = read_lock(&skills_home)?;
    assert_eq!(lock.skills.len(), 2);
    let alpha = lock
        .skills
        .iter()
        .find(|entry| entry.name == "alpha-skill")
        .context("missing alpha skill")?;
    let bravo = lock
        .skills
        .iter()
        .find(|entry| entry.name == "bravo-skill")
        .context("missing bravo skill")?;
    assert_eq!(alpha.resolved_commit, first_commit);
    assert_eq!(bravo.resolved_commit, first_commit);
    assert!(Path::new(&alpha.install_dir).exists());
    assert!(Path::new(&bravo.install_dir).exists());

    write_skill(&repo_dir, "alpha-skill", "Alpha v2")?;
    let second_commit = git_commit_all(&repo_dir, "Update alpha")?;

    let output = run_skill_with_env(&["sync", "@acme-skills"], &skills_home, None, &[])?;
    assert!(output.status.success());

    let lock = read_lock(&skills_home)?;
    let alpha = lock
        .skills
        .iter()
        .find(|entry| entry.name == "alpha-skill")
        .context("missing alpha skill")?;
    let bravo = lock
        .skills
        .iter()
        .find(|entry| entry.name == "bravo-skill")
        .context("missing bravo skill")?;
    assert_eq!(alpha.resolved_commit, second_commit);
    assert_eq!(bravo.resolved_commit, first_commit);

    Ok(())
}

#[test]
fn when_syncing_with_missing_selection_should_warn() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;

    let repo_dir = temp.path().join("skills-repo");
    fs::create_dir_all(&repo_dir)?;
    write_skill(&repo_dir, "alpha-skill", "Alpha")?;
    init_repo(&repo_dir, "Initial skills")?;

    let repo_url = format!("file://{}", repo_dir.display());
    write_config(
        &skills_home,
        "acme-skills",
        &repo_url,
        SelectionConfig::List {
            skills: vec![
                "skills/alpha-skill".to_string(),
                "skills/missing-skill".to_string(),
            ],
        },
    )?;

    let output = run_skill_with_env(&["sync", "@acme-skills"], &skills_home, None, &[])?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("selected skill(s) not found"));
    assert!(stdout.contains("Missing: skills/missing-skill"));

    Ok(())
}

#[test]
fn when_syncing_source_with_root_skill_should_install() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;

    let repo_dir = temp.path().join("skills-repo");
    fs::create_dir_all(&repo_dir)?;
    write_root_skill(&repo_dir, "root-skill", "Root skill")?;
    let commit = init_repo(&repo_dir, "Initial root skill")?;

    let repo_url = format!("file://{}", repo_dir.display());
    write_config(&skills_home, "acme-skills", &repo_url, SelectionConfig::All)?;

    let output = run_skill_with_env(&["sync", "@acme-skills"], &skills_home, None, &[])?;
    assert!(output.status.success());

    let lock = read_lock(&skills_home)?;
    assert_eq!(lock.skills.len(), 1);
    let root = lock
        .skills
        .iter()
        .find(|entry| entry.name == "root-skill")
        .context("missing root skill")?;
    assert_eq!(root.resolved_commit, commit);
    assert_eq!(root.path, ".");
    assert!(Path::new(&root.install_dir).exists());

    Ok(())
}

#[test]
fn when_syncing_source_with_root_and_nested_skills_should_install_all() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;

    let repo_dir = temp.path().join("skills-repo");
    fs::create_dir_all(&repo_dir)?;
    write_root_skill(&repo_dir, "root-skill", "Root skill")?;
    write_skill(&repo_dir, "nested-skill", "Nested skill")?;
    init_repo(&repo_dir, "Initial mixed skills")?;

    let repo_url = format!("file://{}", repo_dir.display());
    write_config(&skills_home, "acme-skills", &repo_url, SelectionConfig::All)?;

    let output = run_skill_with_env(&["sync", "@acme-skills"], &skills_home, None, &[])?;
    assert!(output.status.success());

    let lock = read_lock(&skills_home)?;
    assert_eq!(lock.skills.len(), 2);
    let root = lock
        .skills
        .iter()
        .find(|entry| entry.name == "root-skill")
        .context("missing root skill")?;
    let nested = lock
        .skills
        .iter()
        .find(|entry| entry.name == "nested-skill")
        .context("missing nested skill")?;
    assert_eq!(root.path, ".");
    assert_eq!(nested.path, "skills/nested-skill");
    assert!(Path::new(&root.install_dir).exists());
    assert!(Path::new(&nested.install_dir).exists());

    Ok(())
}
