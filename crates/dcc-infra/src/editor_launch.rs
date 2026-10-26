//! "Abrir no editor" (`shell_open_in_editor`): the editors DCC knows, in the
//! picker's order, and how each opens a folder — the app by name on macOS,
//! its command line elsewhere. Shared by the desktop and the native app.

use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Editor {
    /// `EditorId` (`dcc-preferred-editor`).
    pub id: &'static str,
    pub label: &'static str,
    /// The application `open -a` launches on macOS.
    pub mac_app: &'static str,
    /// The command elsewhere.
    pub binary: &'static str,
}

/// `EDITOR_OPTIONS`; the first is the default.
pub const EDITORS: [Editor; 5] = [
    Editor {
        id: "cursor",
        label: "Cursor",
        mac_app: "Cursor",
        binary: "cursor",
    },
    Editor {
        id: "zed",
        label: "Zed",
        mac_app: "Zed",
        binary: "zed",
    },
    Editor {
        id: "vscode",
        label: "VS Code",
        mac_app: "Visual Studio Code",
        binary: "code",
    },
    Editor {
        id: "vscode-insiders",
        label: "VS Code Insiders",
        mac_app: "Visual Studio Code - Insiders",
        binary: "code-insiders",
    },
    Editor {
        id: "trae",
        label: "Trae",
        mac_app: "Trae",
        binary: "trae",
    },
];

pub fn editor(id: &str) -> Option<&'static Editor> {
    EDITORS.iter().find(|editor| editor.id == id)
}

/// Opens `path` in `editor` (a known id, or an application/command name).
pub fn open_in_editor(path: &Path, editor_id: &str) -> Result<(), String> {
    let editor_id = editor_id.trim();
    if editor_id.is_empty() {
        return Err("Editor is required".into());
    }
    let known = editor(editor_id);
    let status = if cfg!(target_os = "macos") {
        Command::new("open")
            .arg("-a")
            .arg(known.map_or(editor_id, |editor| editor.mac_app))
            .arg(path)
            .status()
    } else {
        Command::new(known.map_or(editor_id, |editor| editor.binary))
            .arg(path)
            .status()
    };
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err(format!("Failed to open {} in {editor_id}", path.display())),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_editors_keep_the_pickers_order_and_names() {
        let ids: Vec<&str> = EDITORS.iter().map(|editor| editor.id).collect();
        assert_eq!(ids, ["cursor", "zed", "vscode", "vscode-insiders", "trae"]);
        assert_eq!(
            editor("vscode").map(|e| e.mac_app),
            Some("Visual Studio Code")
        );
        assert_eq!(
            editor("vscode-insiders").map(|e| e.binary),
            Some("code-insiders")
        );
        assert!(editor("vim").is_none());
        assert!(open_in_editor(Path::new("/tmp"), "  ").is_err());
    }
}
