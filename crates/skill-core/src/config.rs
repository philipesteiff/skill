use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fs;

use crate::paths::Paths;
use crate::util::slugify;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub sources: Vec<SourceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub selection: SelectionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SelectionConfig {
    All,
    List { skills: Vec<String> },
}

impl Default for SelectionConfig {
    fn default() -> Self {
        SelectionConfig::List { skills: Vec::new() }
    }
}

pub fn load(paths: &Paths) -> Result<Config> {
    let path = paths.config_path();
    if !path.exists() {
        return Ok(Config::default());
    }
    let data = fs::read_to_string(&path)?;
    let config = serde_json::from_str(&data)?;
    Ok(config)
}

pub fn save(paths: &Paths, config: &Config) -> Result<()> {
    let path = paths.config_path();
    let data = serde_json::to_string_pretty(config)?;
    fs::write(path, data)?;
    Ok(())
}

pub fn resolve_source(config: &mut Config, input: &str) -> Result<(SourceConfig, bool)> {
    if let Some(id) = input.strip_prefix('@') {
        let source = config
            .sources
            .iter()
            .find(|source| source.id == id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown source: {id}"))?;
        return Ok((source, false));
    }

    let url = normalize_repo_input(input)
        .ok_or_else(|| anyhow!("unsupported repo reference: {input}"))?;

    if let Some(existing) = config.sources.iter().find(|source| source.url == url) {
        return Ok((existing.clone(), false));
    }

    let base_id = source_id_for_url(&url);
    let id = unique_source_id(config, &base_id);
    let source = SourceConfig {
        id,
        url,
        selection: SelectionConfig::default(),
    };
    config.sources.push(source.clone());
    Ok((source, true))
}

pub fn update_source(config: &mut Config, updated: &SourceConfig) -> Result<()> {
    if let Some(entry) = config
        .sources
        .iter_mut()
        .find(|source| source.id == updated.id)
    {
        *entry = updated.clone();
        return Ok(());
    }
    config.sources.push(updated.clone());
    Ok(())
}

fn source_id_for_url(url: &str) -> String {
    if let Some((owner, repo)) = crate::util::parse_github_slug(url) {
        return format!("{owner}-{repo}");
    }
    slugify(url)
}

fn unique_source_id(config: &Config, base: &str) -> String {
    if !config.sources.iter().any(|source| source.id == base) {
        return base.to_string();
    }
    let mut idx = 2;
    loop {
        let candidate = format!("{base}-{idx}");
        if !config.sources.iter().any(|source| source.id == candidate) {
            return candidate;
        }
        idx += 1;
    }
}

fn normalize_repo_input(input: &str) -> Option<String> {
    if input.contains("://") || input.starts_with("git@") {
        return crate::util::normalize_github_url(input);
    }
    if let Some((owner, repo)) =
        crate::util::parse_github_slug(&format!("https://github.com/{input}"))
    {
        return Some(format!("https://github.com/{owner}/{repo}.git"));
    }
    None
}
