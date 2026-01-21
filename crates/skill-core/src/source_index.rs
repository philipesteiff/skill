use anyhow::{Result, anyhow};
use rusqlite::{Connection, params};

use crate::output::Output;
use crate::paths::Paths;

#[derive(Debug, Clone)]
pub struct IndexedSkill {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub path: String,
    pub updated_at: String,
    pub commit: String,
    pub content_hash: String,
    pub version: Option<String>,
}

pub fn rebuild_index(
    paths: &Paths,
    source_id: &str,
    skills: &[IndexedSkill],
    output: &mut impl Output,
) -> Result<()> {
    let index_path = paths.source_index_path(source_id);
    if index_path.exists() {
        std::fs::remove_file(&index_path)?;
    }
    if let Some(parent) = index_path.parent() {
        crate::util::ensure_dir(parent)?;
    }

    let conn = Connection::open(index_path)?;
    conn.execute_batch(
        "CREATE TABLE skills (
            name TEXT,
            description TEXT,
            tags TEXT,
            path TEXT,
            updated_at TEXT,
            commit_sha TEXT,
            content_hash TEXT,
            version TEXT
        );
        CREATE VIRTUAL TABLE skills_fts USING fts5(
            name,
            description,
            tags,
            content='skills',
            content_rowid='rowid'
        );",
    )?;

    for skill in skills {
        let tags = skill.tags.join(" ");
        conn.execute(
            "INSERT INTO skills (name, description, tags, path, updated_at, commit_sha, content_hash, version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                skill.name,
                skill.description,
                tags,
                skill.path,
                skill.updated_at,
                skill.commit,
                skill.content_hash,
                skill.version
            ],
        )?;
        let rowid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO skills_fts (rowid, name, description, tags)
             VALUES (?1, ?2, ?3, ?4)",
            params![rowid, skill.name, skill.description, tags],
        )?;
    }

    output.line(format!("Indexed source {source_id}"))?;
    Ok(())
}

pub fn is_compatible(index_path: &std::path::Path) -> Result<bool> {
    let conn = Connection::open(index_path)?;
    let mut stmt = conn.prepare("PRAGMA table_info(skills)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == "content_hash" {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn list_all(paths: &Paths, source_id: &str) -> Result<Vec<IndexedSkill>> {
    let index_path = paths.source_index_path(source_id);
    if !index_path.exists() {
        return Err(anyhow!("source {source_id} is not indexed; run skill sync"));
    }
    let conn = Connection::open(index_path)?;
    let mut stmt = conn.prepare(
        "SELECT name, description, tags, path, updated_at, commit_sha, content_hash, version
         FROM skills
         ORDER BY updated_at DESC, name ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(IndexedSkill {
            name: row.get(0)?,
            description: row.get(1)?,
            tags: split_tags(row.get::<_, String>(2)?),
            path: row.get(3)?,
            updated_at: row.get(4)?,
            commit: row.get(5)?,
            content_hash: row.get(6)?,
            version: row.get(7)?,
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

pub fn search(paths: &Paths, source_id: &str, query: &str) -> Result<Vec<IndexedSkill>> {
    let index_path = paths.source_index_path(source_id);
    if !index_path.exists() {
        return Err(anyhow!("source {source_id} is not indexed; run skill sync"));
    }
    let conn = Connection::open(index_path)?;
    let mut stmt = conn.prepare(
        "SELECT s.name, s.description, s.tags, s.path, s.updated_at, s.commit_sha, s.content_hash, s.version
         FROM skills_fts f
         JOIN skills s ON f.rowid = s.rowid
         WHERE skills_fts MATCH ?1
         ORDER BY s.updated_at DESC, s.name ASC
         LIMIT 100",
    )?;
    let rows = stmt.query_map(params![query], |row| {
        Ok(IndexedSkill {
            name: row.get(0)?,
            description: row.get(1)?,
            tags: split_tags(row.get::<_, String>(2)?),
            path: row.get(3)?,
            updated_at: row.get(4)?,
            commit: row.get(5)?,
            content_hash: row.get(6)?,
            version: row.get(7)?,
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

fn split_tags(value: String) -> Vec<String> {
    value
        .split_whitespace()
        .filter(|tag| !tag.is_empty())
        .map(|tag| tag.to_string())
        .collect()
}
