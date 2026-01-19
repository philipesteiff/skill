use anyhow::Result;
use clap::Args;

use skill_core::config;
use skill_core::paths::Paths;
use skill_core::registry;

#[derive(Args, Clone, Debug)]
pub struct SyncArgs {
    #[arg(long)]
    pub registry: Option<String>,
}

pub fn run(paths: &Paths, args: SyncArgs) -> Result<()> {
    paths.ensure_base_dirs()?;
    let config = config::load(paths)?;
    let registries = config::select_registries(&config, args.registry.as_deref())?;
    let label = args
        .registry
        .as_deref()
        .map(|registry| format!("skill sync {registry}"))
        .unwrap_or_else(|| "skill sync".to_string());

    let mut ui = skill_core::ui::log::LogUi::new(label)?;
    let result: Result<()> = (|| {
        for registry in registries {
            registry::sync_registry(paths, &registry, &mut ui)?;
        }
        Ok(())
    })();
    let finish = ui.finish();
    result?;
    finish?;
    Ok(())
}
