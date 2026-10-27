//! The panel's frame — "Sessão / Atividade e detalhes do workspace" with
//! the milestones' count and the tabs — and the activity's delegations
//! (`DelegationsSection`): one turn's delegations compared, then each one's
//! status, task, report, files and actions.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::{SessionPanel, SessionTab};
use crate::data::conversation::DelegationRecord;
use crate::data::session_activity::{comparisons, is_milestone, visible};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Badge, BadgeVariant, Icon, Tracked};

pub(super) fn provider(id: &str) -> String {
    crate::project::provider_label(id).map_or_else(|| id.to_string(), str::to_string)
}

impl SessionPanel {
    pub(super) fn render_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let workspace = self.data(cx).filter(|data| {
            !data.task_id.is_empty() && data.workspace_name.is_some() && data.branch.is_some()
        });
        if workspace.is_none() {
            return div()
                .debug_selector(|| "session-panel-empty".into())
                .flex()
                .size_full()
                .items_center()
                .justify_center()
                .px(px(16.0))
                .py(px(24.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(4.0))
                        .max_w(px(320.0))
                        .text_center()
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors.foreground)
                                .child(cx.t("inspector.empty.title")),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(colors.muted_foreground)
                                .child(cx.t("inspector.empty.description")),
                        ),
                )
                .into_any_element();
        }
        let milestones = self.data(cx).map_or(0, |data| {
            data.activity
                .iter()
                .filter(|event| is_milestone(event))
                .count()
        });
        let tabs = [SessionTab::Activity]
            .into_iter()
            .map(|tab| {
                let active = self.tab == tab;
                div()
                    .id(SharedString::from(format!("session-tab-{}", tab.key())))
                    .debug_selector(move || format!("session-tab-{}", tab.key()))
                    .flex()
                    .h(px(36.0))
                    .items_center()
                    .px(px(12.0))
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .border_b_2()
                    .border_color(if active {
                        colors.foreground
                    } else {
                        gpui::transparent_black()
                    })
                    .text_color(if active {
                        colors.foreground
                    } else {
                        colors.muted_foreground
                    })
                    .hover(|tab| tab.text_color(colors.foreground))
                    .on_click(cx.listener(move |panel, _, _, cx| panel.set_tab(tab, cx)))
                    .child(cx.t(&format!("inspector.tabs.{}", tab.key())))
            })
            .collect::<Vec<_>>();
        let content = match self.tab {
            SessionTab::Activity => div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .child(self.delegations(cx))
                .child(self.feed(cx))
                .into_any_element(),
            _ => div().into_any_element(),
        };
        div()
            .debug_selector(|| "session-panel".into())
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .text_color(colors.foreground)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .border_b_1()
                    .border_color(colors.border.opacity(0.4))
                    .px(px(12.0))
                    .py(px(8.0))
                    .child(
                        div()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.muted_foreground)
                                    .child(Tracked::new(
                                        cx.t("inspector.gitSection.kicker").to_uppercase(),
                                        11.0,
                                        0.08,
                                    )),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(cx.t("inspector.gitSection.title")),
                            ),
                    )
                    .child(
                        Badge::new(
                            cx.t_with(
                                "inspector.gitSection.eventsCount",
                                &Args::count(milestones as i64),
                            ),
                            BadgeVariant::Outline,
                        )
                        .height(24.0)
                        .text_size(10.0)
                        .weight(FontWeight::NORMAL),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .border_b_1()
                    .border_color(colors.border.opacity(0.4))
                    .bg(colors.muted.opacity(0.15))
                    .px(px(8.0))
                    .children(tabs),
            )
            .child(content)
            .into_any_element()
    }

    fn delegations(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let list: Vec<DelegationRecord> = self
            .data(cx)
            .map(|data| data.delegations.values().cloned().collect())
            .unwrap_or_default();
        let shown = visible(&list, None);
        let groups = comparisons(&shown, provider);
        let groups =
            groups
                .into_iter()
                .map(|group| {
                    let statuses: Vec<String> = group
                        .statuses
                        .iter()
                        .filter_map(|(status, count)| {
                            let key = match status.as_str() {
                                "completed" => "inspector.delegations.statusCompleted",
                                "review_pending" => "inspector.delegations.statusReview",
                                "running" => "inspector.delegations.statusRunning",
                                "failed" => "inspector.delegations.statusFailed",
                                _ => return None,
                            };
                            Some(cx.t_with(key, &Args::count(*count as i64)).to_string())
                        })
                        .collect();
                    let status = if statuses.is_empty() {
                        cx.t("inspector.delegations.statusPending").to_string()
                    } else {
                        statuses.join(" · ")
                    };
                    div()
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(colors.primary.opacity(0.2))
                        .bg(colors.primary.opacity(0.05))
                        .px(px(8.0))
                        .py(px(8.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .text_size(px(11.5))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(cx.t_with(
                                            "inspector.delegations.reportTitle",
                                            &Args::count(group.count as i64),
                                        )),
                                )
                                .child(div().max_w(px(144.0)).overflow_hidden().child(
                                    Badge::new(status, BadgeVariant::Outline).text_size(10.0),
                                )),
                        )
                        .child(
                            div()
                                .mt(px(4.0))
                                .text_size(px(11.0))
                                .text_color(colors.muted_foreground)
                                .child(cx.t_with(
                                    "inspector.delegations.providers",
                                    &Args::default().with("providers", group.providers.join(", ")),
                                )),
                        )
                        .when(!group.files.is_empty(), |card| {
                            card.child(
                                div()
                                    .mt(px(4.0))
                                    .truncate()
                                    .font_family(cx.theme().fonts.mono.clone())
                                    .text_size(px(10.5))
                                    .text_color(colors.muted_foreground.opacity(0.8))
                                    .child(format!(
                                        "{}: {}",
                                        cx.t_with(
                                            "inspector.delegations.files",
                                            &Args::count(group.files.len() as i64)
                                        ),
                                        group
                                            .files
                                            .iter()
                                            .take(3)
                                            .cloned()
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    )),
                            )
                        })
                })
                .collect::<Vec<_>>();
        let cards: Vec<AnyElement> = shown
            .iter()
            .map(|delegation| self.delegation_card(delegation, cx))
            .collect();
        div()
            .debug_selector(|| "session-delegations".into())
            .flex_none()
            .border_b_1()
            .border_color(colors.border.opacity(0.4))
            .bg(colors.sidebar)
            .px(px(12.0))
            .py(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .mb(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                Icon::new("git-fork")
                                    .size(14.0)
                                    .stroke(2.0)
                                    .color(colors.muted_foreground),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors.muted_foreground)
                                    .child(Tracked::new(
                                        cx.t("inspector.delegations.title").to_uppercase(),
                                        11.0,
                                        0.025,
                                    )),
                            ),
                    )
                    .child(
                        Badge::new(list.len().to_string(), BadgeVariant::Outline)
                            .height(20.0)
                            .padding_x(6.0)
                            .text_size(10.0)
                            .weight(FontWeight::NORMAL),
                    ),
            )
            .child(if list.is_empty() {
                div()
                    .rounded(px(6.0))
                    .border_1()
                    .border_dashed()
                    .border_color(colors.border.opacity(0.5))
                    .bg(colors.muted.opacity(0.1))
                    .p(px(8.0))
                    .text_size(px(11.5))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("inspector.delegations.empty"))
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .children(groups)
                    .children(cards)
                    .into_any_element()
            })
            .into_any_element()
    }
}
