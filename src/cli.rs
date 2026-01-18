use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "skill", version, about = "A GitHub-only Agent Skills CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Search(SearchArgs),
    Install(InstallArgs),
    Upgrade,
    Remove(RemoveArgs),
    List,
    AddRegistry(AddRegistryArgs),
    Sync(SyncArgs),
    Publish(PublishArgs),
    Export(ExportArgs),
}

#[derive(Args)]
pub struct SearchArgs {
    pub query: String,
}

#[derive(Args)]
pub struct InstallArgs {
    pub reference: String,
    #[arg(long)]
    pub pick: bool,
    #[arg(long)]
    pub registry: Option<String>,
    #[arg(long)]
    pub tui: bool,
}

#[derive(Args)]
pub struct RemoveArgs {
    #[arg(required_unless_present = "all")]
    pub reference: Option<String>,
    #[arg(long, conflicts_with = "reference")]
    pub all: bool,
}

#[derive(Args)]
pub struct AddRegistryArgs {
    pub git_url: String,
}

#[derive(Args)]
pub struct SyncArgs {
    #[arg(long)]
    pub registry: Option<String>,
}

#[derive(Args)]
pub struct PublishArgs {
    #[arg(long)]
    pub registry: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct ExportArgs {
    pub target: String,
    #[arg(long)]
    pub scope: Option<String>,
}
