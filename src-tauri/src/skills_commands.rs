//! Provider-neutral skills: single source of truth in `.devcommandcenter/skills/`,
//! compiled to each agent's native format. This module owns the source CRUD and the
//! compiler. Claude and Codex targets are faithful native skill copies; legacy
//! always-on targets are flattened into idempotent, delimited instruction blocks.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{command, State};

use crate::{db_error, ApiResult};
use dcc_core::domain::workspace::WorkspaceId;
use dcc_tauri::state::WorkspaceCommandState;

/// Target agent the source skill compiles to.
pub const TARGET_CLAUDE: &str = "claude";
/// Codex native progressive-disclosure skills in `.agents/skills`.
pub const TARGET_CODEX: &str = "codex";
/// Legacy always-on target for Droid and tools that consume `AGENTS.md`.
pub const TARGET_AGENTS_MD: &str = "agents";
/// Gemini CLI reads `GEMINI.md` (same always-on model as AGENTS.md).
pub const TARGET_GEMINI: &str = "gemini";
/// Cursor reads `.cursor/rules/<name>.mdc` (one file per rule).
pub const TARGET_CURSOR: &str = "cursor";

const AGENTS_BLOCK_START: &str = "<!-- dcc:skills:start -->";
const AGENTS_BLOCK_END: &str = "<!-- dcc:skills:end -->";
const MANAGED_MARKER_FILE: &str = ".dcc-managed.json";

fn default_scope() -> String {
    "project".to_string()
}

