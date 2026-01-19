use anyhow::{Result, anyhow};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::RegistryConfig;
use crate::git;
use crate::output::Output;
use crate::paths::Paths;
use crate::util::{ensure_dir, read_to_string};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySkillFile {
    pub namespace: String,
    pub name: String,
    pub description: String,
    pub repo_url: String,
    pub path: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub latest: Option<RegistryLatest>,
    #[serde(default)]
    pub versions: Vec<RegistryVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryLatest {
    pub version: String,
    pub commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryVersion {
    pub version: String,
    pub commit: String,
    pub published_at: String,
}

#[derive(Debug, Clone)]
pub struct RegistryRow {
    pub namespace: String,
    pub name: String,
    pub repo_url: String,
    pub path: String,
    pub latest_version: Option<String>,
    pub latest_commit: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub namespace: String,
    pub name: String,
    pub description: String,
    pub latest_version: Option<String>,
}

pub fn sync_registry(
    paths: &Paths,
    registry: &RegistryConfig,
    output: &mut impl Output,
) -> Result<()> {
    let repo_dir = paths.registry_repo_dir(&registry.id);
    ensure_dir(paths.registry_dir(&registry.id).as_path())?;

    if !repo_dir.exists() {
        output.line(format!("Cloning registry {}...", registry.url))?;
        git::clone_repo(&registry.url, &repo_dir)?;
    } else {
        output.line(format!("Syncing registry {}...", registry.url))?;
        git::fetch_repo(&repo_dir)?;
    }

    let head = git::repo_head(&repo_dir)?;
    let head_path = paths.registry_head_path(&registry.id);
    let previous = fs::read_to_string(&head_path).unwrap_or_default();
    let index_path = paths.registry_index_path(&registry.id);
    if previous.trim() != head || !index_path.exists() {
        rebuild_index(registry, &repo_dir, &index_path, output)?;
        fs::write(head_path, head)?;
    }

    Ok(())
}

pub fn rebuild_index(
    registry: &RegistryConfig,
    repo_dir: &Path,
    index_path: &Path,
    output: &mut impl Output,
) -> Result<()> {
    if index_path.exists() {
        fs::remove_file(index_path)?;
    }

    let conn = Connection::open(index_path)?;
    conn.execute_batch(
        "CREATE TABLE skills (
            namespace TEXT,
            name TEXT,
            description TEXT,
            tags TEXT,
            repo_url TEXT,
            path TEXT,
            latest_version TEXT,
            latest_commit TEXT
        );
        CREATE VIRTUAL TABLE skills_fts USING fts5(
            namespace,
            name,
            description,
            tags,
            content='skills',
            content_rowid='rowid'
        );",
    )?;

    let skills_dir = repo_dir.join("skills");
    if !skills_dir.exists() {
        return Err(anyhow!(
            "registry {} missing skills/ directory",
            registry.url
        ));
    }

    for entry in walkdir::WalkDir::new(&skills_dir) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let data = read_to_string(entry.path())?;
        let skill: RegistrySkillFile = serde_json::from_str(&data)?;
        let tags = skill.tags.join(" ");
        let latest_version = skill.latest.as_ref().map(|l| l.version.clone());
        let latest_commit = skill.latest.as_ref().map(|l| l.commit.clone());

        conn.execute(
            "INSERT INTO skills (namespace, name, description, tags, repo_url, path, latest_version, latest_commit)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                skill.namespace,
                skill.name,
                skill.description,
                tags,
                skill.repo_url,
                skill.path,
                latest_version,
                latest_commit
            ],
        )?;
        let rowid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO skills_fts (rowid, namespace, name, description, tags)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![rowid, skill.namespace, skill.name, skill.description, tags],
        )?;
    }

    output.line(format!("Indexed registry {}", registry.url))?;
    Ok(())
}

