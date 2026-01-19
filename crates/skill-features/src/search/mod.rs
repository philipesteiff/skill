use anyhow::Result;
use clap::Args;

use skill_core::config;
use skill_core::paths::Paths;
use skill_core::registry;

#[derive(Args, Clone, Debug)]
pub struct SearchArgs {
    pub query: String,
}

pub fn run(paths: &Paths, args: SearchArgs) -> Result<()> {
    let config = config::load(paths)?;
    let query = args.query;
    let mut ui = skill_core::ui::log::LogUi::new(format!("skill search {query}"))?;
    let result = registry::search(paths, &config, &query, &mut ui);
    let finish = ui.finish();
    result?;
    finish?;
    Ok(())
}
