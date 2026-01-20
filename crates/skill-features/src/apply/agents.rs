use anyhow::{Result, anyhow};
use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum AgentId {
    ClaudeCode,
    Cursor,
    Vscode,
    Copilot,
    Goose,
    Opencode,
    Codex,
}

impl AgentId {
    pub fn label(self) -> &'static str {
        match self {
            AgentId::ClaudeCode => "Claude Code",
            AgentId::Cursor => "Cursor",
            AgentId::Vscode => "VS Code",
            AgentId::Copilot => "Copilot",
            AgentId::Goose => "Goose",
            AgentId::Opencode => "OpenCode",
            AgentId::Codex => "Codex",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            AgentId::ClaudeCode => "cc",
            AgentId::Cursor => "cu",
            AgentId::Vscode => "vs",
            AgentId::Copilot => "cp",
            AgentId::Goose => "gs",
            AgentId::Opencode => "oc",
            AgentId::Codex => "cdx",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Scope {
    Global,
    Project,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Scope::Global => "global",
            Scope::Project => "project",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Scope::Global => "g",
            Scope::Project => "p",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct TargetKey {
    pub agent: AgentId,
    pub scope: Scope,
}

impl TargetKey {
    pub fn label(&self) -> String {
        let agent = self.agent.label();
        let scope = self.scope.label();
        format!("{agent} {scope}")
    }
}

impl FromStr for TargetKey {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        let parts: Vec<&str> = value.split(':').collect();
        if parts.len() != 2 {
            return Err(anyhow!("invalid target: {value}"));
        }
        let agent = AgentId::from_str(parts[0])?;
        let scope = Scope::from_str(parts[1])?;
        Ok(TargetKey { agent, scope })
    }
}

impl FromStr for AgentId {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "cc" | "claude" | "claude-code" => Ok(AgentId::ClaudeCode),
            "cu" | "cursor" => Ok(AgentId::Cursor),
            "vs" | "vscode" | "code" => Ok(AgentId::Vscode),
            "cp" | "copilot" => Ok(AgentId::Copilot),
            "gs" | "goose" => Ok(AgentId::Goose),
            "oc" | "opencode" => Ok(AgentId::Opencode),
            "cdx" | "codex" => Ok(AgentId::Codex),
            _ => Err(anyhow!("unknown agent: {value}")),
        }
    }
}

impl FromStr for Scope {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "g" | "global" => Ok(Scope::Global),
            "p" | "project" => Ok(Scope::Project),
            _ => Err(anyhow!("unknown scope: {value}")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DetectedAgent {
    pub id: AgentId,
    pub detected_project: bool,
    pub detected_env: bool,
    pub supports_global: bool,
    pub supports_project: bool,
    pub global_dir: Option<PathBuf>,
    pub project_dir: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct AgentTarget {
    pub key: TargetKey,
    pub label: String,
    #[allow(dead_code)]
    pub short: String,
    pub base_dir: PathBuf,
    pub detected: bool,
    pub enabled: bool,
    pub default_selected: bool,
}

struct AgentDef {
    id: AgentId,
    global_rel: &'static str,
    project_rel: &'static str,
    supports_global: bool,
    supports_project: bool,
    project_markers: &'static [&'static str],
    env_keys: &'static [&'static str],
    term_substrings: &'static [&'static str],
}

struct EnvInfo {
    keys: HashSet<String>,
    term_program: Option<String>,
}

impl EnvInfo {
    fn from_env() -> Self {
        let keys = env::vars().map(|(key, _)| key).collect::<HashSet<_>>();
        let term_program = env::var("TERM_PROGRAM").ok();
        Self { keys, term_program }
    }

    fn has_key(&self, key: &str) -> bool {
        self.keys.contains(key)
    }

    fn term_contains(&self, needle: &str) -> bool {
        self.term_program
            .as_deref()
            .map(|value| value.contains(needle))
            .unwrap_or(false)
    }
}

