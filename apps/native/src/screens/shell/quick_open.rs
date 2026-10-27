//! ⌘P and ⌘⇧F over the window (`setIsQuickOpenOpen`,
//! `setIsWorkspaceSearchOpen`): only with a task's checkout open, even from
//! a field (a modifier chord is never typed). The picked file opens on the
//! stage, at the line for a search hit, marked in the column's tree.

use gpui::{AnyElement, AppContext, Context, Entity, IntoElement, Subscription, Window};

use super::{Shell, Surface};
use crate::screens::quick_open::{QuickOpen, QuickOpenEvent};
use crate::screens::workspace_search::{WorkspaceSearch, WorkspaceSearchEvent};

gpui::actions!(
    dcc_shell,
    [
        /// ⌘P: a file of the checkout by name.
        OpenQuickOpen,
        /// ⌘⇧F: text across the checkout.
        OpenWorkspaceSearch
    ]
);

pub(super) fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("cmd-p", OpenQuickOpen, None),
        gpui::KeyBinding::new("cmd-shift-f", OpenWorkspaceSearch, None),
    ]);
}

pub(super) enum Finder {
    QuickOpen(Entity<QuickOpen>),
    Search(Entity<WorkspaceSearch>),
}

pub(super) struct FileFinder {
    finder: Finder,
    _events: Subscription,
}

impl Shell {
    /// The open task's checkout (`selectedWorkspacePath`).
    fn checkout(&self, cx: &gpui::App) -> Option<String> {
        self.conversation.read(cx).workspace_path()
    }

    pub(super) fn open_quick_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Already open: `setIsQuickOpenOpen(true)` again changes nothing.
        if matches!(
            self.file_finder.as_ref().map(|open| &open.finder),
            Some(Finder::QuickOpen(_))
        ) {
            return;
        }
        let Some(root) = self.checkout(cx) else {
            return;
        };
        let open = cx.new(|cx| QuickOpen::new(root, window, cx));
        let events = cx.subscribe_in(&open, window, |shell, _, event, window, cx| {
            shell.file_finder = None;
            if let QuickOpenEvent::Picked(path) = event {
                shell.open_checkout_file(path.clone(), None, window, cx);
            }
            cx.notify();
        });
        self.file_finder = Some(FileFinder {
            finder: Finder::QuickOpen(open),
            _events: events,
        });
        cx.notify();
    }

    pub(super) fn open_workspace_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(
            self.file_finder.as_ref().map(|open| &open.finder),
            Some(Finder::Search(_))
        ) {
            return;
        }
        let Some(root) = self.checkout(cx) else {
            return;
        };
        let search = cx.new(|cx| WorkspaceSearch::new(root, window, cx));
        let events = cx.subscribe_in(&search, window, |shell, _, event, window, cx| {
            shell.file_finder = None;
            if let WorkspaceSearchEvent::Picked { path, line } = event {
                shell.open_checkout_file(path.clone(), Some(*line), window, cx);
            }
            cx.notify();
        });
        self.file_finder = Some(FileFinder {
            finder: Finder::Search(search),
            _events: events,
        });
        cx.notify();
    }

    /// `requestSurfaceSelection({ kind: "file-edit", focusLine })`.
    fn open_checkout_file(
        &mut self,
        path: String,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.checkout(cx) else {
            return;
        };
        self.code_column.update(cx, |column, cx| {
            column.set_open_diff(None, cx);
            column.reveal(Some(path.clone()), cx);
        });
        self.stage.update(cx, |stage, cx| {
            stage.open_file(root, path, line, window, cx)
        });
        self.show(Surface::Conversation, cx);
    }

    pub(super) fn file_finder_overlay(&self) -> Option<AnyElement> {
        self.file_finder.as_ref().map(|open| match &open.finder {
            Finder::QuickOpen(view) => view.clone().into_any_element(),
            Finder::Search(view) => view.clone().into_any_element(),
        })
    }
}
