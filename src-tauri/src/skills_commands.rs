//! Provider-neutral skills: single source of truth in `.devcommandcenter/skills/`,
//! compiled to each agent's native format. This module owns the source CRUD and the
//! compiler. Claude, Codex and Grok targets are faithful native skill copies; legacy
//! always-on targets are flattened into idempotent, delimited instruction blocks.

use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::{command, State};
use url::Url;

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
/// Grok Build discovers project-native skills in `.grok/skills`.
pub const TARGET_GROK: &str = "grok";

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
    #[serde(default)]
    pub source_url: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillImportFile {
    path: String,
    content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillImportPreview {
    source_url: String,
    skill: SkillRecord,
    files: Vec<SkillImportFile>,
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubContentEntry {
    name: String,
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    content: String,
    encoding: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
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
    #[serde(default)]
    source_url: Option<String>,
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

fn is_valid_agent_skill_name(name: &str) -> bool {
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
                source_url: entry.source_url,
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
        let content = render_skill_md(
            &skill.name,
            &skill.description,
            &skill.body,
            skill.disable_model_invocation && !write_codex_policy,
        );
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

const MAX_IMPORTED_SKILL_FILES: usize = 100;
const MAX_IMPORTED_SKILL_FILE_BYTES: usize = 1_000_000;

fn parse_github_skill_url(raw_url: &str) -> ApiResult<(String, String, String, String)> {
    let url = Url::parse(raw_url.trim()).map_err(|_| db_error("invalid GitHub skill URL"))?;
    if url.scheme() != "https" || url.host_str() != Some("github.com") {
        return Err(db_error("use a GitHub HTTPS URL"));
    }
    let parts: Vec<&str> = url
        .path_segments()
        .ok_or_else(|| db_error("invalid GitHub skill URL"))?
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() < 4 || !matches!(parts[2], "tree" | "blob") {
        return Err(db_error(
            "use https://github.com/owner/repo/tree/branch/path-to-skill",
        ));
    }
    let owner = parts[0];
    let repo = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
    let reference = parts[3];
    let mut path = parts[4..].join("/");
    if parts[2] == "blob" {
        if !path.ends_with("/SKILL.md") && path != "SKILL.md" {
            return Err(db_error("a GitHub file URL must point to SKILL.md"));
        }
        path = path.strip_suffix("/SKILL.md").unwrap_or("").to_string();
    }
    if [owner, repo, reference]
        .iter()
        .any(|value| value.is_empty())
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(db_error("invalid GitHub skill path"));
    }
    Ok((
        owner.to_string(),
        repo.to_string(),
        reference.to_string(),
        path,
    ))
}

async fn github_contents(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    reference: &str,
    path: &str,
) -> ApiResult<Vec<GitHubContentEntry>> {
    let mut url = Url::parse("https://api.github.com/")
        .map_err(|error| db_error(format!("GitHub URL: {error}")))?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| db_error("cannot build GitHub API URL"))?;
        segments
            .pop_if_empty()
            .push("repos")
            .push(owner)
            .push(repo)
            .push("contents");
        for part in path.split('/').filter(|part| !part.is_empty()) {
            segments.push(part);
        }
    }
    url.query_pairs_mut().append_pair("ref", reference);
    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| db_error(format!("GitHub request failed: {error}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(db_error(format!(
            "GitHub could not read this path ({status}); check that the repository and branch are public and the URL points to a folder"
        )));
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|error| db_error(format!("invalid GitHub response: {error}")))?;
    if value.is_array() {
        serde_json::from_value(value).map_err(|error| db_error(format!("GitHub response: {error}")))
    } else {
        let entry: GitHubContentEntry = serde_json::from_value(value)
            .map_err(|error| db_error(format!("GitHub response: {error}")))?;
        Ok(vec![entry])
    }
}

