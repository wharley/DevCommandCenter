//! "Fontes" on an agent's page (`dcc_core::domain::agent_source`): what it
//! watches in the person's connectors — one row each, with how often it is
//! read and when it last was, a switch and "Remover". A source is made by
//! asking the agent in its conversation; it proposes, the person creates.

use dcc_core::domain::agent_source::AgentSource;
use gpui::{AnyElement, BorrowAppContext, Context, FontWeight, SharedString, div, prelude::*, px};

use super::AgentPage;
use crate::data::agent_sources::{Sources, read};
use crate::data::time::medium_date_time;
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Switch, Toaster};

impl AgentPage {
    fn sources(&self, cx: &gpui::App) -> Vec<AgentSource> {
        cx.try_global::<Sources>()
            .map(|sources| sources.of(&self.agent_id).cloned().collect())
            .unwrap_or_default()
    }

    fn update_sources(&self, cx: &mut Context<Self>, change: impl FnOnce(&mut Vec<AgentSource>)) {
        if cx.has_global::<Sources>() {
            cx.update_global::<Sources, _>(|sources, _| change(&mut sources.list));
        }
    }

    /// Runs `job` on the desktop, then reads the list again.
    fn source_call(
        &self,
        cx: &mut Context<Self>,
        job: impl FnOnce(
            crate::data::desktop_api::DesktopApi,
        ) -> Result<(), crate::data::desktop_api::ApiError>
        + Send
        + 'static,
    ) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        cx.spawn(async move |_, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { job(api).map(|()| read()) })
                .await;
            let _ = cx.update(|cx| match done {
                Ok(list) => cx.set_global(Sources { list }),
                Err(error) => Toaster::error(error.to_string(), cx),
            });
        })
        .detach();
    }

    fn source_row(&self, index: usize, source: &AgentSource, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let english = cx.global::<I18n>().locale() == Locale::En;
        let every = cx.t_with(
            "agents.sources.every",
            &Args::default().with("minutes", source.every_minutes.to_string()),
        );
        let detail = match source.last_checked_at.as_deref() {
            Some(at) => format!(
                "{every} · {}",
                cx.t_with(
                    "agents.sources.lastRead",
                    &Args::default().with("when", medium_date_time(at, english)),
                )
            ),
            None => every.to_string(),
        };
        let page = cx.entity().downgrade();
        let (toggled, removed) = (source.clone(), source.clone());
        div()
            .debug_selector(move || format!("agent-source-{index}"))
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(Icon::new("radar").size(16.0).color(colors.muted_foreground))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(format!("{} — {}", source.connector, source.watch)),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(detail),
                    ),
            )
            .child(
                Switch::new(
                    SharedString::from(format!("agent-source-switch-{index}")),
                    source.enabled,
                )
                .on_toggle(move |on, _, cx| {
                    let source = toggled.clone();
                    let _ = page.update(cx, |page, cx| {
                        let id = source.id.clone();
                        page.update_sources(cx, |list| {
                            for found in list.iter_mut().filter(|found| found.id == id) {
                                found.enabled = on;
                            }
                        });
                        page.source_call(cx, move |api| api.switch_source(&source, on));
                        cx.notify();
                    });
                }),
            )
            .child(
                Button::new(SharedString::from(format!("agent-source-delete-{index}")))
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .on_click(cx.listener(move |page, _, _, cx| {
                        let source = removed.clone();
                        let gone = source.id.clone();
                        page.update_sources(cx, |list| list.retain(|found| found.id != gone));
                        page.source_call(cx, move |api| api.delete_source(&source));
                        cx.notify();
                    }))
                    .child(cx.t("agents.sources.delete")),
            )
            .into_any_element()
    }

    pub(super) fn sources_card(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let sources = self.sources(cx);
        let rows: Vec<AnyElement> = sources
            .iter()
            .enumerate()
            .map(|(index, source)| self.source_row(index, source, cx))
            .collect();
        Self::card(cx)
            .debug_selector(|| "agent-sources".into())
            .flex()
            .flex_col()
            .gap(px(14.0))
            .p(px(20.0))
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(cx.t("agents.sources.title")),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(19.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("agents.sources.hint")),
            )
            .children(rows)
            .into_any_element()
    }
}
