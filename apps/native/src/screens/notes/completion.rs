//! "Essa ideia já foi implementada?": a task linked to notes was completed;
//! its open notes can be completed too, or kept open. Also the link a new
//! task leaves on its note.

use gpui::{AnyElement, Context, div, prelude::*, px};

use super::NotesWorkspace;
use crate::data::notes::{NotePatch, ProjectNote};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, Icon, Toaster, dialog_header};

impl NotesWorkspace {
    /// `completionNotes`: open notes whose task was just completed.
    pub fn completion_notes(&self, cx: &gpui::App) -> Vec<ProjectNote> {
        self.store
            .read(cx)
            .notes
            .iter()
            .filter(|note| {
                note.status == "open"
                    && note
                        .implementation_task_id
                        .as_ref()
                        .is_some_and(|task| self.completion_task_ids.contains(task))
            })
            .cloned()
            .collect()
    }

    /// `setNotesCompletionTaskIds`: the tasks completed (a bundle's members too).
    pub fn task_completed(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
        self.completion_task_ids = ids;
        cx.notify();
    }

    /// "Manter abertas"
    pub fn keep_open(&mut self, cx: &mut Context<Self>) {
        if !self.busy {
            self.completion_task_ids.clear();
            cx.notify();
        }
    }

    /// "Marcar como concluída": each saved, its balloon closed.
    pub fn complete_linked(&mut self, cx: &mut Context<Self>) {
        for note in self.completion_notes(cx) {
            self.store.update(cx, |store, cx| {
                store.edit_now(
                    &note.id,
                    NotePatch {
                        status: Some("completed".into()),
                        ..NotePatch::default()
                    },
                    cx,
                )
            });
            self.close_balloon(&note.id, cx);
        }
        self.completion_task_ids.clear();
        cx.notify();
    }

    /// The task made from the note: linked to it, saved now.
    pub fn link_task(&mut self, note_id: &str, task_id: String, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.edit_now(
                note_id,
                NotePatch {
                    implementation_task_id: Some(Some(task_id)),
                    ..NotePatch::default()
                },
                cx,
            )
        });
        Toaster::success(cx.t("notes.taskDraftReady"), cx);
    }

    pub(super) fn completion_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let notes = self.completion_notes(cx);
        if notes.is_empty() {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let busy = self.busy;
        let close = cx.entity().downgrade();
        Some(
            Dialog::new("notes-completion", move |_, cx| {
                let _ = close.update(cx, |workspace, cx| workspace.keep_open(cx));
            })
            .child(
                div()
                    .debug_selector(|| "notes-completion".into())
                    .child(dialog_header(
                        cx.t("notes.completeLinkedTitle"),
                        Some(cx.t("notes.completeLinkedDescription")),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .children(notes.iter().map(|note| {
                        div()
                            .rounded(px(8.0))
                            .bg(colors.muted.opacity(0.4))
                            .px(px(12.0))
                            .py(px(8.0))
                            .child(if note.title.is_empty() {
                                cx.t("notes.untitled").to_string()
                            } else {
                                note.title.clone()
                            })
                    })),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        Button::new("notes-keep-open")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Default)
                            .disabled(busy)
                            .on_click(cx.listener(|workspace, _, _, cx| workspace.keep_open(cx)))
                            .child(cx.t("notes.keepOpen")),
                    )
                    .child(
                        Button::new("notes-complete-linked")
                            .size(ButtonSize::Default)
                            .disabled(busy)
                            .on_click(
                                cx.listener(|workspace, _, _, cx| workspace.complete_linked(cx)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(Icon::new("check").size(14.0))
                                    .child(cx.t("notes.complete")),
                            ),
                    ),
            )
            .into_any_element(),
        )
    }
}
