//! Skills (`SkillsDialog`): the checkout's skills and context read here
//! (`dcc_infra::skills`), the GitHub preview fetched here, and every write
//! sent to the desktop (`/api/v1/skills/*`) so it runs in the workspace's
//! mutation queue. The bundled catalog and the agents a skill compiles to.

use dcc_infra::skills::{SkillImportPreview, SkillRecord};
use serde_json::json;

use super::desktop_api::{ApiError, DesktopApi};

/// `SKILL_TARGETS` in the picker's order (the legacy `agents` hidden).
pub const TARGETS: [(&str, &str, &str); 5] = [
    ("claude", "Claude", ".claude/skills"),
    ("codex", "Codex", ".agents/skills"),
    ("gemini", "Gemini", "GEMINI.md"),
    ("cursor", "Cursor", ".cursor/rules"),
    ("grok", "Grok", ".grok/skills"),
];

/// A target's name and folder; `agents` is the legacy `AGENTS.md` one.
pub fn target(id: &str) -> (String, String) {
    if id == "agents" {
        return ("AGENTS.md · legacy".into(), "AGENTS.md".into());
    }
    TARGETS
        .iter()
        .find(|(candidate, _, _)| *candidate == id)
        .map(|(_, name, path)| (name.to_string(), path.to_string()))
        .unwrap_or_else(|| (id.to_string(), String::new()))
}

/// `NAME_PATTERN`: lower-case words of letters and digits, single hyphens.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('-').all(|word| {
            !word.is_empty()
                && word
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

/// `filteredSkills`: name, description and the targets' names hold the query.
pub fn matches(skill: &SkillRecord, query: &str) -> bool {
    let targets: Vec<String> = skill.target_agents.iter().map(|id| target(id).0).collect();
    format!("{} {} {}", skill.name, skill.description, targets.join(" "))
        .to_lowercase()
        .contains(&query.trim().to_lowercase())
}

/// `ORCHESTRATION_PRESET`, from the app's bundled `SKILL.md`.
pub fn orchestration_preset() -> SkillRecord {
    const MARKDOWN: &str =
        include_str!("../../assets/skills/dcc-orchestration/SKILL.md");
    let (frontmatter, body) = MARKDOWN
        .strip_prefix("---")
        .and_then(|rest| rest.trim_start_matches(['\r', '\n']).split_once("\n---"))
        .unwrap_or(("", MARKDOWN));
    let value = |key: &str| {
        frontmatter
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{key}:")))
            .map(|value| value.trim().to_string())
            .unwrap_or_default()
    };
    SkillRecord {
        name: value("name"),
        description: value("description"),
        body: body.trim_start_matches(['\r', '\n']).trim().to_string(),
        target_agents: ["claude", "codex", "gemini", "cursor", "grok"]
            .map(str::to_string)
            .to_vec(),
        disable_model_invocation: false,
        scope: "project".into(),
        source_url: None,
    }
}

/// `DCC_SKILL_CATALOG`: (skill, title key, description key).
pub fn catalog() -> Vec<(SkillRecord, &'static str, &'static str)> {
    vec![(
        orchestration_preset(),
        "skills.presets.orchestration",
        "skills.design.presetHint",
    )]
}

impl DesktopApi {
    /// `skills_save`
    pub fn skills_save(&self, root: &str, task: &str, skill: &SkillRecord) -> Result<(), ApiError> {
        self.post(
            "/api/v1/skills/save",
            &json!({ "projectRoot": root, "workspaceId": task, "skill": skill }),
        )
        .map(|_| ())
    }

    /// `skills_delete`
    pub fn skills_delete(&self, root: &str, task: &str, name: &str) -> Result<(), ApiError> {
        self.post(
            "/api/v1/skills/delete",
            &json!({ "projectRoot": root, "workspaceId": task, "name": name }),
        )
        .map(|_| ())
    }

    /// `skills_import`: the preview with the person's edits to its record.
    pub fn skills_import(
        &self,
        root: &str,
        task: &str,
        preview: &SkillImportPreview,
        skill: &SkillRecord,
    ) -> Result<(), ApiError> {
        self.post(
            "/api/v1/skills/import",
            &json!({ "projectRoot": root, "workspaceId": task, "preview": preview, "skill": skill }),
        )
        .map(|_| ())
    }

    /// `skills_compile`: the source into every agent's format.
    pub fn skills_compile(&self, root: &str, task: &str) -> Result<(), ApiError> {
        self.post(
            "/api/v1/skills/compile",
            &json!({ "checkoutRoot": root, "workspaceId": task }),
        )
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_targets_and_the_preset() {
        assert!(valid_name("review-pr"));
        assert!(valid_name("skill1"));
        assert!(!valid_name("Review"));
        assert!(!valid_name("a--b"));
        assert!(!valid_name("-a"));
        assert!(!valid_name("a-"));
        assert!(!valid_name(""));
        assert_eq!(target("codex"), ("Codex".into(), ".agents/skills".into()));
        assert_eq!(target("agents").1, "AGENTS.md");
        let preset = orchestration_preset();
        assert!(valid_name(&preset.name), "{}", preset.name);
        assert!(!preset.description.is_empty());
        assert!(!preset.body.starts_with("---"));
        assert_eq!(preset.target_agents.len(), 5);
        let skill = SkillRecord {
            name: "review-pr".into(),
            description: "Reviews".into(),
            body: String::new(),
            target_agents: vec!["cursor".into()],
            disable_model_invocation: false,
            scope: "project".into(),
            source_url: None,
        };
        assert!(matches(&skill, "CURSOR"));
        assert!(matches(&skill, " review "));
        assert!(!matches(&skill, "gemini"));
    }
}
