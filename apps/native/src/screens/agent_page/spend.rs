//! "Este mês" on an agent's page (`dcc_core::domain::agent_budget`): the
//! tokens its conversations used this month (cached reads at a tenth), the
//! cost the providers reported and, with a limit, how far it is; past it,
//! its turns wait and "Continuar mesmo assim" lets it go on until the month
//! ends. Read off the UI thread when the page shows an agent, never on the
//! sidebar's refresh.

use chrono::Local;
use dcc_core::domain::agent_budget::{AgentSpend, month_key, month_start, over_limit};
use gpui::{AnyElement, Context, FontWeight, div, prelude::*, px};
use serde_json::json;

use super::AgentPage;
use super::editor_budget::tokens_short;
use crate::data::desktop_api::{ApiError, DesktopApi, encode};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Toaster};

/// The month read for one agent.
#[derive(Clone, Debug, Default)]
pub(crate) struct SpendRead {
    pub(crate) agent_id: Option<String>,
    pub(crate) spend: Option<AgentSpend>,
    /// The month it was let past its limit, if any.
    pub(crate) override_month: Option<String>,
}

impl DesktopApi {
    /// "Continuar mesmo assim".
    pub fn let_agent_past_limit(&self, agent_id: &str) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/agents/{}/limit-override", encode(agent_id)),
            &json!({}),
        )
        .map(|_| ())
    }
}

/// The agent's month from DCC's database.
pub(crate) fn read_month(agent_id: &str) -> Option<(AgentSpend, Option<String>)> {
    let db = crate::data::loader::database_path();
    let repo = dcc_infra::db::SqliteSessionRepo::open_read_only(&db).ok()?;
    let spend = repo
        .agent_spend_since(agent_id, &month_start(Local::now()))
        .ok()?;
    let override_month = repo
        .load_resident_agent(agent_id)
        .ok()
        .flatten()
        .and_then(|agent| agent.limit_override_month);
    Some((spend, override_month))
}

impl AgentPage {
    /// Reads the shown agent's month once (tests feed it themselves).
    fn ensure_spend(&mut self, cx: &mut Context<Self>) {
        if cfg!(test) || self.spend.agent_id.as_deref() == Some(self.agent_id.as_str()) {
            return;
        }
        let agent_id = self.agent_id.clone();
        self.spend = SpendRead {
            agent_id: Some(agent_id.clone()),
            ..SpendRead::default()
        };
        cx.spawn(async move |page, cx| {
            let read = cx
                .background_executor()
                .spawn({
                    let agent_id = agent_id.clone();
                    async move { read_month(&agent_id) }
                })
                .await;
            let _ = page.update(cx, |page, cx| {
                if page.spend.agent_id.as_deref() == Some(agent_id.as_str()) {
                    if let Some((spend, override_month)) = read {
                        page.spend.spend = Some(spend);
                        page.spend.override_month = override_month;
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn let_past_limit(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        let agent_id = self.agent_id.clone();
        cx.spawn(async move |page, cx| {
            let done = cx
                .background_executor()
                .spawn({
                    let agent_id = agent_id.clone();
                    async move { api.let_agent_past_limit(&agent_id) }
                })
                .await;
            let _ = page.update(cx, |page, cx| match done {
                Ok(()) => {
                    page.spend.override_month = Some(month_key(Local::now()));
                    cx.notify();
                }
                Err(error) => Toaster::error(error.to_string(), cx),
            });
        })
        .detach();
    }

    pub(super) fn spend_card(
        &mut self,
        limit: Option<u64>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.ensure_spend(cx);
        let spend = self.spend.spend?;
        let colors = cx.theme().colors.clone();
        let month = month_key(Local::now());
        let over = over_limit(&spend, limit, self.spend.override_month.as_deref(), &month);
        let let_through = limit.is_some_and(|limit| spend.tokens >= limit) && !over;
        let mut parts = vec![cx.t_with(
            "agents.spend.tokens",
            &Args::default().with("tokens", tokens_short(spend.tokens)),
        )];
        if let Some(cost) = spend.cost_usd {
            parts.push(format!("US$ {cost:.2}").into());
        }
        parts.push(cx.t_with("agents.spend.turns", &Args::count(spend.turns as i64)));
        let summary = parts
            .iter()
            .map(|part| part.to_string())
            .collect::<Vec<_>>()
            .join(" · ");
        let bar = limit.map(|limit| {
            let ratio = (spend.tokens as f32 / limit as f32).clamp(0.0, 1.0);
            let fill = if ratio >= 1.0 {
                tw::AMBER_500.hsla()
            } else {
                colors.accent_calm
            };
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .h(px(6.0))
                        .w_full()
                        .rounded_full()
                        .bg(colors.foreground.opacity(0.08))
                        .child(
                            div()
                                .h_full()
                                .w(gpui::relative(ratio))
                                .rounded_full()
                                .bg(fill),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t_with(
                            "agents.spend.ofLimit",
                            &Args::default().with("limit", tokens_short(limit)),
                        )),
                )
        });
        let warning = over.then(|| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(tw::AMBER_600.hsla())
                        .child(cx.t("agents.spend.over")),
                )
                .child(
                    Button::new("agent-spend-continue")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .on_click(cx.listener(|page, _, _, cx| page.let_past_limit(cx)))
                        .child(cx.t("agents.spend.continue")),
                )
        });
        let through = let_through.then(|| {
            div()
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("agents.spend.letThrough"))
        });
        Some(
            Self::card(cx)
                .debug_selector(|| "agent-spend".into())
                .flex()
                .flex_col()
                .gap(px(12.0))
                .p(px(20.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(cx.t("agents.spend.title")),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(colors.foreground)
                        .child(summary),
                )
                .children(bar)
                .children(warning)
                .children(through)
                .into_any_element(),
        )
    }
}
