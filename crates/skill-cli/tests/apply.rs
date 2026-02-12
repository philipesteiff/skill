mod support;

use anyhow::Result;
use std::fs;
use std::path::Path;
use std::process::Command;

use support::{run_skill, run_skill_in_dir};

use skill_core::lockfile::{LockedSkill, Lockfile};

fn run_git<I, S>(args: I, cwd: &Path) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new("git").args(args).current_dir(cwd).output()?;
    if output.status.success() {
        return Ok(());
    }
    Err(anyhow::anyhow!(
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn seed_install(paths: &Path, source_id: &str, name: &str, source: &Path) -> Result<()> {
    let install_dir = paths
        .join("installed")
        .join(source_id)
        .join(name)
        .join("latest");
    fs::create_dir_all(&install_dir)?;
    fs::copy(source, install_dir.join("SKILL.md"))?;
    let lockfile = Lockfile {
        skills: vec![LockedSkill {
            source_id: source_id.to_string(),
            name: name.to_string(),
            resolved_version: None,
            resolved_commit: "deadbeef".to_string(),
            content_hash: None,
            path: format!("skills/{name}"),
            install_dir: install_dir.to_string_lossy().to_string(),
            updated_at: None,
        }],
    };
    fs::write(
        paths.join("lock.json"),
        serde_json::to_string_pretty(&lockfile)?,
    )?;
    Ok(())
}

fn assert_applied_dir(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    assert!(!metadata.file_type().is_symlink());
    assert!(path.is_dir());
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
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;
    let skill_md = temp.path().join("SKILL.md");
    fs::write(&skill_md, "---\nname: echo-skill\ndescription: test\n---\n")?;
    seed_install(&skills_home, "acme", "echo-skill", &skill_md)?;

    let project_dir = temp.path().join("project");
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
        &skills_home,
        Some(&project_dir),
    )?;
    assert!(output.status.success());

    let applied_dir = project_dir.join(".claude/skills").join("acme__echo-skill");
    assert!(applied_dir.join("SKILL.md").exists());
    assert_applied_dir(&applied_dir)?;

    Ok(())
}

#[test]
fn when_applying_all_targets_should_link_to_all_agents() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;
    let skill_md = temp.path().join("SKILL.md");
    fs::write(&skill_md, "---\nname: echo-skill\ndescription: test\n---\n")?;
    seed_install(&skills_home, "acme", "echo-skill", &skill_md)?;

    let home_dir = temp.path().join("home");
    fs::create_dir_all(&home_dir)?;

    let project_dir = temp.path().join("project-all");
    fs::create_dir_all(&project_dir)?;

    let output = support::run_skill_with_env(
        &[
            "apply",
            "--no-tui",
            "--all-targets",
            "--skills",
            "acme/echo-skill",
        ],
        &skills_home,
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
            .join("acme__echo-skill")
            .join("SKILL.md");
        assert!(
            global_dest.exists(),
            "missing global {}",
            global_dest.display()
        );
        assert_applied_dir(&home_dir.join(global_rel).join("acme__echo-skill"))?;

        let project_dest = project_dir
            .join(project_rel)
            .join("acme__echo-skill")
            .join("SKILL.md");
        assert!(
            project_dest.exists(),
            "missing project {}",
            project_dest.display()
        );
        assert_applied_dir(&project_dir.join(project_rel).join("acme__echo-skill"))?;
    }

    Ok(())
}

#[test]
fn when_unapplying_should_remove_skill_from_target() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;
    let skill_md = temp.path().join("SKILL.md");
    fs::write(&skill_md, "---\nname: echo-skill\ndescription: test\n---\n")?;
    seed_install(&skills_home, "acme", "echo-skill", &skill_md)?;

    let project_dir = temp.path().join("project-unapply");
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
        &skills_home,
        Some(&project_dir),
    )?;
    assert!(output.status.success());

    let applied_dir = project_dir.join(".claude/skills").join("acme__echo-skill");
    assert!(applied_dir.join("SKILL.md").exists());
    assert_applied_dir(&applied_dir)?;

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
        &skills_home,
        Some(&project_dir),
    )?;
    assert!(output.status.success());

    assert!(!applied_dir.exists());

    Ok(())
}

#[test]
fn when_unapplying_should_remove_managed_tracking_entry() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;
    let skill_md = temp.path().join("SKILL.md");
    fs::write(&skill_md, "---\nname: echo-skill\ndescription: test\n---\n")?;
    seed_install(&skills_home, "acme", "echo-skill", &skill_md)?;

    let project_dir = temp.path().join("project-unapply-managed");
    fs::create_dir_all(project_dir.join(".claude/skills"))?;
    run_git(["init", "-q"], &project_dir)?;
    run_git(["config", "user.email", "test@example.com"], &project_dir)?;
    run_git(["config", "user.name", "Test"], &project_dir)?;

    let output = run_skill(
        &[
            "apply",
            "--no-tui",
            "--targets",
            "claude:project",
            "--skills",
            "acme/echo-skill",
        ],
        &skills_home,
        Some(&project_dir),
    )?;
    assert!(output.status.success());

    let exclude_path = project_dir.join(".git/info/exclude");
    fs::write(
        &exclude_path,
        [
            "# File patterns to ignore; see `git help ignore` for more information.",
            "",
            "# >>> skill managed git tracking >>>",
            ".claude/skills/acme__echo-skill",
            "# <<< skill managed git tracking <<<",
            "",
        ]
        .join("\n"),
    )?;

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
        &skills_home,
        Some(&project_dir),
    )?;
    assert!(output.status.success());

    let exclude = fs::read_to_string(&exclude_path)?;
    assert!(!exclude.contains(".claude/skills/acme__echo-skill"));

    Ok(())
}