fn decode_github_content(entry: &GitHubContentEntry) -> ApiResult<Vec<u8>> {
    if entry.kind != "file" || entry.encoding.as_deref() != Some("base64") {
        return Err(db_error(format!(
            "{} is not a readable GitHub file",
            entry.path
        )));
    }
    let compact: String = entry
        .content
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(compact)
        .map_err(|error| db_error(format!("invalid base64 in {}: {error}", entry.path)))?;
    if decoded.len() > MAX_IMPORTED_SKILL_FILE_BYTES {
        return Err(db_error(format!("{} is larger than 1 MB", entry.path)));
    }
    Ok(decoded)
}

async fn collect_skill_resources(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    reference: &str,
    path: &str,
    files: &mut Vec<SkillImportFile>,
) -> ApiResult<()> {
    let mut pending = vec![path.to_string()];
    let mut visited_directories = 0usize;
    while let Some(current_path) = pending.pop() {
        visited_directories += 1;
        if visited_directories > MAX_IMPORTED_SKILL_FILES {
            return Err(db_error("a skill may contain at most 100 resource folders"));
        }
        let entries = github_contents(client, owner, repo, reference, &current_path).await?;
        for entry in entries {
            if files.len() >= MAX_IMPORTED_SKILL_FILES {
                return Err(db_error(
                    "a skill may contain at most 100 imported resource files",
                ));
            }
            match entry.kind.as_str() {
                "dir" => pending.push(entry.path),
                "file" => {
                    let prefix = if path.is_empty() {
                        String::new()
                    } else {
                        format!("{}/", path.trim_end_matches('/'))
                    };
                    let relative = entry.path.strip_prefix(&prefix).ok_or_else(|| {
                        db_error("GitHub returned a file outside the skill folder")
                    })?;
                    if relative
                        .split('/')
                        .any(|part| part.is_empty() || part == "." || part == "..")
                    {
                        return Err(db_error("GitHub returned an unsafe skill resource path"));
                    }
                    let content = decode_github_content(&entry)?;
                    let already_imported_bytes: usize =
                        files.iter().map(|file| file.content_base64.len()).sum();
                    if already_imported_bytes.saturating_add(content.len()) > 13_981_016 {
                        return Err(db_error(
                            "supporting files for a skill must total 10 MB or less",
                        ));
                    }
                    files.push(SkillImportFile {
                        path: relative.to_string(),
                        content_base64: base64::engine::general_purpose::STANDARD.encode(content),
                    });
                }
                // Do not follow symlinks or submodules when importing third-party content.
                _ => {}
            }
        }
    }
    Ok(())
}

fn parse_imported_skill(
    raw: &str,
    folder_name: &str,
) -> ApiResult<(String, String, String, Vec<String>)> {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let frontmatter_and_body = raw
        .strip_prefix("---")
        .ok_or_else(|| db_error("SKILL.md must begin with YAML frontmatter delimited by ---"))?;
    let frontmatter_and_body = frontmatter_and_body
        .strip_prefix('\n')
        .or_else(|| frontmatter_and_body.strip_prefix("\r\n"))
        .ok_or_else(|| db_error("SKILL.md has invalid frontmatter delimiters"))?;
    let end = frontmatter_and_body
        .find("\n---")
        .ok_or_else(|| db_error("SKILL.md has no closing frontmatter delimiter"))?;
    let yaml = &frontmatter_and_body[..end];
    let yaml: SkillFrontmatter = serde_yaml::from_str(yaml)
        .map_err(|error| db_error(format!("SKILL.md frontmatter is not valid YAML: {error}")))?;
    let name = yaml.name.unwrap_or_default().trim().to_string();
    let description = yaml.description.unwrap_or_default().trim().to_string();
    if !is_valid_agent_skill_name(&name) {
        return Err(db_error(
            "SKILL.md needs a lower-case, hyphenated name (up to 64 characters)",
        ));
    }
    if description.is_empty() {
        return Err(db_error(
            "SKILL.md needs a non-empty description in its YAML frontmatter",
        ));
    }
    if description.chars().count() > 1024 {
        return Err(db_error(
            "SKILL.md description must be 1,024 characters or fewer",
        ));
    }
    let body = frontmatter_and_body[end + 4..].trim().to_string();
    if body.is_empty() {
        return Err(db_error(
            "SKILL.md needs Markdown instructions after the frontmatter",
        ));
    }
    let warnings = if name != folder_name {
        vec![format!("The SKILL.md name '{name}' differs from its folder '{folder_name}'. DCC will use the SKILL.md name.")]
    } else {
        Vec::new()
    };
    Ok((name, description, body, warnings))
}

