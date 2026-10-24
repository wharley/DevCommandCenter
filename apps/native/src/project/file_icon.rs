//! The Material icon of a file or folder in the code column
//! (`getMaterialFileIcon` / `getMaterialFolderIcon` from the desktop's
//! file-extension-icon-js). The icons are colored, so they draw as images.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use gpui::{Image, ImageFormat};
use serde::Deserialize;

mod generated {
    include!(concat!(env!("OUT_DIR"), "/file_icons.rs"));
}

#[derive(Deserialize)]
struct Names {
    files: HashMap<String, String>,
    extensions: HashMap<String, String>,
    folders: HashMap<String, String>,
}

fn names() -> &'static Names {
    static NAMES: OnceLock<Names> = OnceLock::new();
    NAMES.get_or_init(|| {
        serde_json::from_str(include_str!("../../assets/file-icons/names.json"))
            .expect("file icon names")
    })
}

/// `getMaterialFileIcon`: the longest dotted suffix with a name or an
/// extension of its own (`Cargo.lock`, `d.ts`, `ts`), else the plain file.
pub fn file_icon_name(file_name: &str) -> &'static str {
    let names = names();
    let parts: Vec<&str> = file_name.split('.').collect();
    for start in 0..parts.len() {
        let candidate = parts[start..].join(".");
        if let Some(icon) = names
            .files
            .get(&candidate)
            .or_else(|| names.extensions.get(&candidate))
        {
            return icon.as_str();
        }
    }
    "file"
}

/// `getMaterialFolderIcon`: by the folder's own name, else the plain folder.
pub fn folder_icon_name(folder_name: &str) -> &'static str {
    names()
        .folders
        .get(folder_name)
        .map(String::as_str)
        .unwrap_or("folder")
}

fn svg(table: &'static [(&'static str, &'static str)], name: &str) -> Option<&'static str> {
    table
        .binary_search_by(|(candidate, _)| candidate.cmp(&name))
        .ok()
        .map(|index| table[index].1)
}

/// The decoded image, kept once per icon.
fn image(key: String, source: &'static str) -> Arc<Image> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Image>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache
        .entry(key)
        .or_insert_with(|| {
            Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                source.as_bytes().to_vec(),
            ))
        })
        .clone()
}

pub fn file_icon(file_name: &str) -> Arc<Image> {
    let name = file_icon_name(file_name);
    let source = svg(generated::FILE_ICONS, name)
        .or_else(|| svg(generated::FILE_ICONS, "file"))
        .unwrap_or_default();
    image(format!("file:{name}"), source)
}

pub fn folder_icon(folder_name: &str, open: bool) -> Arc<Image> {
    let name = folder_icon_name(folder_name);
    let table = if open {
        generated::FOLDER_OPEN_ICONS
    } else {
        generated::FOLDER_ICONS
    };
    let source = svg(table, name)
        .or_else(|| svg(table, "folder"))
        .unwrap_or_default();
    image(
        format!("{}:{name}", if open { "open" } else { "folder" }),
        source,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_follow_the_desktop_tables() {
        assert_eq!(file_icon_name("main.rs"), "rust");
        assert_eq!(file_icon_name("package.json"), "nodejs");
        assert_eq!(file_icon_name("types.d.ts"), file_icon_name("x.d.ts"));
        assert_eq!(file_icon_name("noextension"), "file");
        assert_eq!(folder_icon_name("src"), "folder-src");
        assert_eq!(folder_icon_name("whatever-else"), "folder");
        assert!(svg(generated::FOLDER_OPEN_ICONS, "folder-src").is_some());
        assert!(svg(generated::FILE_ICONS, "file").is_some());
    }

    /// What the desktop's file-extension-icon-js picks for the same names.
    #[test]
    fn matches_the_desktop_picks() {
        for (name, icon) in [
            ("main.rs", "rust"),
            ("Cargo.toml", "settings"),
            ("Cargo.lock", "lock"),
            ("package.json", "nodejs"),
            ("yarn.lock", "yarn"),
            ("index.d.ts", "typescript-def"),
            ("App.tsx", "react_ts"),
            ("README.md", "markdown"),
            (".gitignore", "git"),
            ("Dockerfile", "file"),
            ("x.test.ts", "test-ts"),
            ("logo.svg", "svg"),
            (".env", "tune"),
            ("tsconfig.json", "json"),
            ("build.rs", "rust"),
            ("noext", "file"),
        ] {
            assert_eq!(file_icon_name(name), icon, "{name}");
        }
        for (name, icon) in [
            ("src", "folder-src"),
            ("node_modules", "folder-node"),
            (".github", "folder-github"),
            ("tests", "folder-test"),
            ("docs", "folder-docs"),
            ("apps", "folder-app"),
            ("crates", "folder"),
            ("whatever", "folder"),
        ] {
            assert_eq!(folder_icon_name(name), icon, "{name}");
        }
    }
}
