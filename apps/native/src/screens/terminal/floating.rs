//! The floating terminal (Orca's "floating terminal"): shells that belong
//! to no task and no project. The button in the sidebar's top bar or ⌘⌥T
//! opens it over whatever screen is up, in `~`, with tabs of its own kept
//! in `terminal-tabs-floating.json`; hiding it leaves its shells running.
//! It is the task dock's machinery with nothing of a task on its bar: no
//! scope, no agent, no browser. Like Orca's, it is a window of its own in
//! the bottom-right corner (`floating-terminal-panel-bounds.ts`): moved by
//! its bar, sized by its top and left edges, maximized by its button.

use std::path::PathBuf;

use gpui::{
    AnyElement, Bounds, Context, CursorStyle, DragMoveEvent, EmptyView, MouseButton,
    MouseDownEvent, MouseUpEvent, SharedString, Size, Window, div, point, prelude::*, px, size,
};

use super::dock::{ScopeKind, TerminalDock, TerminalPlace};
use super::dock_view::tip;
use super::tabs::TabStore;
use crate::data::desktop_api::DesktopApi;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon};

const TABS_FILE: &str = "terminal-tabs-floating.json";
/// Its scope's key: shells are `terminal:floating:<tab>` in the desktop,
/// which no task's count reads.
pub const FLOATING_SCOPE: &str = "floating";
/// Orca's `DEFAULT_PANEL_WIDTH` / `DEFAULT_PANEL_HEIGHT`.
const DEFAULT_WIDTH: f32 = 920.0;
const DEFAULT_HEIGHT: f32 = 560.0;
const MIN_WIDTH: f32 = 420.0;
const MIN_HEIGHT: f32 = 240.0;
/// Its gap to the window's edges, and where it starts (Orca's right gap).
const EDGE: f32 = 12.0;
const CORNER_GAP: f32 = 24.0;
/// The resize edges' hit size.
const GRAB: f32 = 6.0;

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

/// The window's place, kept from the bottom-right corner so it stays there
/// when the app's window is resized.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Float {
    pub right: f32,
    pub bottom: f32,
    pub width: f32,
    pub height: f32,
    drag: Option<FloatDrag>,
}

impl Default for Float {
    fn default() -> Self {
        Self {
            right: CORNER_GAP,
            bottom: CORNER_GAP,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            drag: None,
        }
    }
}

/// What a drag on the window changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grip {
    Move,
    Top,
    Left,
    TopLeft,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FloatDrag {
    grip: Grip,
    pointer: (f32, f32),
    /// Its right, bottom, width and height when the drag started.
    from: [f32; 4],
}

/// The drag payload, so only the window's own listener sees it.
#[derive(Clone)]
pub struct FloatGrip;

impl Float {
    /// Its rectangle in a window this big: sized and kept inside it.
    pub fn rect(&self, viewport: Size<f32>, maximized: bool) -> Bounds<f32> {
        let room_w = (viewport.width - 2.0 * EDGE).max(0.0);
        let room_h = (viewport.height - 2.0 * EDGE).max(0.0);
        if maximized {
            return Bounds::new(point(EDGE, EDGE), size(room_w, room_h));
        }
        let width = self.width.clamp(MIN_WIDTH.min(room_w), room_w);
        let height = self.height.clamp(MIN_HEIGHT.min(room_h), room_h);
        let right = self
            .right
            .clamp(EDGE, (viewport.width - width - EDGE).max(EDGE));
        let bottom = self
            .bottom
            .clamp(EDGE, (viewport.height - height - EDGE).max(EDGE));
        Bounds::new(
            point(
                viewport.width - right - width,
                viewport.height - bottom - height,
            ),
            size(width, height),
        )
    }

    /// The place it is drawn at, so a drag starts from what shows.
    fn settled(&self, viewport: Size<f32>) -> Self {
        let rect = self.rect(viewport, false);
        Self {
            right: viewport.width - rect.origin.x - rect.size.width,
            bottom: viewport.height - rect.origin.y - rect.size.height,
            width: rect.size.width,
            height: rect.size.height,
            drag: None,
        }
    }

