//! What a project gives its agents as context: DCC's own skills and the
//! instruction files and skill folders of each agent CLI. Read only; shared
//! by the desktop's skills commands and the native sidebar's count.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Skills in `.devcommandcenter/skills/skills.json`, the DCC source manifest.
pub fn manifest_skill_count(project_root: &str) -> usize {
    #[derive(Deserialize, Default)]
    struct Manifest {
        #[serde(default)]
        skills: Vec<serde_json::Value>,
    }
    let path = Path::new(project_root)
        .join(".devcommandcenter")
        .join("skills")
        .join("skills.json");
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Manifest>(&raw).ok())
        .map(|manifest| manifest.skills.len())
        .unwrap_or(0)
}

/// `getTotalSkillContextCount`: DCC's skills plus what the person keeps
/// outside DCC's management.
pub fn total_skill_context_count(detections: &[SkillContextDetection]) -> usize {
    let dcc: usize = detections
        .iter()
        .filter(|item| item.kind == "dcc_source")
        .map(|item| item.count)
        .sum();
    let external: usize = detections
        .iter()
        .filter(|item| item.kind != "dcc_source")
        .map(|item| item.external_count)
        .sum();
    dcc + external
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillContextDetection {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub relative_path: String,
    pub root_kind: String,
    pub count: usize,
    pub managed_count: usize,
    pub external_count: usize,
    pub has_dcc_block: bool,
}

pub fn count_skill_md_dirs(root: &Path) -> usize {
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("SKILL.md").is_file())
        .count()
}

pub fn count_files_with_extension(root: &Path, extension: &str) -> usize {
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        })
        .count()
}

