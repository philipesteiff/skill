mod support;

use anyhow::{Context, Result};
use std::fs;

use support::{
    GitRepoBuilder, ManifestBuilder, Playground, TestSkill, run_skill, run_skill_with_env,
    skill_by_name,
};

use skill_core::lockfile::Lockfile;

#[test]
fn when_installing_from_registry_should_write_lockfile_and_files() -> Result<()> {
    let playground = Playground::new()?;

    let output = run_skill(
        &["install", "acme/notes-skill"],
        &playground.skills_home,
        None,
    )?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "install failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let install_dir = playground
        .skills_home
        .join("installed")
        .join("acme")
        .join("notes-skill")
        .join("0.2.0");
    assert!(install_dir.join("SKILL.md").exists());

    let lock_path = playground.skills_home.join("lock.json");
    let lock_data = fs::read_to_string(&lock_path)?;
    let lockfile: Lockfile = serde_json::from_str(&lock_data)?;
    let entry = lockfile
        .skills
        .iter()
        .find(|entry| entry.name == "notes-skill")
        .context("notes-skill missing from lockfile")?;

    assert_eq!(entry.namespace, "acme");
    assert_eq!(entry.requested, "@latest");
    assert_eq!(entry.resolved_version.as_deref(), Some("0.2.0"));
    assert_eq!(entry.resolved_commit, playground.skills_commit);
    assert_eq!(entry.repo_url, playground.skills_repo_url);
    assert_eq!(entry.path, "skills/notes-skill");
    assert_eq!(
        entry.registry_id.as_deref(),
        Some(playground.registry_id.as_str())
    );
    assert_eq!(entry.install_dir, install_dir.to_string_lossy().to_string());

    Ok(())
}

#[test]
fn when_installing_from_manifest_should_install_all_dependencies() -> Result<()> {
    let playground = Playground::new()?;
    let project_dir = playground.root().join("project");
    fs::create_dir_all(&project_dir)?;
    ManifestBuilder::new()
        .add_ref("echo", "acme/echo-skill")
        .add_ref("notes", "acme/notes-skill")
        .write_to(&project_dir)?;

    let output = run_skill(&["install"], &playground.skills_home, Some(&project_dir))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "manifest install failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let echo_dir = playground
        .skills_home
        .join("installed")
        .join("acme")
        .join("echo-skill")
        .join("1.0.0");
    let notes_dir = playground
        .skills_home
        .join("installed")
        .join("acme")
        .join("notes-skill")
        .join("0.2.0");
    assert!(echo_dir.join("SKILL.md").exists());
    assert!(notes_dir.join("SKILL.md").exists());

    Ok(())
}

#[test]
fn when_installing_from_git_repo_with_single_skill_should_install() -> Result<()> {
    let playground = Playground::new()?;
    let echo = skill_by_name("echo-skill").context("missing echo-skill")?;
    let repo = GitRepoBuilder::new(playground.root(), "octo", "single-skill")
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

    let install_dir = playground
        .skills_home
        .join("installed")
        .join("octo")
        .join("echo-skill")
        .join("latest");
    assert!(install_dir.join("SKILL.md").exists());

    let lock_path = playground.skills_home.join("lock.json");
    let lock_data = fs::read_to_string(&lock_path)?;
    let lockfile: Lockfile = serde_json::from_str(&lock_data)?;
    let entry = lockfile
        .skills
        .iter()
        .find(|entry| entry.name == "echo-skill")
        .context("echo-skill missing from lockfile")?;

    assert_eq!(entry.namespace, "octo");
    assert_eq!(entry.requested, "@latest");
    assert!(entry.resolved_version.is_none());
    assert_eq!(entry.resolved_commit, repo.commit);
    assert_eq!(entry.repo_url, repo.url);
    assert_eq!(entry.path, "skills/echo-skill");
    assert!(entry.registry_id.is_none());
    assert_eq!(entry.install_dir, install_dir.to_string_lossy().to_string());

    Ok(())
}

