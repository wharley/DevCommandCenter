//! Importing a skill from a public GitHub folder: its SKILL.md and the
//! `scripts`, `references` and `assets` beside it (100 files, 1 MB each,
//! 10 MB in all), never following links or submodules.

use std::fs;
use std::path::Path;

use base64::Engine;
use serde::Deserialize;
use url::Url;

use super::{
    default_scope, is_valid_agent_skill_name, manifest_path, read_manifest, save_skill,
    skill_error, skills_root, SkillImportFile, SkillImportPreview, SkillRecord, SkillResult,
    TARGET_CLAUDE,
};

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

const MAX_IMPORTED_SKILL_FILES: usize = 100;
const MAX_IMPORTED_SKILL_FILE_BYTES: usize = 1_000_000;

fn parse_github_skill_url(raw_url: &str) -> SkillResult<(String, String, String, String)> {
    let url = Url::parse(raw_url.trim()).map_err(|_| skill_error("invalid GitHub skill URL"))?;
    if url.scheme() != "https" || url.host_str() != Some("github.com") {
        return Err(skill_error("use a GitHub HTTPS URL"));
    }
    let parts: Vec<&str> = url
        .path_segments()
        .ok_or_else(|| skill_error("invalid GitHub skill URL"))?
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() < 4 || !matches!(parts[2], "tree" | "blob") {
        return Err(skill_error(
            "use https://github.com/owner/repo/tree/branch/path-to-skill",
        ));
    }
    let owner = parts[0];
    let repo = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
    let reference = parts[3];
    let mut path = parts[4..].join("/");
    if parts[2] == "blob" {
        if !path.ends_with("/SKILL.md") && path != "SKILL.md" {
            return Err(skill_error("a GitHub file URL must point to SKILL.md"));
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
        return Err(skill_error("invalid GitHub skill path"));
    }
    Ok((
        owner.to_string(),
        repo.to_string(),
        reference.to_string(),
        path,
    ))
}

fn github_contents(
    client: &reqwest::blocking::Client,
    owner: &str,
    repo: &str,
    reference: &str,
    path: &str,
) -> SkillResult<Vec<GitHubContentEntry>> {
    let mut url = Url::parse("https://api.github.com/")
        .map_err(|error| skill_error(format!("GitHub URL: {error}")))?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| skill_error("cannot build GitHub API URL"))?;
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
        .map_err(|error| skill_error(format!("GitHub request failed: {error}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(skill_error(format!(
            "GitHub could not read this path ({status}); check that the repository and branch are public and the URL points to a folder"
        )));
    }
    let value: serde_json::Value = response
        .json()
        .map_err(|error| skill_error(format!("invalid GitHub response: {error}")))?;
    if value.is_array() {
        serde_json::from_value(value)
            .map_err(|error| skill_error(format!("GitHub response: {error}")))
    } else {
        let entry: GitHubContentEntry = serde_json::from_value(value)
            .map_err(|error| skill_error(format!("GitHub response: {error}")))?;
        Ok(vec![entry])
    }
}

