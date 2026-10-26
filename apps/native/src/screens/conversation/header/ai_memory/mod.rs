//! ai-memory in the header, after the delivery: the export indicator
//! (`Cloud`, `CloudOff` amber after a failed try, turning while it is read)
//! and `AiMemorySourcesMenu` — what the agent received from the project's
//! memory on its latest turn, pinned, corrected or ignored by the person
//! for the next one. Read for the conversation on screen, again every 30s.

mod source;
mod view;

use std::collections::HashMap;
use std::time::Duration;

use gpui::{AppContext, Context, Entity, Task, Window};

use crate::data::ai_memory::{ExportStatus, SourceAction, SourceHit};
use crate::data::desktop_api::ApiError;
use crate::i18n::Translate;
use crate::screens::conversation::ConversationView;
use crate::ui::Toaster;
use crate::ui::text_area::{TextArea, TextAreaEvent};
use crate::ui::text_input::TextInput;

/// `refetchInterval: 30_000`
const EVERY: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct AiMemory {
    /// The conversation it reads for.
    pub session: Option<String>,
    /// The export that waits (`null` once synced).
    pub export: Option<ExportStatus>,
    /// The export is being read (`isFetching`).
    pub export_reading: bool,
    /// The latest turn's sources; `None` until first read.
    pub sources: Option<Vec<SourceHit>>,
    pub actions: HashMap<String, SourceAction>,
    /// The sidecar's mode (`managed`, `remote`, `shared`, `unavailable`, …).
    pub sidecar: Option<String>,
    pub open: bool,
    pub show_ignored: bool,
    pub search: Option<Entity<TextInput>>,
    /// The source whose correction is being written.
    pub editing: Option<(String, Entity<TextArea>)>,
    pub saving: Option<String>,
    _read: Option<Task<()>>,
    _editing: Option<gpui::Subscription>,
}

impl AiMemory {
    /// `sidecarActive`
    pub fn active(&self) -> bool {
        matches!(
            self.sidecar.as_deref(),
            Some("managed" | "remote" | "shared")
        )
    }

    pub fn unavailable(&self) -> bool {
        self.sidecar.as_deref() == Some("unavailable")
    }
}

impl ConversationView {
    /// Another conversation on screen: its ai-memory is read from scratch.
    pub(crate) fn follow_ai_memory(&mut self, session: Option<String>, cx: &mut Context<Self>) {
        if self.header.ai_memory.session == session {
            return;
        }
        self.header.ai_memory = AiMemory {
            session: session.clone(),
            ..AiMemory::default()
        };
        let (Some(session), Some(api)) = (session, self.header_api(cx)) else {
            return;
        };
        self.header.ai_memory._read = Some(cx.spawn(async move |this, cx| {
            let mut first = true;
            loop {
                let reading = this.update(cx, |view, cx| {
                    view.header.ai_memory.export_reading = true;
                    cx.notify();
                });
                if reading.is_err() {
                    return;
                }
                let api = api.clone();
                let id = session.clone();
                let read = cx
                    .background_executor()
                    .spawn(async move {
                        let export = api.ai_memory_export_status(&id).ok().flatten();
                        let sidecar = api.ai_memory_sidecar_mode().ok().flatten();
                        let sources = api.ai_memory_sources(&id).ok();
                        let actions = first.then(|| api.ai_memory_source_actions().ok()).flatten();
                        (export, sidecar, sources, actions)
                    })
                    .await;
                let (export, sidecar, sources, actions) = read;
                let alive = this.update(cx, |view, cx| {
                    let memory = &mut view.header.ai_memory;
                    if memory.session.as_deref() != Some(session.as_str()) {
                        return false;
                    }
                    memory.export_reading = false;
                    memory.export = export;
                    memory.sidecar = sidecar;
                    // A failed read keeps what was shown (an empty list once).
                    memory.sources = sources
                        .or_else(|| memory.sources.take())
                        .or(Some(Vec::new()));
                    if let Some(actions) = actions {
                        memory.actions = by_key(actions);
                    }
                    cx.notify();
                    true
                });
                if !matches!(alive, Ok(true)) {
                    return;
                }
                first = false;
                cx.background_executor().timer(EVERY).await;
            }
        }));
    }

    pub(super) fn set_ai_memory_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let memory = &mut self.header.ai_memory;
        memory.open = open;
        if open && memory.search.is_none() {
            let placeholder = cx.t("workbench.aiMemory.sourcesSearchPlaceholder");
            let input = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
            cx.observe(&input, |_, _, cx| cx.notify()).detach();
            self.header.ai_memory.search = Some(input);
        }
        if !open {
            self.header.ai_memory.editing = None;
            self.header.ai_memory._editing = None;
        }
        cx.notify();
    }

    pub(super) fn toggle_ignored(&mut self, cx: &mut Context<Self>) {
        let memory = &mut self.header.ai_memory;
        memory.show_ignored = !memory.show_ignored;
        cx.notify();
    }

    /// "Corrigir" / "Editar": the correction's field, with what it says.
    pub(super) fn edit_source(
        &mut self,
        key: String,
        current: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let placeholder = cx.t("workbench.aiMemory.correctPlaceholder");
        let area = cx.new(|cx| {
            let mut area = TextArea::new(window, cx).with_placeholder(placeholder);
            area.set_text(current.as_deref().unwrap_or_default(), cx);
            area
        });
        area.update(cx, |area, cx| {
            gpui::Focusable::focus_handle(area, cx).focus(window, cx)
        });
        let watched = key.clone();
        let events = cx.subscribe(
            &area,
            move |view, area, event: &TextAreaEvent, cx| match event {
                TextAreaEvent::Submitted => {
                    let text = area.read(cx).text().trim().to_string();
                    if !text.is_empty() {
                        view.save_source(watched.clone(), "corrected", Some(text), cx);
                    }
                }
                TextAreaEvent::Cancelled => view.cancel_edit(cx),
                TextAreaEvent::Changed => cx.notify(),
                _ => {}
            },
        );
        self.header.ai_memory.editing = Some((key, area));
        self.header.ai_memory._editing = Some(events);
        cx.notify();
    }

    pub(super) fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        self.header.ai_memory.editing = None;
        self.header.ai_memory._editing = None;
        cx.notify();
    }

    /// `save`: the action kept, then the curation read again.
    pub(super) fn save_source(
        &mut self,
        key: String,
        action: &'static str,
        correction: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.header.ai_memory.saving.is_some() {
            return;
        }
        let Some(api) = self.header_api(cx) else {
            return;
        };
        self.header.ai_memory.saving = Some(key.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move {
                    api.save_ai_memory_source_action(&key, action, correction.as_deref())?;
                    api.ai_memory_source_actions()
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                let memory = &mut view.header.ai_memory;
                memory.saving = None;
                match saved {
                    Ok(actions) => {
                        memory.actions = by_key(actions);
                        if action == "corrected" {
                            memory.editing = None;
                            memory._editing = None;
                        }
                    }
                    Err(error) => {
                        let message = match error {
                            ApiError::DesktopClosed => cx.t("workbench.aiMemory.actionError"),
                            error => error.to_string().into(),
                        };
                        Toaster::error(message, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn by_key(actions: Vec<SourceAction>) -> HashMap<String, SourceAction> {
    actions
        .into_iter()
        .map(|action| (action.source_key.clone(), action))
        .collect()
}