    fn start(&mut self, grip: Grip, pointer: (f32, f32), viewport: Size<f32>) {
        *self = self.settled(viewport);
        self.drag = Some(FloatDrag {
            grip,
            pointer,
            from: [self.right, self.bottom, self.width, self.height],
        });
    }

    /// Follows the pointer; the corner it is kept from moves only on Move.
    fn follow(&mut self, pointer: (f32, f32)) -> bool {
        let Some(drag) = self.drag else {
            return false;
        };
        let (dx, dy) = (pointer.0 - drag.pointer.0, pointer.1 - drag.pointer.1);
        let [right, bottom, width, height] = drag.from;
        let next = match drag.grip {
            Grip::Move => [right - dx, bottom - dy, width, height],
            Grip::Top => [right, bottom, width, height - dy],
            Grip::Left => [right, bottom, width - dx, height],
            Grip::TopLeft => [right, bottom, width - dx, height - dy],
        };
        let changed = next != [self.right, self.bottom, self.width, self.height];
        [self.right, self.bottom, self.width, self.height] = next;
        changed
    }

    fn finish(&mut self, viewport: Size<f32>) -> bool {
        if self.drag.is_none() {
            return false;
        }
        *self = self.settled(viewport);
        true
    }
}

fn viewport_of(window: &Window) -> Size<f32> {
    let viewport = window.viewport_size();
    size(f32::from(viewport.width), f32::from(viewport.height))
}

impl TerminalDock {
    pub fn floating_tabs_path() -> PathBuf {
        crate::prefs::native_file(TABS_FILE)
    }

    pub fn floating(tabs: TabStore, api: Option<DesktopApi>) -> Self {
        Self::floating_in(home(), tabs, api)
    }

    pub(super) fn floating_in(cwd: String, tabs: TabStore, api: Option<DesktopApi>) -> Self {
        let mut dock = Self::new(DEFAULT_HEIGHT, tabs, api);
        dock.floating = true;
        dock.place = Some(floating_place(cwd));
        dock.kind = ScopeKind::Project;
        dock
    }

