//! `DelegationLineageMenu`: the open conversation's lineage, hidden when it
//! has none. The `GitFork` button (sky with a count while some still run)
//! opens 320px with "Delegada por" (the conversation that asked) and
//! "Delegações desta conversa": each with its status dot, provider, mode,
//! status and when, its title, opened with a click; a running one can be
//! cancelled.

use dcc_core::domain::delegation::DelegationStatus;
use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, div, prelude::*, px};

use crate::data::conversation::DelegationRecord;
use crate::data::delegation_lineage::{children, is_active, parent, session_title, title};
use crate::data::desktop_api::ApiError;
use crate::i18n::{Args, Locale, Translate};
use crate::project::ProviderIcon;
use crate::screens::conversation::ConversationView;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::popover::{Popover, PopoverAlign};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Toaster, Tooltip, spinner};

/// `STATUS_DOT`
fn status_dot(status: &DelegationStatus, muted: Hsla, destructive: Hsla) -> Hsla {
    match status {
        DelegationStatus::Queued => tw::SKY_500.alpha(0.7),
        DelegationStatus::Running => tw::SKY_500.hsla(),
        DelegationStatus::ReviewPending => tw::AMBER_500.hsla(),
        DelegationStatus::Completed => tw::EMERALD_500.hsla(),
        DelegationStatus::Failed => destructive,
        DelegationStatus::Draft | DelegationStatus::Cancelled => muted.opacity(0.45),
    }
}

