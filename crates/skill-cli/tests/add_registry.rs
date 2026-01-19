mod support;

use anyhow::Result;
use std::fs;

use support::{RegistryFixture, run_skill};

use skill_core::config::Config;

#[test]
fn when_adding_registry_should_write_config_and_index() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    let fixture = RegistryFixture::build(temp.path(), "main", true)?;

    let output = run_skill(&["add-registry", &fixture.url], &skills_home, None)?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "add-registry failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let config_path = skills_home.join("config.json");
    let data = fs::read_to_string(&config_path)?;
    let config: Config = serde_json::from_str(&data)?;
    assert_eq!(config.registries.len(), 1);
    let registry = &config.registries[0];
    assert_eq!(registry.id, fixture.registry_id);
    assert_eq!(registry.url, fixture.url);

    let registry_dir = skills_home.join("registry").join(&fixture.registry_id);
    assert!(registry_dir.join("repo").join(".git").exists());
    assert!(registry_dir.join("index.sqlite").exists());
    assert!(registry_dir.join("head.txt").exists());

    Ok(())
}

#[test]
fn when_adding_registry_twice_should_not_duplicate() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    let fixture = RegistryFixture::build(temp.path(), "main", true)?;

    let output = run_skill(&["add-registry", &fixture.url], &skills_home, None)?;
    assert!(output.status.success());
    let output = run_skill(&["add-registry", &fixture.url], &skills_home, None)?;
    assert!(output.status.success());

    let config_path = skills_home.join("config.json");
    let data = fs::read_to_string(&config_path)?;
    let config: Config = serde_json::from_str(&data)?;
    assert_eq!(config.registries.len(), 1);

    Ok(())
}

#[test]
fn when_adding_registry_missing_skills_dir_should_error() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    let fixture = RegistryFixture::build(temp.path(), "main", false)?;

    let output = run_skill(&["add-registry", &fixture.url], &skills_home, None)?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing skills/ directory"));

    Ok(())
}
