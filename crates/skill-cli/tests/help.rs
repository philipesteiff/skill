mod support;

use anyhow::Result;

use support::run_skill;

#[test]
fn when_showing_apply_help_should_describe_tui_vs_cli_modes() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let output = run_skill(&["apply", "--help"], temp.path(), None)?;
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("In TUI apply mode, selection is desired state"));
    assert!(stdout.contains("In CLI mode (`--no-tui` or explicit `--targets/--skills`)"));
    assert!(stdout.contains("other applied skills are left unchanged"));

    Ok(())
}