#[test]
fn when_unapplying_all_targets_should_remove_from_all_agents() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;
    let skill_md = temp.path().join("SKILL.md");
    fs::write(&skill_md, "---\nname: echo-skill\ndescription: test\n---\n")?;
    seed_install(&skills_home, "acme", "echo-skill", &skill_md)?;

    let home_dir = temp.path().join("home-unapply");
    fs::create_dir_all(&home_dir)?;

    let project_dir = temp.path().join("project-unapply-all");
    fs::create_dir_all(&project_dir)?;

    let output = support::run_skill_with_env(
        &[
            "apply",
            "--no-tui",
            "--all-targets",
            "--skills",
            "acme/echo-skill",
        ],
        &skills_home,
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
        &skills_home,
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
        let global_dest = home_dir.join(global_rel).join("acme__echo-skill");
        assert!(
            !global_dest.exists(),
            "global still exists {}",
            global_dest.display()
        );

        let project_dest = project_dir.join(project_rel).join("acme__echo-skill");
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
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;
    let skill_md = temp.path().join("SKILL.md");
    fs::write(&skill_md, "---\nname: echo-skill\ndescription: test\n---\n")?;
    seed_install(&skills_home, "acme", "echo-skill", &skill_md)?;

    let home_dir = temp.path().join("home-unapply-global");
    fs::create_dir_all(&home_dir)?;

    let project_dir = temp.path().join("project-unapply-global");
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
        &skills_home,
        Some(&project_dir),
        &[("HOME".to_string(), home_dir.to_string_lossy().to_string())],
    )?;
    assert!(output.status.success());

    let global_dest = home_dir.join(".claude/skills").join("acme__echo-skill");
    assert!(global_dest.join("SKILL.md").exists());
    assert_applied_dir(&global_dest)?;

    let project_dest = project_dir.join(".claude/skills").join("acme__echo-skill");
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
        &skills_home,
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
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;
    let skill_md = temp.path().join("SKILL.md");
    fs::write(&skill_md, "---\nname: echo-skill\ndescription: test\n---\n")?;
    seed_install(&skills_home, "acme", "echo-skill", &skill_md)?;

    let home_dir = temp.path().join("home-isolation");
    fs::create_dir_all(&home_dir)?;

    let project_a = temp.path().join("project-a");
    let project_b = temp.path().join("project-b");
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
        &skills_home,
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
        &skills_home,
        Some(&project_b),
        &[("HOME".to_string(), home_dir.to_string_lossy().to_string())],
    )?;
    assert!(output.status.success());

    let project_a_dest = project_a.join(".claude/skills").join("acme__echo-skill");
    let project_b_dest = project_b.join(".claude/skills").join("acme__echo-skill");
    assert!(project_a_dest.join("SKILL.md").exists());
    assert!(project_b_dest.join("SKILL.md").exists());
    assert_applied_dir(&project_a_dest)?;
    assert_applied_dir(&project_b_dest)?;

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
        &skills_home,
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
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;
    let skill_md = temp.path().join("SKILL.md");
    fs::write(&skill_md, "---\nname: echo-skill\ndescription: test\n---\n")?;
    seed_install(&skills_home, "acme", "echo-skill", &skill_md)?;

    let project_dir = temp.path().join("project-no-markers");
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
        &skills_home,
        Some(&project_dir),
    )?;
    assert!(output.status.success());

    let applied_dir = project_dir.join(".claude/skills").join("acme__echo-skill");
    assert!(applied_dir.join("SKILL.md").exists());
    assert_applied_dir(&applied_dir)?;

    Ok(())
}

#[test]
fn when_reapplying_should_report_skipped_action() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;
    let skill_md = temp.path().join("SKILL.md");
    fs::write(&skill_md, "---\nname: echo-skill\ndescription: test\n---\n")?;
    seed_install(&skills_home, "acme", "echo-skill", &skill_md)?;

    let project_dir = temp.path().join("project-reapply-skipped");
    fs::create_dir_all(&project_dir)?;

    let first = run_skill(
        &[
            "apply",
            "--no-tui",
            "--targets",
            "claude:project",
            "--skills",
            "acme/echo-skill",
        ],
        &skills_home,
        Some(&project_dir),
    )?;
    assert!(first.status.success());

    let second = run_skill(
        &[
            "apply",
            "--no-tui",
            "--targets",
            "claude:project",
            "--skills",
            "acme/echo-skill",
        ],
        &skills_home,
        Some(&project_dir),
    )?;
    assert!(second.status.success());
    let stdout = String::from_utf8_lossy(&second.stdout);
    assert!(stdout.contains("Skipped:"));
    assert!(stdout.contains("acme/echo-skill on Claude Code project"));

    Ok(())
}

