pub struct TestSkill {
    pub name: &'static str,
    pub description: &'static str,
    pub version: &'static str,
    pub tags: &'static [&'static str],
    pub body: &'static str,
}

pub fn test_skills() -> Vec<TestSkill> {
    vec![
        TestSkill {
            name: "echo-skill",
            description: "Echo input with basic validation.",
            version: "1.0.0",
            tags: &["cli", "example"],
            body: "Responds with the input string and validates length.",
        },
        TestSkill {
            name: "notes-skill",
            description: "Manage plain-text notes locally.",
            version: "0.2.0",
            tags: &["notes", "example"],
            body: "Creates and lists local notes files.",
        },
    ]
}

pub fn skill_by_name(name: &str) -> Option<TestSkill> {
    test_skills().into_iter().find(|skill| skill.name == name)
}

pub fn skill_markdown(skill: &TestSkill) -> String {
    let tags = skill.tags.join(", ");
    format!(
        r#"---
name: {name}
description: {description}
metadata:
  version: {version}
  tags: [{tags}]
  namespace: acme
---

# {title}

{body}
"#,
        name = skill.name,
        description = skill.description,
        version = skill.version,
        tags = tags,
        title = skill_title(skill.name),
        body = skill.body,
    )
}

fn skill_title(name: &str) -> String {
    name.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
