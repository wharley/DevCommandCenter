//! A file of a task that runs on another machine, on the stage: its text
//! is asked from the machine by the task's id and shown to read. It is not
//! edited from here (nothing writes on the machine through this), a binary
//! or oversized one says so instead of showing half, and an image is not
//! brought over yet. A change's diff is read the same way: its two versions
//! come from the machine's git.

use dcc_core::domain::indentation::{FileIndentation, resolve_file_indentation};
use dcc_core::domain::workspace_git::WorkspaceGitPreviewScope;
use gpui::App;

use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::data::machines::machine_of_path;
use crate::data::remote_files::Unreadable;
use crate::i18n::Translate;

/// Where a tab's file is read when its folder is on another machine.
#[derive(Clone)]
pub(crate) struct RemoteSource {
    api: DesktopApi,
    task: String,
    binary: String,
    too_big: String,
    pub image: String,
}

impl RemoteSource {
    /// `None` for a folder of this computer, or with no task to ask by.
    pub fn of(
        root: &str,
        task: Option<&str>,
        endpoint: Option<DesktopApi>,
        cx: &App,
    ) -> Option<Self> {
        let machine = machine_of_path(root)?;
        let api = endpoint.or_else(DesktopApi::load)?.on_machine(&machine);
        Some(Self {
            api,
            task: task?.to_string(),
            binary: cx.t("fileSurface.remoteBinary").to_string(),
            too_big: cx.t("fileSurface.remoteTooBig").to_string(),
            image: cx.t("fileSurface.remoteImage").to_string(),
        })
    }

    /// The file's text and its indentation (the project's `.editorconfig`
    /// on the machine, then the file's own), as a local file is read.
    pub fn read(&self, path: &str) -> Result<(String, FileIndentation), String> {
        let text = self
            .api
            .remote_file(&self.task, path)
            .map_err(reason)?
            .whole_text()
            .map_err(|why| match why {
                Unreadable::Binary => self.binary.clone(),
                Unreadable::TooBig => self.too_big.clone(),
            })?;
        let config = self
            .api
            .remote_file(&self.task, ".editorconfig")
            .ok()
            .and_then(|file| file.whole_text().ok());
        let indentation = resolve_file_indentation(config.as_deref(), path, &text);
        Ok((text, indentation))
    }

    /// A change's two versions and whether it reads as one column, as the
    /// local diff reads them from this computer's git.
    pub fn preview(
        &self,
        path: &str,
        status: &str,
        scope: &WorkspaceGitPreviewScope,
        base: Option<&str>,
    ) -> Result<(String, String, bool), String> {
        self.api
            .remote_preview(&self.task, path, status, scope, base)
            .map(|preview| (preview.original, preview.modified, preview.inline))
            .map_err(reason)
    }
}

fn reason(error: ApiError) -> String {
    match error {
        ApiError::Failed(message) if !message.trim().is_empty() => message,
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::*;
    use crate::test_support::fake_desktop_routes;

    const FILE: &str =
        "/api/v1/machines/m-1/proxy/api/v1/mobile/workspaces/w-1/file?path=src%2Fmain.rs";
    const CONFIG: &str =
        "/api/v1/machines/m-1/proxy/api/v1/mobile/workspaces/w-1/file?path=.editorconfig";
    const PREVIEW: &str = "/api/v1/machines/m-1/proxy/api/v1/mobile/workspaces/w-1/preview?path=README.md&status=M&scope=committed&base=origin%2Fmain";
    const BIG: &str = "/api/v1/machines/m-1/proxy/api/v1/mobile/workspaces/w-1/file?path=data.sql";

    #[gpui::test]
    fn a_machines_file_is_read_there_and_a_local_folder_is_not(cx: &mut TestAppContext) {
        cx.update(|cx| crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx));
        let (port, desktop) = fake_desktop_routes(vec![
            (
                FILE,
                200,
                r#"{"text":"fn main() {}\n","truncated":false,"binary":false}"#.into(),
            ),
            (
                CONFIG,
                200,
                r#"{"text":"[*]\nindent_style = tab\n","truncated":false,"binary":false}"#.into(),
            ),
            (
                PREVIEW,
                200,
                r#"{"original":"old\n","modified":"new\n","inline":false}"#.into(),
            ),
            (
                BIG,
                200,
                r#"{"text":"half","truncated":true,"binary":false}"#.into(),
            ),
        ]);
        let api = DesktopApi::new(port, "k");
        let source = cx.update(|cx| {
            // This computer's folder, or no task to ask by: read from disk.
            assert!(
                RemoteSource::of("/Users/me/app", Some("w-1"), Some(api.clone()), cx).is_none()
            );
            assert!(
                RemoteSource::of("dcc-machine://m-1/srv/app", None, Some(api.clone()), cx)
                    .is_none()
            );
            RemoteSource::of("dcc-machine://m-1/srv/app", Some("w-1"), Some(api), cx).unwrap()
        });
        let (text, indentation) = source.read("src/main.rs").unwrap();
        assert_eq!(text, "fn main() {}\n");
        assert!(indentation.use_tabs, "the machine's .editorconfig");
        // Half a file is not shown as the whole.
        assert_eq!(source.read("data.sql").unwrap_err(), source.too_big);
        // A change's two versions come from the machine's git.
        let (original, modified, inline) = source
            .preview(
                "README.md",
                "M",
                &WorkspaceGitPreviewScope::Committed,
                Some("origin/main"),
            )
            .unwrap();
        assert_eq!(
            (original.as_str(), modified.as_str(), inline),
            ("old\n", "new\n", false)
        );
        desktop.join().unwrap();
    }
}
