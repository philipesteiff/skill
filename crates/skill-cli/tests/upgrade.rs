mod support;

use anyhow::{Context, Result};
use std::fs;

use support::{
    GitRepoBuilder, Playground, RegistryFixture, run_skill, run_skill_with_env, skill_by_name,
    write_config,
};

use skill_core::config::RegistryConfig;
use skill_core::lockfile::{LockedSkill, Lockfile};

#[test]
fn when_upgrading_without_installed_skills_should_print_message() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");

    let output = run_skill(&["upgrade"], &skills_home, None)?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No installed skills"));

    Ok(())
}

#[test]
fn when_upgrading_latest_without_changes_should_report_up_to_date() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    let fixture = RegistryFixture::build(temp.path(), "main", true)?;

    write_config(
        &skills_home,
        vec![RegistryConfig {
            id: fixture.registry_id.clone(),
            url: fixture.url.clone(),
        }],
    )?;
    let output = run_skill(&["sync"], &skills_home, None)?;
    assert!(output.status.success());
    let output = run_skill(&["install", "acme/echo-skill"], &skills_home, None)?;
    assert!(output.status.success());

    let output = run_skill(&["upgrade"], &skills_home, None)?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("All @latest skills are up to date"));

    Ok(())
}

#[test]
fn when_upgrading_latest_with_new_registry_version_should_update() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    let fixture = RegistryFixture::build(temp.path(), "main", true)?;

    write_config(
        &skills_home,
        vec![RegistryConfig {
            id: fixture.registry_id.clone(),
            url: fixture.url.clone(),
        }],
    )?;
    let output = run_skill(&["sync"], &skills_home, None)?;
    assert!(output.status.success());
    let output = run_skill(&["install", "acme/echo-skill"], &skills_home, None)?;
    assert!(output.status.success());

    let new_commit =
        fixture.update_skill_version("echo-skill", "1.1.0", "Updated echo skill body.")?;
    let output = run_skill(&["upgrade"], &skills_home, None)?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Updated 1 skill(s)"));

    let install_dir = skills_home
        .join("installed")
        .join("acme")
        .join("echo-skill")
        .join("1.1.0");
    assert!(install_dir.join("SKILL.md").exists());
    assert!(
        !skills_home
            .join("installed")
            .join("acme")
            .join("echo-skill")
            .join("1.0.0")
            .exists()
    );

    let lock_path = skills_home.join("lock.json");
    let lock_data = fs::read_to_string(&lock_path)?;
    let lockfile: Lockfile = serde_json::from_str(&lock_data)?;
    let entry = lockfile
        .skills
        .iter()
        .find(|entry| entry.name == "echo-skill")
        .context("echo-skill missing from lockfile")?;
    assert_eq!(entry.resolved_version.as_deref(), Some("1.1.0"));
    assert_eq!(entry.resolved_commit, new_commit);

    Ok(())
}

#[test]
fn when_upgrading_with_missing_registry_config_should_error() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;

    let entry = LockedSkill {
        namespace: "acme".to_string(),
        name: "echo-skill".to_string(),
        requested: "@latest".to_string(),
        resolved_version: Some("1.0.0".to_string()),
        resolved_commit: "deadbeef".to_string(),
        repo_url: "file:///tmp/skills-repo".to_string(),
        path: "skills/echo-skill".to_string(),
        install_dir: skills_home
            .join("installed")
            .join("acme")
            .join("echo-skill")
            .join("1.0.0")
            .to_string_lossy()
            .to_string(),
        registry_id: Some("missing-registry".to_string()),
    };
    let lockfile = Lockfile {
        skills: vec![entry],
    };
    fs::write(
        skills_home.join("lock.json"),
        serde_json::to_string_pretty(&lockfile)?,
    )?;

    let output = run_skill(&["upgrade"], &skills_home, None)?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing registry"));

    Ok(())
}

#[test]
fn when_upgrading_latest_without_registry_should_update_from_git_head() -> Result<()> {
    let playground = Playground::new()?;
    let echo = skill_by_name("echo-skill").context("missing echo-skill")?;
    let repo = GitRepoBuilder::new(playground.root(), "octo", "upgrade-git")
        .add_skill("skills/echo-skill", echo)
        .build()?;

    let reference = repo.reference_with_path("skills/echo-skill");
    let output = run_skill_with_env(
        &["install", &reference],
        &playground.skills_home,
        None,
        &repo.git_env(),
    )?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git install failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let updated = GitRepoBuilder::new(playground.root(), "octo", "upgrade-git")
        .add_skill(
            "skills/echo-skill",
            support::TestSkill {
                name: "echo-skill",
                description: "Echo input with basic validation.",
                version: "1.1.0",
                tags: &["cli", "example"],
                body: "Updated echo skill body.",
            },
        )
        .build()?;

    let output = run_skill_with_env(
        &["upgrade"],
        &playground.skills_home,
        None,
        &updated.git_env(),
    )?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Updated 1 skill(s)"));

    let install_dir = playground
        .skills_home
        .join("installed")
        .join("octo")
        .join("echo-skill")
        .join("latest");
    let skill_md = install_dir.join("SKILL.md");
    assert!(skill_md.exists());
    let contents = fs::read_to_string(&skill_md)?;
    assert!(contents.contains("version: 1.1.0"));

    let lock_path = playground.skills_home.join("lock.json");
    let lock_data = fs::read_to_string(&lock_path)?;
    let lockfile: Lockfile = serde_json::from_str(&lock_data)?;
    let entry = lockfile
        .skills
        .iter()
        .find(|entry| entry.name == "echo-skill")
        .context("echo-skill missing from lockfile")?;
    assert_eq!(entry.resolved_commit, updated.commit);
    assert!(entry.resolved_version.is_none());

    Ok(())
}
