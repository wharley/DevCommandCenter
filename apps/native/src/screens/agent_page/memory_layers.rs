//! The layers of "Memória": the person's notes, which hold in every project,
//! and each project's, which hold only in that project's tasks. The card
//! shows one layer at a time; what it lists, how full it reads and where a
//! new note goes all follow the layer picked here.

use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};

use super::memory::MemoryCard;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::Icon;
use crate::ui::menu::{Menu, MenuEntry, MenuItem};

/// A project offered as a layer: its id and name.
pub type Layer = (String, String);

impl MemoryCard {
    /// The projects the agent works in (the page sets them).
    pub fn set_layers(&mut self, layers: Vec<Layer>, cx: &mut Context<Self>) {
        if self.layers != layers {
            self.layers = layers;
            cx.notify();
        }
    }

    /// The projects to pick: the agent's, and any other that keeps notes.
    pub(crate) fn layer_choices(&self) -> Vec<Layer> {
        let mut choices = self.layers.clone();
        for note in self.memory.iter().flat_map(|memory| &memory.notes) {
            if let Some(project) = &note.project_id
                && !choices.iter().any(|(id, _)| id == project)
            {
                choices.push((project.clone(), project.clone()));
            }
        }
        choices
    }

    fn notes_in(&self, layer: Option<&str>) -> usize {
        self.memory.as_ref().map_or(0, |memory| {
            memory
                .notes
                .iter()
                .filter(|note| note.project_id.as_deref() == layer)
                .count()
        })
    }

    pub(crate) fn pick_layer(&mut self, layer: Option<String>, cx: &mut Context<Self>) {
        if self.layer != layer {
            self.layer = layer;
            self.editing = None;
            cx.notify();
        }
    }

    /// "Você ▾" / "Checkout ▾": the layer shown, and the menu to change it.
    pub(crate) fn layer_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let choices = self.layer_choices();
        let person = cx.t("agents.memory.layerPerson");
        let summary: SharedString = match &self.layer {
            None => person.clone(),
            Some(project) => choices
                .iter()
                .find(|(id, _)| id == project)
                .map_or_else(|| project.clone(), |(_, name)| name.clone())
                .into(),
        };
        let count = |n: usize| -> SharedString {
            cx.t_with(
                "agents.memory.layerNotes",
                &Args::default().with("count", n.to_string()),
            )
        };
        let mut items: Vec<(Option<String>, SharedString, SharedString)> =
            vec![(None, person, cx.t("agents.memory.layerPersonHint"))];
        for (id, name) in choices {
            let n = self.notes_in(Some(&id));
            items.push((Some(id), name.into(), count(n)));
        }
        let current = self.layer.clone();
        let card = cx.entity().downgrade();
        Menu::dropdown(
            "agent-memory-layer",
            move |_| {
                div()
                    .debug_selector(|| "agent-memory-layer".into())
                    .flex()
                    .flex_none()
                    .h(px(28.0))
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.8))
                    .px(px(10.0))
                    .text_size(px(12.0))
                    .text_color(colors.foreground)
                    .cursor_pointer()
                    .hover(|trigger| trigger.bg(colors.accent.opacity(0.5)))
                    .child(div().max_w(px(160.0)).truncate().child(summary))
                    .child(
                        Icon::new("chevron-down")
                            .size(13.0)
                            .color(colors.muted_foreground),
                    )
                    .into_any_element()
            },
            move |_, _| {
                items
                    .iter()
                    .map(|(layer, label, detail)| {
                        let (card, picked) = (card.clone(), layer.clone());
                        MenuEntry::Item(
                            MenuItem::new(label.clone(), move |_: &mut Window, cx| {
                                let picked = picked.clone();
                                let _ = card.update(cx, |card, cx| card.pick_layer(picked, cx));
                            })
                            .detail(detail.clone())
                            .trailing(if *layer == current { "✓" } else { "" }),
                        )
                    })
                    .collect()
            },
        )
        .min_width(220.0)
        .into_any_element()
    }
}
