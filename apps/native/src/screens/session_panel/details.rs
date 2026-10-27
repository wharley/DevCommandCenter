//! "Detalhes": the workspace, what the composer runs on, and three folded
//! groups — the session's runtime, the checkout's forge (who it acts as,
//! "Trocar conta" / "Conectar no app", the accounts to pick) and the
//! providers' catalog — then the terminal note. No CodeRabbit (left out of
//! the native app on purpose).

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::SessionPanel;
use crate::data::provider_display::{Chip, capability_chips, health};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Badge, BadgeVariant, Icon, Tracked};

fn chip(chip: Chip) -> BadgeVariant {
    match chip {
        Chip::Success => BadgeVariant::Success,
        Chip::Warn => BadgeVariant::Warn,
        Chip::Destructive => BadgeVariant::Destructive,
        Chip::Outline => BadgeVariant::Outline,
    }
}

/// `DetailRow`: the label (76px, uppercase) and the mono value.
pub(super) fn row(label: SharedString, value: AnyElement, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .gap(px(12.0))
        .border_b_1()
        .border_color(colors.border.opacity(0.35))
        .py(px(8.0))
        .text_size(px(11.0))
        .child(
            div()
                .w(px(76.0))
                .flex_none()
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.muted_foreground)
                .child(Tracked::new(label.to_uppercase(), 11.0, 0.06)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(cx.theme().fonts.mono.clone())
                .text_size(px(11.5))
                .text_color(colors.foreground)
                .child(value),
        )
        .into_any_element()
}

pub(super) fn text_row(label: SharedString, value: Option<String>, cx: &gpui::App) -> AnyElement {
    row(
        label,
        div()
            .child(value.unwrap_or_else(|| "—".into()))
            .into_any_element(),
        cx,
    )
}

fn group(title: SharedString, rows: Vec<AnyElement>, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .child(
            div()
                .mb(px(8.0))
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.muted_foreground)
                .child(Tracked::new(title.to_uppercase(), 11.0, 0.025)),
        )
        .child(
            div()
                .rounded(px(6.0))
                .border_1()
                .border_color(colors.border.opacity(0.5))
                .bg(colors.muted.opacity(0.1))
                .px(px(8.0))
                .children(rows),
        )
        .into_any_element()
}
impl SessionPanel {
    /// `DetailDisclosure`: folded until opened.
    pub(super) fn disclosure(
        &self,
        key: &'static str,
        title: SharedString,
        meta: Option<AnyElement>,
        body: impl FnOnce(&mut Context<Self>) -> AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let open = self.open.contains(key);
        div()
            .overflow_hidden()
            .rounded(px(6.0))
            .border_1()
            .border_color(colors.border.opacity(0.5))
            .bg(colors.muted.opacity(0.1))
            .child(
                div()
                    .id(SharedString::from(format!("session-details-{key}")))
                    .debug_selector(move || format!("session-details-{key}"))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .cursor_pointer()
                    .hover(|summary| summary.bg(colors.muted.opacity(0.25)))
                    .on_click(cx.listener(move |panel, _, _, cx| {
                        if !panel.open.remove(key) {
                            panel.open.insert(key);
                        }
                        cx.notify();
                    }))
                    .child(
                        Icon::new(if open {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        })
                        .size(14.0)
                        .color(colors.muted_foreground),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.foreground)
                            .child(title),
                    )
                    .children(meta),
            )
            .when(open, |details| {
                details.child(
                    div()
                        .border_t_1()
                        .border_color(colors.border.opacity(0.4))
                        .px(px(8.0))
                        .py(px(4.0))
                        .child(body(cx)),
                )
            })
            .into_any_element()
    }

    pub(super) fn details(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.ensure_forge(false, cx);
        let colors = cx.theme().colors.clone();
        let Some(data) = self.data(cx).cloned() else {
            return div().into_any_element();
        };
        let composer = self.conversation.read(cx).composer.clone();
        let (provider_label, model_label, catalog) = {
            let composer = composer.read(cx);
            (
                composer.provider().map(|provider| provider.label.clone()),
                composer.model().map(|model| model.label.clone()),
                composer
                    .catalog
                    .clone()
                    .map(|catalog| catalog.as_ref().clone())
                    .unwrap_or_default(),
            )
        };
        let state = data
            .facts
            .as_ref()
            .map_or("idle".to_string(), |facts| facts.state.clone());
        let state_label = cx.t(&format!("workbench.sessionState.{state}"));
        let field = |key: &str, cx: &gpui::App| cx.t(&format!("inspector.fields.{key}"));
        let workspace = group(
            cx.t("inspector.groups.workspace"),
            vec![
                text_row(field("name", cx), data.workspace_name.clone(), cx),
                text_row(field("id", cx), Some(data.task_id.clone()), cx),
                text_row(field("branch", cx), data.branch.clone(), cx),
                text_row(field("path", cx), data.workspace_path.clone(), cx),
            ],
            cx,
        );
        let composer_group = group(
            cx.t("inspector.groups.composer"),
            vec![
                text_row(field("provider", cx), provider_label, cx),
                text_row(field("model", cx), model_label, cx),
            ],
            cx,
        );
        let runtime_meta = div()
            .text_size(px(11.0))
            .text_color(colors.muted_foreground)
            .child(state_label.clone())
            .into_any_element();
        let runtime_data = data.clone();
        let runtime = self.disclosure(
            "runtime",
            cx.t("inspector.groups.runtime"),
            Some(runtime_meta),
            move |cx| {
                let session = runtime_data.session_id.clone();
                let state = row(
                    field("state", cx),
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.0))
                        .child(state_label.clone())
                        .children(session.map(|id| {
                            let short = if id.chars().count() > 14 {
                                format!("{}…", id.chars().take(12).collect::<String>())
                            } else {
                                id
                            };
                            Badge::new(short, BadgeVariant::Outline)
                                .text_size(10.0)
                                .weight(FontWeight::NORMAL)
                        }))
                        .into_any_element(),
                    cx,
                );
                let mut rows = vec![state];
                match &runtime_data.facts {
                    Some(facts) => rows.extend([
                        text_row(field("turns", cx), Some(facts.turn_count.to_string()), cx),
                        text_row(
                            field("checkpoints", cx),
                            Some(facts.checkpoint_count.to_string()),
                            cx,
                        ),
                        text_row(field("lastTurn", cx), facts.last_turn_prompt.clone(), cx),
                        text_row(field("providerId", cx), Some(facts.provider_id.clone()), cx),
                    ]),
                    None => rows.push(text_row(
                        field("session", cx),
                        Some(cx.t("inspector.sessionFallback").to_string()),
                        cx,
                    )),
                }
                div().children(rows).into_any_element()
            },
            cx,
        );
        let forge = self.forge_group(cx);
        let count = catalog.len();
        let providers = self.disclosure(
            "providers",
            cx.t("inspector.groups.providers"),
            (count > 0).then(|| {
                Badge::new(
                    cx.t_with("inspector.providersRegistered", &Args::count(count as i64)),
                    BadgeVariant::Secondary,
                )
                .height(20.0)
                .text_size(10.0)
                .weight(FontWeight::NORMAL)
                .into_any_element()
            }),
            move |cx| providers_dense(&catalog, cx),
            cx,
        );
        div()
            .id("session-details")
            .debug_selector(|| "session-details".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(12.0))
            .pb(px(12.0))
            .pt(px(8.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(workspace)
                    .child(composer_group)
                    .child(runtime)
                    .child(forge)
                    .child(providers)
                    .child(
                        div()
                            .text_size(px(10.0))
                            .line_height(px(16.25))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("inspector.terminalNote")),
                    ),
            )
            .into_any_element()
    }
}

