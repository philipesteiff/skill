use anyhow::Result;
use clap::Parser;

mod cli;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let paths = skill_core::paths::Paths::new()?;

    match cli.command {
        cli::Commands::Browse(args) => skill_features::browse::run(&paths, args)?,
        cli::Commands::Sync(args) => skill_features::sync::run(&paths, args)?,
        cli::Commands::Apply(args) => skill_features::apply::run(&paths, args)?,
    }

    Ok(())
}
