//! "Abrir no editor" (`openInEditor`): the task's folder in the chosen
//! editor, which becomes the preferred one; ⌘O opens the preferred one
//! (outside fields, like the desktop's `shouldIgnoreGlobalShortcutTarget`).

use gpui::Context;

use super::Shell;
use crate::prefs::Prefs;
use crate::ui::Toaster;

gpui::actions!(
    dcc_shell,
    [
        /// ⌘O: the task's folder in the preferred editor.
        OpenInPreferredEditor
    ]
);

pub(super) fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("cmd-o", OpenInPreferredEditor, None),
        gpui::KeyBinding::new("cmd-o", gpui::NoAction, Some("TextArea")),
        gpui::KeyBinding::new("cmd-o", gpui::NoAction, Some("TextInput")),
        gpui::KeyBinding::new("cmd-o", gpui::NoAction, Some("CodeEditor")),
        gpui::KeyBinding::new("cmd-o", gpui::NoAction, Some("Terminal")),
    ]);
}

impl Shell {
    pub(super) fn open_in_editor(&mut self, editor: &str, cx: &mut Context<Self>) {
        let Some(path) = self
            .conversation
            .read(cx)
            .data()
            .and_then(|data| data.workspace_path.clone())
        else {
            return;
        };
        if self.prefs.preferred_editor != editor {
            self.prefs = Prefs {
                preferred_editor: editor.to_string(),
                ..self.prefs.clone()
            };
            if let Err(error) = self.prefs.save(&self.prefs_path) {
                eprintln!("[dcc-native] could not save the editor: {error}");
            }
            self.sync_header(cx);
        }
        let editor = editor.to_string();
        cx.spawn(async move |this, cx| {
            let opened = cx
                .background_executor()
                .spawn(async move {
                    dcc_infra::editor_launch::open_in_editor(std::path::Path::new(&path), &editor)
                })
                .await;
            if let Err(message) = opened {
                let _ = this.update(cx, |_, cx| Toaster::error(message, cx));
            }
        })
        .detach();
    }

    pub(super) fn open_in_preferred_editor(&mut self, cx: &mut Context<Self>) {
        let editor = self.prefs.preferred_editor.clone();
        self.open_in_editor(&editor, cx);
    }

    /// The preferred editor's id, as a `&'static` the header can keep.
    pub(super) fn preferred_editor(&self) -> &'static str {
        dcc_infra::editor_launch::editor(&self.prefs.preferred_editor)
            .unwrap_or(&dcc_infra::editor_launch::EDITORS[0])
            .id
    }
}
