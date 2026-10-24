//! Mission specs live as `.devcommandcenter/specs/*.spec.md` inside the
//! workspace. The listing rules (inside the workspace, regular files, sorted
//! by name) are shared by the desktop's `list_mission_specs` and the native
//! composer's provider handoff.

use std::path::PathBuf;

/// The specs folder (canonical) and each spec file's name and path.
pub type SpecFiles = (PathBuf, Vec<(String, PathBuf)>);

/// The spec files sorted by name; `None` when the workspace has no specs folder.
pub fn spec_files(workspace_root: &str) -> Result<Option<SpecFiles>, String> {
    let root = workspace_root.trim();
    if root.is_empty() {
        return Ok(None);
    }
    let specs_dir = PathBuf::from(root).join(".devcommandcenter").join("specs");
    if !specs_dir.is_dir() {
        return Ok(None);
    }
    let root_canonical = PathBuf::from(root)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let specs_canonical = specs_dir
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !specs_canonical.starts_with(&root_canonical) {
        return Err("mission specs directory must stay inside the workspace".to_string());
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&specs_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !file_name.ends_with(".spec.md") {
            continue;
        }
        let file_canonical = path.canonicalize().map_err(|error| error.to_string())?;
        if !file_canonical.starts_with(&specs_canonical) {
            continue;
        }
        files.push((file_name.to_string(), path));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(Some((specs_canonical, files)))
}

/// The first spec's text (`specs[0].content`), for the provider handoff.
pub fn first_spec_content(workspace_root: &str) -> Option<String> {
    let (_, files) = spec_files(workspace_root).ok()??;
    let (_, path) = files.first()?;
    std::fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_specs_sorted_and_reads_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let specs = dir.path().join(".devcommandcenter").join("specs");
        std::fs::create_dir_all(&specs).unwrap();
        std::fs::write(specs.join("b.spec.md"), "second").unwrap();
        std::fs::write(specs.join("a.spec.md"), "first").unwrap();
        std::fs::write(specs.join("notes.md"), "skip").unwrap();
        let root = dir.path().to_string_lossy().to_string();
        let (_, files) = spec_files(&root).unwrap().unwrap();
        assert_eq!(
            files
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["a.spec.md", "b.spec.md"]
        );
        assert_eq!(first_spec_content(&root).as_deref(), Some("first"));
        assert_eq!(spec_files("/definitely/not/here").unwrap(), None);
    }
}
