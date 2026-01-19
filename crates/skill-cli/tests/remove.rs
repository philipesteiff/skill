mod support;

use anyhow::Result;
use std::fs;

use support::{Playground, run_skill};

use skill_core::lockfile::Lockfile;

#[test]
fn when_removing_installed_skill_should_remove_files_and_lock() -> Result<()> {
    let playground = Playground::new()?;

    let output = run_skill(&["sync"], &playground.skills_home, None)?;
    assert!(output.status.success());
    let output = run_skill(
        &["install", "acme/echo-skill"],
        &playground.skills_home,
        None,
    )?;
    assert!(output.status.success());

    let install_dir = playground
        .skills_home
        .join("installed")
        .join("acme")
        .join("echo-skill")
        .join("1.0.0");
    assert!(install_dir.join("SKILL.md").exists());

    let output = run_skill(
        &["remove", "acme/echo-skill"],
        &playground.skills_home,
        None,
    )?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Removed acme/echo-skill"));

    assert!(!install_dir.exists());

    let lock_path = playground.skills_home.join("lock.json");
    let lock: Lockfile = serde_json::from_str(&fs::read_to_string(lock_path)?)?;
    assert!(lock.skills.iter().all(|entry| entry.name != "echo-skill"));

    Ok(())
}

#[test]
fn when_removing_missing_skill_should_error() -> Result<()> {
    let playground = Playground::new()?;

    let output = run_skill(&["sync"], &playground.skills_home, None)?;
    assert!(output.status.success());
    let output = run_skill(
        &["install", "acme/echo-skill"],
        &playground.skills_home,
        None,
    )?;
    assert!(output.status.success());

    let output = run_skill(
        &["remove", "acme/missing-skill"],
        &playground.skills_home,
        None,
    )?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("skill not installed: acme/missing-skill"));

    Ok(())
}

#[test]
fn when_removing_all_should_clear_installs_and_lock() -> Result<()> {
    let playground = Playground::new()?;

    let output = run_skill(&["sync"], &playground.skills_home, None)?;
    assert!(output.status.success());
    let output = run_skill(
        &["install", "acme/echo-skill"],
        &playground.skills_home,
        None,
    )?;
    assert!(output.status.success());
    let output = run_skill(
        &["install", "acme/notes-skill"],
        &playground.skills_home,
        None,
    )?;
    assert!(output.status.success());

    let output = run_skill(&["remove", "--all"], &playground.skills_home, None)?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Removed 2 skill(s)"));

    let installed_dir = playground.skills_home.join("installed");
    assert!(installed_dir.exists());
    assert!(!installed_dir.join("acme").join("echo-skill").exists());
    assert!(!installed_dir.join("acme").join("notes-skill").exists());

    let lock_path = playground.skills_home.join("lock.json");
    let lock: Lockfile = serde_json::from_str(&fs::read_to_string(lock_path)?)?;
    assert!(lock.skills.is_empty());

    Ok(())
}

#[test]
fn when_removing_all_without_installs_should_report() -> Result<()> {
    let playground = Playground::new()?;
    fs::create_dir_all(&playground.skills_home)?;

    let output = run_skill(&["remove", "--all"], &playground.skills_home, None)?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No installed skills"));

    Ok(())
}
