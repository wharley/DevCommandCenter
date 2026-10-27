//! "AI Memory": two tabs, the configuration (the sidecar's status, its
//! mode, server, token and namespaces, saved as one, and its folders) and
//! the export history (the queue with "Reprocessar", then what was
//! exported).

mod config;
mod history;

use gpui::{AnyElement, AppContext, Context, Entity, Subscription, Window, div, prelude::*, px};

use super::Settings;
use crate::data::ai_memory_settings::{
    Draft, ExportEntry, OutboxEntry, Settings as Saved, SidecarStatus,
};
use crate::data::desktop_api::ApiError;
use crate::i18n::Translate;
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::{Tabs, Toaster};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    BaseUrl,
    Token,
    Workspace,
    Project,
    DataDir,
}

/// What the status card knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum StatusState {
    Loading,
    Failed,
    Ready(SidecarStatus),
}

pub(super) struct Memory {
    /// 0: Configuração, 1: Histórico de exportações.
    pub(super) tab: usize,
    pub(super) status: StatusState,
    pub(super) status_loading: bool,
    pub(super) saved: Option<Saved>,
    pub(super) draft: Draft,
    pub(super) saving: bool,
    pub(super) restart_required: bool,
    pub(super) base_url: Entity<TextInput>,
    pub(super) token: Entity<TextInput>,
    pub(super) workspace: Entity<TextInput>,
    pub(super) project: Entity<TextInput>,
    pub(super) data_dir: Entity<TextInput>,
    /// "Escopo e armazenamento avançados" open.
    pub(super) advanced_open: bool,
    pub(super) outbox: Option<Vec<OutboxEntry>>,
    pub(super) exports: Option<Vec<ExportEntry>>,
    pub(super) history_loading: bool,
    pub(super) retrying: Option<String>,
    _inputs: Vec<Subscription>,
}

impl Memory {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Settings>) -> Self {
        let draft = Draft::default();
        let t = |key: &str, cx: &Context<Settings>| Translate::t(&**cx, key);
        let make = |text: String,
                    placeholder: gpui::SharedString,
                    window: &mut Window,
                    cx: &mut Context<Settings>| {
            let input = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
            input.update(cx, |input, cx| input.set_text(text, cx));
            input
        };
        let base_url = make(String::new(), "http://127.0.0.1:49374".into(), window, cx);
        let token_placeholder = t("settings.aiMemory.tokenPlaceholder", cx);
        let token = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder(token_placeholder)
                .masked()
        });
        let workspace = make(draft.workspace.clone(), "".into(), window, cx);
        let project = make(draft.project.clone(), "".into(), window, cx);
        let data_dir = make(
            String::new(),
            t("settings.aiMemory.dataDirPlaceholder", cx),
            window,
            cx,
        );
        let inputs = [
            (base_url.clone(), Field::BaseUrl),
            (token.clone(), Field::Token),
            (workspace.clone(), Field::Workspace),
            (project.clone(), Field::Project),
            (data_dir.clone(), Field::DataDir),
        ]
        .into_iter()
        .map(|(input, field)| {
            cx.subscribe(&input, move |settings, input, event, cx| {
                if matches!(event, TextInputEvent::Changed) {
                    let text = input.read(cx).text().to_string();
                    settings.memory_typed(field, text, cx);
                }
            })
        })
        .collect();
        Self {
            tab: 0,
            status: StatusState::Loading,
            status_loading: false,
            saved: None,
            draft,
            saving: false,
            restart_required: false,
            base_url,
            token,
            workspace,
            project,
            data_dir,
            advanced_open: false,
            outbox: None,
            exports: None,
            history_loading: false,
            retrying: None,
            _inputs: inputs,
        }
    }
}

fn message(error: ApiError, fallback: &str, cx: &gpui::App) -> String {
    match error {
        ApiError::DesktopClosed => cx.t("native.desktopClosed").to_string(),
        ApiError::Failed(text) if !text.is_empty() => text,
        _ => cx.t(fallback).to_string(),
    }
}