#[test]
fn when_installing_from_git_repo_with_multiple_skills_should_install_all() -> Result<()> {
    let playground = Playground::new()?;
    let echo = skill_by_name("echo-skill").context("missing echo-skill")?;
    let notes = skill_by_name("notes-skill").context("missing notes-skill")?;
    let repo = GitRepoBuilder::new(playground.root(), "octo", "multi-skill")
        .add_skill("skills/echo-skill", echo)
        .add_skill("skills/notes-skill", notes)
        .build()?;

    let output = run_skill_with_env(
        &["install", &repo.url],
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

    let echo_dir = playground
        .skills_home
        .join("installed")
        .join("octo")
        .join("echo-skill")
        .join("latest");
    let notes_dir = playground
        .skills_home
        .join("installed")
        .join("octo")
        .join("notes-skill")
        .join("latest");
    assert!(echo_dir.join("SKILL.md").exists());
    assert!(notes_dir.join("SKILL.md").exists());

    Ok(())
}

#[test]
fn when_installing_from_git_repo_with_no_skills_should_error() -> Result<()> {
    let playground = Playground::new()?;
    let repo = GitRepoBuilder::new(playground.root(), "octo", "no-skills")
        .add_file("README.md", "nothing to see here")
        .build()?;

    let output = run_skill_with_env(
        &["install", &repo.url],
        &playground.skills_home,
        None,
        &repo.git_env(),
    )?;

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no valid SKILL.md found in repo"));

    Ok(())
}

#[test]
fn when_installing_from_git_repo_with_subfolder_skill_should_install() -> Result<()> {
    let playground = Playground::new()?;
    let echo = skill_by_name("echo-skill").context("missing echo-skill")?;
    let repo = GitRepoBuilder::new(playground.root(), "octo", "nested-skill")
        .add_skill("nested/skills/echo-skill", echo)
        .build()?;

    let reference = repo.reference_with_path("nested/skills/echo-skill");
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

    let install_dir = playground
        .skills_home
        .join("installed")
        .join("octo")
        .join("echo-skill")
        .join("latest");
    assert!(install_dir.join("SKILL.md").exists());

    let lock_path = playground.skills_home.join("lock.json");
    let lock_data = fs::read_to_string(&lock_path)?;
    let lockfile: Lockfile = serde_json::from_str(&lock_data)?;
    let entry = lockfile
        .skills
        .iter()
        .find(|entry| entry.name == "echo-skill")
        .context("echo-skill missing from lockfile")?;
    assert_eq!(entry.path, "nested/skills/echo-skill");

    Ok(())
}

#[test]
fn when_installing_from_git_repo_with_root_skill_should_error() -> Result<()> {
    let playground = Playground::new()?;
    let notes = skill_by_name("notes-skill").context("missing notes-skill")?;
    let repo = GitRepoBuilder::new(playground.root(), "octo", "root-skill")
        .add_root_skill(notes)
        .build()?;

    let output = run_skill_with_env(
        &["install", &repo.url],
        &playground.skills_home,
        None,
        &repo.git_env(),
    )?;

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no valid SKILL.md found in repo"));

    Ok(())
}

#[test]
fn when_installing_from_git_repo_with_owner_repo_shorthand_should_install() -> Result<()> {
    let playground = Playground::new()?;
    let echo = skill_by_name("echo-skill").context("missing echo-skill")?;
    let repo = GitRepoBuilder::new(playground.root(), "octo", "shorthand")
        .add_skill("skills/echo-skill", echo)
        .build()?;

    let output = run_skill_with_env(
        &["install", &repo.shorthand()],
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

    let install_dir = playground
        .skills_home
        .join("installed")
        .join("octo")
        .join("echo-skill")
        .join("latest");
    assert!(install_dir.join("SKILL.md").exists());

    Ok(())
}

#[test]
fn when_installing_from_git_repo_with_commit_selector_should_record_requested() -> Result<()> {
    let playground = Playground::new()?;
    let echo = skill_by_name("echo-skill").context("missing echo-skill")?;
    let repo = GitRepoBuilder::new(playground.root(), "octo", "commit-skill")
        .add_skill("skills/echo-skill", echo)
        .build()?;

    let reference = repo.reference_with_path_and_commit("skills/echo-skill");
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

    let lock_path = playground.skills_home.join("lock.json");
    let lock_data = fs::read_to_string(&lock_path)?;
    let lockfile: Lockfile = serde_json::from_str(&lock_data)?;
    let entry = lockfile
        .skills
        .iter()
        .find(|entry| entry.name == "echo-skill")
        .context("echo-skill missing from lockfile")?;

    assert_eq!(entry.requested, format!("@{}", repo.commit));
    assert_eq!(entry.resolved_commit, repo.commit);

    Ok(())
}

#[test]
fn when_installing_from_git_repo_with_existing_skill_should_overwrite() -> Result<()> {
    let playground = Playground::new()?;
    let echo = skill_by_name("echo-skill").context("missing echo-skill")?;
    let first_repo = GitRepoBuilder::new(playground.root(), "octo", "overwrite-one")
        .add_skill("skills/echo-skill", echo)
        .build()?;

    let first_reference = first_repo.reference_with_path("skills/echo-skill");
    let first_output = run_skill_with_env(
        &["install", &first_reference],
        &playground.skills_home,
        None,
        &first_repo.git_env(),
    )?;
    if !first_output.status.success() {
        return Err(anyhow::anyhow!(
            "first git install failed: {}",
            String::from_utf8_lossy(&first_output.stderr)
        ));
    }

    let updated = GitRepoBuilder::new(playground.root(), "octo", "overwrite-two")
        .add_skill(
            "skills/echo-skill",
            TestSkill {
                name: "echo-skill",
                description: "Echo input with basic validation.",
                version: "1.1.0",
                tags: &["cli", "example"],
                body: "Updated echo skill body.",
            },
        )
        .build()?;

    let second_reference = updated.reference_with_path("skills/echo-skill");
    let second_output = run_skill_with_env(
        &["install", &second_reference],
        &playground.skills_home,
        None,
        &updated.git_env(),
    )?;
    if !second_output.status.success() {
        return Err(anyhow::anyhow!(
            "second git install failed: {}",
            String::from_utf8_lossy(&second_output.stderr)
        ));
    }

    let install_dir = playground
        .skills_home
        .join("installed")
        .join("octo")
        .join("echo-skill")
        .join("latest");
    assert!(install_dir.join("SKILL.md").exists());

    let lock_path = playground.skills_home.join("lock.json");
    let lock_data = fs::read_to_string(&lock_path)?;
    let lockfile: Lockfile = serde_json::from_str(&lock_data)?;
    let entries: Vec<_> = lockfile
        .skills
        .iter()
        .filter(|entry| entry.name == "echo-skill")
        .collect();
    assert_eq!(entries.len(), 1);
    let entry = entries[0];
    assert!(entry.resolved_version.is_none());
    assert_eq!(entry.resolved_commit, updated.commit);
    assert_eq!(entry.repo_url, updated.url);

    Ok(())
}
