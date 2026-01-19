mod support;

use anyhow::Result;

use support::{RegistryFixture, run_skill, write_config};

use skill_core::config::RegistryConfig;

#[test]
fn when_searching_without_registries_should_error() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");

    let output = run_skill(&["search", "echo"], &skills_home, None)?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no registries configured"));

    Ok(())
}

#[test]
fn when_searching_without_index_should_error() -> Result<()> {
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

    let output = run_skill(&["search", "echo"], &skills_home, None)?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not indexed; run skill sync"));

    Ok(())
}

#[test]
fn when_searching_with_matches_should_print_results() -> Result<()> {
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

    let output = run_skill(&["search", "echo"], &skills_home, None)?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("acme/echo-skill"));
    assert!(stdout.contains("Echo input with basic validation."));

    Ok(())
}

#[test]
fn when_searching_with_no_matches_should_print_message() -> Result<()> {
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

    let output = run_skill(&["search", "doesnotexist"], &skills_home, None)?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "search failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No matches for 'doesnotexist'"));

    Ok(())
}

#[test]
fn when_searching_across_multiple_registries_should_include_results() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    let fixture_a = RegistryFixture::build(temp.path(), "a", true)?;
    let fixture_b = RegistryFixture::build(temp.path(), "b", true)?;
    fixture_b.add_skill_entry("extra-skill", "Extra skill")?;

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
    let output = run_skill(&["sync"], &skills_home, None)?;
    assert!(output.status.success());

    let output = run_skill(&["search", "extra"], &skills_home, None)?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("acme/extra-skill"));

    Ok(())
}

#[test]
fn when_searching_with_fts_phrase_should_match() -> Result<()> {
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

    let output = run_skill(&["search", "\"echo\""], &skills_home, None)?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("acme/echo-skill"));

    Ok(())
}

#[test]
fn when_searching_with_fts_or_should_match_multiple() -> Result<()> {
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

    let output = run_skill(&["search", "echo OR notes"], &skills_home, None)?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("acme/echo-skill"));
    assert!(stdout.contains("acme/notes-skill"));

    Ok(())
}

#[test]
fn when_searching_with_fts_prefix_should_match() -> Result<()> {
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

    let output = run_skill(&["search", "ech*"], &skills_home, None)?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("acme/echo-skill"));

    Ok(())
}

#[test]
fn when_searching_with_invalid_fts_syntax_should_error() -> Result<()> {
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

    let output = run_skill(&["search", "echo NOT"], &skills_home, None)?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("SQL"));

    Ok(())
}
