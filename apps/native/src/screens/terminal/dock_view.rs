//! The dock as `WorkspaceTerminalDrawer` draws it, split under the
//! conversation: the 6px resize grip, the 40px toolbar (the scope, the
//! tabs with their activity dot, `+`, hide) and the active tab's surface.

use gpui::{
    AnyElement, BoxShadow, Context, CursorStyle, DragMoveEvent, EmptyView, FontWeight, MouseButton,
    MouseDownEvent, MouseUpEvent, Render, SharedString, Window, div, point, prelude::*, px,
};

use super::dock::{ScopeKind, TerminalDock};
use super::session::SessionStatus;
use super::tabs::{MAX_TABS, owner_key};
use crate::data::terminal_api::ActivityStatus;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip};

/// `--dcc-terminal-toolbar-height`
const TOOLBAR: f32 = 40.0;

/// The dock's own drag, so only its parent's `on_drag_move` sees it.
#[derive(Clone)]
pub struct DockResize;

/// `statusDotClass`
fn dot_color(status: ActivityStatus, cx: &gpui::App) -> gpui::Hsla {
    let colors = &cx.theme().colors;
    match status {
        ActivityStatus::Running => tw::SKY_500.hsla(),
        ActivityStatus::Waiting => tw::AMBER_400.hsla(),
        ActivityStatus::Error => colors.destructive,
        ActivityStatus::Exited => colors.muted_foreground,
        ActivityStatus::Idle => colors.muted_foreground.opacity(0.45),
    }
}

fn tip(id: &'static str, text: SharedString, child: impl IntoElement) -> AnyElement {
    Tooltip::new(id, child, move |_, _| {
        div().child(text.clone()).into_any_element()
    })
    .into_any_element()
}

