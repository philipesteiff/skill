use anyhow::{Result, anyhow};
use rusqlite::{Connection, params};

use crate::output::Output;
use crate::paths::Paths;

const INDEX_SCHEMA_VERSION: i64 = 2;

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
    create_schema(&conn)?;

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

fn create_schema(conn: &Connection) -> Result<()> {
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
    conn.pragma_update(None, "user_version", INDEX_SCHEMA_VERSION)?;
    Ok(())
}

pub fn is_compatible(index_path: &std::path::Path) -> Result<bool> {
    let conn = Connection::open(index_path)?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    Ok(version == INDEX_SCHEMA_VERSION)
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
    search_with_fallback(&conn, query)
}

fn search_with_fallback(conn: &Connection, query: &str) -> Result<Vec<IndexedSkill>> {
    match search_fts(conn, query) {
        Ok(results) => Ok(results),
        Err(error) if is_fts_syntax_error(&error) => search_like(conn, query),
        Err(error) => Err(error),
    }
}

fn search_fts(conn: &Connection, query: &str) -> Result<Vec<IndexedSkill>> {
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

fn search_like(conn: &Connection, query: &str) -> Result<Vec<IndexedSkill>> {
    let pattern = format!("%{}%", escape_like(query.trim()));
    let mut stmt = conn.prepare(
        "SELECT name, description, tags, path, updated_at, commit_sha, content_hash, version
         FROM skills
         WHERE name LIKE ?1 ESCAPE '\\'
            OR description LIKE ?1 ESCAPE '\\'
            OR tags LIKE ?1 ESCAPE '\\'
         ORDER BY updated_at DESC, name ASC
         LIMIT 100",
    )?;
    let rows = stmt.query_map(params![pattern], |row| {
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

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn is_fts_syntax_error(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_lowercase();
    message.contains("unterminated string")
        || message.contains("syntax error")
        || (message.contains("fts5") && message.contains("parse"))
}

fn split_tags(value: String) -> Vec<String> {
    value
        .split_whitespace()
        .filter(|tag| !tag.is_empty())
        .map(|tag| tag.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn seed_index(conn: &Connection) -> Result<()> {
        create_schema(conn)?;
        conn.execute(
            "INSERT INTO skills (name, description, tags, path, updated_at, commit_sha, content_hash, version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
            params![
                "echo-skill",
                "Echoes text input",
                "utility prompt",
                "skills/echo-skill",
                "2026-01-01",
                "deadbeef",
                "hash-1",
            ],
        )?;
        let rowid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO skills_fts (rowid, name, description, tags) VALUES (?1, ?2, ?3, ?4)",
            params![rowid, "echo-skill", "Echoes text input", "utility prompt"],
        )?;
        Ok(())
    }

    #[test]
    fn when_schema_matches_current_version_should_be_compatible() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let index_path = temp.path().join("index.sqlite");
        let conn = Connection::open(&index_path)?;
        create_schema(&conn)?;
        drop(conn);

        assert!(is_compatible(&index_path)?);
        Ok(())
    }

    #[test]
    fn when_schema_version_is_stale_should_be_incompatible() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let index_path = temp.path().join("index.sqlite");
        let conn = Connection::open(&index_path)?;
        create_schema(&conn)?;
        conn.pragma_update(None, "user_version", INDEX_SCHEMA_VERSION - 1)?;
        drop(conn);

        assert!(!is_compatible(&index_path)?);
        Ok(())
    }

    #[test]
    fn when_search_query_has_unmatched_quote_should_fallback_without_error() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        seed_index(&conn)?;

        let results = search_with_fallback(&conn, "\"")?;
        assert!(results.is_empty());

        Ok(())
    }

    #[test]
    fn when_search_query_is_valid_should_use_indexed_results() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        seed_index(&conn)?;

        let results = search_with_fallback(&conn, "echo")?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "echo-skill");

        Ok(())
    }
}
