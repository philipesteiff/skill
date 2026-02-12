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

fn write_skill_with_version(
    repo_dir: &Path,
    name: &str,
    description: &str,
    version: &str,
) -> Result<PathBuf> {
    let skill_dir = repo_dir.join("skills").join(name);
    fs::create_dir_all(&skill_dir)?;
    let contents = format!(
        "---\nname: {name}\ndescription: {description}\nmetadata:\n  version: \"{version}\"\n---\n"
    );
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
    write_sources_config(
        skills_home,
        vec![SourceConfig {
            id: source_id.to_string(),
            url: url.to_string(),
            selection,
        }],
    )
}

fn write_sources_config(skills_home: &Path, sources: Vec<SourceConfig>) -> Result<()> {
    let config = Config { sources };
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

#[test]
fn when_syncing_should_refresh_applied_copies() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;

    let repo_dir = temp.path().join("skills-repo");
    fs::create_dir_all(&repo_dir)?;
    write_skill(&repo_dir, "alpha-skill", "Alpha v1")?;
    init_repo(&repo_dir, "Initial skills")?;

    let repo_url = format!("file://{}", repo_dir.display());
    write_config(&skills_home, "acme-skills", &repo_url, SelectionConfig::All)?;

    let output = run_skill_with_env(&["sync", "@acme-skills"], &skills_home, None, &[])?;
    assert!(output.status.success());

    let project_dir = temp.path().join("project");
    fs::create_dir_all(&project_dir)?;
    let output = run_skill_with_env(
        &[
            "apply",
            "--no-tui",
            "--targets",
            "claude:project",
            "--skills",
            "acme-skills/alpha-skill",
        ],
        &skills_home,
        Some(&project_dir),
        &[],
    )?;
    assert!(output.status.success());

    let applied_dir = project_dir
        .join(".claude/skills")
        .join("acme-skills__alpha-skill");
    let applied_contents = fs::read_to_string(applied_dir.join("SKILL.md"))?;
    assert!(applied_contents.contains("Alpha v1"));

    write_skill(&repo_dir, "alpha-skill", "Alpha v2")?;
    git_commit_all(&repo_dir, "Update alpha")?;

    let output = run_skill_with_env(&["sync", "@acme-skills"], &skills_home, None, &[])?;
    assert!(output.status.success());

    let applied_contents = fs::read_to_string(applied_dir.join("SKILL.md"))?;
    assert!(applied_contents.contains("Alpha v2"));

    Ok(())
}

#[test]
fn when_syncing_without_source_should_sync_all_configured_sources() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;

    let repo_a = temp.path().join("skills-repo-a");
    fs::create_dir_all(&repo_a)?;
    write_skill(&repo_a, "alpha-skill", "Alpha")?;
    let commit_a = init_repo(&repo_a, "Initial alpha")?;

    let repo_b = temp.path().join("skills-repo-b");
    fs::create_dir_all(&repo_b)?;
    write_skill(&repo_b, "beta-skill", "Beta")?;
    let commit_b = init_repo(&repo_b, "Initial beta")?;

    write_sources_config(
        &skills_home,
        vec![
            SourceConfig {
                id: "source-a".to_string(),
                url: format!("file://{}", repo_a.display()),
                selection: SelectionConfig::All,
            },
            SourceConfig {
                id: "source-b".to_string(),
                url: format!("file://{}", repo_b.display()),
                selection: SelectionConfig::All,
            },
        ],
    )?;

    let output = run_skill_with_env(&["sync"], &skills_home, None, &[])?;
    assert!(output.status.success());

    let lock = read_lock(&skills_home)?;
    assert_eq!(lock.skills.len(), 2);
    let alpha = lock
        .skills
        .iter()
        .find(|entry| entry.source_id == "source-a" && entry.name == "alpha-skill")
        .context("missing source-a alpha")?;
    let beta = lock
        .skills
        .iter()
        .find(|entry| entry.source_id == "source-b" && entry.name == "beta-skill")
        .context("missing source-b beta")?;
    assert_eq!(alpha.resolved_commit, commit_a);
    assert_eq!(beta.resolved_commit, commit_b);

    Ok(())
}