fn decode_github_content(entry: &GitHubContentEntry) -> SkillResult<Vec<u8>> {
    if entry.kind != "file" || entry.encoding.as_deref() != Some("base64") {
        return Err(skill_error(format!(
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
        .map_err(|error| skill_error(format!("invalid base64 in {}: {error}", entry.path)))?;
    if decoded.len() > MAX_IMPORTED_SKILL_FILE_BYTES {
        return Err(skill_error(format!("{} is larger than 1 MB", entry.path)));
    }
    Ok(decoded)
}

fn collect_skill_resources(
    client: &reqwest::blocking::Client,
    owner: &str,
    repo: &str,
    reference: &str,
    path: &str,
    files: &mut Vec<SkillImportFile>,
) -> SkillResult<()> {
    let mut pending = vec![path.to_string()];
    let mut visited_directories = 0usize;
    while let Some(current_path) = pending.pop() {
        visited_directories += 1;
        if visited_directories > MAX_IMPORTED_SKILL_FILES {
            return Err(skill_error(
                "a skill may contain at most 100 resource folders",
            ));
        }
        let entries = github_contents(client, owner, repo, reference, &current_path)?;
        for entry in entries {
            if files.len() >= MAX_IMPORTED_SKILL_FILES {
                return Err(skill_error(
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
                        skill_error("GitHub returned a file outside the skill folder")
                    })?;
                    if relative
                        .split('/')
                        .any(|part| part.is_empty() || part == "." || part == "..")
                    {
                        return Err(skill_error("GitHub returned an unsafe skill resource path"));
                    }
                    let content = decode_github_content(&entry)?;
                    let already_imported_bytes: usize =
                        files.iter().map(|file| file.content_base64.len()).sum();
                    if already_imported_bytes.saturating_add(content.len()) > 13_981_016 {
                        return Err(skill_error(
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
) -> SkillResult<(String, String, String, Vec<String>)> {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let frontmatter_and_body = raw
        .strip_prefix("---")
        .ok_or_else(|| skill_error("SKILL.md must begin with YAML frontmatter delimited by ---"))?;
    let frontmatter_and_body = frontmatter_and_body
        .strip_prefix('\n')
        .or_else(|| frontmatter_and_body.strip_prefix("\r\n"))
        .ok_or_else(|| skill_error("SKILL.md has invalid frontmatter delimiters"))?;
    let end = frontmatter_and_body
        .find("\n---")
        .ok_or_else(|| skill_error("SKILL.md has no closing frontmatter delimiter"))?;
    let yaml = &frontmatter_and_body[..end];
    let yaml: SkillFrontmatter = serde_yaml::from_str(yaml)
        .map_err(|error| skill_error(format!("SKILL.md frontmatter is not valid YAML: {error}")))?;
    let name = yaml.name.unwrap_or_default().trim().to_string();
    let description = yaml.description.unwrap_or_default().trim().to_string();
    if !is_valid_agent_skill_name(&name) {
        return Err(skill_error(
            "SKILL.md needs a lower-case, hyphenated name (up to 64 characters)",
        ));
    }
    if description.is_empty() {
        return Err(skill_error(
            "SKILL.md needs a non-empty description in its YAML frontmatter",
        ));
    }
    if description.chars().count() > 1024 {
        return Err(skill_error(
            "SKILL.md description must be 1,024 characters or fewer",
        ));
    }
    let body = frontmatter_and_body[end + 4..].trim().to_string();
    if body.is_empty() {
        return Err(skill_error(
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

pub fn preview(source_url: &str) -> SkillResult<SkillImportPreview> {
    let (owner, repo, reference, path) = parse_github_skill_url(source_url)?;
    let client = reqwest::blocking::Client::builder()
        .user_agent("DevCommandCenter")
        .build()
        .map_err(|error| skill_error(format!("HTTP client: {error}")))?;
    let entries = github_contents(&client, &owner, &repo, &reference, &path)?;
    let skill_entry = entries
        .iter()
        .find(|entry| entry.name == "SKILL.md" && entry.kind == "file")
        .ok_or_else(|| skill_error("This folder is not a skill: it has no SKILL.md file"))?;
    let markdown = String::from_utf8(decode_github_content(skill_entry)?)
        .map_err(|_| skill_error("SKILL.md must be UTF-8 text"))?;
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
            )?;
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

fn write_imported_resources(root: &Path, files: &[SkillImportFile]) -> SkillResult<()> {
    if files.len() > MAX_IMPORTED_SKILL_FILES {
        return Err(skill_error(
            "a skill may contain at most 100 imported resource files",
        ));
    }
    if fs::symlink_metadata(root)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(skill_error(
            "cannot import resources through a symbolic link",
        ));
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
            return Err(skill_error("skill import contains an unsafe resource path"));
        }
        let content = base64::engine::general_purpose::STANDARD
            .decode(&file.content_base64)
            .map_err(|error| skill_error(format!("invalid imported resource encoding: {error}")))?;
        if content.len() > MAX_IMPORTED_SKILL_FILE_BYTES {
            return Err(skill_error("an imported resource is larger than 1 MB"));
        }
        total_bytes = total_bytes.saturating_add(content.len());
        if total_bytes > 10 * 1024 * 1024 {
            return Err(skill_error(
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
                return Err(skill_error(
                    "cannot import resources through a symbolic link",
                ));
            }
            if !is_final && !current.exists() {
                fs::create_dir(&current).map_err(|error| {
                    skill_error(format!("create imported resource folder: {error}"))
                })?;
            }
        }
        fs::write(destination, content)
            .map_err(|error| skill_error(format!("write imported resource: {error}")))?;
    }
    Ok(())
}

fn reject_import_symlink(path: &Path) -> SkillResult<()> {
    if fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(skill_error(format!(
            "cannot import a skill through symbolic link {}",
            path.display()
        )));
    }
    Ok(())
}

/// `skills_import`: a previewed GitHub skill (with the person's edits to
/// its record) saved with its resources; refused through symbolic links or
/// over a skill of the same name.
pub fn import_skill(
    authorized: &str,
    mut preview: SkillImportPreview,
    skill: SkillRecord,
) -> SkillResult<()> {
    preview.skill = skill;
    if !is_valid_agent_skill_name(&preview.skill.name) {
        return Err(skill_error(
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
        return Err(skill_error(format!(
            "skill '{}' already exists in this project",
            preview.skill.name
        )));
    }
    save_skill(authorized, preview.skill)?;
    write_imported_resources(&skill_dir, &preview.files)?;
    Ok(())
}
