use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

pub fn run_skill_with_env(
    args: &[&str],
    skills_home: &Path,
    cwd: Option<&Path>,
    envs: &[(String, String)],
) -> Result<std::process::Output> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_skill"));
    cmd.args(args);
    cmd.env("SKILLS_HOME", skills_home);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    cmd.output().context("run skill")
}
