mod support;

use anyhow::Result;
use std::fs;
use std::thread;
use std::time::Duration;

use support::{RegistryFixture, run_skill, write_config};

use skill_core::config::RegistryConfig;

#[test]
fn when_syncing_registry_should_clone_and_index() -> Result<()> {
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
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "sync failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let registry_dir = skills_home.join("registry").join(&fixture.registry_id);
    assert!(registry_dir.join("repo").join(".git").exists());
    assert!(registry_dir.join("index.sqlite").exists());
    assert!(registry_dir.join("head.txt").exists());

    Ok(())
}

#[test]
fn when_syncing_registry_without_changes_should_not_update_head() -> Result<()> {
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
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "sync failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let head_path = skills_home
        .join("registry")
        .join(&fixture.registry_id)
        .join("head.txt");
    let first_head = fs::read_to_string(&head_path)?;
    let first_meta = fs::metadata(&head_path)?.modified()?;

    thread::sleep(Duration::from_secs(1));
    let output = run_skill(&["sync"], &skills_home, None)?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "sync failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let second_head = fs::read_to_string(&head_path)?;
    let second_meta = fs::metadata(&head_path)?.modified()?;

    assert_eq!(first_head, second_head);
    assert_eq!(first_meta, second_meta);

    Ok(())
}

#[test]
fn when_syncing_registry_after_update_should_refresh_head() -> Result<()> {
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
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "sync failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let head_path = skills_home
        .join("registry")
        .join(&fixture.registry_id)
        .join("head.txt");
    let first_head = fs::read_to_string(&head_path)?;

    fixture.add_skill_entry("new-skill", "New registry entry")?;
    let output = run_skill(&["sync"], &skills_home, None)?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "sync failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let second_head = fs::read_to_string(&head_path)?;
    assert_ne!(first_head, second_head);

    Ok(())
}

#[test]
fn when_syncing_with_unknown_registry_should_error() -> Result<()> {
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

    let output = run_skill(&["sync", "--registry", "missing"], &skills_home, None)?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("registry not found"));

    Ok(())
}

#[test]
fn when_syncing_specific_registry_should_only_sync_selected() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    let fixture_a = RegistryFixture::build(temp.path(), "a", true)?;
    let fixture_b = RegistryFixture::build(temp.path(), "b", true)?;

    write_config(
        &skills_home,
        vec![
            RegistryConfig {
                id: fixture_a.registry_id.clone(),
                url: fixture_a.url.clone(),
            },
            RegistryConfig {
                id: fixture_b.registry_id.clone(),
                url: fixture_b.url.clone(),
            },
        ],
    )?;

    let output = run_skill(
        &["sync", "--registry", &fixture_a.registry_id],
        &skills_home,
        None,
    )?;
    assert!(output.status.success());

    let registry_a = skills_home
        .join("registry")
        .join(&fixture_a.registry_id)
        .join("index.sqlite");
    let registry_b = skills_home
        .join("registry")
        .join(&fixture_b.registry_id)
        .join("index.sqlite");

    assert!(registry_a.exists());
    assert!(!registry_b.exists());

    Ok(())
}

#[test]
fn when_syncing_registry_missing_skills_dir_should_error() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    let fixture = RegistryFixture::build(temp.path(), "main", false)?;

    write_config(
        &skills_home,
        vec![RegistryConfig {
            id: fixture.registry_id.clone(),
            url: fixture.url.clone(),
        }],
    )?;

    let output = run_skill(&["sync"], &skills_home, None)?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing skills/ directory"));

    Ok(())
}
