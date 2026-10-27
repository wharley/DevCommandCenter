//! The configuration tab: the card with TypeSafe Jev's switch, mode, model
//! routing, key, URL and model, the decision points, the security note and
//! "Salvar configuração".

use gpui::{AnyElement, Context, Entity, FontWeight, SharedString, div, prelude::*, px};

use super::super::Settings;
use crate::data::decision_provider::Point;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::text_input::TextInput;
use crate::ui::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Icon, Switch, ToggleGroup, spinner,
};

fn label(text: SharedString, cx: &gpui::App) -> AnyElement {
    div()
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().colors.foreground)
        .child(text)
        .into_any_element()
}

fn small(text: impl Into<SharedString>, cx: &gpui::App) -> AnyElement {
    div()
        .text_size(px(11.0))
        .line_height(px(17.9))
        .text_color(cx.theme().colors.muted_foreground)
        .child(text.into())
        .into_any_element()
}

/// shadcn's `Input`: h32, rounded-md, the field's border.
pub(super) fn field(id: &'static str, input: &Entity<TextInput>, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .debug_selector(move || id.to_string())
        .flex()
        .items_center()
        .w_full()
        .h(px(32.0))
        .px(px(12.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(colors.input)
        .text_size(px(14.0))
        .child(input.clone())
        .into_any_element()
}

fn cell(children: Vec<AnyElement>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .min_w_0()
        .children(children)
        .into_any_element()
}

impl Settings {
    fn decision_switch(
        &self,
        id: &'static str,
        on: bool,
        text: SharedString,
        size: f32,
        change: impl Fn(&mut crate::data::decision_provider::Draft, bool) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let this = cx.entity().downgrade();
        let change = std::rc::Rc::new(change);
        div()
            .flex()
            .items_center()
            .gap(px(if size > 11.5 { 12.0 } else { 8.0 }))
            .when(size > 11.5, |row| row.h(px(32.0)))
            .child(Switch::new(id, on).on_toggle(move |on, _, cx| {
                let change = change.clone();
                let _ = this.update(cx, |settings, cx| {
                    settings.update_decision(|draft| change(draft, on), cx)
                });
            }))
            .child(
                div()
                    .text_size(px(size))
                    .text_color(cx.theme().colors.muted_foreground)
                    .child(text),
            )
            .into_any_element()
    }

    pub(super) fn decision_configuration(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let decision = &self.decision;
        let draft = &decision.draft;
        let active = |on: bool, cx: &gpui::App| {
            cx.t(if on {
                "settings.decisionProvider.active"
            } else {
                "settings.decisionProvider.inactive"
            })
        };
        let configured = decision
            .saved
            .as_ref()
            .is_some_and(|saved| saved.api_key_configured);
        let provider_on = draft.provider == "typesafe";
        let automatic = draft.model_routing == "automatic";
        let modes = ToggleGroup::new(
            "settings-decision-mode",
            vec![
                cx.t("settings.decisionProvider.modes.observe"),
                cx.t("settings.decisionProvider.modes.enforce"),
            ],
            Some(usize::from(draft.mode == "enforce")),
        )
        .on_select({
            let this = cx.entity().downgrade();
            move |index, _, cx| {
                let _ = this.update(cx, |settings, cx| {
                    settings.update_decision(
                        |draft| draft.mode = if index == 1 { "enforce" } else { "observe" }.into(),
                        cx,
                    )
                });
            }
        });
        let cells = vec![
            cell(vec![
                label(cx.t("settings.decisionProvider.providerLabel"), cx),
                self.decision_switch(
                    "settings-decision-provider",
                    provider_on,
                    active(provider_on, cx),
                    12.0,
                    |draft, on| draft.provider = if on { "typesafe" } else { "disabled" }.into(),
                    cx,
                ),
                small("TypeSafe Jev", cx),
            ]),
            cell(vec![
                label(cx.t("settings.decisionProvider.modeLabel"), cx),
                div().flex().child(modes).into_any_element(),
            ]),
            cell(vec![
                label(cx.t("settings.decisionProvider.modelRoutingModeLabel"), cx),
                self.decision_switch(
                    "settings-decision-routing",
                    automatic,
                    cx.t(if automatic {
                        "settings.decisionProvider.modelRoutingModes.automatic"
                    } else {
                        "settings.decisionProvider.modelRoutingModes.manual"
                    }),
                    12.0,
                    |draft, on| {
                        draft.model_routing = if on { "automatic" } else { "manual" }.into()
                    },
                    cx,
                ),
                small(cx.t("settings.decisionProvider.modelRoutingHint"), cx),
            ]),
            cell(
                [
                    Some(label(cx.t("settings.decisionProvider.apiKeyLabel"), cx)),
                    Some(field("settings-decision-key", &decision.api_key, cx)),
                    configured.then(|| {
                        Button::new("settings-decision-clear-key")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm)
                            .padding_x(8.0)
                            .text_size(11.0)
                            .on_click(
                                cx.listener(|settings, _, _, cx| settings.clear_decision_key(cx)),
                            )
                            .child(cx.t(if draft.clear_api_key {
                                "settings.decisionProvider.clearPending"
                            } else {
                                "settings.decisionProvider.clearKey"
                            }))
                            .into_any_element()
                    }),
                ]
                .into_iter()
                .flatten()
                .collect(),
            ),
            cell(vec![
                label(cx.t("settings.decisionProvider.baseUrlLabel"), cx),
                field("settings-decision-url", &decision.base_url, cx),
            ]),
            cell(vec![
                label(cx.t("settings.decisionProvider.modelLabel"), cx),
                field("settings-decision-model", &decision.model, cx),
            ]),
        ];
        let points = decision.thresholds.iter().map(|(point, input)| {
            let key = point.key();
            let on = draft.enabled(*point);
            let point = *point;
            div()
                .rounded(px(10.0))
                .border_1()
                .border_color(colors.border.opacity(0.5))
                .p(px(12.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.0))
                        .child(label(
                            cx.t(&format!("settings.decisionProvider.points.{key}.label")),
                            cx,
                        ))
                        .child(self.decision_switch(
                            match point {
                                Point::MemoryFilter => "settings-decision-point-memory_filter",
                                Point::SkillRouter => "settings-decision-point-skill_router",
                                Point::ModelRouter => "settings-decision-point-model_router",
                                Point::CompletionReview => {
                                    "settings-decision-point-completion_review"
                                }
                                Point::ToolGuard => "settings-decision-point-tool_guard",
                            },
                            on,
                            active(on, cx),
                            11.0,
                            move |draft, on| draft.set_enabled(point, on),
                            cx,
                        )),
                )
                .child(div().mt(px(4.0)).child(small(
                    cx.t(&format!("settings.decisionProvider.points.{key}.hint")),
                    cx,
                )))
                .child(
                    div()
                        .mt(px(8.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(small(
                            cx.t(&format!("settings.decisionProvider.points.{key}.threshold")),
                            cx,
                        ))
                        .child(field("settings-decision-threshold", input, cx)),
                )
                .into_any_element()
        });
        let points: Vec<AnyElement> = points.collect();
        let saving = decision.saving;
        div()
            .debug_selector(|| "settings-decision-configuration".into())
            .rounded(px(14.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.muted.opacity(0.15))
            .p(px(16.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(px(36.0))
                            .rounded(px(10.0))
                            .bg(colors.primary.opacity(0.1))
                            .child(Icon::new("brain-circuit").size(16.0).color(colors.primary)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(cx.t("settings.decisionProvider.title")),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(12.0))
                                    .line_height(px(19.5))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("settings.decisionProvider.hint")),
                            ),
                    ),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .grid()
                    .grid_cols(2)
                    .gap(px(16.0))
                    .children(cells),
            )
            .child(
                div()
                    .mt(px(20.0))
                    .border_t_1()
                    .border_color(colors.border.opacity(0.5))
                    .pt(px(16.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(cx.t("settings.decisionProvider.pointsTitle")),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .child(small(cx.t("settings.decisionProvider.pointsHint"), cx)),
                    )
                    .child(
                        div()
                            .mt(px(12.0))
                            .grid()
                            .grid_cols(2)
                            .gap(px(16.0))
                            .children(points),
                    ),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .child(small(cx.t("settings.decisionProvider.securityHint"), cx)),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        Button::new("settings-decision-save")
                            .size(ButtonSize::Sm)
                            .disabled(saving)
                            .on_click(cx.listener(|settings, _, _, cx| settings.save_decision(cx)))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .children(
                                        saving.then(|| spinner("settings-decision-saving", 14.0)),
                                    )
                                    .child(cx.t("settings.decisionProvider.save")),
                            ),
                    )
                    .when(configured, |row| {
                        row.child(Badge::new(
                            cx.t("settings.decisionProvider.configured"),
                            BadgeVariant::Secondary,
                        ))
                    }),
            )
            .into_any_element()
    }
}
