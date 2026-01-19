use anyhow::{Result, anyhow};
use clap::Args;
use std::env;

use skill_core::config;
use skill_core::installer::Installer;
use skill_core::manifest;
use skill_core::paths::Paths;
use skill_core::progress::Reporter;

#[derive(Args, Clone, Debug)]
pub struct InstallArgs {
    pub reference: Option<String>,
    #[arg(long)]
    pub pick: bool,
    #[arg(long)]
    pub registry: Option<String>,
}

pub fn run(paths: &Paths, args: InstallArgs) -> Result<()> {
    paths.ensure_base_dirs()?;
    let config = config::load(paths)?;
    let installer = Installer::new(paths, &config);

    if let Some(reference) = args.reference.as_deref() {
        let mut reporter = Reporter::new()?;
        reporter.set_context(format!("skill install {reference}"))?;
        installer.install_reference(
            reference,
            args.pick,
            args.registry.as_deref(),
            &mut reporter,
        )?;
        reporter.finish("Done")?;
        return Ok(());
    }

    let cwd = env::current_dir()?;
    let manifest_path = cwd.join(manifest::MANIFEST_FILE);
    if !manifest_path.exists() {
        return Err(anyhow!(
            "{manifest} not found in {cwd}",
            manifest = manifest::MANIFEST_FILE,
            cwd = cwd.display()
        ));
    }

    let mut reporter = Reporter::new()?;
    reporter.set_context("skill install".to_string())?;
    installer.install_manifest(
        &manifest_path,
        args.pick,
        args.registry.as_deref(),
        &mut reporter,
    )?;
    reporter.finish("Installed skills from skills.toml")?;
    Ok(())
}