async fn load_github_skill_preview(source_url: &str) -> ApiResult<SkillImportPreview> {
    let (owner, repo, reference, path) = parse_github_skill_url(source_url)?;
    let client = reqwest::Client::builder()
        .user_agent(concat!("DevCommandCenter/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| db_error(format!("HTTP client: {error}")))?;
    let entries = github_contents(&client, &owner, &repo, &reference, &path).await?;
    let skill_entry = entries
        .iter()
        .find(|entry| entry.name == "SKILL.md" && entry.kind == "file")
        .ok_or_else(|| db_error("This folder is not a skill: it has no SKILL.md file"))?;
    let markdown = String::from_utf8(decode_github_content(skill_entry)?)
        .map_err(|_| db_error("SKILL.md must be UTF-8 text"))?;
    let folder_name = path
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(&repo);
    let (name, description, body, warnings) = parse_imported_skill(&markdown, folder_name)?;
    let mut files = Vec::new();
    for directory in ["scripts", "references", "assets"] {
        if entries
            .iter()
            .any(|entry| entry.name == directory && entry.kind == "dir")
        {
            let resource_path = if path.is_empty() {
                directory.to_string()
            } else {
                format!("{path}/{directory}")
            };
            collect_skill_resources(
                &client,
                &owner,
                &repo,
                &reference,
                &resource_path,
                &mut files,
            )
            .await?;
        }
    }
    Ok(SkillImportPreview {
        source_url: source_url.trim().to_string(),
        skill: SkillRecord {
            name,
            description,
            body,
            target_agents: vec![TARGET_CLAUDE.to_string()],
            disable_model_invocation: false,
            scope: default_scope(),
            source_url: Some(source_url.trim().to_string()),
        },
        files,
        warnings,
    })
}

fn write_imported_resources(root: &Path, files: &[SkillImportFile]) -> ApiResult<()> {
    if files.len() > MAX_IMPORTED_SKILL_FILES {
        return Err(db_error(
            "a skill may contain at most 100 imported resource files",
        ));
    }
    if fs::symlink_metadata(root)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(db_error("cannot import resources through a symbolic link"));
    }
    let mut total_bytes = 0usize;
    for file in files {
        let relative = Path::new(&file.path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
            || !matches!(
                relative
                    .components()
                    .next()
                    .and_then(|part| part.as_os_str().to_str()),
                Some("scripts" | "references" | "assets")
            )
        {
            return Err(db_error("skill import contains an unsafe resource path"));
        }
        let content = base64::engine::general_purpose::STANDARD
            .decode(&file.content_base64)
            .map_err(|error| db_error(format!("invalid imported resource encoding: {error}")))?;
        if content.len() > MAX_IMPORTED_SKILL_FILE_BYTES {
            return Err(db_error("an imported resource is larger than 1 MB"));
        }
        total_bytes = total_bytes.saturating_add(content.len());
        if total_bytes > 10 * 1024 * 1024 {
            return Err(db_error(
                "supporting files for a skill must total 10 MB or less",
            ));
        }
        let destination = root.join(relative);
        let mut current = root.to_path_buf();
        let components: Vec<_> = relative.components().collect();
        for (index, component) in components.iter().enumerate() {
            current.push(component.as_os_str());
            let is_final = index + 1 == components.len();
            if fs::symlink_metadata(&current)
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false)
            {
                return Err(db_error("cannot import resources through a symbolic link"));
            }
            if !is_final && !current.exists() {
                fs::create_dir(&current).map_err(|error| {
                    db_error(format!("create imported resource folder: {error}"))
                })?;
            }
        }
        fs::write(destination, content)
            .map_err(|error| db_error(format!("write imported resource: {error}")))?;
    }
    Ok(())
}

fn reject_import_symlink(path: &Path) -> ApiResult<()> {
    if fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(db_error(format!(
            "cannot import a skill through symbolic link {}",
            path.display()
        )));
    }
    Ok(())
}