pub fn read_managed_names(root: &Path) -> Vec<String> {
    fs::read_to_string(root.join(MANAGED_MARKER_FILE))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn detect_file_variant(root: &Path, variants: &[&str]) -> Option<String> {
    variants
        .iter()
        .find(|relative| root.join(relative).is_file())
        .map(|relative| (*relative).to_string())
}

pub fn detect_context_sources(
    project_root: &str,
    target_root: Option<&str>,
) -> Vec<SkillContextDetection> {
    let project = Path::new(project_root);
    let target = target_root
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(Path::new)
        .unwrap_or(project);
    let mut detections = Vec::new();

    let dcc_source = project.join(".devcommandcenter").join("skills");
    let source_count = manifest_skill_count(project_root).max(count_skill_md_dirs(&dcc_source));
    if source_count > 0 {
        detections.push(SkillContextDetection {
            id: "dcc-source".to_string(),
            kind: "dcc_source".to_string(),
            title: "DCC skills".to_string(),
            relative_path: ".devcommandcenter/skills".to_string(),
            root_kind: "project_root".to_string(),
            count: source_count,
            managed_count: source_count,
            external_count: 0,
            has_dcc_block: false,
        });
    }

    for (id, title, variants) in [
        ("agents-md", "AGENTS.md", &["AGENTS.md", "agents.md"][..]),
        ("claude-md", "CLAUDE.md", &["CLAUDE.md", "claude.md"][..]),
        ("gemini-md", "GEMINI.md", &["GEMINI.md", "gemini.md"][..]),
    ] {
        if let Some(relative_path) = detect_file_variant(target, variants) {
            let raw = fs::read_to_string(target.join(&relative_path)).unwrap_or_default();
            let has_dcc_block = raw.contains(AGENTS_BLOCK_START) && raw.contains(AGENTS_BLOCK_END);
            detections.push(SkillContextDetection {
                id: id.to_string(),
                kind: "instructions_file".to_string(),
                title: title.to_string(),
                relative_path,
                root_kind: "target_root".to_string(),
                count: 1,
                managed_count: usize::from(has_dcc_block),
                external_count: usize::from(!has_dcc_block),
                has_dcc_block,
            });
        }
    }

    let claude_skills = target.join(".claude").join("skills");
    let claude_count = count_skill_md_dirs(&claude_skills);
    if claude_count > 0 {
        let managed_count = read_managed_names(&claude_skills).len().min(claude_count);
        detections.push(SkillContextDetection {
            id: "claude-skills".to_string(),
            kind: "claude_skills".to_string(),
            title: "Claude skills".to_string(),
            relative_path: ".claude/skills".to_string(),
            root_kind: "target_root".to_string(),
            count: claude_count,
            managed_count,
            external_count: claude_count.saturating_sub(managed_count),
            has_dcc_block: managed_count > 0,
        });
    }

    let cursor_rules = target.join(".cursor").join("rules");
    let cursor_count = count_files_with_extension(&cursor_rules, "mdc");
    if cursor_count > 0 {
        let managed_count = read_managed_names(&cursor_rules).len().min(cursor_count);
        detections.push(SkillContextDetection {
            id: "cursor-rules".to_string(),
            kind: "cursor_rules".to_string(),
            title: "Cursor rules".to_string(),
            relative_path: ".cursor/rules".to_string(),
            root_kind: "target_root".to_string(),
            count: cursor_count,
            managed_count,
            external_count: cursor_count.saturating_sub(managed_count),
            has_dcc_block: managed_count > 0,
        });
    }

    let codex_skills = target.join(".agents").join("skills");
    let codex_count = count_skill_md_dirs(&codex_skills);
    if codex_count > 0 {
        let managed_count = read_managed_names(&codex_skills).len().min(codex_count);
        detections.push(SkillContextDetection {
            id: "codex-skills".to_string(),
            kind: "codex_skills".to_string(),
            title: "Codex skills".to_string(),
            relative_path: ".agents/skills".to_string(),
            root_kind: "target_root".to_string(),
            count: codex_count,
            managed_count,
            external_count: codex_count.saturating_sub(managed_count),
            has_dcc_block: managed_count > 0,
        });
    }

    let grok_skills = target.join(".grok").join("skills");
    let grok_count = count_skill_md_dirs(&grok_skills);
    if grok_count > 0 {
        let managed_count = read_managed_names(&grok_skills).len().min(grok_count);
        detections.push(SkillContextDetection {
            id: "grok-skills".to_string(),
            kind: "grok_skills".to_string(),
            title: "Grok skills".to_string(),
            relative_path: ".grok/skills".to_string(),
            root_kind: "target_root".to_string(),
            count: grok_count,
            managed_count,
            external_count: grok_count.saturating_sub(managed_count),
            has_dcc_block: managed_count > 0,
        });
    }

    // Keep inventory support for the pre-standard path without writing to it.
    let legacy_codex_skills = target.join(".codex").join("skills");
    let legacy_codex_count = count_skill_md_dirs(&legacy_codex_skills);
    if legacy_codex_count > 0 {
        detections.push(SkillContextDetection {
            id: "codex-skills-legacy".to_string(),
            kind: "codex_skills".to_string(),
            title: "Codex skills (legacy)".to_string(),
            relative_path: ".codex/skills".to_string(),
            root_kind: "target_root".to_string(),
            count: legacy_codex_count,
            managed_count: 0,
            external_count: legacy_codex_count,
            has_dcc_block: false,
        });
    }

    detections
}

pub const AGENTS_BLOCK_START: &str = "<!-- dcc:skills:start -->";

pub const AGENTS_BLOCK_END: &str = "<!-- dcc:skills:end -->";

pub const MANAGED_MARKER_FILE: &str = ".dcc-managed.json";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_dcc_skills_and_unmanaged_context() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let skills = root.join(".devcommandcenter").join("skills");
        fs::create_dir_all(skills.join("a")).unwrap();
        fs::write(skills.join("a").join("SKILL.md"), "x").unwrap();
        fs::write(
            skills.join("skills.json"),
            r#"{"skills":[{"name":"a"},{"name":"b"}]}"#,
        )
        .unwrap();
        fs::write(root.join("CLAUDE.md"), "manual").unwrap();
        let detections = detect_context_sources(root.to_str().unwrap(), None);
        assert_eq!(total_skill_context_count(&detections), 3);
    }
}
