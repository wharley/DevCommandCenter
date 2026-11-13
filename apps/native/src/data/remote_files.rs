//! The files of a task that runs on another machine, to read: the whole
//! list the tree draws (what git tracks there, plus the ignored files
//! outside ignored folders) and one file's text. Both are asked by the
//! task's id on the machine's API (`DesktopApi::on_machine`); nothing here
//! writes.

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
