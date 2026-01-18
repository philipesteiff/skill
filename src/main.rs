use anyhow::Result;
use clap::Parser;

mod cli;
mod config;
mod export;
mod git;
mod install;
mod lockfile;
mod paths;
mod publish;
mod refs;
mod registry;
mod skills;
mod tui;
mod util;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let paths = paths::Paths::new()?;

    match cli.command {
        cli::Commands::Search(args) => {
            let config = config::load(&paths)?;
            registry::search(&paths, &config, &args.query)?;
        }
        cli::Commands::Install(args) => {
            paths.ensure_base_dirs()?;
            let config = config::load(&paths)?;
            install::install_reference(
                &paths,
                &config,
                &args.reference,
                args.pick,
                args.registry.as_deref(),
            )?;
        }
        cli::Commands::Upgrade => {
            paths.ensure_base_dirs()?;
            let config = config::load(&paths)?;
            install::upgrade_latest(&paths, &config)?;
        }
        cli::Commands::Remove(args) => {
            paths.ensure_base_dirs()?;
            install::remove_skill(&paths, &args.reference)?;
        }
        cli::Commands::List => {
            paths.ensure_base_dirs()?;
            install::list_installed(&paths)?;
        }
        cli::Commands::AddRegistry(args) => {
            paths.ensure_base_dirs()?;
            let mut config = config::load(&paths)?;
            let registry = config::add_registry(&mut config, &args.git_url)?;
            config::save(&paths, &config)?;
            registry::sync_registry(&paths, &registry)?;
        }
        cli::Commands::Sync(args) => {
            paths.ensure_base_dirs()?;
            let config = config::load(&paths)?;
            let registries = config::select_registries(&config, args.registry.as_deref())?;
            for registry in registries {
                registry::sync_registry(&paths, &registry)?;
            }
        }
        cli::Commands::Publish(args) => {
            paths.ensure_base_dirs()?;
            let config = config::load(&paths)?;
            let registry = config::select_single_registry(&config, args.registry.as_deref())?;
            publish::publish(&paths, &registry, args.dry_run)?;
        }
        cli::Commands::Export(args) => {
            paths.ensure_base_dirs()?;
            export::export_skills(&paths, &args.target, args.scope.as_deref())?;
        }
    }

    Ok(())
}