#[test]
fn when_syncing_without_source_with_one_failure_should_continue_and_fail_at_end() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;

    let repo_ok = temp.path().join("skills-repo-ok");
    fs::create_dir_all(&repo_ok)?;
    write_skill(&repo_ok, "ok-skill", "OK")?;
    init_repo(&repo_ok, "Initial ok")?;

    write_sources_config(
        &skills_home,
        vec![
            SourceConfig {
                id: "broken".to_string(),
                url: format!("file://{}", temp.path().join("missing-repo").display()),
                selection: SelectionConfig::All,
            },
            SourceConfig {
                id: "ok".to_string(),
                url: format!("file://{}", repo_ok.display()),
                selection: SelectionConfig::All,
            },
        ],
    )?;

    let output = run_skill_with_env(&["sync"], &skills_home, None, &[])?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("sync failed for 1 source(s): @broken"));

    let lock = read_lock(&skills_home)?;
    let ok_skill = lock
        .skills
        .iter()
        .find(|entry| entry.source_id == "ok" && entry.name == "ok-skill");
    assert!(
        ok_skill.is_some(),
        "expected successful source to still sync"
    );

    Ok(())
}

#[test]
fn when_syncing_without_source_and_no_configured_sources_should_error() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;

    let output = run_skill_with_env(&["sync"], &skills_home, None, &[])?;
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no configured sources"));

    Ok(())
}

#[test]
fn when_syncing_without_source_and_one_source_has_empty_selection_should_skip_not_fail()
-> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;

    let repo_empty = temp.path().join("skills-repo-empty");
    fs::create_dir_all(&repo_empty)?;
    write_skill(&repo_empty, "alpha-skill", "Alpha")?;
    init_repo(&repo_empty, "Initial alpha")?;

    let repo_active = temp.path().join("skills-repo-active");
    fs::create_dir_all(&repo_active)?;
    write_skill(&repo_active, "beta-skill", "Beta")?;
    init_repo(&repo_active, "Initial beta")?;

    write_sources_config(
        &skills_home,
        vec![
            SourceConfig {
                id: "empty".to_string(),
                url: format!("file://{}", repo_empty.display()),
                selection: SelectionConfig::List { skills: Vec::new() },
            },
            SourceConfig {
                id: "active".to_string(),
                url: format!("file://{}", repo_active.display()),
                selection: SelectionConfig::All,
            },
        ],
    )?;

    let output = run_skill_with_env(&["sync"], &skills_home, None, &[])?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Skipped source: no selected skills"));

    let lock = read_lock(&skills_home)?;
    assert_eq!(lock.skills.len(), 1);
    let active_skill = lock
        .skills
        .iter()
        .find(|entry| entry.source_id == "active" && entry.name == "beta-skill");
    assert!(active_skill.is_some());

    Ok(())
}

#[test]
fn when_syncing_explicit_source_with_empty_selection_should_error() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;

    let repo_empty = temp.path().join("skills-repo-empty");
    fs::create_dir_all(&repo_empty)?;
    write_skill(&repo_empty, "alpha-skill", "Alpha")?;
    init_repo(&repo_empty, "Initial alpha")?;

    write_sources_config(
        &skills_home,
        vec![SourceConfig {
            id: "empty".to_string(),
            url: format!("file://{}", repo_empty.display()),
            selection: SelectionConfig::List { skills: Vec::new() },
        }],
    )?;

    let output = run_skill_with_env(&["sync", "@empty"], &skills_home, None, &[])?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no skills selected"));

    Ok(())
}

#[test]
fn when_syncing_source_with_unsafe_version_should_fail_and_not_escape_install_root() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;

    let repo_dir = temp.path().join("skills-repo");
    fs::create_dir_all(&repo_dir)?;
    let outside = temp.path().join("outside-install");
    write_skill_with_version(
        &repo_dir,
        "evil-skill",
        "Evil",
        outside.to_string_lossy().as_ref(),
    )?;
    init_repo(&repo_dir, "Initial unsafe version")?;

    let repo_url = format!("file://{}", repo_dir.display());
    write_config(&skills_home, "acme-skills", &repo_url, SelectionConfig::All)?;

    let output = run_skill_with_env(&["sync", "@acme-skills"], &skills_home, None, &[])?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsafe skill version label"));
    assert!(
        !outside.join("SKILL.md").exists(),
        "expected no writes outside skills home"
    );

    let lock_path = skills_home.join("lock.json");
    if lock_path.exists() {
        let lock = read_lock(&skills_home)?;
        assert!(lock.skills.is_empty());
    }

    Ok(())
}
