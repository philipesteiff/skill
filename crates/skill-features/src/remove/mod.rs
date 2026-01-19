use anyhow::{Result, anyhow};
use clap::Args;
use std::path::Path;

use skill_core::lockfile;
use skill_core::output::Output;
use skill_core::paths::Paths;
use skill_core::refs;
use skill_core::util::{ensure_dir, remove_dir_if_exists};

#[derive(Args, Clone, Debug)]
pub struct RemoveArgs {
    #[arg(required_unless_present = "all")]
    pub reference: Option<String>,
    #[arg(long, conflicts_with = "reference")]
    pub all: bool,
}

pub fn run(paths: &Paths, args: RemoveArgs) -> Result<()> {
    paths.ensure_base_dirs()?;
    if args.all {
        let mut ui = skill_core::ui::log::LogUi::new("skill remove --all")?;
        let result = remove_all_skills(paths, &mut ui);
        let finish = ui.finish();
        result?;
        finish?;
        return Ok(());
    }
    if let Some(reference) = args.reference.as_deref() {
        let mut ui = skill_core::ui::log::LogUi::new(format!("skill remove {reference}"))?;
        let result = remove_skill(paths, reference, &mut ui);
        let finish = ui.finish();
        result?;
        finish?;
    }
    Ok(())
}

fn remove_skill(paths: &Paths, reference: &str, output: &mut impl Output) -> Result<()> {
    let parsed = refs::parse_reference(reference);
    let segments = refs::split_segments(&parsed.base);
    if segments.len() < 2 {
        return Err(anyhow!("invalid reference: {reference}"));
    }
    let namespace = segments.first().cloned().unwrap_or_default();
    let name = segments.last().cloned().unwrap_or_default();

    let mut lock = lockfile::load(paths)?;
    let entry = lockfile::remove(&mut lock, &namespace, &name)
        .ok_or_else(|| anyhow!("skill not installed: {namespace}/{name}"))?;

    remove_dir_if_exists(Path::new(&entry.install_dir))?;
    lockfile::save(paths, &lock)?;
    output.line(format!("Removed {namespace}/{name}"))?;
    Ok(())
}

fn remove_all_skills(paths: &Paths, output: &mut impl Output) -> Result<()> {
    let mut lock = lockfile::load(paths)?;
    if lock.skills.is_empty() {
        remove_dir_if_exists(&paths.installed_dir())?;
        ensure_dir(&paths.installed_dir())?;
        output.line("No installed skills")?;
        return Ok(());
    }

    let count = lock.skills.len();
    for entry in &lock.skills {
        remove_dir_if_exists(Path::new(&entry.install_dir))?;
    }
    lock.skills.clear();
    lockfile::save(paths, &lock)?;
    remove_dir_if_exists(&paths.installed_dir())?;
    ensure_dir(&paths.installed_dir())?;
    output.line(format!("Removed {count} skill(s)"))?;
    Ok(())
}
