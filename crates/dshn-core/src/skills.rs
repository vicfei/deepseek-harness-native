//! Native skills v0: discovery over the cross-harness SKILL.md convention
//! (`<home>/skills/<name>/SKILL.md`, yaml frontmatter name+description) and
//! the `skill` tool whose round-trip shape is pinned by the recorded
//! official toolrun: `skill` call `{"name": "..."}` -> result text
//! `<skill_content name="...">…body…</skill_content>`.

use std::path::Path;

use serde_json::{json, Value};

use crate::tools::{Tool, ToolOutcome};

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub content: String, // full SKILL.md body (after frontmatter)
}

/// Discover skills under `<home>/skills/*/SKILL.md`. Missing dir = no skills.
pub fn discover(home: &Path) -> Vec<Skill> {
    let dir = home.join("skills");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut skills = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path().join("SKILL.md");
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some(skill) = parse(&raw) {
            skills.push(skill);
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

/// Minimal frontmatter parse: `---` fence, then `name:`/`description:` lines.
fn parse(raw: &str) -> Option<Skill> {
    let mut lines = raw.lines();
    if lines.next()?.trim() != "---" {
        return None; // no frontmatter: not a skill file
    }
    let (mut name, mut description) = (None, None);
    let mut body_start = None;
    for (i, line) in lines.enumerate() {
        if line.trim() == "---" {
            body_start = Some(i + 2); // +1 enum offset, +1 past the fence
            break;
        }
        if let Some(v) = line.strip_prefix("name:") {
            name = Some(v.trim().to_string());
        }
        if let Some(v) = line.strip_prefix("description:") {
            description = Some(v.trim().to_string());
        }
    }
    let body_start = body_start?;
    let body: String = raw.lines().skip(body_start).collect::<Vec<_>>().join("\n");
    Some(Skill {
        name: name?,
        description: description.unwrap_or_default(),
        content: body.trim().to_string(),
    })
}

/// The `skill` tool: loads one skill's instructions into the conversation.
pub struct SkillsTool {
    pub skills: Vec<Skill>,
}

impl SkillsTool {
    pub fn new(skills: Vec<Skill>) -> Self {
        SkillsTool { skills }
    }
}

impl Tool for SkillsTool {
    fn name(&self) -> &str {
        "skill"
    }
    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "skill",
                "description": "Load one skill's reusable task instructions",
                "parameters": {
                    "type": "object",
                    "properties": { "name": { "type": "string" } },
                    "required": ["name"]
                }
            }
        })
    }
    fn execute(&self, arguments: &str) -> ToolOutcome {
        let wanted = serde_json::from_str::<Value>(arguments)
            .ok()
            .and_then(|v| v.get("name").and_then(Value::as_str).map(str::to_string));
        let Some(wanted) = wanted else {
            return ToolOutcome {
                text: "invalid arguments: {\"name\": \"...\"} expected".into(),
                is_error: true,
            };
        };
        match self.skills.iter().find(|s| s.name == wanted) {
            Some(skill) => ToolOutcome {
                text: format!(
                    "<skill_content name=\"{}\">\n{}\n</skill_content>",
                    skill.name, skill.content
                ),
                is_error: false,
            },
            None => {
                let available: Vec<&str> = self.skills.iter().map(|s| s.name.as_str()).collect();
                ToolOutcome {
                    text: format!(
                        "unknown skill {wanted}; available: {}",
                        available.join(", ")
                    ),
                    is_error: true,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_frontmatter_and_body() {
        let dir = std::env::temp_dir().join("dshn-skill-test");
        let _ = std::fs::remove_dir_all(&dir);
        let s = dir.join("skills/demo-skill");
        std::fs::create_dir_all(&s).unwrap();
        std::fs::write(
            s.join("SKILL.md"),
            "---\nname: demo-skill\ndescription: A demo\n---\n# Demo\nUse it well.\n",
        )
        .unwrap();
        let skills = discover(&dir);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "demo-skill");
        assert_eq!(skills[0].description, "A demo");
        assert!(skills[0].content.contains("Use it well"));

        let tool = SkillsTool::new(skills);
        let out = tool.execute(r#"{"name": "demo-skill"}"#);
        assert!(out.text.starts_with("<skill_content name=\"demo-skill\">"));
        assert!(!out.is_error);
        assert!(tool.execute(r#"{"name": "nope"}"#).is_error);
    }
}