/// `ProviderCatalogDense`
fn providers_dense(
    catalog: &[dcc_core::domain::provider::ProviderDescriptor],
    cx: &mut Context<SessionPanel>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    if catalog.is_empty() {
        return div()
            .rounded(px(6.0))
            .border_1()
            .border_dashed()
            .border_color(colors.border.opacity(0.6))
            .px(px(12.0))
            .py(px(8.0))
            .text_size(px(11.0))
            .text_color(colors.muted_foreground)
            .child("No providers registered in the catalog yet.")
            .into_any_element();
    }
    let count = catalog.len();
    let rows = catalog.iter().enumerate().map(|(index, provider)| {
        let (health_label, health_chip) = health(&provider.health);
        let small = |label: String, variant: BadgeVariant| {
            Badge::new(label, variant)
                .text_size(10.0)
                .weight(FontWeight::NORMAL)
        };
        div()
            .px(px(10.0))
            .py(px(8.0))
            .when(index + 1 < count, |row| {
                row.border_b_1().border_color(colors.border.opacity(0.45))
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .child(provider.label.clone()),
                    )
                    .child(
                        div()
                            .max_w(px(220.0))
                            .text_right()
                            .text_size(px(10.0))
                            .text_color(colors.muted_foreground)
                            .child(provider.description.clone()),
                    ),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(4.0))
                    .child(small(
                        if provider.stable {
                            "stable"
                        } else {
                            "experimental"
                        }
                        .into(),
                        if provider.stable {
                            BadgeVariant::Success
                        } else {
                            BadgeVariant::Outline
                        },
                    ))
                    .child(small(health_label, chip(health_chip)))
                    .children(
                        capability_chips(provider)
                            .into_iter()
                            .map(|(label, variant)| small(label.into(), chip(variant))),
                    )
                    .children(provider.models.iter().map(|model| {
                        small(
                            model.label.clone(),
                            if model.recommended {
                                BadgeVariant::Success
                            } else {
                                BadgeVariant::Outline
                            },
                        )
                    })),
            )
    });
    div()
        .rounded(px(6.0))
        .border_1()
        .border_color(colors.border.opacity(0.5))
        .bg(colors.muted.opacity(0.1))
        .children(rows)
        .into_any_element()
}
