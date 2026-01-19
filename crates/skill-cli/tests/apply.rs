mod support;

use anyhow::Result;
use std::fs;
use std::path::Path;

use support::{Playground, run_skill, run_skill_in_dir};

use skill_core::lockfile::{LockedSkill, Lockfile};

fn seed_install(paths: &Path, namespace: &str, name: &str, source: &Path) -> Result<()> {
    let install_dir = paths
        .join("installed")
        .join(namespace)
        .join(name)
        .join("latest");
    fs::create_dir_all(&install_dir)?;
    fs::copy(source, install_dir.join("SKILL.md"))?;
    let lockfile = Lockfile {
        skills: vec![LockedSkill {
            namespace: namespace.to_string(),
            name: name.to_string(),
            requested: "@latest".to_string(),
            resolved_version: None,
            resolved_commit: "deadbeef".to_string(),
            repo_url: "file:///tmp/skills-repo".to_string(),
            path: format!("skills/{name}"),
            install_dir: install_dir.to_string_lossy().to_string(),
            registry_id: None,
        }],
    };
    fs::write(
        paths.join("lock.json"),
        serde_json::to_string_pretty(&lockfile)?,
    )?;
    Ok(())
}

fn assert_symlink(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    assert!(metadata.file_type().is_symlink());
    Ok(())
}

#[test]
fn when_applying_with_no_installed_skills_should_report() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;

    let repo_root = temp.path().join("repo");
    fs::create_dir_all(&repo_root)?;
    let output = run_skill_in_dir(&["apply", "--no-tui"], &skills_home, &repo_root)?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "apply failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No installed skills to apply"));

    Ok(())
}

#[test]
fn when_applying_without_args_should_error() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;
    let skill_md = temp.path().join("SKILL.md");
    fs::write(&skill_md, "---\nname: echo-skill\ndescription: test\n---\n")?;
    seed_install(&skills_home, "acme", "echo-skill", &skill_md)?;

    let repo_root = temp.path().join("repo");
    fs::create_dir_all(&repo_root)?;
    let output = run_skill_in_dir(&["apply", "--no-tui"], &skills_home, &repo_root)?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no targets provided"));

    Ok(())
}

#[test]
fn when_applying_non_interactive_should_link_skill() -> Result<()> {
    let playground = Playground::new()?;

    let output = run_skill(&["sync"], &playground.skills_home, None)?;
    assert!(output.status.success());
    let output = run_skill(
        &["install", "acme/echo-skill"],
        &playground.skills_home,
        None,
    )?;
    assert!(output.status.success());

    let project_dir = playground.root().join("project");
    fs::create_dir_all(project_dir.join(".claude/skills"))?;

    let output = run_skill(
        &[
            "apply",
            "--no-tui",
            "--targets",
            "claude:project",
            "--skills",
            "acme/echo-skill",
        ],
        &playground.skills_home,
        Some(&project_dir),
    )?;
    assert!(output.status.success());

    let applied_dir = project_dir
        .join(".claude/skills")
        .join("acme")
        .join("echo-skill");
    assert!(applied_dir.join("SKILL.md").exists());
    assert_symlink(&applied_dir)?;

    Ok(())
}

#[test]
fn when_applying_all_targets_should_link_to_all_agents() -> Result<()> {
    let playground = Playground::new()?;
    let home_dir = playground.root().join("home");
    fs::create_dir_all(&home_dir)?;

    let output = run_skill(&["sync"], &playground.skills_home, None)?;
    assert!(output.status.success());
    let output = run_skill(
        &["install", "acme/echo-skill"],
        &playground.skills_home,
        None,
    )?;
    assert!(output.status.success());

    let project_dir = playground.root().join("project-all");
    fs::create_dir_all(&project_dir)?;

    let output = support::run_skill_with_env(
        &[
            "apply",
            "--no-tui",
            "--all-targets",
            "--skills",
            "acme/echo-skill",
        ],
        &playground.skills_home,
        Some(&project_dir),
        &[("HOME".to_string(), home_dir.to_string_lossy().to_string())],
    )?;
    assert!(output.status.success());

    let targets = [
        (".claude/skills", ".claude/skills"),
        (".cursor/skills", ".cursor/skills"),
        (".vscode/skills", ".vscode/skills"),
        (".copilot/skills", ".copilot/skills"),
        (".config/goose/skills", ".goose/skills"),
        (".config/opencode/skill", ".opencode/skill"),
        (".codex/skills", ".codex/skills"),
    ];

    for (global_rel, project_rel) in targets {
        let global_dest = home_dir
            .join(global_rel)
            .join("acme")
            .join("echo-skill")
            .join("SKILL.md");
        assert!(
            global_dest.exists(),
            "missing global {}",
            global_dest.display()
        );
        assert_symlink(&home_dir.join(global_rel).join("acme").join("echo-skill"))?;

        let project_dest = project_dir
            .join(project_rel)
            .join("acme")
            .join("echo-skill")
            .join("SKILL.md");
        assert!(
            project_dest.exists(),
            "missing project {}",
            project_dest.display()
        );
        assert_symlink(
            &project_dir
                .join(project_rel)
                .join("acme")
                .join("echo-skill"),
        )?;
    }

    Ok(())
}