/// Writes only when content differs — keeps compilation idempotent so recompiling
/// on every workspace switch does not churn file mtimes or trip file watchers.
fn write_if_changed(path: &Path, content: &str) -> std::io::Result<()> {
    if let Ok(existing) = fs::read_to_string(path) {
        if existing == content {
            return Ok(());
        }
    }
    fs::write(path, content)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRecord {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub target_agents: Vec<String>,
    #[serde(default)]
    pub disable_model_invocation: bool,
    #[serde(default = "default_scope")]
    pub scope: String,
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

/// Metadata persisted in the manifest (everything except the body, which lives in SKILL.md).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestEntry {
    name: String,
    description: String,
    #[serde(default)]
    target_agents: Vec<String>,
    #[serde(default)]
    disable_model_invocation: bool,
    #[serde(default = "default_scope")]
    scope: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Manifest {
    #[serde(default)]
    skills: Vec<ManifestEntry>,
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

fn skills_root(project_root: &str) -> PathBuf {
    Path::new(project_root)
        .join(".devcommandcenter")
        .join("skills")
}

fn manifest_path(project_root: &str) -> PathBuf {
    skills_root(project_root).join("skills.json")
}

fn skill_md_path(project_root: &str, name: &str) -> PathBuf {
    skills_root(project_root).join(name).join("SKILL.md")
}

fn count_skill_md_dirs(root: &Path) -> usize {
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("SKILL.md").is_file())
        .count()
}

fn count_files_with_extension(root: &Path, extension: &str) -> usize {
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

fn read_managed_names(root: &Path) -> Vec<String> {
    fs::read_to_string(root.join(MANAGED_MARKER_FILE))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn detect_file_variant(root: &Path, variants: &[&str]) -> Option<String> {
    variants
        .iter()
        .find(|relative| root.join(relative).is_file())
        .map(|relative| (*relative).to_string())
}

// ---------------------------------------------------------------------------
// Pure helpers (unit-tested)
// ---------------------------------------------------------------------------

/// Skill names map to directory names and must be safe, lower-kebab slugs.
fn is_valid_skill_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Faithful Claude/Agent-SDK SKILL.md: frontmatter + body.
fn render_skill_md(name: &str, description: &str, body: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: {description}\n---\n\n{body}\n",
        body = body.trim_end()
    )
}

/// Optional Codex UI/invocation metadata. DCC always writes the policy explicitly so
/// toggling `disable_model_invocation` remains deterministic across recompiles.
fn render_codex_openai_yaml(disable_model_invocation: bool) -> String {
    format!(
        "policy:\n  allow_implicit_invocation: {}\n",
        !disable_model_invocation
    )
}

/// Strips the leading frontmatter block, returning the body only.
fn extract_body(skill_md: &str) -> String {
    let trimmed = skill_md.trim_start_matches('\u{feff}');
    if let Some(rest) = trimmed.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            let after = &rest[end + 4..];
            return after
                .trim_start_matches(['\n', '\r'])
                .trim_end()
                .to_string();
        }
    }
    trimmed.trim().to_string()
}

/// The generated AGENTS.md content (without the start/end markers).
fn render_agents_inner(skills: &[SkillRecord]) -> String {
    let mut out =
        String::from("## Skills (generated by Dev Command Center — do not edit by hand)\n");
    for skill in skills {
        out.push_str(&format!(
            "\n### {name}\n{description}\n\n{body}\n",
            name = skill.name,
            description = skill.description.trim(),
            body = skill.body.trim()
        ));
    }
    out
}

/// Idempotently inserts/replaces the DCC block in an existing AGENTS.md, preserving
/// hand-written content. When `inner` is empty the block is removed entirely.
fn upsert_agents_block(existing: &str, inner: &str) -> String {
    let block = if inner.is_empty() {
        String::new()
    } else {
        format!(
            "{AGENTS_BLOCK_START}\n{}\n{AGENTS_BLOCK_END}\n",
            inner.trim_end()
        )
    };

    if let (Some(start), Some(end)) = (
        existing.find(AGENTS_BLOCK_START),
        existing.find(AGENTS_BLOCK_END),
    ) {
        if end >= start {
            let end = end + AGENTS_BLOCK_END.len();
            let before = existing[..start].trim_end();
            let after = existing[end..].trim_start();
            let mut result = String::new();
            if !before.is_empty() {
                result.push_str(before);
                result.push_str("\n\n");
            }
            result.push_str(block.trim_end());
            if !after.is_empty() {
                result.push_str("\n\n");
                result.push_str(after);
            }
            let result = result.trim().to_string();
            return if result.is_empty() {
                String::new()
            } else {
                format!("{result}\n")
            };
        }
    }

    if block.is_empty() {
        return existing.to_string();
    }
    let base = existing.trim_end();
    if base.is_empty() {
        block
    } else {
        format!("{base}\n\n{block}")
    }
}

/// Cursor rule: own frontmatter (`description`, `alwaysApply`) + body.
fn render_cursor_mdc(skill: &SkillRecord) -> String {
    format!(
        "---\ndescription: {desc}\nalwaysApply: false\n---\n\n{body}\n",
        desc = skill.description.trim(),
        body = skill.body.trim()
    )
}

/// Skills targeting `target`, excluding those hidden from model invocation
/// (always-on artifacts must not carry hidden skills).
fn skills_for_target(skills: &[SkillRecord], target: &str) -> Vec<SkillRecord> {
    skills
        .iter()
        .filter(|s| !s.disable_model_invocation && s.target_agents.iter().any(|t| t == target))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Source CRUD
// ---------------------------------------------------------------------------

fn read_manifest(project_root: &str) -> Manifest {
    let path = manifest_path(project_root);
    let Ok(raw) = fs::read_to_string(&path) else {
        return Manifest::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn write_manifest(project_root: &str, manifest: &Manifest) -> ApiResult<()> {
    let root = skills_root(project_root);
    fs::create_dir_all(&root).map_err(|e| db_error(format!("{}: {e}", root.display())))?;
    let raw = serde_json::to_string_pretty(manifest)
        .map_err(|e| db_error(format!("serialize manifest: {e}")))?;
    fs::write(manifest_path(project_root), raw)
        .map_err(|e| db_error(format!("write manifest: {e}")))?;
    Ok(())
}

fn load_skills(project_root: &str) -> Vec<SkillRecord> {
    let manifest = read_manifest(project_root);
    manifest
        .skills
        .into_iter()
        .map(|entry| {
            let body = fs::read_to_string(skill_md_path(project_root, &entry.name))
                .map(|raw| extract_body(&raw))
                .unwrap_or_default();
            SkillRecord {
                name: entry.name,
                description: entry.description,
                body,
                target_agents: entry.target_agents,
                disable_model_invocation: entry.disable_model_invocation,
                scope: entry.scope,
            }
        })
        .collect()
}

fn detect_context_sources(
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
    let source_count = load_skills(project_root)
        .len()
        .max(count_skill_md_dirs(&dcc_source));
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

// ---------------------------------------------------------------------------
// Compiler
// ---------------------------------------------------------------------------

fn compile_skills(project_root: &str, target_root: &str) -> ApiResult<()> {
    // A freshly opened checkout may contain previously compiled, committed
    // targets but no DCC source manifest yet. Treat that as uninitialized
    // rather than interpreting it as an empty source and deleting targets.
    if !manifest_path(project_root).is_file() {
        return Ok(());
    }
    let skills = load_skills(project_root);
    let target = Path::new(target_root);

    // --- Native progressive-disclosure targets ---
    compile_native_skills(
        &target.join(".claude").join("skills"),
        &skills,
        TARGET_CLAUDE,
        false,
    )?;
    compile_native_skills(
        &target.join(".agents").join("skills"),
        &skills,
        TARGET_CODEX,
        true,
    )?;

    // --- Always-on block targets: flatten (skip disabled-from-model skills) ---
    compile_block_file(
        target,
        "AGENTS.md",
        &skills_for_target(&skills, TARGET_AGENTS_MD),
    )?;
    compile_block_file(
        target,
        "GEMINI.md",
        &skills_for_target(&skills, TARGET_GEMINI),
    )?;

    // --- Cursor target: one .mdc rule per skill ---
    compile_cursor(target, &skills_for_target(&skills, TARGET_CURSOR))?;

    Ok(())
}

/// Writes native skill directories and removes only directories recorded as DCC-managed.
fn compile_native_skills(
    native_root: &Path,
    skills: &[SkillRecord],
    target_name: &str,
    write_codex_policy: bool,
) -> ApiResult<()> {
    let native_skills: Vec<&SkillRecord> = skills
        .iter()
        .filter(|skill| {
            skill
                .target_agents
                .iter()
                .any(|target| target == target_name)
        })
        .collect();
    let names: Vec<String> = native_skills
        .iter()
        .map(|skill| skill.name.clone())
        .collect();

    let managed_path = native_root.join(MANAGED_MARKER_FILE);
    let previous = read_managed_names(native_root);
    for stale in previous.iter().filter(|name| !names.contains(name)) {
        let dir = native_root.join(stale);
        if dir.exists() {
            let _ = fs::remove_dir_all(&dir);
        }
    }

    if !names.is_empty() {
        fs::create_dir_all(native_root)
            .map_err(|e| db_error(format!("{}: {e}", native_root.display())))?;
    }
    for skill in native_skills {
        let dir = native_root.join(&skill.name);
        fs::create_dir_all(&dir).map_err(|e| db_error(format!("{}: {e}", dir.display())))?;
        let content = render_skill_md(&skill.name, &skill.description, &skill.body);
        write_if_changed(&dir.join("SKILL.md"), &content)
            .map_err(|e| db_error(format!("write SKILL.md: {e}")))?;
        if write_codex_policy {
            let agents_dir = dir.join("agents");
            fs::create_dir_all(&agents_dir)
                .map_err(|e| db_error(format!("{}: {e}", agents_dir.display())))?;
            write_if_changed(
                &agents_dir.join("openai.yaml"),
                &render_codex_openai_yaml(skill.disable_model_invocation),
            )
            .map_err(|e| db_error(format!("write agents/openai.yaml: {e}")))?;
        }
    }
    if native_root.exists() {
        let raw = serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string());
        let _ = write_if_changed(&managed_path, &raw);
    }
    Ok(())
}

/// Regenerates the DCC block inside an always-on instructions file (AGENTS.md, GEMINI.md).
/// Removes the file if it ends up empty (no hand-written content left).
fn compile_block_file(
    target: &Path,
    file_name: &str,
    block_skills: &[SkillRecord],
) -> ApiResult<()> {
    let path = target.join(file_name);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let inner = if block_skills.is_empty() {
        String::new()
    } else {
        render_agents_inner(block_skills)
    };
    let next = upsert_agents_block(&existing, &inner);
    if next != existing {
        if next.trim().is_empty() {
            if path.exists() {
                let _ = fs::remove_file(&path);
            }
        } else {
            fs::write(&path, next).map_err(|e| db_error(format!("write {file_name}: {e}")))?;
        }
    }
    Ok(())
}

/// Writes one `.cursor/rules/<name>.mdc` per skill, removing stale DCC-managed rules.
fn compile_cursor(target: &Path, cursor_skills: &[SkillRecord]) -> ApiResult<()> {
    let rules_dir = target.join(".cursor").join("rules");
    let names: Vec<String> = cursor_skills.iter().map(|s| s.name.clone()).collect();

    let managed_path = rules_dir.join(MANAGED_MARKER_FILE);
    let previous: Vec<String> = fs::read_to_string(&managed_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    for stale in previous.iter().filter(|n| !names.contains(n)) {
        let file = rules_dir.join(format!("{stale}.mdc"));
        if file.exists() {
            let _ = fs::remove_file(&file);
        }
    }

    if !names.is_empty() {
        fs::create_dir_all(&rules_dir)
            .map_err(|e| db_error(format!("{}: {e}", rules_dir.display())))?;
    }
    for skill in cursor_skills {
        write_if_changed(
            &rules_dir.join(format!("{}.mdc", skill.name)),
            &render_cursor_mdc(skill),
        )
        .map_err(|e| db_error(format!("write cursor rule: {e}")))?;
    }
    if rules_dir.exists() {
        let raw = serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string());
        let _ = write_if_changed(&managed_path, &raw);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

fn save_skill(project_root: &str, skill: SkillRecord) -> ApiResult<()> {
    if !is_valid_skill_name(&skill.name) {
        return Err(db_error(format!(
            "invalid skill name '{}': use lower-case letters, digits and hyphens",
            skill.name
        )));
    }
    let dir = skills_root(project_root).join(&skill.name);
    fs::create_dir_all(&dir).map_err(|e| db_error(format!("{}: {e}", dir.display())))?;
    let content = render_skill_md(&skill.name, &skill.description, &skill.body);
    fs::write(dir.join("SKILL.md"), content)
        .map_err(|e| db_error(format!("write SKILL.md: {e}")))?;

    let mut manifest = read_manifest(project_root);
    let entry = ManifestEntry {
        name: skill.name.clone(),
        description: skill.description,
        target_agents: skill.target_agents,
        disable_model_invocation: skill.disable_model_invocation,
        scope: skill.scope,
    };
    match manifest
        .skills
        .iter_mut()
        .find(|entry| entry.name == skill.name)
    {
        Some(existing) => *existing = entry,
        None => manifest.skills.push(entry),
    }
    write_manifest(project_root, &manifest)
}

#[command]
pub fn skills_list(project_root: String) -> ApiResult<Vec<SkillRecord>> {
    Ok(load_skills(&project_root))
}

#[command]
pub async fn skills_save(
    state: State<'_, WorkspaceCommandState>,
    project_root: String,
    workspace_id: String,
    skill: SkillRecord,
) -> ApiResult<()> {
    match state
        .run_registered_workspace_mutation_for_workspace_id(
            &WorkspaceId(workspace_id),
            &project_root,
            move |authorized| {
                let authorized = authorized
                    .to_str()
                    .ok_or_else(|| db_error("workspace path encoding is unsupported"))?;
                save_skill(authorized, skill)
            },
        )
        .await
    {
        Ok(result) => result,
        Err(error) => Err(db_error(error)),
    }
}

#[command]
pub async fn skills_delete(
    state: State<'_, WorkspaceCommandState>,
    project_root: String,
    workspace_id: String,
    name: String,
) -> ApiResult<()> {
    match state
        .run_registered_workspace_mutation_for_workspace_id(
            &WorkspaceId(workspace_id),
            &project_root,
            move |authorized| {
                let authorized = authorized
                    .to_str()
                    .ok_or_else(|| db_error("workspace path encoding is unsupported"))?;
                let dir = skills_root(authorized).join(&name);
                if dir.exists() {
                    fs::remove_dir_all(&dir)
                        .map_err(|e| db_error(format!("{}: {e}", dir.display())))?;
                }
                let mut manifest = read_manifest(authorized);
                manifest.skills.retain(|entry| entry.name != name);
                write_manifest(authorized, &manifest)
            },
        )
        .await
    {
        Ok(result) => result,
        Err(error) => Err(db_error(error)),
    }
}

#[command]
pub async fn skills_compile(
    state: State<'_, WorkspaceCommandState>,
    checkout_root: String,
    workspace_id: String,
) -> ApiResult<()> {
    match state
        .run_registered_workspace_mutation_for_workspace_id(
            &WorkspaceId(workspace_id),
            &checkout_root,
            move |authorized| {
                let authorized = authorized
                    .to_str()
                    .ok_or_else(|| db_error("workspace path encoding is unsupported"))?;
                compile_skills(authorized, authorized)
            },
        )
        .await
    {
        Ok(result) => result,
        Err(error) => Err(db_error(error)),
    }
}

#[command]
pub fn skills_detect_context(
    project_root: String,
    target_root: Option<String>,
) -> ApiResult<Vec<SkillContextDetection>> {
    Ok(detect_context_sources(
        &project_root,
        target_root.as_deref(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn skill(name: &str, targets: &[&str]) -> SkillRecord {
        SkillRecord {
            name: name.to_string(),
            description: format!("desc for {name}"),
            body: format!("Body of {name}."),
            target_agents: targets.iter().map(|s| s.to_string()).collect(),
            disable_model_invocation: false,
            scope: "project".to_string(),
        }
    }

    #[test]
    fn validates_skill_names() {
        assert!(is_valid_skill_name("revisar-pr"));
        assert!(is_valid_skill_name("skill1"));
        assert!(!is_valid_skill_name("Revisar"));
        assert!(!is_valid_skill_name("-leading"));
        assert!(!is_valid_skill_name("has space"));
        assert!(!is_valid_skill_name(""));
        assert!(!is_valid_skill_name("../escape"));
    }

    #[test]
    fn renders_and_round_trips_body() {
        let md = render_skill_md("foo", "does foo", "Line one.\nLine two.");
        assert!(md.starts_with("---\nname: foo\ndescription: does foo\n---\n"));
        assert_eq!(extract_body(&md), "Line one.\nLine two.");
    }

    #[test]
    fn renders_codex_invocation_policy() {
        assert_eq!(
            render_codex_openai_yaml(false),
            "policy:\n  allow_implicit_invocation: true\n"
        );
        assert_eq!(
            render_codex_openai_yaml(true),
            "policy:\n  allow_implicit_invocation: false\n"
        );
    }

    #[test]
    fn extract_body_without_frontmatter() {
        assert_eq!(extract_body("just a body"), "just a body");
    }

    #[test]
    fn inserts_agents_block_preserving_handwritten() {
        let existing = "# AGENTS\n\nHand written rules.\n";
        let inner = render_agents_inner(&[skill("a", &[TARGET_AGENTS_MD])]);
        let out = upsert_agents_block(existing, &inner);
        assert!(out.contains("Hand written rules."));
        assert!(out.contains(AGENTS_BLOCK_START));
        assert!(out.contains("### a"));
        assert!(out.contains(AGENTS_BLOCK_END));
    }

    #[test]
    fn replaces_block_idempotently() {
        let existing = "Top.\n";
        let first = upsert_agents_block(
            &existing,
            &render_agents_inner(&[skill("a", &[TARGET_AGENTS_MD])]),
        );
        let second = upsert_agents_block(
            &first,
            &render_agents_inner(&[skill("a", &[TARGET_AGENTS_MD])]),
        );
        assert_eq!(first, second, "recompiling the same skills must be stable");
        assert_eq!(first.matches(AGENTS_BLOCK_START).count(), 1);
    }

    #[test]
    fn renders_cursor_mdc_with_frontmatter() {
        let mdc = render_cursor_mdc(&skill("review-pr", &[TARGET_CURSOR]));
        assert!(mdc.starts_with("---\ndescription: desc for review-pr\nalwaysApply: false\n---\n"));
        assert!(mdc.contains("Body of review-pr."));
    }

    #[test]
    fn skills_for_target_excludes_disabled() {
        let mut hidden = skill("hidden", &[TARGET_AGENTS_MD, TARGET_GEMINI]);
        hidden.disable_model_invocation = true;
        let visible = skill("visible", &[TARGET_GEMINI]);
        let all = vec![hidden, visible];
        let gemini = skills_for_target(&all, TARGET_GEMINI);
        assert_eq!(gemini.len(), 1);
        assert_eq!(gemini[0].name, "visible");
    }

    #[test]
    fn empty_inner_removes_block_keeps_handwritten() {
        let existing = "# AGENTS\n\nKeep me.\n";
        let with = upsert_agents_block(
            existing,
            &render_agents_inner(&[skill("a", &[TARGET_AGENTS_MD])]),
        );
        let without = upsert_agents_block(&with, "");
        assert!(!without.contains(AGENTS_BLOCK_START));
        assert!(without.contains("Keep me."));
    }

    #[test]
    fn detects_passive_project_context_sources() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join(".devcommandcenter/skills/review")).unwrap();
        fs::write(
            root.join(".devcommandcenter/skills/review/SKILL.md"),
            "body",
        )
        .unwrap();
        fs::write(
            root.join("AGENTS.md"),
            format!("{AGENTS_BLOCK_START}\nmanaged\n{AGENTS_BLOCK_END}\nmanual"),
        )
        .unwrap();
        fs::write(root.join("CLAUDE.md"), "manual").unwrap();
        fs::create_dir_all(root.join(".cursor/rules")).unwrap();
        fs::write(root.join(".cursor/rules/manual.mdc"), "manual").unwrap();

        let detections = detect_context_sources(root.to_str().unwrap(), None);
        assert!(detections.iter().any(|item| item.id == "dcc-source"));
        let agents = detections
            .iter()
            .find(|item| item.id == "agents-md")
            .expect("agents detected");
        assert!(agents.has_dcc_block);
        assert_eq!(agents.managed_count, 1);
        let cursor = detections
            .iter()
            .find(|item| item.id == "cursor-rules")
            .expect("cursor rules detected");
        assert_eq!(cursor.count, 1);
        assert_eq!(cursor.external_count, 1);
    }

    #[test]
    fn compiles_codex_skills_to_agents_directory() {
        let source = tempdir().expect("source tempdir");
        let target = tempdir().expect("target tempdir");
        let mut orchestration = skill("dcc-orchestration", &[TARGET_CODEX]);
        orchestration.description = "Delegate suitable work".to_string();
        orchestration.disable_model_invocation = false;

        save_skill(source.path().to_string_lossy().as_ref(), orchestration).unwrap();
        compile_skills(
            &source.path().to_string_lossy(),
            &target.path().to_string_lossy(),
        )
        .unwrap();

        let native_root = target.path().join(".agents/skills/dcc-orchestration");
        let skill_md = fs::read_to_string(native_root.join("SKILL.md")).unwrap();
        let openai_yaml = fs::read_to_string(native_root.join("agents/openai.yaml")).unwrap();
        assert!(skill_md.contains("name: dcc-orchestration"));
        assert!(skill_md.contains("Body of dcc-orchestration."));
        assert!(openai_yaml.contains("allow_implicit_invocation: true"));

        let detections = detect_context_sources(
            &source.path().to_string_lossy(),
            Some(&target.path().to_string_lossy()),
        );
        let codex = detections
            .iter()
            .find(|item| item.id == "codex-skills")
            .expect("Codex native skills detected");
        assert_eq!(codex.relative_path, ".agents/skills");
        assert_eq!(codex.managed_count, 1);
        assert_eq!(codex.external_count, 0);
    }

    #[test]
    fn missing_source_manifest_does_not_remove_managed_targets() {
        let source = tempdir().expect("source tempdir");
        let target = tempdir().expect("target tempdir");
        let native = target.path().join(".agents/skills/existing");
        fs::create_dir_all(&native).unwrap();
        fs::write(native.join("SKILL.md"), "committed skill").unwrap();
        fs::write(
            target.path().join(".agents/skills/.dcc-managed.json"),
            "[\"existing\"]",
        )
        .unwrap();

        compile_skills(
            source.path().to_string_lossy().as_ref(),
            target.path().to_string_lossy().as_ref(),
        )
        .unwrap();

        assert!(native.join("SKILL.md").is_file());
        assert_eq!(
            fs::read_to_string(native.join("SKILL.md")).unwrap(),
            "committed skill"
        );
    }

    #[test]
    fn compiles_source_and_targets_in_the_same_checkout() {
        let checkout = tempdir().expect("active checkout");
        save_skill(
            checkout.path().to_string_lossy().as_ref(),
            skill("same-checkout", &[TARGET_CLAUDE]),
        )
        .unwrap();

        let root = checkout.path().to_string_lossy();
        compile_skills(&root, &root).unwrap();

        assert!(checkout
            .path()
            .join(".claude/skills/same-checkout/SKILL.md")
            .is_file());
    }
}
