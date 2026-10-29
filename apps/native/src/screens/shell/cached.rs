//! The heavy views (sidebar, conversation, code column) drawn from their
//! last frame until they, or an entity they read, change: a running agent's
//! pulse in the sidebar or a terminal printing no longer lays out the
//! conversation and the diff again. A cached view is not measured, so each
//! gets the size its own root would take.
//!
//! While the Browser's page is up they are drawn fresh: an overlay marks
//! itself for the page as it prepaints (`ui::occlusion`), which a reused
//! frame skips.

use gpui::{AnyElement, App, Entity, Render, StyleRefinement, Styled, prelude::*, px};

use super::Shell;

impl Shell {
    fn cached<V: Render>(&self, view: &Entity<V>, style: StyleRefinement) -> AnyElement {
        if self.browser_open() {
            view.clone().into_any_element()
        } else {
            view.clone().cached(style).into_any_element()
        }
    }

    /// The sidebar fills its region.
    pub(super) fn sidebar_view(&self) -> AnyElement {
        self.cached(&self.sidebar, StyleRefinement::default().size_full())
    }

    /// `flex-1 min-h-0 min-w-0 size-full`, like the conversation's root.
    pub(super) fn conversation_view(&self) -> AnyElement {
        self.cached(
            &self.conversation,
            StyleRefinement::default()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .size_full(),
        )
    }

    /// The column at its tab's width, the full height.
    pub(super) fn code_column_view(&self, cx: &App) -> AnyElement {
        let width = self.code_column.read(cx).tab.width();
        self.cached(
            &self.code_column,
            StyleRefinement::default()
                .flex_shrink_0()
                .h_full()
                .w(px(width)),
        )
    }
}
