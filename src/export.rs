use anyhow::{Result, anyhow};
use std::env;
use std::path::PathBuf;

use crate::lockfile;
use crate::output::Output;
use crate::paths::Paths;
use crate::util::{copy_dir_recursive, ensure_dir, remove_dir_if_exists};

pub fn export_skills(
    paths: &Paths,
    target: &str,
    scope: Option<&str>,
    output: &mut impl Output,
) -> Result<()> {
    match target {
        "codex" => export_codex(paths, scope.unwrap_or("user"), output),
        _ => Err(anyhow!("unsupported export target: {}", target)),
    }
}

fn export_codex(paths: &Paths, scope: &str, output: &mut impl Output) -> Result<()> {
    let dest_base = match scope {
        "user" => {
            let home = env::var("HOME").map_err(|_| anyhow!("HOME is not set"))?;
            PathBuf::from(home).join(".codex").join("skills")
        }
        "repo" => env::current_dir()?.join(".codex").join("skills"),
        _ => return Err(anyhow!("invalid scope: {} (use user or repo)", scope)),
    };

    ensure_dir(&dest_base)?;

    let lockfile = lockfile::load(paths)?;
    if lockfile.skills.is_empty() {
        output.line("No installed skills to export")?;
        return Ok(());
    }

    for entry in lockfile.skills {
        let src = PathBuf::from(&entry.install_dir);
        let dest = dest_base.join(&entry.namespace).join(&entry.name);
        remove_dir_if_exists(&dest)?;
        copy_dir_recursive(&src, &dest)?;
    }

    output.line(format!("Exported skills to {}", dest_base.display()))?;
    Ok(())
}
