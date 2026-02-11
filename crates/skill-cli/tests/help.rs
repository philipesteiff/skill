use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

fn run_skill(args: &[&str], skills_home: &Path) -> Result<std::process::Output> {
    Command::new(env!("CARGO_BIN_EXE_skill"))
        .args(args)
        .env("SKILLS_HOME", skills_home)
        .output()
        .context("run skill")
}

#[test]
fn when_showing_apply_help_should_describe_tui_vs_cli_modes() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let output = run_skill(&["apply", "--help"], temp.path())?;
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("In TUI apply mode, selection is desired state"));
    assert!(stdout.contains("In CLI mode (`--no-tui` or explicit `--targets/--skills`)"));
    assert!(stdout.contains("other applied skills are left unchanged"));

    Ok(())
}

#[test]
fn when_showing_sync_help_should_describe_no_arg_sync_all() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let output = run_skill(&["sync", "--help"], temp.path())?;
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("With no source argument, syncs all configured sources."));
    assert!(stdout.contains("With a source argument, syncs only that source."));
    assert!(stdout.contains("continues across sources and exits non-zero if any source fails."));

    Ok(())
}