    /// Where the shell draws the window, in a window this big.
    pub fn floating_rect(&self, viewport: Size<f32>) -> Bounds<f32> {
        self.float.rect(viewport, self.expanded)
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

    fn start_float(&mut self, grip: Grip, event: &MouseDownEvent, window: &Window) {
        if self.expanded && grip != Grip::Move {
            return;
        }
        let pointer = (f32::from(event.position.x), f32::from(event.position.y));
        self.float.start(grip, pointer, viewport_of(window));
    }

    /// An edge (or the bar) the window is dragged by.
    fn grip_area(&self, grip: Grip, cx: &mut Context<Self>) -> gpui::Div {
        let cursor = match grip {
            Grip::Move => CursorStyle::Arrow,
            Grip::Top => CursorStyle::ResizeUpDown,
            Grip::Left => CursorStyle::ResizeLeftRight,
            Grip::TopLeft => CursorStyle::ResizeUpLeftDownRight,
        };
        div().cursor(cursor).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |dock, event: &MouseDownEvent, window, _| {
                dock.start_float(grip, event, window)
            }),
        )
    }

    fn edges(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if self.expanded {
            return Vec::new();
        }
        let edge = |grip: Grip, id: &'static str, cx: &mut Context<Self>| {
            self.grip_area(grip, cx)
                .id(id)
                .absolute()
                .occlude()
                .on_drag(FloatGrip, move |_, _, window, cx| {
                    let style = match grip {
                        Grip::Top => CursorStyle::ResizeUpDown,
                        Grip::Left => CursorStyle::ResizeLeftRight,
                        _ => CursorStyle::ResizeUpLeftDownRight,
                    };
                    cx.set_active_drag_cursor_style(style, window);
                    cx.new(|_| EmptyView)
                })
        };
        vec![
            edge(Grip::Top, "floating-terminal-top", cx)
                .top_0()
                .left(px(GRAB * 2.0))
                .right_0()
                .h(px(GRAB))
                .into_any_element(),
            edge(Grip::Left, "floating-terminal-left", cx)
                .left_0()
                .top(px(GRAB * 2.0))
                .bottom_0()
                .w(px(GRAB))
                .into_any_element(),
            edge(Grip::TopLeft, "floating-terminal-corner", cx)
                .top_0()
                .left_0()
                .size(px(GRAB * 2.0))
                .into_any_element(),
        ]
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
        self.grip_area(Grip::Move, cx)
            .id("floating-terminal-bar")
            .debug_selector(|| "floating-terminal-bar".into())
            .on_drag(FloatGrip, |_, _, _, cx| cx.new(|_| EmptyView))
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

    /// The window itself; the shell places it (`floating_rect`).
    pub(super) fn render_floating(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let edges = self.edges(cx);
        div()
            .id("floating-terminal")
            .debug_selector(|| "floating-terminal".into())
            .occlude()
            .relative()
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
                cx.listener(|dock, event: &DragMoveEvent<FloatGrip>, _, cx| {
                    let pointer = event.event.position;
                    if dock
                        .float
                        .follow((f32::from(pointer.x), f32::from(pointer.y)))
                    {
                        cx.notify();
                    }
                }),
            )
            .capture_any_mouse_up(cx.listener(|dock, _: &MouseUpEvent, window, cx| {
                if dock.float.finish(viewport_of(window)) {
                    cx.notify();
                }
            }))
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
            .children(edges)
            .children(self.rename_dialog(window, cx))
            .children(self.close_dialog(cx))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Size<f32> {
        size(1600.0, 1000.0)
    }

    #[test]
    fn it_starts_in_the_bottom_right_corner_like_orca() {
        let rect = Float::default().rect(viewport(), false);
        assert_eq!(rect.size, size(920.0, 560.0));
        assert_eq!(
            rect.origin,
            point(1600.0 - 24.0 - 920.0, 1000.0 - 24.0 - 560.0)
        );
        // Maximized, it takes the whole window but its gap.
        let max = Float::default().rect(viewport(), true);
        assert_eq!(max, Bounds::new(point(12.0, 12.0), size(1576.0, 976.0)));
    }

    #[test]
    fn a_small_window_keeps_it_inside() {
        let rect = Float::default().rect(size(700.0, 400.0), false);
        assert_eq!(rect.size, size(676.0, 376.0));
        assert_eq!(rect.origin, point(12.0, 12.0));
    }

    #[test]
    fn its_bar_moves_it_and_its_corner_sizes_it() {
        let mut float = Float::default();
        float.start(Grip::Move, (1000.0, 500.0), viewport());
        assert!(float.follow((700.0, 300.0)));
        assert!(float.finish(viewport()));
        let rect = float.rect(viewport(), false);
        assert_eq!(rect.origin, point(656.0 - 300.0, 416.0 - 200.0));
        assert_eq!(rect.size, size(920.0, 560.0));
        // Dragged past the edge, it stops at the gap.
        float.start(Grip::Move, (0.0, 0.0), viewport());
        float.follow((-5000.0, 0.0));
        float.finish(viewport());
        assert_eq!(float.rect(viewport(), false).origin.x, 12.0);
        // The top-left corner grows it up and left; the bottom-right stays.
        let mut float = Float::default();
        let before = float.rect(viewport(), false);
        float.start(Grip::TopLeft, (0.0, 0.0), viewport());
        float.follow((-100.0, -40.0));
        float.finish(viewport());
        let after = float.rect(viewport(), false);
        assert_eq!(after.size, size(1020.0, 600.0));
        assert_eq!(after.bottom_right(), before.bottom_right());
        // Never smaller than its minimum.
        float.start(Grip::Left, (0.0, 0.0), viewport());
        float.follow((5000.0, 0.0));
        float.finish(viewport());
        assert_eq!(float.rect(viewport(), false).size.width, MIN_WIDTH);
    }
}