#[test]
fn when_unapplying_should_remove_skill_from_target() -> Result<()> {
    let playground = Playground::new()?;

    let output = run_skill(&["sync"], &playground.skills_home, None)?;
    assert!(output.status.success());
    let output = run_skill(
        &["install", "acme/echo-skill"],
        &playground.skills_home,
        None,
    )?;
    assert!(output.status.success());

    let project_dir = playground.root().join("project-unapply");
    fs::create_dir_all(project_dir.join(".claude/skills"))?;

    let output = run_skill(
        &[
            "apply",
            "--no-tui",
            "--targets",
            "claude:project",
            "--skills",
            "acme/echo-skill",
        ],
        &playground.skills_home,
        Some(&project_dir),
    )?;
    assert!(output.status.success());

    let applied_dir = project_dir
        .join(".claude/skills")
        .join("acme")
        .join("echo-skill");
    assert!(applied_dir.join("SKILL.md").exists());
    assert_symlink(&applied_dir)?;

    let output = run_skill(
        &[
            "apply",
            "--no-tui",
            "--unapply",
            "--targets",
            "claude:project",
            "--skills",
            "acme/echo-skill",
        ],
        &playground.skills_home,
        Some(&project_dir),
    )?;
    assert!(output.status.success());

    assert!(!applied_dir.exists());

    Ok(())
}

#[test]
fn when_unapplying_all_targets_should_remove_from_all_agents() -> Result<()> {
    let playground = Playground::new()?;
    let home_dir = playground.root().join("home-unapply");
    fs::create_dir_all(&home_dir)?;

    let output = run_skill(&["sync"], &playground.skills_home, None)?;
    assert!(output.status.success());
    let output = run_skill(
        &["install", "acme/echo-skill"],
        &playground.skills_home,
        None,
    )?;
    assert!(output.status.success());

    let project_dir = playground.root().join("project-unapply-all");
    fs::create_dir_all(&project_dir)?;

    let output = support::run_skill_with_env(
        &[
            "apply",
            "--no-tui",
            "--all-targets",
            "--skills",
            "acme/echo-skill",
        ],
        &playground.skills_home,
        Some(&project_dir),
        &[("HOME".to_string(), home_dir.to_string_lossy().to_string())],
    )?;
    assert!(output.status.success());

    let output = support::run_skill_with_env(
        &[
            "apply",
            "--no-tui",
            "--unapply",
            "--all-targets",
            "--skills",
            "acme/echo-skill",
        ],
        &playground.skills_home,
        Some(&project_dir),
        &[("HOME".to_string(), home_dir.to_string_lossy().to_string())],
    )?;
    assert!(output.status.success());

    let targets = [
        (".claude/skills", ".claude/skills"),
        (".cursor/skills", ".cursor/skills"),
        (".vscode/skills", ".vscode/skills"),
        (".copilot/skills", ".copilot/skills"),
        (".config/goose/skills", ".goose/skills"),
        (".config/opencode/skill", ".opencode/skill"),
        (".codex/skills", ".codex/skills"),
    ];

    for (global_rel, project_rel) in targets {
        let global_dest = home_dir.join(global_rel).join("acme").join("echo-skill");
        assert!(
            !global_dest.exists(),
            "global still exists {}",
            global_dest.display()
        );

        let project_dest = project_dir
            .join(project_rel)
            .join("acme")
            .join("echo-skill");
        assert!(
            !project_dest.exists(),
            "project still exists {}",
            project_dest.display()
        );
    }

    Ok(())
}

