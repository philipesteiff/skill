use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "skill",
    version,
    about = "Manage Agent Skills",
    after_help = "Getting started (registry flow):\n  skill add-registry <git-url>\n  skill sync\n  skill search <query>\n  skill install <namespace/name>\n  skill apply\n\nGetting started (direct install):\n  skill install <owner/repo/skill-name[@latest]>\n  skill apply\n\nRefs:\n  registry: namespace/name[@latest|@1.2.3]\n  git:      owner/repo/skill-name[@latest]\n  git url:  https://github.com/owner/repo.git#path/to/skill[@latest]"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(
        about = "Search the local registry index (FTS supported)",
        display_order = 20
    )]
    Search(skill_features::search::SearchArgs),
    #[command(
        about = "Install a skill by ref or from skills.toml",
        display_order = 30
    )]
    Install(skill_features::install::InstallArgs),
    #[command(
        about = "Refresh @latest installs (uses registry if configured)",
        display_order = 40
    )]
    Upgrade(skill_features::upgrade::UpgradeArgs),
    #[command(
        about = "Remove a skill (or --all) and update the lockfile",
        display_order = 50
    )]
    Remove(skill_features::remove::RemoveArgs),
    #[command(about = "List installed skills with pinned commit", display_order = 60)]
    List(skill_features::list::ListArgs),
    #[command(about = "Add a registry metadata repo and sync it", display_order = 10)]
    AddRegistry(skill_features::add_registry::AddRegistryArgs),
    #[command(
        about = "Refresh registry index (all or --registry)",
        display_order = 15
    )]
    Sync(skill_features::sync::SyncArgs),
    #[command(
        about = "Apply installed skills to agent dirs (TUI or CLI)",
        display_order = 70
    )]
    Apply(skill_features::apply::ApplyArgs),
}
