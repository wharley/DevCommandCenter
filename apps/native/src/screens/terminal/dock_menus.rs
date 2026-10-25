//! The bar's menus and buttons past the tabs: the scope picker (`w-56`,
//! the two scopes with their folder or why they have none), the local
//! server found, the actions menu, expand and hide.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::dev_server::address;
use super::dock::{ScopeKind, TerminalDock};
use super::dock_view::tip;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Menu, MenuAlign, MenuEntry, MenuItem};

fn scope_icon(kind: ScopeKind) -> Icon {
    match kind {
        ScopeKind::Worktree => Icon::new("shield-check")
            .size(14.0)
            .color(tw::EMERALD_500.hsla()),
        ScopeKind::Project => Icon::new("triangle-alert")
            .size(14.0)
            .color(tw::AMBER_500.hsla()),
    }
}

impl TerminalDock {
    /// The scope's button and its menu.
    pub(super) fn scope_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let place = self.place.clone()?;
        let scope = self.scope()?;
        let colors = cx.theme().colors.clone();
        let mut text = format!(
            "{} · {}",
            self.scope_label(scope.kind, cx),
            scope.project_label
        );
        if let Some(branch) = &scope.branch {
            text.push_str(&format!(" · {branch}"));
        }
        let hover = colors.muted.opacity(0.6);
        let trigger = move |_open: bool| {
            div()
                .flex()
                .h(px(28.0))
                .flex_shrink_0()
                .items_center()
                .gap(px(4.0))
                .mr(px(4.0))
                .rounded(px(6.0))
                .bg(colors.muted.opacity(0.3))
                .hover(move |style| style.bg(hover))
                .px(px(8.0))
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.muted_foreground)
                .cursor_pointer()
                .debug_selector(|| "terminal-scope".into())
                .child(scope_icon(scope.kind))
                .child(div().max_w(px(288.0)).truncate().child(text.clone()))
                .child(
                    div()
                        .opacity(0.5)
                        .child(Icon::new("chevron-down").size(12.0)),
                )
                .into_any_element()
        };
        let dock = cx.entity().downgrade();
        let current = scope.kind;
        let labels = (
            self.scope_label(ScopeKind::Worktree, cx),
            self.scope_label(ScopeKind::Project, cx),
        );
        let missing = (
            cx.t("terminalDock.scopes.noWorktreePath"),
            cx.t("terminalDock.scopes.noProjectPath"),
        );
        Some(
            Menu::dropdown("terminal-scope-menu", trigger, move |_, _| {
                place
                    .scopes()
                    .into_iter()
                    .map(|scope| {
                        let (label, missing) = match scope.kind {
                            ScopeKind::Worktree => (labels.0.clone(), missing.0.clone()),
                            ScopeKind::Project => (labels.1.clone(), missing.1.clone()),
                        };
                        let detail: SharedString = scope
                            .branch
                            .clone()
                            .or_else(|| scope.cwd.clone())
                            .map(SharedString::from)
                            .unwrap_or(missing);
                        let kind = scope.kind;
                        let dock = dock.clone();
                        let mut item = MenuItem::new(
                            format!("{label} · {}", scope.project_label),
                            move |window, cx| {
                                let _ = dock.update(cx, |dock, cx| dock.set_kind(kind, window, cx));
                            },
                        )
                        .icon(scope_icon(kind))
                        .detail(detail)
                        .disabled(scope.cwd.is_none());
                        if kind == current {
                            item = item.trailing("✓");
                        }
                        MenuEntry::from(item)
                    })
                    .collect()
            })
            .min_width(224.0)
            .into_any_element(),
        )
    }

    /// "Abrir localhost:5173": the newest local server, in the browser.
    pub(super) fn server_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let server = self.detected_server(cx)?;
        let url = server.url.clone();
        let label = cx.t_with(
            "terminalDock.openDevServer",
            &Args::default().with("address", address(&server.url)),
        );
        let hint = cx.t_with(
            "terminalDock.openDevServerHint",
            &Args::default().with("url", server.url.clone()),
        );
        let button = Button::new("terminal-dev-server")
            .variant(ButtonVariant::Outline)
            .size(ButtonSize::Sm)
            .padding_x(8.0)
            .text_size(12.0)
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(super::dock::DockEvent::OpenServer(url.clone()))
            }))
            .child(
                div()
                    .flex()
                    .max_w(px(192.0))
                    .items_center()
                    .gap(px(6.0))
                    .child(Icon::new("globe-2").size(14.0))
                    .child(div().truncate().child(label)),
            );
        Some(tip("terminal-dev-server-tip", hint, button))
    }

    /// "Ações do terminal"
    pub(super) fn actions_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.active_session(cx);
        let has_session = session.is_some();
        let busy = session
            .as_ref()
            .is_some_and(|session| session.read(cx).busy());
        let has_cwd = self.scope().and_then(|scope| scope.cwd).is_some();
        let labels = [
            cx.t("terminalDock.sendToAgent"),
            cx.t("terminalDock.clear"),
            cx.t("terminalDock.interrupt"),
            cx.t("terminalDock.restart"),
            cx.t("terminalDock.openExternal"),
        ];
        let dock = cx.entity().downgrade();
        let trigger = |_open: bool| {
            Button::new("terminal-actions")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .height(32.0)
                .padding_x(8.0)
                .child(Icon::new("ellipsis").size(16.0))
                .into_any_element()
        };
        Menu::dropdown("terminal-actions-menu", trigger, move |_, _| {
            let action = |label: &SharedString,
                          icon: &'static str,
                          enabled: bool,
                          run: fn(
                &mut TerminalDock,
                &mut gpui::Window,
                &mut Context<TerminalDock>,
            )| {
                let dock = dock.clone();
                MenuEntry::from(
                    MenuItem::new(label.clone(), move |window, cx| {
                        let _ = dock.update(cx, |dock, cx| run(dock, window, cx));
                    })
                    .icon(Icon::new(icon).size(16.0))
                    .disabled(!enabled),
                )
            };
            vec![
                action(&labels[0], "sparkles", has_session, |dock, _, cx| {
                    dock.send_to_agent(cx)
                }),
                action(&labels[1], "eraser", has_session, |dock, _, cx| {
                    dock.clear_active(cx)
                }),
                action(&labels[2], "square", has_session && busy, |dock, _, cx| {
                    dock.interrupt_active(cx)
                }),
                action(
                    &labels[3],
                    "refresh-ccw",
                    has_session && has_cwd && !busy,
                    |dock, window, cx| dock.restart_active(window, cx),
                ),
                action(&labels[4], "external-link", has_cwd, |dock, _, _| {
                    dock.open_external()
                }),
            ]
        })
        .align(MenuAlign::End)
        .min_width(224.0)
        .into_any_element()
    }

    pub(super) fn expand_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let expanded = self.expanded;
        let button = Button::new("terminal-expand")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
            .height(32.0)
            .padding_x(8.0)
            .on_click(cx.listener(move |dock, _, _, cx| dock.set_expanded(!expanded, cx)))
            .child(Icon::new(if expanded { "minimize-2" } else { "maximize-2" }).size(16.0));
        let hint = if expanded {
            cx.t("terminalDock.collapseHint")
        } else {
            cx.t("terminalDock.expand")
        };
        tip("terminal-expand-tip", hint, button)
    }
}