impl TerminalDock {
    fn grip(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let hover = colors.muted.opacity(0.6);
        let handle = div()
            .id("terminal-dock-resize")
            .group("terminal-dock-resize")
            .flex()
            .h(px(6.0))
            .w_full()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .border_b_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.muted.opacity(0.2))
            .hover(move |style| style.bg(hover))
            .cursor(CursorStyle::ResizeUpDown)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|dock, event: &MouseDownEvent, _, _| {
                    dock.start_resize(f32::from(event.position.y))
                }),
            )
            .on_drag(DockResize, |_, _, window, cx| {
                cx.set_active_drag_cursor_style(CursorStyle::ResizeUpDown, window);
                cx.new(|_| EmptyView)
            })
            .child(
                div()
                    .opacity(0.0)
                    .group_hover("terminal-dock-resize", |style| style.opacity(1.0))
                    .child(
                        Icon::new("grip-horizontal")
                            .size(12.0)
                            .color(colors.muted_foreground),
                    ),
            );
        tip(
            "terminal-dock-resize-tip",
            cx.t("terminalDock.resizeHint"),
            handle,
        )
    }

    fn scope_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let scope = self.scope()?;
        let colors = cx.theme().colors.clone();
        let (label, icon, color) = match scope.kind {
            ScopeKind::Worktree => (
                cx.t("terminalDock.scopes.worktree"),
                "shield-check",
                tw::EMERALD_500.hsla(),
            ),
            ScopeKind::Project => (
                cx.t("terminalDock.scopes.project"),
                "triangle-alert",
                tw::AMBER_500.hsla(),
            ),
        };
        let mut text = format!("{label} · {}", scope.project_label);
        if let Some(branch) = &scope.branch {
            text.push_str(&format!(" · {branch}"));
        }
        Some(
            Button::new("terminal-scope")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .padding_x(8.0)
                .text_size(12.0)
                .text_color(colors.muted_foreground)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .mr(px(4.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(Icon::new(icon).size(14.0).color(color))
                        .child(div().max_w(px(288.0)).truncate().child(text))
                        .child(
                            div()
                                .opacity(0.5)
                                .child(Icon::new("chevron-down").size(12.0)),
                        ),
                )
                .into_any_element(),
        )
        .map(|button| {
            div()
                .flex_shrink_0()
                .mr(px(4.0))
                .rounded(px(6.0))
                .bg(colors.muted.opacity(0.3))
                .child(button)
                .into_any_element()
        })
    }

    fn tab_strip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let scope = self.scope()?;
        let colors = cx.theme().colors.clone();
        let scope_tabs = self.tabs.scope(&scope.key);
        let active = scope_tabs.active().map(|tab| tab.id.clone());
        let tabs =
            scope_tabs.tabs.iter().enumerate().map(|(index, tab)| {
                let session = self.sessions.get(&owner_key(&scope.key, &tab.id));
                let (status, label) = session
                    .map(|session| {
                        let session = session.read(cx);
                        let status = match (&session.status, &session.activity) {
                            (SessionStatus::Exited(_), _) => ActivityStatus::Exited,
                            (SessionStatus::Failed, _) => ActivityStatus::Error,
                            (_, Some(activity)) => activity.status,
                            _ => ActivityStatus::Idle,
                        };
                        let label = session
                            .activity
                            .as_ref()
                            .and_then(|activity| activity.process_label.clone());
                        (status, label)
                    })
                    .unwrap_or((ActivityStatus::Idle, None));
                // `terminalDisplayTitle`: "Terminal N" shows what runs in it.
                let numbered = tab
                    .title
                    .strip_prefix("Terminal ")
                    .is_some_and(|rest| rest.parse::<u32>().is_ok());
                let title = match (&label, numbered) {
                    (Some(label), true) => label.clone(),
                    _ => tab.title.clone(),
                };
                let is_active = active.as_deref() == Some(tab.id.as_str());
                let group: SharedString = format!("terminal-tab-{index}").into();
                let id = tab.id.clone();
                let close_id = tab.id.clone();
                let hover_bg = colors.muted.opacity(0.5);
                let foreground = colors.foreground;
                // `--dcc-elevation-1`
                let shadow = vec![
                    BoxShadow {
                        color: gpui::black().opacity(0.08),
                        offset: point(px(0.0), px(1.0)),
                        blur_radius: px(3.0),
                        spread_radius: px(0.0),
                        inset: false,
                    },
                    BoxShadow {
                        color: gpui::white().opacity(0.02),
                        offset: point(px(0.0), px(0.0)),
                        blur_radius: px(0.0),
                        spread_radius: px(1.0),
                        inset: false,
                    },
                ];
                let icon_button = |name: &'static str, element: (&'static str, usize)| {
                    let muted = colors.muted_foreground.opacity(0.6);
                    let hover = colors.muted;
                    div()
                        .id(element)
                        .rounded(px(4.0))
                        .p(px(2.0))
                        .text_color(muted)
                        .opacity(0.0)
                        .group_hover(group.clone(), |style| style.opacity(1.0))
                        .hover(move |style| style.bg(hover).text_color(foreground))
                        .child(Icon::new(name).size(12.0))
                };
                div()
                    .id(("terminal-tab", index))
                    .debug_selector(move || format!("terminal-tab-{index}"))
                    .group(group.clone())
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(6.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .text_size(px(12.0))
                    .map(|tab| {
                        if is_active {
                            tab.bg(colors.background)
                                .text_color(colors.foreground)
                                .shadow(shadow)
                        } else {
                            tab.text_color(colors.muted_foreground)
                                .hover(move |style| style.bg(hover_bg).text_color(foreground))
                        }
                    })
                    .child(
                        div()
                            .id(("terminal-tab-select", index))
                            .flex()
                            .max_w(px(144.0))
                            .items_center()
                            .gap(px(6.0))
                            .on_click(cx.listener(move |dock, _, window, cx| {
                                dock.select_tab(&id, window, cx)
                            }))
                            .child(
                                div()
                                    .size(px(6.0))
                                    .flex_shrink_0()
                                    .rounded_full()
                                    .bg(dot_color(status, cx)),
                            )
                            .child(div().truncate().child(title)),
                    )
                    .child(icon_button("pencil", ("terminal-tab-rename", index)))
                    .child(
                        icon_button("x", ("terminal-tab-close", index))
                            .debug_selector(move || format!("terminal-tab-close-{index}"))
                            .on_click(cx.listener(move |dock, _, _, cx| {
                                cx.stop_propagation();
                                dock.close_tab(&close_id, cx)
                            })),
                    )
            });
        let at_cap = scope_tabs.tabs.len() >= MAX_TABS;
        let plus_tip = if at_cap {
            cx.t_with(
                "terminalDock.maxTabs",
                &Args::default().with("count", MAX_TABS.to_string()),
            )
        } else if scope.cwd.is_some() {
            let label = match scope.kind {
                ScopeKind::Worktree => cx.t("terminalDock.scopes.worktree"),
                ScopeKind::Project => cx.t("terminalDock.scopes.project"),
            };
            cx.t_with(
                "terminalDock.newScopedTab",
                &Args::default().with("scope", label.to_string()),
            )
        } else {
            cx.t("terminalDock.noPath")
        };
        let plus = Button::new("terminal-new-tab")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
            .padding_x(6.0)
            .disabled(at_cap || scope.cwd.is_none())
            .on_click(cx.listener(|dock, _, window, cx| dock.add_tab(window, cx)))
            .child(Icon::new("plus").size(16.0));
        Some(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(4.0))
                .children(tabs)
                .child(tip("terminal-new-tab-tip", plus_tip, plus))
                .into_any_element(),
        )
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let hide = Button::new("terminal-hide")
            .variant(ButtonVariant::Outline)
            .size(ButtonSize::Sm)
            .height(32.0)
            .padding_x(8.0)
            .on_click(cx.listener(|dock, _, _, cx| dock.hide(cx)))
            .child(Icon::new("panel-bottom-close").size(16.0));
        div()
            .flex()
            .h(px(TOOLBAR))
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0))
            .border_b_1()
            .border_color(colors.border.opacity(0.6))
            .px(px(8.0))
            .child(
                div()
                    .id("terminal-toolbar-tabs")
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(4.0))
                    .overflow_x_scroll()
                    .children(self.scope_button(cx))
                    .children(self.tab_strip(cx)),
            )
            .child(tip(
                "terminal-hide-tip",
                cx.t("terminalDock.hideHint"),
                hide,
            ))
            .into_any_element()
    }

    fn panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let content: AnyElement = match self.active_session(cx) {
            Some(session) => session.into_any_element(),
            None => {
                let key = if self.scope().and_then(|scope| scope.cwd).is_some() {
                    "terminalDock.empty"
                } else {
                    "terminalDock.noPath"
                };
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.0))
                    .text_color(cx.theme().colors.muted_foreground)
                    .child(cx.t(key))
                    .into_any_element()
            }
        };
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px(px(8.0))
            .pb(px(8.0))
            .pt(px(4.0))
            .child(content)
            .into_any_element()
    }
}

impl Render for TerminalDock {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let colors = cx.theme().colors.clone();
        let parent = f32::from(window.viewport_size().height);
        div()
            .id("terminal-dock")
            .debug_selector(|| "terminal-dock".into())
            .flex()
            .flex_col()
            .flex_shrink_0()
            .h(px(self.height))
            .min_w_0()
            .overflow_hidden()
            .border_t_1()
            .border_color(colors.border)
            .bg(colors.card)
            .text_color(colors.card_foreground)
            .on_drag_move(
                cx.listener(move |dock, event: &DragMoveEvent<DockResize>, _, cx| {
                    dock.resize_to(f32::from(event.event.position.y), parent, cx)
                }),
            )
            .capture_any_mouse_up(
                cx.listener(|dock, _: &MouseUpEvent, _, cx| dock.finish_resize(cx)),
            )
            .child(self.grip(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .flex_col()
                    .child(self.toolbar(cx))
                    .child(self.panel(cx)),
            )
            .into_any_element()
    }
}