impl Settings {
    /// The status and the settings, read when Settings opens.
    pub(super) fn load_memory(&mut self, cx: &mut Context<Self>) {
        self.load_memory_status(cx);
        let Some(api) = self.general.api() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move { api.ai_memory_settings() })
                .await;
            let _ = this.update(cx, |settings, cx| {
                if let Ok(saved) = saved {
                    settings.apply_memory(saved, cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn load_memory_status(&mut self, cx: &mut Context<Self>) {
        let api = self.general.api();
        self.memory.status_loading = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let status = cx
                .background_executor()
                .spawn(async move { api.map(|api| api.ai_memory_sidecar()) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                let memory = &mut settings.memory;
                memory.status_loading = false;
                memory.status = match status {
                    Some(Ok(status)) => StatusState::Ready(status),
                    _ => StatusState::Failed,
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_memory(&mut self, saved: Saved, cx: &mut Context<Self>) {
        let memory = &mut self.memory;
        memory.draft = Draft::from(&saved);
        let placeholder = cx.t(if saved.token_configured {
            "settings.aiMemory.tokenConfigured"
        } else {
            "settings.aiMemory.tokenPlaceholder"
        });
        memory.token.update(cx, |input, cx| {
            input.set_text("", cx);
            input.set_placeholder(placeholder, cx);
        });
        let draft = memory.draft.clone();
        for (input, text) in [
            (&memory.base_url, draft.base_url.unwrap_or_default()),
            (&memory.workspace, draft.workspace),
            (&memory.project, draft.project),
            (&memory.data_dir, draft.data_dir.unwrap_or_default()),
        ] {
            input.update(cx, |input, cx| input.set_text(text, cx));
        }
        memory.saved = Some(saved);
        cx.notify();
    }

    fn memory_typed(&mut self, field: Field, text: String, cx: &mut Context<Self>) {
        let draft = &mut self.memory.draft;
        let value = (!text.is_empty()).then(|| text.clone());
        match field {
            Field::BaseUrl => draft.base_url = value,
            // `token: event.target.value` (empty keeps the saved one).
            Field::Token => draft.token = Some(text),
            Field::Workspace => draft.workspace = text,
            Field::Project => draft.project = text,
            Field::DataDir => draft.data_dir = value,
        }
        cx.notify();
    }

    pub(super) fn set_memory_mode(&mut self, mode: &str, cx: &mut Context<Self>) {
        self.memory.draft.mode = mode.to_string();
        cx.notify();
    }

    pub(super) fn save_memory(&mut self, cx: &mut Context<Self>) {
        let api = self.general.api();
        let draft = self.memory.draft.clone();
        self.memory.saving = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move { api.ok_or(ApiError::DesktopClosed)?.save_ai_memory(&draft) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.memory.saving = false;
                match saved {
                    Ok(saved) => {
                        let restart = saved.restart_required;
                        settings.memory.restart_required = restart;
                        Toaster::success(
                            cx.t(if restart {
                                "settings.aiMemory.savedRestart"
                            } else {
                                "settings.aiMemory.saved"
                            }),
                            cx,
                        );
                        settings.apply_memory(saved, cx);
                        settings.load_memory_status(cx);
                    }
                    Err(error) => {
                        Toaster::error(message(error, "settings.aiMemory.saveError", cx), cx)
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn restart_desktop(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        cx.background_executor()
            .spawn(async move {
                let _ = api.restart_desktop();
            })
            .detach();
    }

    pub(super) fn select_memory_tab(&mut self, tab: usize, cx: &mut Context<Self>) {
        self.memory.tab = tab;
        if tab == 1 && self.memory.outbox.is_none() {
            self.load_memory_history(cx);
        }
        cx.notify();
    }

    /// The queue and the exports, together (the "Atualizar" of the tab).
    pub(super) fn load_memory_history(&mut self, cx: &mut Context<Self>) {
        let api = self.general.api();
        self.memory.history_loading = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    api.map(|api| (api.ai_memory_outbox().ok(), api.ai_memory_exports().ok()))
                })
                .await;
            let _ = this.update(cx, |settings, cx| {
                let memory = &mut settings.memory;
                memory.history_loading = false;
                let (outbox, exports) = read.unwrap_or_default();
                memory.outbox = Some(outbox.unwrap_or_default());
                memory.exports = Some(exports.unwrap_or_default());
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn retry_memory(&mut self, session_id: String, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        self.memory.retrying = Some(session_id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let retried = cx
                .background_executor()
                .spawn(async move { api.retry_ai_memory(&session_id) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.memory.retrying = None;
                match retried {
                    Ok(()) => {
                        Toaster::success(cx.t("settings.aiMemory.retryQueued"), cx);
                        settings.load_memory_history(cx);
                    }
                    Err(error) => {
                        Toaster::error(message(error, "settings.aiMemory.retryError", cx), cx)
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn memory_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let tabs = Tabs::new(
            "settings-memory-tab",
            vec![
                cx.t("settings.aiMemory.tabs.configuration"),
                cx.t("settings.aiMemory.tabs.history"),
            ],
            self.memory.tab,
        )
        .on_select({
            let this = cx.entity().downgrade();
            move |tab, _, cx| {
                let _ = this.update(cx, |settings, cx| settings.select_memory_tab(tab, cx));
            }
        });
        let content = if self.memory.tab == 0 {
            self.memory_configuration(cx)
        } else {
            self.memory_history(cx)
        };
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(div().w_full().child(tabs))
            .child(content)
            .into_any_element()
    }
}
