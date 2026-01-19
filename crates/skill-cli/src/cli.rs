use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "skill", version, about = "A GitHub-only Agent Skills CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Search(skill_features::search::SearchArgs),
    Install(skill_features::install::InstallArgs),
    Upgrade(skill_features::upgrade::UpgradeArgs),
    Remove(skill_features::remove::RemoveArgs),
    List(skill_features::list::ListArgs),
    AddRegistry(skill_features::add_registry::AddRegistryArgs),
    Sync(skill_features::sync::SyncArgs),
    Apply(skill_features::apply::ApplyArgs),
}
