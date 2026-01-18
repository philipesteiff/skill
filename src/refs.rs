#[derive(Debug, Clone)]
pub enum Selector {
    Latest,
    Version(String),
    None,
}

impl Selector {
    pub fn requested_string(&self) -> String {
        match self {
            Selector::Latest => "@latest".to_string(),
            Selector::Version(value) => format!("@{}", value),
            Selector::None => "@latest".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParsedRef {
    pub base: String,
    pub selector: Selector,
}

pub fn parse_reference(reference: &str) -> ParsedRef {
    let trimmed = reference.trim();
    let (base, selector) = split_selector(trimmed);
    ParsedRef { base, selector }
}

fn split_selector(reference: &str) -> (String, Selector) {
    if let Some(idx) = reference.rfind('@') {
        let (left, right) = reference.split_at(idx);
        let suffix = &right[1..];
        if !suffix.is_empty() && !suffix.contains('/') && !suffix.contains('#') {
            let selector = if suffix == "latest" {
                Selector::Latest
            } else {
                Selector::Version(suffix.to_string())
            };
            return (left.to_string(), selector);
        }
    }
    (reference.to_string(), Selector::None)
}

pub fn is_git_url(value: &str) -> bool {
    value.contains("://") || value.starts_with("git@")
}

pub fn split_git_url(value: &str) -> (String, Option<String>) {
    if let Some((url, path)) = value.split_once('#') {
        return (url.to_string(), Some(path.to_string()));
    }
    (value.to_string(), None)
}

pub fn split_segments(value: &str) -> Vec<String> {
    value
        .split('/')
        .filter(|seg| !seg.is_empty())
        .map(|seg| seg.to_string())
        .collect()
}