const AGENTS: &[AgentDef] = &[
    AgentDef {
        id: AgentId::ClaudeCode,
        global_rel: ".claude/skills",
        project_rel: ".claude/skills",
        supports_global: true,
        supports_project: true,
        project_markers: &[".claude", ".claude/skills"],
        env_keys: &["CLAUDE_CODE", "ANTHROPIC_API_KEY"],
        term_substrings: &[],
    },
    AgentDef {
        id: AgentId::Cursor,
        global_rel: ".cursor/skills",
        project_rel: ".cursor/skills",
        supports_global: true,
        supports_project: true,
        project_markers: &[".cursor", ".cursor/skills", ".cursor/settings.json"],
        env_keys: &["CURSOR", "CURSOR_EDITOR"],
        term_substrings: &["Cursor", "cursor"],
    },
    AgentDef {
        id: AgentId::Vscode,
        global_rel: ".vscode/skills",
        project_rel: ".vscode/skills",
        supports_global: true,
        supports_project: true,
        project_markers: &[".vscode", ".vscode/skills", ".vscode/settings.json"],
        env_keys: &["VSCODE_CWD", "VSCODE_GIT_IPC_HANDLE"],
        term_substrings: &["vscode", "VSCode"],
    },
    AgentDef {
        id: AgentId::Copilot,
        global_rel: ".copilot/skills",
        project_rel: ".copilot/skills",
        supports_global: true,
        supports_project: true,
        project_markers: &[".copilot", ".copilot/skills"],
        env_keys: &["COPILOT_EDITOR"],
        term_substrings: &[],
    },
    AgentDef {
        id: AgentId::Goose,
        global_rel: ".config/goose/skills",
        project_rel: ".goose/skills",
        supports_global: true,
        supports_project: true,
        project_markers: &[".goose", ".goose/skills"],
        env_keys: &["GOOSE_CLI", "GOOSE_HOME"],
        term_substrings: &[],
    },
    AgentDef {
        id: AgentId::Opencode,
        global_rel: ".config/opencode/skill",
        project_rel: ".opencode/skill",
        supports_global: true,
        supports_project: true,
        project_markers: &[".opencode", ".opencode/skill"],
        env_keys: &["OPENCODE_HOME"],
        term_substrings: &[],
    },
    AgentDef {
        id: AgentId::Codex,
        global_rel: ".codex/skills",
        project_rel: ".codex/skills",
        supports_global: true,
        supports_project: true,
        project_markers: &[".codex", ".codex/skills"],
        env_keys: &["CODEX_HOME"],
        term_substrings: &[],
    },
];

pub fn detect_agents(repo_root: &Path) -> Result<Vec<DetectedAgent>> {
    let home = env::var("HOME").map_err(|_| anyhow!("HOME is not set"))?;
    let env_info = EnvInfo::from_env();
    Ok(detect_agents_with_env(
        repo_root,
        &PathBuf::from(home),
        &env_info,
    ))
}

fn detect_agents_with_env(repo_root: &Path, home: &Path, env_info: &EnvInfo) -> Vec<DetectedAgent> {
    AGENTS
        .iter()
        .map(|def| {
            let detected_project = def
                .project_markers
                .iter()
                .any(|marker| repo_root.join(marker).exists());
            let detected_env = def.env_keys.iter().any(|key| env_info.has_key(key))
                || def
                    .term_substrings
                    .iter()
                    .any(|needle| env_info.term_contains(needle));
            let global_dir = def.supports_global.then(|| home.join(def.global_rel));
            let project_dir = def
                .supports_project
                .then(|| repo_root.join(def.project_rel));
            DetectedAgent {
                id: def.id,
                detected_project,
                detected_env,
                supports_global: def.supports_global,
                supports_project: def.supports_project,
                global_dir,
                project_dir,
            }
        })
        .collect()
}

pub fn targets_for_agents(agents: &[DetectedAgent]) -> Vec<AgentTarget> {
    let mut targets = Vec::new();
    for agent in agents {
        let detected = agent.detected_project || agent.detected_env;
        if agent.supports_global
            && let Some(base_dir) = agent.global_dir.clone()
        {
            let label = agent.id.label();
            let scope = Scope::Global.label();
            let short = agent.id.short();
            let short_scope = Scope::Global.short();
            targets.push(AgentTarget {
                key: TargetKey {
                    agent: agent.id,
                    scope: Scope::Global,
                },
                label: format!("{label} ({scope})"),
                short: format!("{short}:{short_scope}"),
                base_dir,
                detected,
                enabled: true,
                default_selected: agent.detected_env && !agent.detected_project,
            });
        }
        if agent.supports_project
            && let Some(base_dir) = agent.project_dir.clone()
        {
            let label = agent.id.label();
            let scope = Scope::Project.label();
            let short = agent.id.short();
            let short_scope = Scope::Project.short();
            targets.push(AgentTarget {
                key: TargetKey {
                    agent: agent.id,
                    scope: Scope::Project,
                },
                label: format!("{label} ({scope})"),
                short: format!("{short}:{short_scope}"),
                base_dir,
                detected,
                enabled: true,
                default_selected: agent.detected_project,
            });
        }
    }
    targets
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn detects_project_markers() {
        let temp = tempfile::tempdir().expect("temp dir");
        fs::create_dir_all(temp.path().join(".vscode")).expect("marker");
        let env_info = EnvInfo {
            keys: HashSet::new(),
            term_program: None,
        };
        let agents = detect_agents_with_env(temp.path(), temp.path(), &env_info);
        let vscode = agents
            .iter()
            .find(|agent| agent.id == AgentId::Vscode)
            .expect("vscode");
        assert!(vscode.detected_project);
    }

    #[test]
    fn builds_default_project_targets() {
        let temp = tempfile::tempdir().expect("temp dir");
        fs::create_dir_all(temp.path().join(".goose")).expect("marker");
        let env_info = EnvInfo {
            keys: HashSet::new(),
            term_program: None,
        };
        let agents = detect_agents_with_env(temp.path(), temp.path(), &env_info);
        let targets = targets_for_agents(&agents);
        let goose_project = targets.iter().find(|target| {
            target.key.agent == AgentId::Goose && target.key.scope == Scope::Project
        });
        assert!(goose_project.is_some());
        assert!(goose_project.unwrap().default_selected);
    }
}
