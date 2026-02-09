use anyhow::{Result, anyhow};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::util::read_to_string;

#[derive(Debug, Clone)]
pub struct SkillSpec {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub tags: Vec<String>,
    pub namespace: Option<String>,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct SkillDetails {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub tags: Vec<String>,
    pub namespace: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Frontmatter {
    name: String,
    description: String,
    metadata: Option<serde_yaml::Value>,
}

pub fn read_skill_spec(skill_dir: &Path) -> Result<SkillSpec> {
    read_skill_spec_with_options(skill_dir, true)
}

pub fn read_skill_spec_with_options(skill_dir: &Path, strict: bool) -> Result<SkillSpec> {
    let skill_md = skill_dir.join("SKILL.md");
    let contents = read_to_string(&skill_md)?;
    let frontmatter = parse_frontmatter(&contents)?;
    validate_name(&frontmatter.name)?;

    let dir_name = skill_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("invalid skill directory name"))?
        .to_string();
    if strict && dir_name != frontmatter.name {
        return Err(anyhow!(
            "skill name '{}' does not match directory '{}'",
            frontmatter.name,
            dir_name
        ));
    }

    let (version, tags, namespace) = extract_metadata(frontmatter.metadata.as_ref());

    Ok(SkillSpec {
        name: frontmatter.name,
        description: frontmatter.description,
        version,
        tags,
        namespace,
        path: skill_dir.to_path_buf(),
    })
}

pub fn parse_skill_summary(contents: &str, dir_name: &str) -> Result<SkillSummary> {
    let frontmatter = parse_frontmatter(contents)?;
    validate_name(&frontmatter.name)?;
    if dir_name != frontmatter.name {
        return Err(anyhow!(
            "skill name '{}' does not match directory '{}'",
            frontmatter.name,
            dir_name
        ));
    }

    Ok(SkillSummary {
        name: frontmatter.name,
        description: frontmatter.description,
    })
}

pub fn parse_skill_details(contents: &str, dir_name: &str) -> Result<SkillDetails> {
    parse_skill_details_with_options(contents, Some(dir_name), true)
}

pub fn parse_skill_details_with_options(
    contents: &str,
    dir_name: Option<&str>,
    strict: bool,
) -> Result<SkillDetails> {
    let frontmatter = parse_frontmatter(contents)?;
    validate_name(&frontmatter.name)?;

    if strict {
        let dir_name = dir_name.ok_or_else(|| anyhow!("invalid skill directory name"))?;
        if dir_name != frontmatter.name {
            return Err(anyhow!(
                "skill name '{}' does not match directory '{}'",
                frontmatter.name,
                dir_name
            ));
        }
    }

    let (version, tags, namespace) = extract_metadata(frontmatter.metadata.as_ref());

    Ok(SkillDetails {
        name: frontmatter.name,
        description: frontmatter.description,
        version,
        tags,
        namespace,
    })
}

fn parse_frontmatter(contents: &str) -> Result<Frontmatter> {
    let mut lines = contents.lines();
    let first = lines.next().ok_or_else(|| anyhow!("SKILL.md is empty"))?;
    if first.trim() != "---" {
        return Err(anyhow!("SKILL.md missing YAML frontmatter"));
    }

    let mut yaml = String::new();
    for line in &mut lines {
        if line.trim() == "---" {
            break;
        }
        yaml.push_str(line);
        yaml.push('\n');
    }

    if yaml.is_empty() {
        return Err(anyhow!("SKILL.md frontmatter is empty"));
    }

    let frontmatter: Frontmatter = serde_yaml::from_str(&yaml)?;
    if frontmatter.description.trim().is_empty() {
        return Err(anyhow!("SKILL.md description is empty"));
    }
    Ok(frontmatter)
}

fn validate_name(name: &str) -> Result<()> {
    let len = name.chars().count();
    if len == 0 || len > 64 {
        return Err(anyhow!("skill name must be 1-64 characters"));
    }
    for ch in name.chars() {
        if !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-') {
            return Err(anyhow!(
                "skill name '{}' must be lowercase alnum or hyphen",
                name
            ));
        }
    }
    Ok(())
}

fn extract_metadata(
    metadata: Option<&serde_yaml::Value>,
) -> (Option<String>, Vec<String>, Option<String>) {
    let mut version = None;
    let mut tags = Vec::new();
    let mut namespace = None;
    if let Some(serde_yaml::Value::Mapping(map)) = metadata {
        if let Some(value) = map.get(serde_yaml::Value::String("version".to_string()))
            && let Some(v) = value.as_str()
        {
            version = Some(v.to_string());
        }
        if let Some(value) = map.get(serde_yaml::Value::String("tags".to_string()))
            && let Some(seq) = value.as_sequence()
        {
            for item in seq {
                if let Some(tag) = item.as_str() {
                    tags.push(tag.to_string());
                }
            }
        }
        if let Some(value) = map.get(serde_yaml::Value::String("namespace".to_string()))
            && let Some(val) = value.as_str()
        {
            namespace = Some(val.to_string());
        }
    }
    (version, tags, namespace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn skill_contents(name: &str) -> String {
        format!("---\nname: {name}\ndescription: Test skill\n---\n")
    }

    #[test]
    fn when_parsing_skill_details_with_strict_directory_check_should_reject_mismatch() {
        let contents = skill_contents("echo-skill");

        let result = parse_skill_details_with_options(&contents, Some("different-dir"), true);

        assert!(result.is_err());
    }

    #[test]
    fn when_parsing_skill_details_without_directory_check_should_accept_root_layout() -> Result<()>
    {
        let contents = skill_contents("root-skill");

        let details = parse_skill_details_with_options(&contents, None, false)?;

        assert_eq!(details.name, "root-skill");
        assert_eq!(details.description, "Test skill");
        Ok(())
    }

    #[test]
    fn when_reading_skill_spec_without_strict_directory_check_should_accept_root_skill()
    -> Result<()> {
        let temp = tempfile::tempdir()?;
        fs::write(temp.path().join("SKILL.md"), skill_contents("root-skill"))?;

        let spec = read_skill_spec_with_options(temp.path(), false)?;

        assert_eq!(spec.name, "root-skill");
        Ok(())
    }
}
