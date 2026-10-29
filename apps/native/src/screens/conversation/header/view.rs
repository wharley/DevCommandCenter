//! The header's row (`<header class="border-b border-border/60
//! bg-background/95 px-4 py-2">`): the title on the left, the 28px ghost
//! buttons on the right with their tooltips.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use crate::data::session_list::can_resume;
use crate::i18n::{Args, Translate};
use crate::screens::conversation::{ConversationEvent, ConversationView};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, spinner};

fn tip(id: &'static str, text: SharedString, child: impl IntoElement) -> AnyElement {
    Tooltip::text(id, child, text).into_any_element()
}

fn icon_button(id: &'static str) -> Button {
    Button::new(id)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconSm)
        .muted()
}

impl ConversationView {
    /// The agent's mascot for a session, when it runs as one.
    pub(super) fn session_agent_badge(
        &self,
        id: impl Into<gpui::ElementId>,
        session: Option<&str>,
        size: f32,
    ) -> Option<AnyElement> {
        let session = session?.to_string();
        let agent = self.header.agents.get(&session)?;
        Some(
            Tooltip::text(
                id,
                div()
                    .debug_selector(move || format!("agent-badge-{session}"))
                    .flex_shrink_0()
                    .child(crate::agent::AgentAvatarView::new(
                        "agent-badge",
                        &agent.avatar,
                        Some(agent.state),
                        size,
                    )),
                agent.name.clone(),
            )
            .into_any_element(),
        )
    }

    fn resume_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let row = self.shown_row();
        let resumable = can_resume(row);
        let tooltip = match row {
            None => cx.t("workbench.resumeTooltipNone"),
            Some(_) if resumable => cx.t("workbench.resumeTooltipOk"),
            Some(_) => cx.t("workbench.resumeTooltipActive"),
        };
        tip(
            "header-resume-tip",
            tooltip,
            icon_button("header-resume")
                .disabled(!resumable)
                .on_click(cx.listener(|view, _, _, cx| view.resume_session(cx)))
                .child(Icon::new("refresh-cw").size(14.0).stroke(1.9)),
        )
    }

    /// `SquareTerminal` with the count of programs running (sky for this
    /// task's, amber for other tasks' only).
    fn terminal_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let counts = self.header.terminals;
        let label = if counts.total > counts.current {
            cx.t_with(
                "workbench.terminal.openWithBackground",
                &Args::default()
                    .with("total", counts.total.to_string())
                    .with("current", counts.current.to_string()),
            )
        } else if counts.current > 0 {
            cx.t_with(
                "workbench.terminal.openWithActive",
                &Args::count(counts.current as i64),
            )
        } else {
            cx.t("workbench.terminal.open")
        };
        let background = cx.theme().colors.background;
        let badge = (counts.total > 0).then(|| {
            div()
                .debug_selector(|| "header-terminal-count".into())
                .absolute()
                .top(px(-4.0))
                .right(px(-4.0))
                .flex()
                .h(px(16.0))
                .min_w(px(16.0))
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(background)
                .px(px(4.0))
                .text_size(px(9.0))
                .font_weight(FontWeight::MEDIUM)
                .line_height(px(9.0))
                .text_color(gpui::white())
                .bg(if counts.current > 0 {
                    tw::SKY_500.hsla()
                } else {
                    tw::AMBER_500.hsla()
                })
                .child(counts.total.to_string())
        });
        tip(
            "header-terminal-tip",
            format!("{label} · ⌘J").into(),
            div()
                .relative()
                .child(
                    icon_button("header-terminal")
                        .on_click(
                            cx.listener(|_, _, _, cx| cx.emit(ConversationEvent::OpenTerminal)),
                        )
                        .child(Icon::new("square-terminal").size(14.0).stroke(1.8)),
                )
                .children(badge),
        )
    }

    fn browser_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let open = self.header.browser_open;
        let label = cx.t(if open {
            "browser.close"
        } else {
            "browser.open"
        });
        let mut button = icon_button("header-browser")
            .on_click(cx.listener(|_, _, _, cx| cx.emit(ConversationEvent::ToggleBrowser)))
            .child(Icon::new("globe-2").size(14.0).stroke(1.8));
        if open {
            button = button
                .text_color(tw::CYAN_400.hsla())
                .hover_text_color(tw::CYAN_300.hsla());
        }
        tip("header-browser-tip", label, button)
    }

    /// `TextSearch` (find in this conversation) and `Search` (past sessions).
    fn find_buttons(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(tip(
                "header-thread-find-tip",
                cx.t("conversation.find.openTooltip"),
                icon_button("header-thread-find")
                    .on_click(cx.listener(|view, _, window, cx| view.open_find(window, cx)))
                    .child(Icon::new("text-search").size(14.0)),
            ))
            .child(tip(
                "header-session-search-tip",
                cx.t("workbench.sessionSearch.buttonTooltip"),
                icon_button("header-session-search")
                    .on_click(
                        cx.listener(|_, _, _, cx| cx.emit(ConversationEvent::OpenSessionSearch)),
                    )
                    .child(Icon::new("search").size(14.0)),
            ))
            .into_any_element()
    }

    fn new_session_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let starting = self.header.starting;
        tip(
            "header-new-session-tip",
            cx.t("workbench.newSessionTooltip"),
            icon_button("header-new-session")
                .disabled(
                    starting
                        || self
                            .data()
                            .and_then(|data| data.new_thread.as_ref())
                            .is_none(),
                )
                .on_click(cx.listener(|view, _, _, cx| view.start_session(cx)))
                .child(if starting {
                    spinner("header-new-session-busy", 14.0).into_any_element()
                } else {
                    Icon::new("plus").size(14.0).into_any_element()
                }),
        )
    }
}