#[cfg(unix)]
#[test]
fn when_git_tracking_update_fails_should_return_non_zero() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;
    let skill_md = temp.path().join("SKILL.md");
    fs::write(&skill_md, "---\nname: echo-skill\ndescription: test\n---\n")?;
    seed_install(&skills_home, "acme", "echo-skill", &skill_md)?;

    let project_dir = temp.path().join("project-tracking-failure");
    fs::create_dir_all(&project_dir)?;
    run_git(["init", "-q"], &project_dir)?;
    run_git(["config", "user.email", "test@example.com"], &project_dir)?;
    run_git(["config", "user.name", "Test"], &project_dir)?;

    let exclude_path = project_dir.join(".git/info/exclude");
    fs::write(&exclude_path, "# initial\n")?;
    let mut readonly = fs::metadata(&exclude_path)?.permissions();
    readonly.set_mode(0o444);
    fs::set_permissions(&exclude_path, readonly)?;

    let output = run_skill(
        &[
            "apply",
            "--no-tui",
            "--targets",
            "claude:project",
            "--skills",
            "acme/echo-skill",
        ],
        &skills_home,
        Some(&project_dir),
    )?;
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Git Tracking Failed"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("tracking failure(s)"));

    Ok(())
}

#[test]
fn when_reapplying_after_managed_drift_should_refresh_content() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;
    let skill_md = temp.path().join("SKILL.md");
    fs::write(
        &skill_md,
        "---\nname: echo-skill\ndescription: original content\n---\n",
    )?;
    seed_install(&skills_home, "acme", "echo-skill", &skill_md)?;

    let project_dir = temp.path().join("project-drift");
    fs::create_dir_all(&project_dir)?;

    let first = run_skill(
        &[
            "apply",
            "--no-tui",
            "--targets",
            "claude:project",
            "--skills",
            "acme/echo-skill",
        ],
        &skills_home,
        Some(&project_dir),
    )?;
    assert!(first.status.success());

    let applied_skill = project_dir
        .join(".claude/skills")
        .join("acme__echo-skill")
        .join("SKILL.md");
    fs::write(
        &applied_skill,
        "---\nname: echo-skill\ndescription: tampered\n---\n",
    )?;

    let second = run_skill(
        &[
            "apply",
            "--no-tui",
            "--targets",
            "claude:project",
            "--skills",
            "acme/echo-skill",
        ],
        &skills_home,
        Some(&project_dir),
    )?;
    assert!(second.status.success());
    let stdout = String::from_utf8_lossy(&second.stdout);
    assert!(stdout.contains("Added:"));
    assert!(!stdout.contains("[=] acme/echo-skill on Claude Code project"));

    let restored = fs::read_to_string(&applied_skill)?;
    assert!(restored.contains("original content"));
    assert!(!restored.contains("tampered"));

    Ok(())
}

#[test]
fn when_applying_over_unmanaged_directory_should_report_failure() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;
    let skill_md = temp.path().join("SKILL.md");
    fs::write(&skill_md, "---\nname: echo-skill\ndescription: test\n---\n")?;
    seed_install(&skills_home, "acme", "echo-skill", &skill_md)?;

    let project_dir = temp.path().join("project-unmanaged");
    fs::create_dir_all(project_dir.join(".claude/skills"))?;
    let unmanaged_dir = project_dir.join(".claude/skills").join("acme__echo-skill");
    fs::create_dir_all(&unmanaged_dir)?;
    fs::write(unmanaged_dir.join("SKILL.md"), "unmanaged")?;

    let output = run_skill(
        &[
            "apply",
            "--no-tui",
            "--targets",
            "claude:project",
            "--skills",
            "acme/echo-skill",
        ],
        &skills_home,
        Some(&project_dir),
    )?;
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("destination exists and is unmanaged"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("failed action(s)"));

    Ok(())
}

#[cfg(unix)]
#[test]
fn when_applying_over_symlink_should_report_migration_hint() -> Result<()> {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir()?;
    let skills_home = temp.path().join("skills-home");
    fs::create_dir_all(&skills_home)?;
    let skill_md = temp.path().join("SKILL.md");
    fs::write(&skill_md, "---\nname: echo-skill\ndescription: test\n---\n")?;
    seed_install(&skills_home, "acme", "echo-skill", &skill_md)?;

    let project_dir = temp.path().join("project-symlink");
    fs::create_dir_all(project_dir.join(".claude/skills"))?;
    let target_dir = project_dir.join(".claude/skills").join("acme__echo-skill");
    let other_dir = temp.path().join("other");
    fs::create_dir_all(&other_dir)?;
    symlink(&other_dir, &target_dir)?;

    let output = run_skill(
        &[
            "apply",
            "--no-tui",
            "--targets",
            "claude:project",
            "--skills",
            "acme/echo-skill",
        ],
        &skills_home,
        Some(&project_dir),
    )?;
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("destination is a symlink"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("failed action(s)"));

    Ok(())
}
