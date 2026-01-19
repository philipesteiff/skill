use anyhow::{Result, anyhow};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::ops::Range;
use std::path::Path;

pub const MANIFEST_FILE: &str = "skills.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub name: String,
    pub reference: String,
    pub registry: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SkillsManifest {
    #[serde(default)]
    dependencies: BTreeMap<String, DependencySpec>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DependencySpec {
    Ref(String),
    Detailed(DependencyDetail),
}

#[derive(Debug, Deserialize)]
struct DependencyDetail {
    #[serde(rename = "ref")]
    reference: String,
    registry: Option<String>,
}

pub fn load_dependencies(path: &Path) -> Result<Vec<Dependency>> {
    let data = fs::read_to_string(path)?;
    let manifest: SkillsManifest =
        toml::from_str(&data).map_err(|err| format_toml_error(path, &data, err))?;

    if manifest.dependencies.is_empty() {
        return Err(anyhow!(
            "{}: missing or empty [dependencies] table",
            path.display()
        ));
    }

    let mut deps = Vec::new();
    for (name, spec) in manifest.dependencies {
        let (reference, registry) = match spec {
            DependencySpec::Ref(reference) => (reference, None),
            DependencySpec::Detailed(detail) => (detail.reference, detail.registry),
        };
        if reference.trim().is_empty() {
            return Err(anyhow!(
                "{}: dependency {name} has an empty ref",
                path.display()
            ));
        }
        deps.push(Dependency {
            name,
            reference,
            registry,
        });
    }

    Ok(deps)
}

fn format_toml_error(path: &Path, data: &str, err: toml::de::Error) -> anyhow::Error {
    if let Some(span) = err.span() {
        let (line, column) = span_to_line_column(data, span);
        return anyhow!("{}:{}:{}: {}", path.display(), line, column, err);
    }
    anyhow!("{}: {}", path.display(), err)
}

fn span_to_line_column(data: &str, span: Range<usize>) -> (usize, usize) {
    let mut line = 1usize;
    let mut column = 1usize;
    for (idx, ch) in data.char_indices() {
        if idx >= span.start {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn load_dependencies_supports_string_and_table() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("skills.toml");
        let contents = r#"
[dependencies]
aws-lambda = "aws/skills/aws-lambda@latest"
notes = { ref = "owner/repo/notes-skill", registry = "custom" }
"#;
        fs::write(&path, contents).expect("write manifest");

        let deps = load_dependencies(&path).expect("load dependencies");
        assert_eq!(
            deps,
            vec![
                Dependency {
                    name: "aws-lambda".to_string(),
                    reference: "aws/skills/aws-lambda@latest".to_string(),
                    registry: None,
                },
                Dependency {
                    name: "notes".to_string(),
                    reference: "owner/repo/notes-skill".to_string(),
                    registry: Some("custom".to_string()),
                },
            ]
        );
    }
}
