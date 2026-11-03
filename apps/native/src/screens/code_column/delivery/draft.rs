//! The ✨ beside a field of the commit preview and of "Criar PR": the task's
//! agent drafts that one field when the person asks. While it writes the
//! sparkles turn into a spinner that stops it; a failure leaves the field as
//! it was. One field at a time.

use gpui::{AnyElement, Context, SharedString, Task, div, prelude::*};

use super::super::CodeColumn;
use super::dialogs::label;
use super::dock::failure_reason;
use crate::i18n::Translate;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Toaster, Tooltip, spinner};

/// A field the ✨ can fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DraftField {
    CommitSubject,
    CommitBody,
    RequestTitle,
    RequestBody,
}

impl DraftField {
    /// The field's name in the desktop's `delivery/draft` action.
    pub(crate) fn api_name(self) -> &'static str {
        match self {
            Self::CommitSubject => "commitSubject",
            Self::CommitBody => "commitBody",
            Self::RequestTitle => "requestTitle",
            Self::RequestBody => "requestBody",
        }
    }

    /// The button's, its spinner's and its tooltip's ids.
    fn ids(self) -> [&'static str; 3] {
        match self {
            Self::CommitSubject => [
                "draft-commit-subject",
                "draft-commit-subject-spin",
                "draft-commit-subject-tip",
            ],
            Self::CommitBody => [
                "draft-commit-body",
                "draft-commit-body-spin",
                "draft-commit-body-tip",
            ],
            Self::RequestTitle => [
                "draft-request-title",
                "draft-request-title-spin",
                "draft-request-title-tip",
            ],
            Self::RequestBody => [
                "draft-request-body",
                "draft-request-body-spin",
                "draft-request-body-tip",
            ],
        }
    }
}

/// The ✨ that is writing.
pub(crate) struct Drafting {
    pub field: DraftField,
    _task: Task<()>,
}

impl CodeColumn {
    /// The dialog's field as the person has it now.
    fn draft_text(&self, field: DraftField, cx: &gpui::App) -> Option<String> {
        let preview = self.delivery.preview.as_ref();
        let request = self.delivery.request.as_ref();
        Some(match field {
            DraftField::CommitSubject => preview?.subject.read(cx).text().to_string(),
            DraftField::CommitBody => preview?.body.read(cx).text().to_string(),
            DraftField::RequestTitle => request?.title.read(cx).text().to_string(),
            DraftField::RequestBody => request?.body.read(cx).text().to_string(),
        })
    }

    /// The other field the draft must agree with.
    fn draft_companion(&self, field: DraftField, cx: &gpui::App) -> Option<String> {
        match field {
            DraftField::CommitBody => self.draft_text(DraftField::CommitSubject, cx),
            DraftField::RequestBody => self.draft_text(DraftField::RequestTitle, cx),
            DraftField::CommitSubject | DraftField::RequestTitle => None,
        }
    }

    /// The ✨ clicked: writes the field, or stops the one writing it.
    pub(crate) fn toggle_draft(&mut self, field: DraftField, cx: &mut Context<Self>) {
        if self.delivery.drafting.is_some() {
            if self.drafting(field) {
                self.delivery.drafting = None;
                cx.notify();
            }
            return;
        }
        let (Some(id), Some(_)) = (self.task_id(), self.draft_text(field, cx)) else {
            return;
        };
        let companion = self.draft_companion(field, cx);
        let api = self.api();
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api?.draft(&id, field.api_name(), companion.as_deref()) })
                .await;
            let _ = this.update(cx, |column, cx| {
                if !column.drafting(field) {
                    return;
                }
                column.delivery.drafting = None;
                match result {
                    Ok(text) => column.fill_draft(field, &text, cx),
                    Err(error) => {
                        let reason = failure_reason(error, cx);
                        Toaster::error_with(cx.t("commit.draft.failed"), reason, cx);
                    }
                }
                cx.notify();
            });
        });
        self.delivery.drafting = Some(Drafting { field, _task: task });
        cx.notify();
    }

    fn drafting(&self, field: DraftField) -> bool {
        self.delivery
            .drafting
            .as_ref()
            .is_some_and(|drafting| drafting.field == field)
    }

    /// The draft into its field, when the dialog is still open.
    fn fill_draft(&mut self, field: DraftField, text: &str, cx: &mut Context<Self>) {
        let preview = self.delivery.preview.as_ref();
        let request = self.delivery.request.as_ref();
        if field == DraftField::RequestTitle {
            if let Some(title) = request.map(|request| request.title.clone()) {
                let text = text.to_string();
                title.update(cx, |input, cx| input.set_text(text, cx));
            }
            return;
        }
        let area = match field {
            DraftField::CommitSubject => preview.map(|preview| preview.subject.clone()),
            DraftField::CommitBody => preview.map(|preview| preview.body.clone()),
            _ => request.map(|request| request.body.clone()),
        };
        if let Some(area) = area {
            area.update(cx, |area, cx| area.set_text(text, cx));
        }
    }
}

/// A field's label with its ✨ at the right end.
pub(super) fn draft_label(
    text: SharedString,
    field: DraftField,
    column: &CodeColumn,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let writing = column.drafting(field);
    let other = column.delivery.drafting.is_some() && !writing;
    let blocked = column.delivery.busy
        || column
            .delivery
            .request
            .as_ref()
            .is_some_and(|request| request.sending);
    let tip = if writing {
        cx.t("commit.draft.stop")
    } else {
        cx.t("commit.draft.write")
    };
    let [id, spin, tip_id] = field.ids();
    let button = Button::new(id)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconXs)
        .muted()
        .disabled(other || blocked)
        .child(if writing {
            spinner(spin, 14.0).into_any_element()
        } else {
            Icon::new("sparkles").size(14.0).into_any_element()
        })
        .on_click(cx.listener(move |column, _, _, cx| column.toggle_draft(field, cx)));
    div()
        .flex()
        .items_center()
        .justify_between()
        .child(label(text))
        .child(Tooltip::text(tip_id, button, tip))
        .into_any_element()
}
