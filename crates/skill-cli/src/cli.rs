use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "skill",
    version,
    about = "Manage Agent Skills",
    after_help = "Skill lifecycle:\n  1) Browse a repo to install skills (skill browse <repo>).\n     - First browse of a repo adds it as a trusted source.\n  2) Browse installed skills to uninstall (skill browse).\n     - Select skills and press Enter to uninstall.\n  3) Keep installed skills in sync with their source (skill sync).\n  Installed skills stay linked to their source of truth.\n\nSources:\n  repo:  https://github.com/owner/repo or owner/repo\n  saved: @source-id\n\nExamples:\n  skill browse https://github.com/acme/skills\n  skill browse @acme --search <term>\n  skill browse\n  skill sync @acme\n  skill apply"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(
        about = "Browse and install skills from a repo or source",
        long_about = "Browse and install skills from a repo or source.\n\nBehavior:\n- With no source, shows installed skills for uninstall.\n- In installed view, select skills and press Enter to uninstall.\n- First browse of a repo implicitly trusts it as a source.\n- Scans SKILL.md files recursively (including repo root) and builds a local index if needed.\n- Opens a TUI list with search and multi-select.\n- Installs only the selected skills and stores selection state for sync.",
        display_order = 10
    )]
    Browse(skill_features::browse::BrowseArgs),
    #[command(
        about = "Sync a source: install missing and update existing",
        long_about = "Sync a source: install missing and update existing skills.\n\nBehavior:\n- Fetches latest repo HEAD and rebuilds the index if it changed.\n- Installs missing skills and updates changed ones for this source.\n- Selection state comes from the last browse (all skills or a selected list).\n- Uninstalled skills are removed from the selection so they won't be reinstalled.",
        display_order = 20
    )]
    Sync(skill_features::sync::SyncArgs),
    #[command(
        about = "Apply installed skills to agent dirs (TUI or CLI)",
        display_order = 30
    )]
    Apply(skill_features::apply::ApplyArgs),
}
