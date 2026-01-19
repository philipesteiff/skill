use anyhow::{Result, anyhow};
use clap::Args;
use std::env;
use std::path::PathBuf;

use skill_core::lockfile;
use skill_core::output::Output;
use skill_core::paths::Paths;
use skill_core::util::{copy_dir_recursive, ensure_dir, remove_dir_if_exists};

#[derive(Args, Clone, Debug)]
pub struct ExportArgs {
    pub target: String,
    #[arg(long)]
    pub scope: Option<String>,
}

pub fn run(paths: &Paths, args: ExportArgs) -> Result<()> {
    paths.ensure_base_dirs()?;
    let ExportArgs { target, scope } = args;
    let label = scope
        .as_deref()
        .map(|scope| format!("skill export {target} --scope {scope}"))
        .unwrap_or_else(|| format!("skill export {target}"));

    let mut ui = skill_core::ui::log::LogUi::new(label)?;
    let result = export_skills(paths, &target, scope.as_deref(), &mut ui);
    let finish = ui.finish();
    result?;
    finish?;
    Ok(())
}

fn export_skills(
    paths: &Paths,
    target: &str,
    scope: Option<&str>,
    output: &mut impl Output,
) -> Result<()> {
    match target {
        "codex" => export_codex(paths, scope.unwrap_or("user"), output),
        _ => Err(anyhow!("unsupported export target: {target}")),
    }
}

fn export_codex(paths: &Paths, scope: &str, output: &mut impl Output) -> Result<()> {
    let dest_base = match scope {
        "user" => {
            let home = env::var("HOME").map_err(|_| anyhow!("HOME is not set"))?;
            PathBuf::from(home).join(".codex").join("skills")
        }
        "repo" => env::current_dir()?.join(".codex").join("skills"),
        _ => return Err(anyhow!("invalid scope: {scope} (use user or repo)")),
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

    output.line(format!(
        "Exported skills to {dest}",
        dest = dest_base.display()
    ))?;
    Ok(())
}
