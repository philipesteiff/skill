use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fs;

use crate::paths::Paths;
use crate::util::slugify;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub registries: Vec<RegistryConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    pub id: String,
    pub url: String,
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

pub fn add_registry(config: &mut Config, git_url: &str) -> Result<RegistryConfig> {
    let id = slugify(git_url);
    if let Some(existing) = config.registries.iter().find(|reg| reg.id == id) {
        return Ok(existing.clone());
    }
    let registry = RegistryConfig {
        id,
        url: git_url.to_string(),
    };
    config.registries.push(registry.clone());
    Ok(registry)
}

pub fn select_registries(config: &Config, selector: Option<&str>) -> Result<Vec<RegistryConfig>> {
    if let Some(selector) = selector {
        let matches: Vec<_> = config
            .registries
            .iter()
            .filter(|reg| reg.id == selector || reg.url == selector)
            .cloned()
            .collect();
        if matches.is_empty() {
            return Err(anyhow!("registry not found: {}", selector));
        }
        return Ok(matches);
    }
    Ok(config.registries.clone())
}

pub fn select_single_registry(config: &Config, selector: Option<&str>) -> Result<RegistryConfig> {
    let registries = select_registries(config, selector)?;
    if registries.is_empty() {
        return Err(anyhow!("no registries configured"));
    }
    if registries.len() > 1 && selector.is_none() {
        return Err(anyhow!("multiple registries configured; pass --registry"));
    }
    Ok(registries[0].clone())
}
