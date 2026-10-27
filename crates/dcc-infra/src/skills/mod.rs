//! Provider-neutral skills: single source of truth in `.devcommandcenter/skills/`,
//! compiled to each agent's native format. This module owns the source CRUD and the
//! compiler. Claude, Codex and Grok targets are faithful native skill copies; legacy
//! always-on targets are flattened into idempotent, delimited instruction blocks.
//! Writes run where the caller serialises them with agent edits (the desktop's
//! workspace mutation queue).

mod import;
#[cfg(test)]
mod tests;

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use crate::skills_context::SkillContextDetection;
use crate::skills_context::{
    read_managed_names, AGENTS_BLOCK_END, AGENTS_BLOCK_START, MANAGED_MARKER_FILE,
};
pub use import::{import_skill, preview};

pub type SkillResult<T> = Result<T, String>;

fn skill_error(message: impl Into<String>) -> String {
    message.into()
}

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
/// Grok Build discovers project-native skills in `.grok/skills`.
pub const TARGET_GROK: &str = "grok";

pub(crate) fn default_scope() -> String {
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
    #[serde(default)]
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillImportFile {
    pub path: String,
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillImportPreview {
    pub source_url: String,
    pub skill: SkillRecord,
    pub files: Vec<SkillImportFile>,
    pub warnings: Vec<String>,
}

/// Metadata persisted in the manifest (everything except the body, which lives in SKILL.md).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ManifestEntry {
    name: String,
    description: String,
    #[serde(default)]
    target_agents: Vec<String>,
    #[serde(default)]
    disable_model_invocation: bool,
    #[serde(default = "default_scope")]
    scope: String,
    #[serde(default)]
    source_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Manifest {
    #[serde(default)]
    skills: Vec<ManifestEntry>,
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

pub fn skills_root(project_root: &str) -> PathBuf {
    Path::new(project_root)
        .join(".devcommandcenter")
        .join("skills")
}

pub fn manifest_path(project_root: &str) -> PathBuf {
    skills_root(project_root).join("skills.json")
}

fn skill_md_path(project_root: &str, name: &str) -> PathBuf {
    skills_root(project_root).join(name).join("SKILL.md")
}

// ---------------------------------------------------------------------------
// Pure helpers (unit-tested)
// ---------------------------------------------------------------------------

/// Skill names map to directory names and must be safe, lower-kebab slugs.
pub fn is_valid_skill_name(name: &str) -> bool {
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

pub fn is_valid_agent_skill_name(name: &str) -> bool {
    is_valid_skill_name(name) && !name.ends_with('-') && !name.contains("--")
}

/// Faithful Claude/Agent-SDK SKILL.md: frontmatter + body.
fn render_skill_md(name: &str, description: &str, body: &str, explicit_only: bool) -> String {
    let invocation = if explicit_only {
        "\ndisable-model-invocation: true"
    } else {
        ""
    };
    format!(
        "---\nname: {name}\ndescription: {description}{invocation}\n---\n\n{body}\n",
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

pub(crate) fn read_manifest(project_root: &str) -> Manifest {
    let path = manifest_path(project_root);
    let Ok(raw) = fs::read_to_string(&path) else {
        return Manifest::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn write_manifest(project_root: &str, manifest: &Manifest) -> SkillResult<()> {
    let root = skills_root(project_root);
    fs::create_dir_all(&root).map_err(|e| skill_error(format!("{}: {e}", root.display())))?;
    let raw = serde_json::to_string_pretty(manifest)
        .map_err(|e| skill_error(format!("serialize manifest: {e}")))?;
    fs::write(manifest_path(project_root), raw)
        .map_err(|e| skill_error(format!("write manifest: {e}")))?;
    Ok(())
}

pub fn load_skills(project_root: &str) -> Vec<SkillRecord> {
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
                source_url: entry.source_url,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Compiler
// ---------------------------------------------------------------------------

pub fn compile_skills(project_root: &str, target_root: &str) -> SkillResult<()> {
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
        &skills_root(project_root),
        &target.join(".claude").join("skills"),
        &skills,
        TARGET_CLAUDE,
        false,
    )?;
    compile_native_skills(
        &skills_root(project_root),
        &target.join(".agents").join("skills"),
        &skills,
        TARGET_CODEX,
        true,
    )?;
    compile_native_skills(
        &skills_root(project_root),
        &target.join(".grok").join("skills"),
        &skills,
        TARGET_GROK,
        false,
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
    source_root: &Path,
    native_root: &Path,
    skills: &[SkillRecord],
    target_name: &str,
    write_codex_policy: bool,
) -> SkillResult<()> {
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
            .map_err(|e| skill_error(format!("{}: {e}", native_root.display())))?;
    }
    for skill in native_skills {
        let dir = native_root.join(&skill.name);
        fs::create_dir_all(&dir).map_err(|e| skill_error(format!("{}: {e}", dir.display())))?;
        let content = render_skill_md(
            &skill.name,
            &skill.description,
            &skill.body,
            skill.disable_model_invocation && !write_codex_policy,
        );
        write_if_changed(&dir.join("SKILL.md"), &content)
            .map_err(|e| skill_error(format!("write SKILL.md: {e}")))?;
        if write_codex_policy {
            let agents_dir = dir.join("agents");
            fs::create_dir_all(&agents_dir)
                .map_err(|e| skill_error(format!("{}: {e}", agents_dir.display())))?;
            write_if_changed(
                &agents_dir.join("openai.yaml"),
                &render_codex_openai_yaml(skill.disable_model_invocation),
            )
            .map_err(|e| skill_error(format!("write agents/openai.yaml: {e}")))?;
        }
        let source_dir = source_root.join(&skill.name);
        for resource_dir in ["scripts", "references", "assets"] {
            let target_dir = dir.join(resource_dir);
            copy_resource_directory(&source_dir.join(resource_dir), &target_dir)?;
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
) -> SkillResult<()> {
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
            fs::write(&path, next).map_err(|e| skill_error(format!("write {file_name}: {e}")))?;
        }
    }
    Ok(())
}

/// Writes one `.cursor/rules/<name>.mdc` per skill, removing stale DCC-managed rules.
fn compile_cursor(target: &Path, cursor_skills: &[SkillRecord]) -> SkillResult<()> {
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
            .map_err(|e| skill_error(format!("{}: {e}", rules_dir.display())))?;
    }
    for skill in cursor_skills {
        write_if_changed(
            &rules_dir.join(format!("{}.mdc", skill.name)),
            &render_cursor_mdc(skill),
        )
        .map_err(|e| skill_error(format!("write cursor rule: {e}")))?;
    }
    if rules_dir.exists() {
        let raw = serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string());
        let _ = write_if_changed(&managed_path, &raw);
    }
    Ok(())
}

pub fn save_skill(project_root: &str, skill: SkillRecord) -> SkillResult<()> {
    if !is_valid_skill_name(&skill.name) {
        return Err(skill_error(format!(
            "invalid skill name '{}': use lower-case letters, digits and hyphens",
            skill.name
        )));
    }
    let dir = skills_root(project_root).join(&skill.name);
    fs::create_dir_all(&dir).map_err(|e| skill_error(format!("{}: {e}", dir.display())))?;
    let content = render_skill_md(&skill.name, &skill.description, &skill.body, false);
    fs::write(dir.join("SKILL.md"), content)
        .map_err(|e| skill_error(format!("write SKILL.md: {e}")))?;

    let mut manifest = read_manifest(project_root);
    let entry = ManifestEntry {
        name: skill.name.clone(),
        description: skill.description,
        target_agents: skill.target_agents,
        disable_model_invocation: skill.disable_model_invocation,
        scope: skill.scope,
        source_url: skill.source_url,
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

/// `skills_delete`: the skill's folder and its manifest entry.
pub fn delete_skill(project_root: &str, name: &str) -> SkillResult<()> {
    let dir = skills_root(project_root).join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| skill_error(format!("{}: {e}", dir.display())))?;
    }
    let mut manifest = read_manifest(project_root);
    manifest.skills.retain(|entry| entry.name != name);
    write_manifest(project_root, &manifest)
}

fn copy_resource_directory(source: &Path, destination: &Path) -> SkillResult<()> {
    if !source.exists() {
        return Ok(());
    }
    fs::create_dir_all(destination)
        .map_err(|error| skill_error(format!("{}: {error}", destination.display())))?;
    for entry in fs::read_dir(source)
        .map_err(|error| skill_error(format!("{}: {error}", source.display())))?
    {
        let entry = entry.map_err(|error| skill_error(format!("read skill resource: {error}")))?;
        let file_type = entry
            .file_type()
            .map_err(|error| skill_error(format!("read skill resource type: {error}")))?;
        if file_type.is_symlink() {
            continue;
        }
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_resource_directory(&entry.path(), &target)?;
        } else if file_type.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| skill_error(format!("{}: {error}", parent.display())))?;
            }
            fs::copy(entry.path(), &target)
                .map_err(|error| skill_error(format!("copy skill resource: {error}")))?;
        }
    }
    Ok(())
}
