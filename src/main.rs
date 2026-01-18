use anyhow::{Result, anyhow};
use clap::Parser;

mod agents;
mod apply;
mod cli;
mod config;
mod export;
mod git;
mod install;
mod lockfile;
mod manifest;
mod output;
mod paths;
mod progress;
mod publish;
mod refs;
mod registry;
mod skills;
mod ui;
mod util;

fn with_log_ui<F>(context: impl Into<String>, task: F) -> Result<()>
where
    F: FnOnce(&mut ui::log::LogUi) -> Result<()>,
{
    let mut ui = ui::log::LogUi::new(context)?;
    let result = task(&mut ui);
    let finish = ui.finish();
    result?;
    finish?;
    Ok(())
}

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let paths = paths::Paths::new()?;

    match cli.command {
        cli::Commands::Search(args) => {
            let config = config::load(&paths)?;
            with_log_ui(format!("skill search {}", args.query), |ui| {
                registry::search(&paths, &config, &args.query, ui)
            })?;
        }
        cli::Commands::Install(args) => {
            paths.ensure_base_dirs()?;
            let config = config::load(&paths)?;
            if let Some(reference) = args.reference.as_deref() {
                install::install_reference(
                    &paths,
                    &config,
                    reference,
                    args.pick,
                    args.registry.as_deref(),
                )?;
            } else {
                let cwd = std::env::current_dir()?;
                let manifest_path = cwd.join(manifest::MANIFEST_FILE);
                if !manifest_path.exists() {
                    return Err(anyhow!(
                        "{} not found in {}",
                        manifest::MANIFEST_FILE,
                        cwd.display()
                    ));
                }
                install::install_manifest(
                    &paths,
                    &config,
                    &manifest_path,
                    args.pick,
                    args.registry.as_deref(),
                )?;
            }
        }
        cli::Commands::Upgrade => {
            paths.ensure_base_dirs()?;
            let config = config::load(&paths)?;
            install::upgrade_latest(&paths, &config)?;
        }
        cli::Commands::Remove(args) => {
            paths.ensure_base_dirs()?;
            if args.all {
                with_log_ui("skill remove --all", |ui| {
                    install::remove_all_skills(&paths, ui)
                })?;
            } else if let Some(reference) = args.reference.as_deref() {
                with_log_ui(format!("skill remove {}", reference), |ui| {
                    install::remove_skill(&paths, reference, ui)
                })?;
            }
        }
        cli::Commands::List => {
            paths.ensure_base_dirs()?;
            with_log_ui("skill list", |ui| install::list_installed(&paths, ui))?;
        }
        cli::Commands::AddRegistry(args) => {
            paths.ensure_base_dirs()?;
            let mut config = config::load(&paths)?;
            let registry = config::add_registry(&mut config, &args.git_url)?;
            config::save(&paths, &config)?;
            with_log_ui(format!("skill add-registry {}", args.git_url), |ui| {
                registry::sync_registry(&paths, &registry, ui)
            })?;
        }
        cli::Commands::Sync(args) => {
            paths.ensure_base_dirs()?;
            let config = config::load(&paths)?;
            let registries = config::select_registries(&config, args.registry.as_deref())?;
            let label = args
                .registry
                .as_deref()
                .map(|registry| format!("skill sync {}", registry))
                .unwrap_or_else(|| "skill sync".to_string());
            with_log_ui(label, |ui| {
                for registry in registries {
                    registry::sync_registry(&paths, &registry, ui)?;
                }
                Ok(())
            })?;
        }
        cli::Commands::Publish(args) => {
            paths.ensure_base_dirs()?;
            let config = config::load(&paths)?;
            let registry = config::select_single_registry(&config, args.registry.as_deref())?;
            let label = if args.dry_run {
                "skill publish --dry-run".to_string()
            } else {
                "skill publish".to_string()
            };
            with_log_ui(label, |ui| {
                publish::publish(&paths, &registry, args.dry_run, ui)
            })?;
        }
        cli::Commands::Export(args) => {
            paths.ensure_base_dirs()?;
            let label = args
                .scope
                .as_deref()
                .map(|scope| format!("skill export {} --scope {}", args.target, scope))
                .unwrap_or_else(|| format!("skill export {}", args.target));
            with_log_ui(label, |ui| {
                export::export_skills(&paths, &args.target, args.scope.as_deref(), ui)
            })?;
        }
        cli::Commands::Apply => {
            paths.ensure_base_dirs()?;
            with_log_ui("skill apply", |ui| apply::apply_installed(&paths, ui))?;
        }
    }

    Ok(())
}
