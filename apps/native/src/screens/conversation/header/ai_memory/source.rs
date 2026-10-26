//! One recovered source in the menu: its kind, pin or ignore mark and
//! when; its title and excerpt (struck through under a correction); the
//! correction or its field; and its actions.

use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, div, prelude::*, px};

use crate::data::ai_memory::{SourceHit, SourceKind, clean_text, timestamp};
use crate::i18n::{Locale, Translate};
use crate::screens::conversation::ConversationView;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, spinner, tabular_nums};

/// `KIND_TONE`: the chip's fill and text.
fn tone(kind: SourceKind, dark: bool, muted: Hsla, muted_text: Hsla) -> (Hsla, Hsla) {
    let pick = |base: tw::Oklch, light: tw::Oklch, darker: tw::Oklch| {
        (
            base.alpha(0.1),
            if dark { darker.hsla() } else { light.hsla() },
        )
    };
    match kind {
        SourceKind::Prompt => pick(tw::SKY_500, tw::SKY_600, tw::SKY_400),
        SourceKind::Answer => pick(tw::VIOLET_500, tw::VIOLET_600, tw::VIOLET_400),
        SourceKind::Page => pick(tw::EMERALD_500, tw::EMERALD_600, tw::EMERALD_400),
        SourceKind::Observation => (muted, muted_text),
    }
}

/// `SourceActionButton`: ghost, 24px, the icon and its word.
fn action_button(
    id: (&'static str, usize),
    icon: &'static str,
    label: SharedString,
    disabled: bool,
    cx: &mut Context<ConversationView>,
    on_click: impl Fn(&mut ConversationView, &mut gpui::Window, &mut Context<ConversationView>)
    + 'static,
) -> AnyElement {
    Button::new(id)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Sm)
        .height(24.0)
        .padding_x(6.0)
        .text_size(11.0)
        .muted()
        .disabled(disabled)
        .on_click(cx.listener(move |view, _, window, cx| {
            cx.stop_propagation();
            on_click(view, window, cx)
        }))
        .child(Icon::new(icon).size(12.0))
        .child(label)
        .into_any_element()
}