/// The header over the conversation of a task.
pub(crate) fn header(view: &ConversationView, cx: &mut Context<ConversationView>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let data = view.data();
    let title = view
        .shown_row()
        .map(|row| row.title.clone())
        .or_else(|| data.and_then(|data| data.workspace_name.clone()))
        .unwrap_or_default();
    let project = data
        .and_then(|data| data.context.as_ref())
        .and_then(|context| context.project_label.clone());
    let full_title: SharedString = match &project {
        Some(project) => format!("{project} / {title}").into(),
        None => title.clone().into(),
    };
    // `SessionAgentBadge`: the mascot of the agent this conversation runs as.
    let badge = view.session_agent_badge(
        "header-agent",
        data.and_then(|data| data.session_id.as_deref()),
        18.0,
    );
    div()
        .debug_selector(|| "conversation-header".into())
        .flex_shrink_0()
        .border_b_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.background.opacity(0.95))
        .px(px(16.0))
        .py(px(8.0))
        .child(
            div()
                .flex()
                .min_w_0()
                .flex_1()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .overflow_hidden()
                .child(
                    div()
                        .flex()
                        .min_w_0()
                        .flex_1()
                        .items_center()
                        .gap(px(8.0))
                        .children(badge)
                        .child(
                            Tooltip::text(
                                "header-title-tip",
                                div()
                                    .id("header-title")
                                    .debug_selector(|| "header-title".into())
                                    .min_w_0()
                                    .flex_1()
                                    .truncate()
                                    .text_size(px(13.0))
                                    .line_height(px(20.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(title),
                                full_title,
                            )
                            // `min-w-0 flex-1`: the title gives way to the buttons
                            // (truncated, down to nothing) instead of under them.
                            .fill(),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_end()
                        .gap(px(4.0))
                        .child(view.resume_button(cx))
                        .child(view.editor_picker(cx))
                        .child(view.terminal_button(cx))
                        .child(view.browser_button(cx))
                        .child(view.delivery.clone())
                        .children(view.ai_memory_export(cx))
                        .children(view.ai_memory_menu(cx))
                        .children(view.lineage_menu(cx))
                        .child(view.call_agents.clone())
                        .child(view.new_session_button(cx))
                        .child(view.find_buttons(cx))
                        .child(view.history_menu(cx)),
                ),
        )
        .into_any_element()
}
