use anyhow::Result;
use clap::Args;

use skill_core::lockfile;
use skill_core::output::Output;
use skill_core::paths::Paths;
use skill_core::util::short_sha;

#[derive(Args, Clone, Debug)]
pub struct ListArgs {}

pub fn run(paths: &Paths, _args: ListArgs) -> Result<()> {
    paths.ensure_base_dirs()?;
    let mut ui = skill_core::ui::log::LogUi::new("skill list")?;
    let result = list_installed(paths, &mut ui);
    let finish = ui.finish();
    result?;
    finish?;
    Ok(())
}

fn list_installed(paths: &Paths, output: &mut impl Output) -> Result<()> {
    let lock = lockfile::load(paths)?;
    if lock.skills.is_empty() {
        output.line("No installed skills")?;
        return Ok(());
    }

    for entry in lock.skills {
        let version = entry.resolved_version.as_deref().unwrap_or("latest");
        let commit = short_sha(&entry.resolved_commit);
        let namespace = entry.namespace;
        let name = entry.name;
        output.line(format!("{namespace}/{name} {version} ({commit})"))?;
    }
    Ok(())
}