fn status_id(status: &DelegationStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// `t(key, { defaultValue })`
fn word(key: String, fallback: &str, cx: &gpui::App) -> SharedString {
    let i18n = cx.global::<crate::i18n::I18n>();
    if i18n.has(&key, &Args::default()) {
        i18n.t(&key)
    } else {
        fallback.to_string().into()
    }
}

impl ConversationView {
    fn lineage_child(
        &self,
        index: usize,
        record: &DelegationRecord,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let english = cx.global::<crate::i18n::I18n>().locale() == Locale::En;
        let sessions = self
            .data()
            .map(|data| data.sessions.clone())
            .unwrap_or_default();
        let when = record
            .started_at
            .as_deref()
            .or(Some(record.created_at.as_str()))
            .and_then(crate::data::time::parse_ms)
            .map(|at| crate::data::time::short_relative(at, crate::data::time::now_ms(), english));
        let status = status_id(&record.status);
        let meta = [
            Some(word(
                format!("inspector.delegations.mode.{}", record.mode),
                &record.mode,
                cx,
            )),
            Some(word(
                format!("inspector.delegations.status.{status}"),
                &status,
                cx,
            )),
            when.map(SharedString::from),
        ];
        let mut line = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(11.0))
            .text_color(colors.muted_foreground);
        for (at, part) in meta.into_iter().flatten().enumerate() {
            if at > 0 {
                line = line.child(div().child("·"));
            }
            line = line.child(if at == 0 {
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground.opacity(0.85))
                    .child(part)
            } else {
                div().child(part)
            });
        }
        let child = record.child_session_id.clone();
        let active = is_active(record);
        let cancelling = self.header.cancelling.as_deref() == Some(record.id.as_str());
        let id = record.id.clone();
        let hover = colors.accent;
        let cancel = active.then(|| {
            Tooltip::text(
                ("lineage-cancel-tip", index),
                div()
                    .id(("lineage-cancel", index))
                    .debug_selector(move || format!("lineage-cancel-{index}"))
                    .mt(px(2.0))
                    .flex()
                    .flex_shrink_0()
                    .size(px(20.0))
                    .items_center()
                    .justify_center()
                    .rounded(px(2.0))
                    .text_color(colors.muted_foreground.opacity(0.7))
                    .when(!cancelling, |button| {
                        button
                            .cursor_pointer()
                            .hover(move |style| style.bg(hover).text_color(colors.destructive))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                cx.stop_propagation();
                                view.cancel_delegation(id.clone(), cx);
                            }))
                    })
                    .child(if cancelling {
                        spinner(("lineage-cancelling", index), 12.0).into_any_element()
                    } else {
                        Icon::new("circle-stop").size(14.0).into_any_element()
                    }),
                cx.t("delegation.lineage.cancel"),
            )
        });
        div()
            .id(("lineage-child", index))
            .debug_selector(move || format!("lineage-child-{index}"))
            .flex()
            .items_start()
            .gap(px(8.0))
            .rounded(px(6.0))
            .px(px(6.0))
            .py(px(8.0))
            .when(child.is_none(), |row| row.opacity(0.5))
            .when_some(child, |row, child| {
                row.cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(
                        cx.listener(move |view, _, _, cx| view.select_session(child.clone(), cx)),
                    )
            })
            .child(
                div()
                    .mt(px(6.0))
                    .size(px(6.0))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(status_dot(
                        &record.status,
                        colors.muted_foreground,
                        colors.destructive,
                    )),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .flex_shrink_0()
                    .child(ProviderIcon::new(&record.provider_id).size(14.0)),
            )
            .child(
                div().min_w_0().flex_1().child(line).child(
                    div()
                        .mt(px(2.0))
                        .max_h(px(32.0))
                        .overflow_hidden()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .child(title(record, &sessions)),
                ),
            )
            .children(cancel)
            .into_any_element()
    }

    fn lineage_list(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let data = self.data()?;
        let session = data.session_id.as_deref();
        let kids: Vec<DelegationRecord> = children(&data.delegations, session)
            .into_iter()
            .cloned()
            .collect();
        let from = parent(&data.delegations, session).cloned();
        if kids.is_empty() && from.is_none() {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let label = |text: SharedString| {
            div()
                .px(px(6.0))
                .py(px(4.0))
                .text_size(px(11.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.muted_foreground)
                .child(text)
        };
        let hover = colors.accent;
        let parent_section = from.map(|record| {
            let parent_title = session_title(&data.sessions, Some(&record.parent_session_id))
                .map(|title| SharedString::from(title.to_string()))
                .unwrap_or_else(|| cx.t("delegation.lineage.parentSession"));
            let target = record.parent_session_id.clone();
            div()
                .child(label(cx.t("delegation.lineage.delegatedBy")))
                .child(
                    div()
                        .id("lineage-parent")
                        .debug_selector(|| "lineage-parent".into())
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .rounded(px(6.0))
                        .px(px(6.0))
                        .py(px(4.0))
                        .text_size(px(14.0))
                        .cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.select_session(target.clone(), cx)
                        }))
                        .child(
                            div()
                                .text_color(colors.muted_foreground)
                                .child(Icon::new("corner-left-up").size(14.0)),
                        )
                        .child(div().min_w_0().flex_1().truncate().child(parent_title)),
                )
        });
        let separator = (parent_section.is_some() && !kids.is_empty())
            .then(|| div().mx(px(-4.0)).my(px(4.0)).h(px(1.0)).bg(colors.border));
        let heading = (!kids.is_empty()).then(|| label(cx.t("delegation.lineage.title")));
        let rows: Vec<AnyElement> = kids
            .iter()
            .enumerate()
            .map(|(index, record)| self.lineage_child(index, record, cx))
            .collect();
        Some(
            div()
                .id("lineage-list")
                .max_h(px(448.0))
                .overflow_y_scroll()
                .children(parent_section)
                .children(separator)
                .children(heading)
                .children(rows)
                .into_any_element(),
        )
    }

    pub(super) fn lineage_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let list = self.lineage_list(cx)?;
        let data = self.data()?;
        let kids = children(&data.delegations, data.session_id.as_deref());
        let count = kids.len();
        let active = kids.iter().filter(|record| is_active(record)).count();
        let label = cx.t_with("delegation.lineage.button", &Args::count(count as i64));
        let badge = (active > 0).then(|| {
            div()
                .absolute()
                .right(px(-4.0))
                .top(px(-4.0))
                .flex()
                .h(px(16.0))
                .min_w(px(16.0))
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(cx.theme().colors.background)
                .bg(tw::SKY_500.hsla())
                .px(px(4.0))
                .text_size(px(9.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(gpui::white())
                .child(active.to_string())
        });
        let mut button = Button::new("header-lineage")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::IconSm);
        let dark = cx.theme().appearance == crate::theme::Appearance::Dark;
        button = if active > 0 {
            button.text_color(if dark {
                tw::SKY_400.hsla()
            } else {
                tw::SKY_600.hsla()
            })
        } else {
            button.muted()
        };
        let trigger = Tooltip::text(
            "header-lineage-tip",
            div()
                .relative()
                .child(button.child(Icon::new("git-fork").size(14.0).stroke(1.9)))
                .children(badge),
            label,
        );
        let view = cx.entity().downgrade();
        Some(
            Popover::new(
                "header-lineage-menu",
                self.header.lineage_open,
                trigger,
                move |_, _| list,
                move |open, _, cx| {
                    let _ = view.update(cx, |view, cx| {
                        view.header.lineage_open = open;
                        cx.notify();
                    });
                },
            )
            .align(PopoverAlign::End)
            .width(320.0)
            .padding(4.0)
            .into_any_element(),
        )
    }

    /// `cancelRunningDelegation`, then the task read again.
    pub(crate) fn cancel_delegation(&mut self, id: String, cx: &mut Context<Self>) {
        if self.header.cancelling.is_some() {
            return;
        }
        let api = self.header_api(cx);
        self.header.cancelling = Some(id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api.ok_or(ApiError::DesktopClosed)?.cancel_delegation(&id) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.header.cancelling = None;
                match result {
                    Ok(()) => view.refresh_now(cx),
                    Err(error) => Toaster::error_with(
                        cx.t("delegation.lineage.cancelFailed"),
                        error.to_string(),
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
