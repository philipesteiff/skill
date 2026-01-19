use anyhow::Result;
use clap::Args;

use skill_core::config;
use skill_core::installer::{Installer, UpgradeOutcome};
use skill_core::paths::Paths;
use skill_core::progress::Reporter;

#[derive(Args, Clone, Debug)]
pub struct UpgradeArgs {}

pub fn run(paths: &Paths, _args: UpgradeArgs) -> Result<()> {
    paths.ensure_base_dirs()?;
    let config = config::load(paths)?;
    let installer = Installer::new(paths, &config);

    let mut reporter = Reporter::new()?;
    reporter.set_context("skill upgrade")?;
    let outcome = installer.upgrade_latest(&mut reporter)?;
    match outcome {
        UpgradeOutcome::NoInstalled => reporter.finish("No installed skills")?,
        UpgradeOutcome::UpToDate => reporter.finish("All @latest skills are up to date")?,
        UpgradeOutcome::Updated(count) => {
            reporter.finish(format!("Updated {count} skill(s)"))?;
        }
    }
    Ok(())
}
