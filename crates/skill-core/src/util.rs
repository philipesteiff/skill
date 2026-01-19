use anyhow::{Result, anyhow};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

pub fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    Ok(())
}

pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
        } else {
            out.push('_');
        }
    }
    out
}

pub fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    if !src.exists() {
        return Err(anyhow!("missing source directory: {}", src.display()));
    }
    ensure_dir(dest)?;
    for entry in WalkDir::new(src) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(src)?;
        let target = dest.join(rel);
        if entry.file_type().is_dir() {
            ensure_dir(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                ensure_dir(parent)?;
            }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

pub fn remove_dir_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

pub fn read_to_string(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?)
}

pub fn is_hexish(value: &str) -> bool {
    if value.len() < 7 {
        return false;
    }
    value.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn short_sha(value: &str) -> String {
    value.chars().take(7).collect()
}

pub fn normalize_github_url(url: &str) -> Option<String> {
    if let Some((owner, repo)) = parse_github_slug(url) {
        return Some(format!("https://github.com/{}/{}.git", owner, repo));
    }
    None
}

pub fn parse_github_slug(url: &str) -> Option<(String, String)> {
    let trimmed = url.trim_end_matches(".git");
    if let Some(rest) = trimmed.strip_prefix("https://github.com/") {
        let mut parts = rest.split('/');
        let owner = parts.next()?.to_string();
        let repo = parts.next()?.to_string();
        return Some((owner, repo));
    }
    if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        let mut parts = rest.split('/');
        let owner = parts.next()?.to_string();
        let repo = parts.next()?.to_string();
        return Some((owner, repo));
    }
    None
}
