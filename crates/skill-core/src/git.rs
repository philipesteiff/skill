use anyhow::{Context, Result, anyhow};
use std::ffi::OsStr;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn run_git<I, S>(args: I, cwd: Option<&Path>) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.env("GIT_ASKPASS", "true");
    let output = cmd.output().context("failed to run git")?;
    if !output.status.success() {
        return Err(anyhow!(
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn clone_repo(url: &str, dest: &Path) -> Result<()> {
    run_git(
        ["clone", "--depth", "1", url, dest.to_str().unwrap_or(".")],
        None,
    )?;
    Ok(())
}

pub fn fetch_repo(repo_dir: &Path) -> Result<()> {
    run_git(
        [
            "-C",
            repo_dir.to_str().unwrap_or("."),
            "fetch",
            "--depth",
            "1",
            "--update-shallow",
            "origin",
        ],
        None,
    )?;
    run_git(
        [
            "-C",
            repo_dir.to_str().unwrap_or("."),
            "reset",
            "--hard",
            "FETCH_HEAD",
        ],
        None,
    )?;
    Ok(())
}

pub fn repo_head(repo_dir: &Path) -> Result<String> {
    run_git(
        ["-C", repo_dir.to_str().unwrap_or("."), "rev-parse", "HEAD"],
        None,
    )
}

pub fn remote_url(repo_dir: &Path) -> Result<String> {
    run_git(
        [
            "-C",
            repo_dir.to_str().unwrap_or("."),
            "config",
            "--get",
            "remote.origin.url",
        ],
        None,
    )
}

pub fn ls_remote_head(repo_url: &str) -> Result<String> {
    let output = run_git(["ls-remote", repo_url, "HEAD"], None)?;
    let mut parts = output.split_whitespace();
    parts
        .next()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("unable to parse ls-remote output"))
}

pub fn ensure_mirror(repo_url: &str, mirror_path: &Path) -> Result<()> {
    if mirror_path.exists() {
        return Ok(());
    }
    if let Some(parent) = mirror_path.parent() {
        fs::create_dir_all(parent)?;
    }
    run_git(
        ["init", "--bare", mirror_path.to_str().unwrap_or(".")],
        None,
    )?;
    run_git(
        [
            "-C",
            mirror_path.to_str().unwrap_or("."),
            "remote",
            "add",
            "origin",
            repo_url,
        ],
        None,
    )?;
    Ok(())
}

pub fn fetch_commit(mirror_path: &Path, commit: &str) -> Result<()> {
    run_git(
        [
            "-C",
            mirror_path.to_str().unwrap_or("."),
            "fetch",
            "--depth",
            "1",
            "origin",
            commit,
        ],
        None,
    )?;
    Ok(())
}

pub fn list_files(mirror_path: &Path, commit: &str) -> Result<Vec<String>> {
    let output = run_git(
        [
            "-C",
            mirror_path.to_str().unwrap_or("."),
            "ls-tree",
            "-r",
            "--name-only",
            commit,
        ],
        None,
    )?;
    Ok(output
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect())
}

pub fn show_file(mirror_path: &Path, commit: &str, path: &str) -> Result<String> {
    let output = Command::new("git")
        .args([
            "-C",
            mirror_path.to_str().unwrap_or("."),
            "show",
            &format!("{}:{}", commit, path),
        ])
        .output()
        .context("failed to run git show")?;
    if !output.status.success() {
        return Err(anyhow!(
            "git show failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8(output.stdout)?)
}

pub fn commit_date_short(mirror_path: &Path, commit: &str) -> Result<String> {
    let output = run_git(
        [
            "-C",
            mirror_path.to_str().unwrap_or("."),
            "log",
            "-1",
            "--format=%ad",
            "--date=short",
            commit,
        ],
        None,
    )?;
    Ok(output)
}

pub fn last_commit_and_date(
    mirror_path: &Path,
    commit: &str,
    path: &str,
) -> Result<(String, String)> {
    let output = run_git(
        [
            "-C",
            mirror_path.to_str().unwrap_or("."),
            "log",
            "-1",
            "--format=%H|%ad",
            "--date=short",
            commit,
            "--",
            path,
        ],
        None,
    )?;
    let mut parts = output.split('|');
    let commit = parts
        .next()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("unable to parse log output"))?;
    let date = parts
        .next()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("unable to parse log date output"))?;
    Ok((commit, date))
}

pub fn archive_path(mirror_path: &Path, commit: &str, path: &str, dest_dir: &Path) -> Result<()> {
    let output = Command::new("git")
        .args([
            "-C",
            mirror_path.to_str().unwrap_or("."),
            "archive",
            "--format=tar",
            commit,
            path,
        ])
        .output()
        .context("failed to run git archive")?;
    if !output.status.success() {
        return Err(anyhow!(
            "git archive failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let mut archive = tar::Archive::new(Cursor::new(output.stdout));
    archive.unpack(dest_dir)?;
    Ok(())
}

pub fn status_porcelain(repo_dir: &Path) -> Result<String> {
    run_git(
        [
            "-C",
            repo_dir.to_str().unwrap_or("."),
            "status",
            "--porcelain",
        ],
        None,
    )
}

pub fn checkout_new_branch(repo_dir: &Path, branch: &str) -> Result<()> {
    run_git(
        [
            "-C",
            repo_dir.to_str().unwrap_or("."),
            "checkout",
            "-b",
            branch,
        ],
        None,
    )?;
    Ok(())
}

pub fn add_all(repo_dir: &Path) -> Result<()> {
    run_git(["-C", repo_dir.to_str().unwrap_or("."), "add", "-A"], None)?;
    Ok(())
}

pub fn commit(repo_dir: &Path, message: &str) -> Result<()> {
    run_git(
        [
            "-C",
            repo_dir.to_str().unwrap_or("."),
            "commit",
            "-m",
            message,
        ],
        None,
    )?;
    Ok(())
}

pub fn push_branch(repo_dir: &Path, branch: &str) -> Result<()> {
    run_git(
        [
            "-C",
            repo_dir.to_str().unwrap_or("."),
            "push",
            "-u",
            "origin",
            branch,
        ],
        None,
    )?;
    Ok(())
}

pub fn diff(repo_dir: &Path) -> Result<String> {
    run_git(["-C", repo_dir.to_str().unwrap_or("."), "diff"], None)
}

pub fn repo_root(cwd: &Path) -> Result<PathBuf> {
    let output = run_git(
        [
            "-C",
            cwd.to_str().unwrap_or("."),
            "rev-parse",
            "--show-toplevel",
        ],
        None,
    )?;
    Ok(PathBuf::from(output))
}

pub fn default_branch(repo_dir: &Path) -> Result<String> {
    let output = match run_git(
        [
            "-C",
            repo_dir.to_str().unwrap_or("."),
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
        ],
        None,
    ) {
        Ok(output) => output,
        Err(_) => return Ok("main".to_string()),
    };
    if let Some(name) = output.split('/').next_back() {
        return Ok(name.to_string());
    }
    Err(anyhow!("unable to determine default branch"))
}
