//! The floating terminal (Orca's "floating terminal"): shells that belong
//! to no task and no project. The button in the sidebar's top bar or ⌘⌥T
//! opens it over whatever screen is up, in `~`, with tabs of its own kept
//! in `terminal-tabs-floating.json`; hiding it leaves its shells running.
//! It is the task dock's machinery with nothing of a task on its bar: no
//! scope, no agent, no browser.

use std::path::PathBuf;

use gpui::{
    AnyElement, Context, DragMoveEvent, MouseUpEvent, SharedString, Window, div, prelude::*, px,
};

use super::dock::{ScopeKind, TerminalDock, TerminalPlace};
use super::dock_view::{DockResize, tip};
use super::tabs::TabStore;
use crate::data::desktop_api::DesktopApi;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon};

const TABS_FILE: &str = "terminal-tabs-floating.json";
/// Its scope's key: shells are `terminal:floating:<tab>` in the desktop,
/// which no task's count reads.
pub const FLOATING_SCOPE: &str = "floating";
/// Taller than the task dock's first height: it is the whole point.
const FIRST_HEIGHT: f32 = 420.0;

/// Where its shells start: the home folder.
fn home() -> String {
    std::env::var("HOME")
        .ok()
        .filter(|home| !home.is_empty())
        .unwrap_or_else(|| "/".into())
}

/// A place with only the "project" folder set, so the one scope there is
/// keyed `floating` and opens in `cwd`.
pub(super) fn floating_place(cwd: String) -> TerminalPlace {
    TerminalPlace {
        workspace_id: FLOATING_SCOPE.into(),
        project_id: Some(FLOATING_SCOPE.into()),
        root_path: Some(cwd),
        project_label: "~".into(),
        ..TerminalPlace::default()
    }
}

impl TerminalDock {
    pub fn floating_tabs_path() -> PathBuf {
        crate::prefs::native_file(TABS_FILE)
    }

    pub fn floating(tabs: TabStore, api: Option<DesktopApi>) -> Self {
        Self::floating_in(home(), tabs, api)
    }

    pub(super) fn floating_in(cwd: String, tabs: TabStore, api: Option<DesktopApi>) -> Self {
        let mut dock = Self::new(FIRST_HEIGHT, tabs, api);
        dock.floating = true;
        dock.place = Some(floating_place(cwd));
        dock.kind = ScopeKind::Project;
        dock
    }

    /// ⌘⌥T and the sidebar's button: opens and focuses it; focuses it when
    /// it shows without the focus; hides it when it has the focus.
    pub fn toggle_floating(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self.open
            && self
                .active_session(cx)
                .is_some_and(|session| session.read(cx).focus_handle().is_focused(window));
        if focused {
            self.hide(cx);
        } else {
            self.show(ScopeKind::Project, window, cx);
        }
    }

    fn floating_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let folder: SharedString = "~".into();
        let hide = Button::new("floating-terminal-hide")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
            .height(32.0)
            .padding_x(8.0)
            .on_click(cx.listener(|dock, _, _, cx| dock.hide(cx)))
            .child(Icon::new("minus").size(16.0));
        div()
            .flex()
            .h(px(crate::theme::Density::current(cx).terminal_toolbar()))
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0))
            .border_b_1()
            .border_color(colors.border.opacity(0.6))
            .px(px(8.0))
            .child(
                div()
                    .id("floating-terminal-tabs")
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(4.0))
                    .overflow_x_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(6.0))
                            .px(px(6.0))
                            .text_size(px(12.0))
                            .text_color(colors.muted_foreground)
                            .child(Icon::new("square-terminal").size(14.0))
                            .child(folder),
                    )
                    .children(self.tab_strip(cx)),
            )
            .child(self.actions_menu(cx))
            .child(self.expand_button(cx))
            .child(tip(
                "floating-terminal-hide-tip",
                cx.t("floatingTerminal.hide"),
                hide,
            ))
            .into_any_element()
    }

    /// The panel itself; the shell places it over the screen.
    pub(super) fn render_floating(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let parent = f32::from(window.viewport_size().height);
        div()
            .id("floating-terminal")
            .debug_selector(|| "floating-terminal".into())
            .occlude()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .rounded(px(10.0))
            .border_1()
            .border_color(colors.border)
            .bg(colors.card)
            .text_color(colors.card_foreground)
            .shadow_lg()
            .on_drag_move(
                cx.listener(move |dock, event: &DragMoveEvent<DockResize>, _, cx| {
                    dock.resize_to(f32::from(event.event.position.y), parent, cx)
                }),
            )
            .capture_any_mouse_up(
                cx.listener(|dock, _: &MouseUpEvent, _, cx| dock.finish_resize(cx)),
            )
            .children((!self.expanded).then(|| self.grip(cx)))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .flex_col()
                    .child(self.floating_toolbar(cx))
                    .child(self.panel(cx)),
            )
            .children(self.rename_dialog(window, cx))
            .children(self.close_dialog(cx))
            .into_any_element()
    }
}
