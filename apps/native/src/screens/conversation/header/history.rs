//! The history (`History`, a 288px list): the open conversations with a
//! delegated child under its parent (`↳`), each with its dot, provider,
//! title, state and ×; the archived ones to restore. Closing one whose turn
//! still runs asks first ("Fechar conversa?").

use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px};

use crate::data::session_list::SessionRow;
use crate::i18n::{Args, Translate};
use crate::project::ProviderIcon;
use crate::screens::conversation::ConversationView;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::popover::{Popover, PopoverAlign};
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Dialog, Icon, Tooltip, dialog_footer, dialog_header, spinner,
};

/// `sessionStateLabel`: the state's word, else the code itself.
fn state_label(state: &str, cx: &gpui::App) -> SharedString {
    let key = format!("workbench.sessionState.{state}");
    let i18n = cx.global::<crate::i18n::I18n>();
    if i18n.has(&key, &Args::default()) {
        i18n.t(&key)
    } else {
        state.to_string().into()
    }
}

impl ConversationView {
    fn history_row(&self, index: usize, row: &SessionRow, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let selected = self.data().and_then(|data| data.session_id.as_deref()) == Some(&row.id);
        let busy = self.header.busy_session.as_deref() == Some(row.id.as_str());
        let hover = colors.accent;
        let select = row.id.clone();
        let close = row.id.clone();
        let close_label = cx.t_with(
            "workbench.closeSessionAria",
            &Args::default().with("title", row.title.clone()),
        );
        div()
            .id(("history-row", index))
            .debug_selector(move || format!("history-row-{index}"))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(6.0))
            .px(px(6.0))
            .py(px(4.0))
            .when(row.depth > 0, |line| line.pl(px(20.0)))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |view, _, _, cx| view.select_session(select.clone(), cx)))
            .when(row.depth > 0, |line| {
                line.child(
                    div()
                        .ml(px(-8.0))
                        .text_size(px(10.0))
                        .text_color(colors.muted_foreground.opacity(0.6))
                        .child("↳"),
                )
            })
            .child(
                div()
                    .size(px(6.0))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(if selected {
                        tw::EMERALD_500.hsla()
                    } else {
                        colors.muted_foreground.opacity(0.45)
                    }),
            )
            .when(!row.provider_id.is_empty(), |line| {
                line.child(
                    div()
                        .flex_shrink_0()
                        .text_color(colors.muted_foreground)
                        .child(ProviderIcon::new(&row.provider_id).size(14.0)),
                )
            })
            .children(self.session_agent_badge(("history-agent", index), Some(&row.id), 16.0))
            .child(div().min_w_0().flex_1().truncate().child(row.title.clone()))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(colors.muted_foreground)
                    .child(state_label(&row.state, cx)),
            )
            .child(Tooltip::text(
                ("history-close-tip", index),
                div()
                    .id(("history-close", index))
                    .debug_selector(move || format!("history-close-{index}"))
                    .flex()
                    .size(px(20.0))
                    .items_center()
                    .justify_center()
                    .rounded(px(2.0))
                    .text_color(colors.muted_foreground.opacity(0.7))
                    .when(busy, |button| button.opacity(0.5))
                    .when(!busy, |button| {
                        button
                            .hover(move |style| style.bg(hover).text_color(colors.foreground))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                cx.stop_propagation();
                                view.request_close_session(&close, cx)
                            }))
                    })
                    .child(if busy {
                        spinner(("history-close-busy", index), 12.0).into_any_element()
                    } else {
                        Icon::new("x").size(12.0).into_any_element()
                    }),
                close_label,
            ))
            .into_any_element()
    }

    fn history_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let rows: Vec<SessionRow> = self
            .data()
            .map(|data| data.sessions.as_ref().clone())
            .unwrap_or_default();
        let label = |text: SharedString| {
            div()
                .px(px(6.0))
                .py(px(4.0))
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .child(text)
        };
        let separator = || div().mx(px(-4.0)).my(px(4.0)).h(px(1.0)).bg(colors.border);
        let open: Vec<AnyElement> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| !row.archived)
            .map(|(index, row)| self.history_row(index, row, cx))
            .collect();
        let archived: Vec<AnyElement> =
            rows.iter()
                .enumerate()
                .filter(|(_, row)| row.archived)
                .map(|(index, row)| {
                    let restore = row.id.clone();
                    let hover = colors.accent;
                    div()
                        .id(("history-archived", index))
                        .debug_selector(move || format!("history-archived-{index}"))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .rounded(px(6.0))
                        .px(px(6.0))
                        .py(px(4.0))
                        .text_size(px(14.0))
                        .cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.restore_session(restore.clone(), cx)
                        }))
                        .child(div().min_w_0().truncate().child(row.title.clone()))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(colors.muted_foreground)
                                .child(cx.t("workbench.restoreSessionLabel")),
                        )
                        .into_any_element()
                })
                .collect();
        let empty = open.is_empty().then(|| {
            div()
                .px(px(6.0))
                .py(px(4.0))
                .text_size(px(14.0))
                .opacity(0.5)
                .child(cx.t("workbench.noSessions"))
        });
        let archived_section = (!archived.is_empty()).then(|| {
            div()
                .child(separator())
                .child(
                    label(cx.t("workbench.archivedSessionsLabel"))
                        .text_color(colors.muted_foreground),
                )
                .children(archived)
        });
        div()
            .id("history-list")
            .max_h(px(384.0))
            .overflow_y_scroll()
            .child(label(cx.t("workbench.sessionHistoryLabel")))
            .child(separator())
            .children(open)
            .children(empty)
            .children(archived_section)
            .into_any_element()
    }

    pub(super) fn history_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let list = self.history_list(cx);
        let trigger = Tooltip::text(
            "header-history-tip",
            Button::new("header-history")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .muted()
                .child(if self.loading {
                    spinner("header-history-busy", 14.0).into_any_element()
                } else {
                    Icon::new("history").size(14.0).into_any_element()
                }),
            cx.t("workbench.sessionHistoryAria"),
        );
        let view = cx.entity().downgrade();
        Popover::new(
            "header-history-menu",
            self.header.history_open,
            trigger,
            move |_, _| list,
            move |open, _, cx| {
                let _ = view.update(cx, |view, cx| {
                    view.header.history_open = open;
                    cx.notify();
                });
            },
        )
        .align(PopoverAlign::End)
        .width(288.0)
        .padding(4.0)
        .into_any_element()
    }

    /// "Fechar conversa?" (`pendingSessionClose`).
    pub(crate) fn close_session_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let request = self.header.pending_close.as_ref()?;
        let busy = self.header.busy_session.is_some();
        let args = Args::default().with("title", request.title.clone());
        let description = cx.t_with(
            if request.delete_history {
                "workbench.closeSessionConfirmEmptyDescription"
            } else {
                "workbench.closeSessionConfirmArchivedDescription"
            },
            &args,
        );
        let muted = cx.theme().colors.muted_foreground;
        let running = request.requires_abort.then(|| {
            div()
                .text_color(muted)
                .child(cx.t("workbench.closeSessionConfirmRunningDescription"))
        });
        let footer = dialog_footer(
            [
                Button::new("close-session-cancel")
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Default)
                    .disabled(busy)
                    .on_click(cx.listener(|view, _, _, cx| view.cancel_close_session(cx)))
                    .child(cx.t("workbench.closeSessionCancel"))
                    .into_any_element(),
                Button::new("close-session-confirm")
                    .variant(ButtonVariant::Destructive)
                    .size(ButtonSize::Default)
                    .disabled(busy)
                    .on_click(cx.listener(|view, _, _, cx| view.confirm_close_session(cx)))
                    .child(cx.t("workbench.closeSessionConfirm"))
                    .into_any_element(),
            ],
            cx,
        );
        let view = cx.entity().downgrade();
        Some(
            Dialog::new("close-session-dialog", move |_: &mut Window, cx| {
                let _ = view.update(cx, |view, cx| view.cancel_close_session(cx));
            })
            .close_button(!busy)
            .child(dialog_header(
                cx.t("workbench.closeSessionConfirmTitle"),
                Some(description),
                cx,
            ))
            .children(running)
            .child(footer)
            .into_any_element(),
        )
    }
}
