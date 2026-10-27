//! "Roda em" (`AgentExecutionPicker`): provider, model and effort in one
//! menu — each level opens the next and the last pick sets all three — with
//! its summary on the trigger.

use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};

use super::editor::AgentEditor;
use crate::data::composer_settings::clamp_effort;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Icon, Menu, MenuEntry, MenuItem};

impl AgentEditor {
    /// The menu's summary: provider · model · effort, or the composer's.
    pub(super) fn summary(&self, cx: &gpui::App) -> SharedString {
        let Some(provider) = self
            .provider_id
            .as_deref()
            .and_then(|id| self.catalog.iter().find(|provider| provider.id.0 == id))
        else {
            return cx.t("agents.editor.providerCurrent");
        };
        let model = self
            .model
            .as_deref()
            .and_then(|id| provider.models.iter().find(|model| model.id == id));
        let effort = match (self.effort.as_deref(), model) {
            (Some(effort), Some(model)) if !model.effort_levels.is_empty() => {
                Some(clamp_effort(effort, &model.effort_levels))
            }
            _ => None,
        };
        [
            Some(provider.label.clone()),
            Some(model.map_or_else(
                || cx.t("agents.editor.modelDefault").to_string(),
                |model| model.label.clone(),
            )),
            effort.map(|effort| cx.t(&format!("composer.effort.{effort}")).to_string()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
        .into()
    }

    pub(super) fn picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let summary = self.summary(cx);
        let this = cx.entity().downgrade();
        let catalog = self.catalog.clone();
        let current = (
            self.provider_id.clone(),
            self.model.clone(),
            self.effort.clone(),
        );
        Menu::dropdown(
            "agent-editor-runs-on",
            move |_| {
                div()
                    .debug_selector(|| "agent-editor-runs-on".into())
                    .flex()
                    .h(px(36.0))
                    .w_full()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.8))
                    .bg(colors.background)
                    .px(px(10.0))
                    .text_size(px(13.0))
                    .text_color(colors.foreground)
                    .cursor_pointer()
                    .hover(|trigger| trigger.bg(colors.accent.opacity(0.5)))
                    .child(div().flex_1().min_w_0().truncate().child(summary))
                    .child(
                        Icon::new("chevron-down")
                            .size(14.0)
                            .color(colors.muted_foreground),
                    )
                    .into_any_element()
            },
            move |_, cx| {
                let pick =
                    |provider: Option<String>, model: Option<String>, effort: Option<String>| {
                        let this = this.clone();
                        move |_: &mut Window, cx: &mut gpui::App| {
                            let (provider, model, effort) =
                                (provider.clone(), model.clone(), effort.clone());
                            let _ = this.update(cx, |editor, cx| {
                                editor.set_execution(provider, model, effort, cx)
                            });
                        }
                    };
                let mark = |on: bool| if on { "✓" } else { "" };
                let mut entries = vec![
                    MenuEntry::Item(
                        MenuItem::new(
                            cx.t("agents.editor.providerCurrent"),
                            pick(None, None, None),
                        )
                        .trailing(mark(current.0.is_none())),
                    ),
                    MenuEntry::Separator,
                ];
                // Switched-off providers stay out, unless the agent already runs on one.
                for provider in catalog.iter().filter(|provider| {
                    provider.enabled || Some(&provider.id.0) == current.0.as_ref()
                }) {
                    let id = provider.id.0.clone();
                    let chosen = current.0.as_deref() == Some(id.as_str());
                    let mut models = vec![MenuEntry::Item(
                        MenuItem::new(
                            cx.t("agents.editor.modelDefault"),
                            pick(Some(id.clone()), None, None),
                        )
                        .trailing(mark(chosen && current.1.is_none())),
                    )];
                    for model in &provider.models {
                        let on = chosen && current.1.as_deref() == Some(model.id.as_str());
                        if model.effort_levels.is_empty() {
                            models.push(MenuEntry::Item(
                                MenuItem::new(
                                    model.label.clone(),
                                    pick(Some(id.clone()), Some(model.id.clone()), None),
                                )
                                .trailing(mark(on)),
                            ));
                            continue;
                        }
                        let effort = current
                            .2
                            .as_deref()
                            .map(|effort| clamp_effort(effort, &model.effort_levels));
                        let mut levels = vec![MenuEntry::Label(cx.t("composer.execution.effort"))];
                        levels.extend(model.effort_levels.iter().map(|level| {
                            MenuEntry::Item(
                                MenuItem::new(
                                    cx.t(&format!("composer.effort.{level}")),
                                    pick(
                                        Some(id.clone()),
                                        Some(model.id.clone()),
                                        Some(level.clone()),
                                    ),
                                )
                                .trailing(mark(on && effort.as_deref() == Some(level.as_str()))),
                            )
                        }));
                        models.push(MenuEntry::Submenu {
                            label: format!("{}{}", model.label, if on { "  ✓" } else { "" }).into(),
                            icon: None,
                            entries: levels,
                        });
                    }
                    entries.push(MenuEntry::Submenu {
                        label: format!("{}{}", provider.label, if chosen { "  ✓" } else { "" })
                            .into(),
                        icon: None,
                        entries: models,
                    });
                }
                entries
            },
        )
        .min_width(224.0)
        .into_any_element()
    }
}
