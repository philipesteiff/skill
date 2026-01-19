use anyhow::Result;
use clap::Parser;

mod cli;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let paths = skill_core::paths::Paths::new()?;

    match cli.command {
        cli::Commands::Search(args) => skill_features::search::run(&paths, args)?,
        cli::Commands::Install(args) => skill_features::install::run(&paths, args)?,
        cli::Commands::Upgrade(args) => skill_features::upgrade::run(&paths, args)?,
        cli::Commands::Remove(args) => skill_features::remove::run(&paths, args)?,
        cli::Commands::List(args) => skill_features::list::run(&paths, args)?,
        cli::Commands::AddRegistry(args) => skill_features::add_registry::run(&paths, args)?,
        cli::Commands::Sync(args) => skill_features::sync::run(&paths, args)?,
        cli::Commands::Apply(args) => skill_features::apply::run(&paths, args)?,
    }

    Ok(())
}