pub fn search(
    paths: &Paths,
    config: &crate::config::Config,
    query: &str,
    output: &mut impl Output,
) -> Result<()> {
    if config.registries.is_empty() {
        return Err(anyhow!("no registries configured; run skill add-registry"));
    }
    let mut results = Vec::new();
    for registry in &config.registries {
        let index_path = paths.registry_index_path(&registry.id);
        if !index_path.exists() {
            return Err(anyhow!(
                "registry {} is not indexed; run skill sync",
                registry.id
            ));
        }
        let mut reg_results = search_registry(&index_path, query, &registry.id)?;
        results.append(&mut reg_results);
    }

    if results.is_empty() {
        output.line(format!("No matches for '{query}'"))?;
        return Ok(());
    }

    for (idx, result) in results.iter().enumerate() {
        let label = format!("{}/{}", result.namespace, result.name);
        let version = result
            .latest_version
            .as_ref()
            .map(|v| format!("@{}", v))
            .unwrap_or_else(|| "".to_string());
        output.line(format!(
            "{}) {}{} - {}",
            idx + 1,
            label,
            version,
            result.description
        ))?;
    }
    Ok(())
}

fn search_registry(
    index_path: &Path,
    query: &str,
    _registry_id: &str,
) -> Result<Vec<SearchResult>> {
    let conn = Connection::open(index_path)?;
    let mut stmt = conn.prepare(
        "SELECT s.namespace, s.name, s.description, s.tags, s.repo_url, s.path, s.latest_version
         FROM skills_fts f
         JOIN skills s ON f.rowid = s.rowid
         WHERE skills_fts MATCH ?1
         LIMIT 50",
    )?;

    let rows = stmt.query_map(params![query], |row| {
        Ok(SearchResult {
            namespace: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            latest_version: row.get(6)?,
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

pub fn load_skill_file(
    paths: &Paths,
    registry: &RegistryConfig,
    namespace: &str,
    name: &str,
) -> Result<Option<RegistrySkillFile>> {
    let file_path = registry_skill_path(paths, registry, namespace, name);
    if !file_path.exists() {
        return Ok(None);
    }
    let data = read_to_string(&file_path)?;
    let skill = serde_json::from_str(&data)?;
    Ok(Some(skill))
}

pub fn write_skill_file(
    paths: &Paths,
    registry: &RegistryConfig,
    skill: &RegistrySkillFile,
) -> Result<()> {
    let file_path = registry_skill_path(paths, registry, &skill.namespace, &skill.name);
    if let Some(parent) = file_path.parent() {
        ensure_dir(parent)?;
    }
    let data = serde_json::to_string_pretty(skill)?;
    fs::write(file_path, data)?;
    Ok(())
}

pub fn find_by_namespace_name(
    paths: &Paths,
    registry: &RegistryConfig,
    namespace: &str,
    name: &str,
) -> Result<Option<RegistryRow>> {
    let index_path = paths.registry_index_path(&registry.id);
    if !index_path.exists() {
        return Err(anyhow!(
            "registry {} is not indexed; run skill sync",
            registry.id
        ));
    }
    let conn = Connection::open(index_path)?;
    let mut stmt = conn.prepare(
        "SELECT namespace, name, repo_url, path, latest_version, latest_commit
         FROM skills WHERE namespace = ?1 AND name = ?2",
    )?;
    let row = stmt.query_row(params![namespace, name], |row| {
        Ok(RegistryRow {
            namespace: row.get(0)?,
            name: row.get(1)?,
            repo_url: row.get(2)?,
            path: row.get(3)?,
            latest_version: row.get(4)?,
            latest_commit: row.get(5)?,
        })
    });

    match row {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn registry_skill_path(
    paths: &Paths,
    registry: &RegistryConfig,
    namespace: &str,
    name: &str,
) -> PathBuf {
    paths
        .registry_repo_dir(&registry.id)
        .join("skills")
        .join(namespace)
        .join(format!("{}.json", name))
}