#[test]
fn when_unapplying_global_target_should_remove_global_only() -> Result<()> {
    let playground = Playground::new()?;
    let home_dir = playground.root().join("home-unapply-global");
    fs::create_dir_all(&home_dir)?;

    let output = run_skill(&["sync"], &playground.skills_home, None)?;
    assert!(output.status.success());
    let output = run_skill(
        &["install", "acme/echo-skill"],
        &playground.skills_home,
        None,
    )?;
    assert!(output.status.success());

    let project_dir = playground.root().join("project-unapply-global");
    fs::create_dir_all(&project_dir)?;

    let output = support::run_skill_with_env(
        &[
            "apply",
            "--no-tui",
            "--targets",
            "claude:global",
            "--skills",
            "acme/echo-skill",
        ],
        &playground.skills_home,
        Some(&project_dir),
        &[("HOME".to_string(), home_dir.to_string_lossy().to_string())],
    )?;
    assert!(output.status.success());

    let global_dest = home_dir
        .join(".claude/skills")
        .join("acme")
        .join("echo-skill");
    assert!(global_dest.join("SKILL.md").exists());
    assert_symlink(&global_dest)?;

    let project_dest = project_dir
        .join(".claude/skills")
        .join("acme")
        .join("echo-skill");
    assert!(!project_dest.exists());

    let output = support::run_skill_with_env(
        &[
            "apply",
            "--no-tui",
            "--unapply",
            "--targets",
            "claude:global",
            "--skills",
            "acme/echo-skill",
        ],
        &playground.skills_home,
        Some(&project_dir),
        &[("HOME".to_string(), home_dir.to_string_lossy().to_string())],
    )?;
    assert!(output.status.success());

    assert!(!global_dest.exists());
    assert!(!project_dest.exists());

    Ok(())
}

#[test]
fn when_applying_across_projects_should_keep_each_project_isolated() -> Result<()> {
    let playground = Playground::new()?;
    let home_dir = playground.root().join("home-isolation");
    fs::create_dir_all(&home_dir)?;

    let output = run_skill(&["sync"], &playground.skills_home, None)?;
    assert!(output.status.success());
    let output = run_skill(
        &["install", "acme/echo-skill"],
        &playground.skills_home,
        None,
    )?;
    assert!(output.status.success());

    let project_a = playground.root().join("project-a");
    let project_b = playground.root().join("project-b");
    fs::create_dir_all(&project_a)?;
    fs::create_dir_all(&project_b)?;

    let output = support::run_skill_with_env(
        &[
            "apply",
            "--no-tui",
            "--targets",
            "claude:project",
            "--skills",
            "acme/echo-skill",
        ],
        &playground.skills_home,
        Some(&project_a),
        &[("HOME".to_string(), home_dir.to_string_lossy().to_string())],
    )?;
    assert!(output.status.success());

    let output = support::run_skill_with_env(
        &[
            "apply",
            "--no-tui",
            "--targets",
            "claude:project",
            "--skills",
            "acme/echo-skill",
        ],
        &playground.skills_home,
        Some(&project_b),
        &[("HOME".to_string(), home_dir.to_string_lossy().to_string())],
    )?;
    assert!(output.status.success());

    let project_a_dest = project_a
        .join(".claude/skills")
        .join("acme")
        .join("echo-skill");
    let project_b_dest = project_b
        .join(".claude/skills")
        .join("acme")
        .join("echo-skill");
    assert!(project_a_dest.join("SKILL.md").exists());
    assert!(project_b_dest.join("SKILL.md").exists());
    assert_symlink(&project_a_dest)?;
    assert_symlink(&project_b_dest)?;

    let output = support::run_skill_with_env(
        &[
            "apply",
            "--no-tui",
            "--unapply",
            "--targets",
            "claude:project",
            "--skills",
            "acme/echo-skill",
        ],
        &playground.skills_home,
        Some(&project_b),
        &[("HOME".to_string(), home_dir.to_string_lossy().to_string())],
    )?;
    assert!(output.status.success());

    assert!(project_a_dest.exists());
    assert!(!project_b_dest.exists());

    Ok(())
}

#[test]
fn when_applying_project_target_without_markers_should_still_apply() -> Result<()> {
    let playground = Playground::new()?;

    let output = run_skill(&["sync"], &playground.skills_home, None)?;
    assert!(output.status.success());
    let output = run_skill(
        &["install", "acme/echo-skill"],
        &playground.skills_home,
        None,
    )?;
    assert!(output.status.success());

    let project_dir = playground.root().join("project-no-markers");
    fs::create_dir_all(&project_dir)?;

    let output = run_skill(
        &[
            "apply",
            "--no-tui",
            "--targets",
            "claude:project",
            "--skills",
            "acme/echo-skill",
        ],
        &playground.skills_home,
        Some(&project_dir),
    )?;
    assert!(output.status.success());

    let applied_dir = project_dir
        .join(".claude/skills")
        .join("acme")
        .join("echo-skill");
    assert!(applied_dir.join("SKILL.md").exists());
    assert_symlink(&applied_dir)?;

    Ok(())
}
