//! What the header's buttons do, through the desktop (`handleResumeSession`,
//! `handleStartSession`, `handleCloseSession`/`performCloseSession`,
//! `handleRestoreSession`).

use gpui::Context;

use super::PendingClose;
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::data::session_list::next_after_close;
use crate::i18n::Translate;
use crate::screens::conversation::ConversationView;
use crate::ui::Toaster;

fn failed(error: ApiError, cx: &mut gpui::App) {
    match error {
        ApiError::DesktopClosed => Toaster::error(cx.t("native.desktopClosed"), cx),
        error => Toaster::error(error.to_string(), cx),
    }
}

impl ConversationView {
    fn header_api(&self, cx: &gpui::App) -> Option<DesktopApi> {
        self.composer
            .read(cx)
            .endpoint
            .clone()
            .or_else(DesktopApi::load)
    }

    /// The conversation on screen.
    pub(crate) fn shown_row(&self) -> Option<&crate::data::session_list::SessionRow> {
        let data = self.data()?;
        let id = data.session_id.as_deref()?;
        data.sessions.iter().find(|row| row.id == id)
    }

    /// "Retomar sessão": only a session that is not already active.
    pub(crate) fn resume_session(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.shown_row().filter(|row| row.state != "active") else {
            return;
        };
        let id = row.id.clone();
        let api = self.header_api(cx);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api.ok_or(ApiError::DesktopClosed)?.resume(&id) })
                .await;
            let _ = this.update(cx, |view, cx| match result {
                Ok(()) => view.refresh_now(cx),
                Err(error) => failed(error, cx),
            });
        })
        .detach();
    }

    /// "Nova sessão": on the composer's pick, titled after the task.
    pub(crate) fn start_session(&mut self, cx: &mut Context<Self>) {
        if self.header.starting {
            return;
        }
        let Some(data) = self.data() else {
            return;
        };
        let Some(start) = data.new_thread.clone() else {
            return;
        };
        let title = data.workspace_name.clone().or_else(|| start.name.clone());
        let composer = self.composer.read(cx);
        let Some(provider) = composer.selected_provider_id() else {
            Toaster::error(cx.t("native.sendFailed"), cx);
            return;
        };
        let model = composer.selected_model_id();
        let api = self.header_api(cx);
        self.header.starting = true;
        self.header.history_open = false;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let started = cx
                .background_executor()
                .spawn(async move {
                    api.ok_or(ApiError::DesktopClosed)?.start_thread(
                        &start,
                        &provider,
                        model.as_deref(),
                        title.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.header.starting = false;
                match started {
                    Ok(session) => view.open_session(session, cx),
                    Err(error) => failed(error, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn select_session(&mut self, session: String, cx: &mut Context<Self>) {
        self.header.history_open = false;
        self.open_session(session, cx);
        cx.notify();
    }

    /// The history's ×: a running turn asks first.
    pub(crate) fn request_close_session(&mut self, session: &str, cx: &mut Context<Self>) {
        if self.header.busy_session.is_some() {
            return;
        }
        let Some(row) = self
            .data()
            .and_then(|data| data.sessions.iter().find(|row| row.id == session))
            .filter(|row| !row.archived)
        else {
            return;
        };
        let request = PendingClose {
            session_id: row.id.clone(),
            title: row.title.clone(),
            delete_history: row.turn_count == 0,
            requires_abort: row.running,
        };
        self.header.history_open = false;
        if request.requires_abort {
            self.header.pending_close = Some(request);
            cx.notify();
            return;
        }
        self.close_session(request, cx);
    }

    pub(crate) fn confirm_close_session(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.header.pending_close.clone() {
            self.close_session(request, cx);
        }
    }

    pub(crate) fn cancel_close_session(&mut self, cx: &mut Context<Self>) {
        if self.header.busy_session.is_none() {
            self.header.pending_close = None;
            cx.notify();
        }
    }

    /// `performCloseSession`: the turn stops first when one runs; closing
    /// the conversation on screen moves to the next one, or starts one.
    fn close_session(&mut self, request: PendingClose, cx: &mut Context<Self>) {
        let Some(data) = self.data() else {
            return;
        };
        let shown = data.session_id.as_deref() == Some(request.session_id.as_str());
        let next = next_after_close(&data.sessions, &request.session_id).map(str::to_string);
        let api = self.header_api(cx);
        self.header.busy_session = Some(request.session_id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let id = request.session_id.clone();
            let closed = cx
                .background_executor()
                .spawn(async move {
                    let api = api.ok_or(ApiError::DesktopClosed)?;
                    if request.requires_abort {
                        api.abort(&id)?;
                    }
                    api.close_session(&id, request.delete_history)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.header.busy_session = None;
                view.header.pending_close = None;
                match closed {
                    Ok(_) if shown => match next {
                        Some(next) => view.open_session(next, cx),
                        None => {
                            view.refresh_now(cx);
                            view.start_session(cx);
                        }
                    },
                    Ok(_) => view.refresh_now(cx),
                    Err(error) => failed(error, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// An archived conversation back, and on screen.
    pub(crate) fn restore_session(&mut self, session: String, cx: &mut Context<Self>) {
        if self.header.busy_session.is_some() {
            return;
        }
        let api = self.header_api(cx);
        self.header.busy_session = Some(session.clone());
        self.header.history_open = false;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let id = session.clone();
            let restored = cx
                .background_executor()
                .spawn(async move { api.ok_or(ApiError::DesktopClosed)?.restore_session(&id) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.header.busy_session = None;
                match restored {
                    Ok(()) => view.open_session(session, cx),
                    Err(error) => failed(error, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
