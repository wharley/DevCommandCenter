//! The files of a task that runs on another machine, to read: the whole
//! list the tree draws (what git tracks there, plus the ignored files
//! outside ignored folders), one file's text, and the two versions of a
//! changed file for its diff. All are asked by the
//! task's id on the machine's API (`DesktopApi::on_machine`); nothing here
//! writes.

use dcc_core::domain::workspace_git::WorkspaceGitPreviewScope;
use serde::Deserialize;

use super::desktop_api::{ApiError, DesktopApi, encode};

/// The machine's listing of a task's checkout.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct RemoteTree {
    pub tracked: Vec<String>,
    pub ignored: Vec<String>,
}

/// One file as the machine read it: cut at its own cap, and no text for a
/// binary one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct RemoteFile {
    pub text: String,
    pub truncated: bool,
    pub binary: bool,
}

/// A changed file's two versions, as the machine's git reads them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct RemotePreview {
    pub original: String,
    pub modified: String,
    /// Added, removed or new: one column instead of side by side.
    pub inline: bool,
}

/// `scope` as the machine's route reads it.
fn scope_name(scope: &WorkspaceGitPreviewScope) -> &'static str {
    match scope {
        WorkspaceGitPreviewScope::Staged => "staged",
        WorkspaceGitPreviewScope::Unstaged => "unstaged",
        WorkspaceGitPreviewScope::Committed => "committed",
    }
}

/// Why a file of the machine does not open here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unreadable {
    Binary,
    /// Past what the machine sends: half a file would read as the whole.
    TooBig,
}

impl RemoteFile {
    /// The file's whole text, or why it is not shown.
    pub fn whole_text(self) -> Result<String, Unreadable> {
        if self.binary {
            Err(Unreadable::Binary)
        } else if self.truncated {
            Err(Unreadable::TooBig)
        } else {
            Ok(self.text)
        }
    }
}

fn decode<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
}

impl DesktopApi {
    pub fn remote_tree(&self, workspace_id: &str) -> Result<RemoteTree, ApiError> {
        decode(self.get(&format!(
            "/api/v1/mobile/workspaces/{}/tree",
            encode(workspace_id)
        ))?)
    }

    /// A change's diff: the file against HEAD when not committed yet, HEAD
    /// against `base` for one of the branch.
    pub fn remote_preview(
        &self,
        workspace_id: &str,
        path: &str,
        status: &str,
        scope: &WorkspaceGitPreviewScope,
        base: Option<&str>,
    ) -> Result<RemotePreview, ApiError> {
        let mut address = format!(
            "/api/v1/mobile/workspaces/{}/preview?path={}&status={}&scope={}",
            encode(workspace_id),
            encode(path),
            encode(status),
            scope_name(scope)
        );
        if let Some(base) = base.filter(|base| !base.is_empty()) {
            address.push_str(&format!("&base={}", encode(base)));
        }
        decode(self.get(&address)?)
    }

    pub fn remote_file(&self, workspace_id: &str, path: &str) -> Result<RemoteFile, ApiError> {
        decode(self.get(&format!(
            "/api/v1/mobile/workspaces/{}/file?path={}",
            encode(workspace_id),
            encode(path)
        ))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_opens_whole_or_says_why_not() {
        let file = |text: &str, truncated, binary| RemoteFile {
            text: text.into(),
            truncated,
            binary,
        };
        assert_eq!(
            file("fn main() {}", false, false).whole_text().unwrap(),
            "fn main() {}"
        );
        assert_eq!(
            file("half", true, false).whole_text(),
            Err(Unreadable::TooBig)
        );
        assert_eq!(file("", false, true).whole_text(), Err(Unreadable::Binary));
    }

    #[test]
    fn the_machines_answers_decode() {
        let tree: RemoteTree = decode(serde_json::json!({
            "tracked": ["src/main.rs"], "ignored": [".env"]
        }))
        .unwrap();
        assert_eq!((tree.tracked.len(), tree.ignored[0].as_str()), (1, ".env"));
        let file: RemoteFile = decode(serde_json::json!({
            "path": "a.rs", "text": "x", "truncated": false, "binary": false, "size": 1
        }))
        .unwrap();
        assert_eq!(file.text, "x");
    }
}