impl ConversationView {
    pub(super) fn ai_memory_source(
        &self,
        index: usize,
        source: &SourceHit,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let dark = cx.theme().appearance == Appearance::Dark;
        let english = cx.global::<crate::i18n::I18n>().locale() == Locale::En;
        let memory = &self.header.ai_memory;
        let key = source.key();
        let action = memory.actions.get(&key);
        let pinned = action.is_some_and(|action| action.action == "pinned");
        let ignored = action.is_some_and(|action| action.action == "ignored");
        let correction = action
            .filter(|action| action.action == "corrected")
            .and_then(|action| action.correction.clone());
        let editing = memory
            .editing
            .as_ref()
            .filter(|(editing, _)| *editing == key)
            .map(|(_, area)| area.clone());
        let saving = memory.saving.as_deref() == Some(key.as_str());
        let kind = source.kind();
        let (fill, text) = tone(kind, dark, colors.muted, colors.muted_foreground);
        let title = source
            .title
            .as_deref()
            .map(clean_text)
            .filter(|title| !title.is_empty())
            .or_else(|| source.path.clone())
            .unwrap_or_else(|| cx.t("workbench.aiMemory.untitled").to_string());
        let snippet = source.snippet.as_deref().map(clean_text);
        let amber = if dark { tw::AMBER_400 } else { tw::AMBER_600 }.hsla();
        let emerald = if dark {
            tw::EMERALD_400
        } else {
            tw::EMERALD_600
        }
        .hsla();
        let head = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .rounded(px(4.0))
                    .px(px(6.0))
                    .py(px(1.0))
                    .bg(fill)
                    .text_color(text)
                    .text_size(px(10.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(cx.t(&format!("workbench.aiMemory.kind.{}", kind.id()))),
            )
            .when(pinned, |head| {
                head.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(2.0))
                        .text_size(px(10.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(amber)
                        .child(Icon::new("pin").size(12.0))
                        .child(cx.t("workbench.aiMemory.pinned")),
                )
            })
            .when(ignored, |head| {
                head.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("workbench.aiMemory.ignoredLabel")),
                )
            })
            .when_some(source.created_at.clone(), |head, at| {
                head.child(
                    div()
                        .ml_auto()
                        .flex_shrink_0()
                        .text_size(px(10.0))
                        .font_features(tabular_nums())
                        .text_color(colors.muted_foreground)
                        .child(timestamp(&at, english)),
                )
            });
        let correction_box = correction
            .clone()
            .filter(|_| editing.is_none())
            .map(|text| {
                div()
                    .mt(px(6.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(tw::EMERALD_500.alpha(0.2))
                    .bg(tw::EMERALD_500.alpha(0.06))
                    .px(px(8.0))
                    .py(px(6.0))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(emerald)
                            .child(cx.t("workbench.aiMemory.correctionLabel")),
                    )
                    .child(
                        div()
                            .mt(px(2.0))
                            .text_size(px(11.0))
                            .line_height(px(17.0))
                            .child(text),
                    )
            });
        let tools: AnyElement = match editing {
            Some(area) => {
                let empty = area.read(cx).text().trim().is_empty();
                let save_key = key.clone();
                let save_area = area.clone();
                div()
                    .mt(px(8.0))
                    .text_size(px(11.0))
                    .child(
                        div()
                            .min_h(px(64.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(colors.input)
                            .px(px(8.0))
                            .py(px(6.0))
                            .child(area),
                    )
                    .child(
                        div()
                            .mt(px(6.0))
                            .flex()
                            .justify_end()
                            .gap(px(4.0))
                            .child(
                                Button::new(("ai-memory-edit-cancel", index))
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::Sm)
                                    .height(24.0)
                                    .padding_x(8.0)
                                    .text_size(11.0)
                                    .on_click(cx.listener(|view, _, _, cx| view.cancel_edit(cx)))
                                    .child(cx.t("workbench.aiMemory.cancel")),
                            )
                            .child(
                                Button::new(("ai-memory-edit-save", index))
                                    .size(ButtonSize::Sm)
                                    .height(24.0)
                                    .padding_x(8.0)
                                    .text_size(11.0)
                                    .disabled(saving || empty)
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        let text = save_area.read(cx).text().trim().to_string();
                                        view.save_source(
                                            save_key.clone(),
                                            "corrected",
                                            Some(text),
                                            cx,
                                        )
                                    }))
                                    .child(cx.t("workbench.aiMemory.correct")),
                            ),
                    )
                    .into_any_element()
            }
            None => {
                let mut row = div()
                    .ml(px(-6.0))
                    .mt(px(4.0))
                    .flex()
                    .items_center()
                    .gap(px(2.0));
                if ignored {
                    let restore = key.clone();
                    row = row.child(action_button(
                        ("ai-memory-restore", index),
                        "rotate-ccw",
                        cx.t("workbench.aiMemory.restore"),
                        saving,
                        cx,
                        move |view, _, cx| view.save_source(restore.clone(), "cleared", None, cx),
                    ));
                } else {
                    let pin = key.clone();
                    row = row.child(action_button(
                        ("ai-memory-pin", index),
                        if pinned { "pin-off" } else { "pin" },
                        cx.t(if pinned {
                            "workbench.aiMemory.unpin"
                        } else {
                            "workbench.aiMemory.pin"
                        }),
                        saving,
                        cx,
                        move |view, _, cx| {
                            let action = if pinned { "cleared" } else { "pinned" };
                            view.save_source(pin.clone(), action, None, cx)
                        },
                    ));
                    let edit = key.clone();
                    let current = correction.clone();
                    row = row.child(action_button(
                        ("ai-memory-correct", index),
                        "pencil-line",
                        cx.t(if correction.is_some() {
                            "workbench.aiMemory.editCorrection"
                        } else {
                            "workbench.aiMemory.correctAction"
                        }),
                        saving,
                        cx,
                        move |view, window, cx| {
                            view.edit_source(edit.clone(), current.clone(), window, cx)
                        },
                    ));
                    if correction.is_some() {
                        let undo = key.clone();
                        row = row.child(action_button(
                            ("ai-memory-undo", index),
                            "rotate-ccw",
                            cx.t("workbench.aiMemory.undoCorrection"),
                            saving,
                            cx,
                            move |view, _, cx| view.save_source(undo.clone(), "cleared", None, cx),
                        ));
                    }
                    let ignore = key.clone();
                    row = row.child(action_button(
                        ("ai-memory-ignore", index),
                        "eye-off",
                        cx.t("workbench.aiMemory.ignore"),
                        saving,
                        cx,
                        move |view, _, cx| view.save_source(ignore.clone(), "ignored", None, cx),
                    ));
                }
                row.when(saving, |row| {
                    row.child(
                        div()
                            .ml(px(4.0))
                            .text_color(colors.muted_foreground)
                            .child(spinner(("ai-memory-saving", index), 12.0)),
                    )
                })
                .into_any_element()
            }
        };
        div()
            .debug_selector(move || format!("ai-memory-source-{index}"))
            .border_b_1()
            .border_color(colors.border.opacity(0.4))
            .px(px(12.0))
            .py(px(10.0))
            .when(pinned, |item| item.bg(tw::AMBER_500.alpha(0.04)))
            .when(ignored, |item| item.opacity(0.6))
            .child(head)
            .child(
                div()
                    .mt(px(6.0))
                    .max_h(px(33.0))
                    .overflow_hidden()
                    .text_size(px(12.0))
                    .line_height(px(16.5))
                    .font_weight(FontWeight::MEDIUM)
                    .child(title),
            )
            .when_some(
                snippet.filter(|snippet| !snippet.is_empty()),
                |item, snippet| {
                    item.child(
                        div()
                            .mt(px(4.0))
                            .max_h(px(54.0))
                            .overflow_hidden()
                            .text_size(px(11.0))
                            .line_height(px(17.875))
                            .text_color(colors.muted_foreground)
                            .when(correction.is_some(), |text| text.line_through())
                            .child(snippet),
                    )
                },
            )
            .children(correction_box)
            .child(tools)
            .into_any_element()
    }
}
