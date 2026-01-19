use anyhow::Result;
use clap::Args;

use skill_core::config;
use skill_core::paths::Paths;
use skill_core::registry;

#[derive(Args, Clone, Debug)]
pub struct AddRegistryArgs {
    pub git_url: String,
}

pub fn run(paths: &Paths, args: AddRegistryArgs) -> Result<()> {
    paths.ensure_base_dirs()?;
    let git_url = args.git_url;
    let mut config = config::load(paths)?;
    let registry = config::add_registry(&mut config, &git_url)?;
    config::save(paths, &config)?;

    let mut ui = skill_core::ui::log::LogUi::new(format!("skill add-registry {git_url}"))?;
    let result = registry::sync_registry(paths, &registry, &mut ui);
    let finish = ui.finish();
    result?;
    finish?;
    Ok(())
}