fn copy_resource_directory(source: &Path, destination: &Path) -> ApiResult<()> {
    if !source.exists() {
        return Ok(());
    }
    fs::create_dir_all(destination)
        .map_err(|error| db_error(format!("{}: {error}", destination.display())))?;
    for entry in
        fs::read_dir(source).map_err(|error| db_error(format!("{}: {error}", source.display())))?
    {
        let entry = entry.map_err(|error| db_error(format!("read skill resource: {error}")))?;
        let file_type = entry
            .file_type()
            .map_err(|error| db_error(format!("read skill resource type: {error}")))?;
        if file_type.is_symlink() {
            continue;
        }
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_resource_directory(&entry.path(), &target)?;
        } else if file_type.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| db_error(format!("{}: {error}", parent.display())))?;
            }
            fs::copy(entry.path(), &target)
                .map_err(|error| db_error(format!("copy skill resource: {error}")))?;
        }
    }
    Ok(())
}

fn save_skill(project_root: &str, skill: SkillRecord) -> ApiResult<()> {
    if !is_valid_skill_name(&skill.name) {
        return Err(db_error(format!(
            "invalid skill name '{}': use lower-case letters, digits and hyphens",
            skill.name
        )));
    }
    let dir = skills_root(project_root).join(&skill.name);
    fs::create_dir_all(&dir).map_err(|e| db_error(format!("{}: {e}", dir.display())))?;
    let content = render_skill_md(&skill.name, &skill.description, &skill.body, false);
    fs::write(dir.join("SKILL.md"), content)
        .map_err(|e| db_error(format!("write SKILL.md: {e}")))?;

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

#[command]
pub async fn skills_import_preview(source_url: String) -> ApiResult<SkillImportPreview> {
    load_github_skill_preview(&source_url).await
}

#[command]
pub async fn skills_import(
    state: State<'_, WorkspaceCommandState>,
    project_root: String,
    workspace_id: String,
    mut preview: SkillImportPreview,
    skill: SkillRecord,
) -> ApiResult<()> {
    preview.skill = skill;
    match state
        .run_registered_workspace_mutation_for_workspace_id(
            &WorkspaceId(workspace_id),
            &project_root,
            move |authorized| {
                let authorized = authorized
                    .to_str()
                    .ok_or_else(|| db_error("workspace path encoding is unsupported"))?;
                if !is_valid_agent_skill_name(&preview.skill.name) {
                    return Err(db_error(
                        "skill name must use lower-case letters, digits and single hyphens",
                    ));
                }
                let project = Path::new(authorized);
                let source_root = skills_root(authorized);
                let skill_dir = source_root.join(&preview.skill.name);
                for protected_path in [
                    project.join(".devcommandcenter"),
                    source_root.clone(),
                    manifest_path(authorized),
                    skill_dir.clone(),
                ] {
                    reject_import_symlink(&protected_path)?;
                }
                let manifest = read_manifest(authorized);
                if manifest
                    .skills
                    .iter()
                    .any(|entry| entry.name == preview.skill.name)
                {
                    return Err(db_error(format!(
                        "skill '{}' already exists in this project",
                        preview.skill.name
                    )));
                }
                save_skill(authorized, preview.skill)?;
                write_imported_resources(&skill_dir, &preview.files)?;
                Ok(())
            },
        )
        .await
    {
        Ok(result) => result,
        Err(error) => Err(db_error(error)),
    }
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
            source_url: None,
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
        let md = render_skill_md("foo", "does foo", "Line one.\nLine two.", false);
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
    fn compiles_multi_provider_skill_in_selected_checkout() {
        let project = tempdir().unwrap();
        let checkout = tempdir().unwrap();
        let source = project.path().to_str().unwrap();
        let target = checkout.path().to_str().unwrap();
        fs::write(
            checkout.path().join("GEMINI.md"),
            "Existing project instructions.\n",
        )
        .unwrap();
        save_skill(
            source,
            skill(
                "team",
                &[
                    TARGET_CLAUDE,
                    TARGET_CODEX,
                    TARGET_GEMINI,
                    TARGET_CURSOR,
                    TARGET_GROK,
                ],
            ),
        )
        .unwrap();
        compile_skills(source, target).unwrap();

        for path in [
            ".claude/skills/team/SKILL.md",
            ".agents/skills/team/SKILL.md",
            ".grok/skills/team/SKILL.md",
            ".cursor/rules/team.mdc",
            "GEMINI.md",
        ] {
            assert!(fs::read_to_string(checkout.path().join(path))
                .unwrap()
                .contains("Body of team."));
            assert!(
                !project.path().join(path).exists(),
                "must write to the selected checkout: {path}"
            );
        }
        assert!(!checkout.path().join("AGENTS.md").exists());
        assert!(fs::read_to_string(checkout.path().join("GEMINI.md"))
            .unwrap()
            .contains("Existing project instructions."));
        let grok = detect_context_sources(source, Some(target))
            .into_iter()
            .find(|item| item.kind == "grok_skills")
            .unwrap();
        assert_eq!(grok.managed_count, 1);
        assert_eq!(grok.relative_path, ".grok/skills");
    }

    #[test]
    fn native_explicit_invocation_survives_compile_and_can_be_reenabled() {
        let checkout = tempdir().unwrap();
        let root = checkout.path().to_str().unwrap();
        let mut record = skill("team", &[TARGET_CLAUDE, TARGET_CODEX, TARGET_GROK]);
        for explicit_only in [true, false] {
            record.disable_model_invocation = explicit_only;
            save_skill(root, record.clone()).unwrap();
            compile_skills(root, root).unwrap();
            for path in [".claude/skills/team/SKILL.md", ".grok/skills/team/SKILL.md"] {
                let content = fs::read_to_string(checkout.path().join(path)).unwrap();
                assert_eq!(
                    content.contains("disable-model-invocation: true"),
                    explicit_only
                );
                assert!(content.contains("Body of team."));
            }
            let policy = fs::read_to_string(
                checkout
                    .path()
                    .join(".agents/skills/team/agents/openai.yaml"),
            )
            .unwrap();
            assert!(policy.contains(&format!("allow_implicit_invocation: {}", !explicit_only)));
        }
    }

    #[test]
    fn removing_grok_target_preserves_external_skills() {
        let checkout = tempdir().unwrap();
        let root = checkout.path().to_str().unwrap();
        let external = checkout.path().join(".grok/skills/external");
        fs::create_dir_all(&external).unwrap();
        fs::write(external.join("SKILL.md"), "External instructions.").unwrap();
        save_skill(root, skill("team", &[TARGET_GROK])).unwrap();
        compile_skills(root, root).unwrap();
        save_skill(root, skill("team", &[TARGET_CLAUDE])).unwrap();
        compile_skills(root, root).unwrap();
        assert!(!checkout.path().join(".grok/skills/team").exists());
        assert_eq!(
            fs::read_to_string(external.join("SKILL.md")).unwrap(),
            "External instructions."
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
