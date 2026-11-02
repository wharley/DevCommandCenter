//! `IdeaCard`: what the idea says (problem, audience, verdict, MVP — amber
//! where still missing), the repository's name (the candidates as chips,
//! the favourite ★), private or public (public warns, and can keep
//! `docs/IDEIA.md` local), the folder it moves to, what is still weak, and
//! "Descartar" / "Ajustar" / "Publicar"; discarding asks first.

use gpui::{
    AnyElement, Context, FontWeight, PathPromptOptions, SharedString, Window, div, prelude::*, px,
};

use super::{IdeaPanel, Pending};
use crate::data::idea::{self, Visibility};
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Dialog, Icon, Switch, ToggleGroup, dialog_footer,
    dialog_header,
};

fn row(label: SharedString, value: SharedString, missing: bool, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .gap(px(12.0))
        .text_size(px(12.0))
        .child(
            div()
                .w(px(112.0))
                .flex_shrink_0()
                .text_color(colors.muted_foreground)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .line_clamp(2)
                .text_color(if missing {
                    tw::AMBER_700.hsla()
                } else {
                    colors.foreground
                })
                .child(value),
        )
        .into_any_element()
}

fn label(text: SharedString, cx: &gpui::App) -> AnyElement {
    div()
        .text_size(px(11.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().colors.muted_foreground)
        .child(text)
        .into_any_element()
}

impl IdeaPanel {
    pub(super) fn card(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let status = self.status.clone();
        let empty = cx.t("agents.research.empty");
        let text = |value: Option<&String>| {
            value
                .filter(|value| !value.is_empty())
                .map(|value| SharedString::from(value.clone()))
                .unwrap_or_else(|| empty.clone())
        };
        let filled = |index: usize| {
            status
                .as_ref()
                .is_some_and(|status| status.milestones[index])
        };
        let verdict = status.as_ref().and_then(|status| status.verdict.clone());
        let name = self.name(cx);
        let valid = idea::is_valid_repository_name(&name);
        let busy = self.busy || self.pending.is_some();
        let warnings: Vec<SharedString> = status
            .as_ref()
            .map(|status| {
                [
                    (
                        status.verdict.as_deref() != Some("seguir"),
                        "agents.research.warnVerdict",
                    ),
                    (!status.has_architecture, "agents.research.warnArchitecture"),
                    (!status.has_roadmap, "agents.research.warnRoadmap"),
                ]
                .into_iter()
                .filter(|(weak, _)| *weak)
                .map(|(_, key)| cx.t(key))
                .collect()
            })
            .unwrap_or_default();
        let candidates = status
            .as_ref()
            .map(|status| (status.names.clone(), status.favorite_name.clone()))
            .unwrap_or_default();
        let chips = candidates
            .0
            .into_iter()
            .enumerate()
            .map(|(index, candidate)| {
                let shown = if candidates.1.as_deref() == Some(candidate.as_str()) {
                    format!("★ {candidate}")
                } else {
                    candidate.clone()
                };
                div()
                    .id(SharedString::from(format!("idea-name-{index}")))
                    .cursor_pointer()
                    .rounded_full()
                    .border_1()
                    .border_color(colors.border.opacity(0.7))
                    .px(px(8.0))
                    .py(px(2.0))
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .hover(|style| style.bg(cx.theme().colors.accent.opacity(0.5)))
                    .on_click(cx.listener(move |panel, _, _, cx| {
                        let picked = idea::repository_name(&candidate);
                        if let Some(input) = panel.name.clone() {
                            input.update(cx, |input, cx| input.set_text(picked, cx));
                        }
                        panel.name_touched = true;
                        cx.notify();
                    }))
                    .child(shown)
            });
        let visibility = ToggleGroup::new(
            "idea-visibility",
            vec![
                cx.t("agents.research.visibility.private"),
                cx.t("agents.research.visibility.public"),
            ],
            Some(usize::from(self.visibility == Visibility::Public)),
        )
        .accent()
        .on_select({
            let panel = cx.entity().downgrade();
            move |index, _, cx| {
                let _ = panel.update(cx, |panel, cx| {
                    panel.visibility = if index == 1 {
                        Visibility::Public
                    } else {
                        Visibility::Private
                    };
                    cx.notify();
                });
            }
        });
        let public = (self.visibility == Visibility::Public).then(|| {
            let panel = cx.entity().downgrade();
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(tw::AMBER_500.hsla().opacity(0.4))
                .bg(tw::AMBER_500.hsla().opacity(0.05))
                .p(px(10.0))
                .text_size(px(11.0))
                .text_color(tw::AMBER_700.hsla())
                .child(cx.t("agents.research.publicWarning"))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_color(colors.foreground)
                        .child(Switch::new("idea-keep-local", self.keep_local).on_toggle(
                            move |checked, _, cx| {
                                let _ = panel.update(cx, |panel, cx| {
                                    panel.keep_local = checked;
                                    cx.notify();
                                });
                            },
                        ))
                        .child(cx.t("agents.research.keepLocal")),
                )
        });
        let folder = format!(
            "{}/{}",
            self.folder(),
            if name.is_empty() { "…" } else { &name }
        );
        let title = cx.t_with(
            "agents.research.cardTitle",
            &Args::default().with(
                "idea",
                status
                    .as_ref()
                    .and_then(|status| status.title.clone())
                    .unwrap_or_else(|| cx.t("agents.research.untitled").to_string()),
            ),
        );
        div()
            .debug_selector(|| "idea-card".into())
            .rounded(px(18.0))
            .border_1()
            .border_color(colors.border.opacity(0.7))
            .bg(colors.card)
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .rounded_full()
                            .border_1()
                            .border_color(colors.border.opacity(0.7))
                            .px(px(8.0))
                            .text_size(px(10.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("agents.research.draft").to_uppercase()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(row(
                        cx.t("agents.research.milestones.problem"),
                        text(status.as_ref().map(|status| &status.problem)),
                        !filled(0),
                        cx,
                    ))
                    .child(row(
                        cx.t("agents.research.milestones.audience"),
                        text(status.as_ref().map(|status| &status.audience)),
                        !filled(1),
                        cx,
                    ))
                    .child(row(
                        cx.t("agents.research.verdict"),
                        verdict
                            .as_ref()
                            .map(|verdict| cx.t(&format!("agents.research.verdicts.{verdict}")))
                            .unwrap_or_else(|| empty.clone()),
                        verdict.as_deref() != Some("seguir"),
                        cx,
                    ))
                    .child(row(
                        "MVP".into(),
                        text(status.as_ref().map(|status| &status.mvp)),
                        status.as_ref().is_none_or(|status| status.mvp.is_empty()),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(label(cx.t("agents.research.name"), cx))
                    .child(
                        div()
                            .h(px(32.0))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(if valid || name.is_empty() {
                                colors.border
                            } else {
                                colors.destructive
                            })
                            .children(self.name.clone()),
                    )
                    .child(div().flex().flex_wrap().gap(px(4.0)).children(chips))
                    .when(!valid && !name.is_empty(), |field| {
                        field.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(colors.destructive)
                                .child(cx.t("agents.research.nameInvalid")),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(label(cx.t("agents.research.repository"), cx))
                    .child(visibility)
                    .children(public),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(label(cx.t("agents.research.folder"), cx))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .rounded(px(6.0))
                                    .border_1()
                                    .border_color(colors.border.opacity(0.7))
                                    .px(px(10.0))
                                    .py(px(6.0))
                                    .font_family(fonts::MONO)
                                    .text_size(px(11.0))
                                    .child(folder),
                            )
                            .child(
                                Button::new("idea-choose-folder")
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Sm)
                                    .on_click(cx.listener(|panel, _, _, cx| panel.pick_folder(cx)))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .child(Icon::new("folder-open").size(14.0))
                                            .child(cx.t("agents.research.chooseFolder")),
                                    ),
                            ),
                    ),
            )
            .when(!warnings.is_empty(), |card| {
                card.child(
                    div()
                        .flex()
                        .flex_col()
                        .text_size(px(11.0))
                        .text_color(tw::AMBER_700.hsla())
                        .children(warnings.into_iter().map(|warning| format!("• {warning}"))),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .pt(px(12.0))
                    .border_t_1()
                    .border_color(colors.border.opacity(0.6))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t(if self.busy {
                                "agents.research.waitTurn"
                            } else {
                                "agents.research.changeLater"
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.0))
                            .child(
                                Button::new("idea-discard")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::Sm)
                                    .disabled(busy)
                                    .on_click(cx.listener(|panel, _, _, cx| {
                                        panel.confirm_discard = true;
                                        cx.notify();
                                    }))
                                    .child(cx.t("agents.research.discard")),
                            )
                            .child(
                                Button::new("idea-adjust")
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Sm)
                                    .on_click(cx.listener(|panel, _, _, cx| {
                                        panel.card_open = false;
                                        cx.notify();
                                    }))
                                    .child(cx.t("agents.research.adjust")),
                            )
                            .child(
                                Button::new("idea-publish")
                                    .size(ButtonSize::Sm)
                                    .disabled(busy || !valid || status.is_none())
                                    .on_click(cx.listener(|panel, _, _, cx| panel.publish(cx)))
                                    .child(if self.pending == Some(Pending::Publish) {
                                        crate::ui::spinner("idea-publishing", 14.0)
                                            .into_any_element()
                                    } else {
                                        cx.t("agents.research.publish").into_any_element()
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn pick_folder(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(cx.t("agents.research.folder")),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |panel, cx| {
                    panel.destination = Some(path.to_string_lossy().into_owned());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// "Descartar ideia?": the task, the conversation and the folder go.
    pub(super) fn discard_dialog(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.confirm_discard {
            return None;
        }
        let pending = self.pending.is_some();
        let footer = dialog_footer(
            [
                Button::new("idea-discard-cancel")
                    .variant(ButtonVariant::Outline)
                    .disabled(pending)
                    .on_click(cx.listener(|panel, _, _, cx| {
                        panel.confirm_discard = false;
                        cx.notify();
                    }))
                    .child(cx.t("agents.editor.cancel"))
                    .into_any_element(),
                Button::new("idea-discard-confirm")
                    .variant(ButtonVariant::Destructive)
                    .disabled(pending)
                    .on_click(cx.listener(|panel, _, _, cx| panel.discard(cx)))
                    .child(if self.pending == Some(Pending::Discard) {
                        crate::ui::spinner("idea-discarding", 14.0).into_any_element()
                    } else {
                        cx.t("agents.research.discardConfirm").into_any_element()
                    })
                    .into_any_element(),
            ],
            cx,
        );
        let close = cx.entity().downgrade();
        Some(
            Dialog::new("idea-discard-dialog", move |_: &mut Window, cx| {
                let _ = close.update(cx, |panel, cx| {
                    if panel.pending.is_none() {
                        panel.confirm_discard = false;
                        cx.notify();
                    }
                });
            })
            .close_button(!pending)
            .child(dialog_header(
                cx.t("agents.research.discardTitle"),
                Some(cx.t_with(
                    "agents.research.discardDescription",
                    &Args::default().with("folder", self.root.clone()),
                )),
                cx,
            ))
            .child(footer)
            .into_any_element(),
        )
    }
}
