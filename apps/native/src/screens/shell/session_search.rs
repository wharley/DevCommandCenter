//! "Sessões passadas" over the window (`handleSelectSessionSearchResult`):
//! the picked conversation's task opens on it, restored first when it was
//! archived.

use dcc_core::domain::session::SessionSearchResult;
use gpui::{AppContext, Context, Entity, Subscription, Window};

use super::{Shell, Surface};
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::screens::session_search::{SessionSearch, SessionSearchEvent};
use crate::ui::Toaster;

pub(super) type OpenSearch = (Entity<SessionSearch>, Subscription);

impl Shell {
    pub(super) fn open_session_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let task = self
            .conversation
            .read(cx)
            .data()
            .map(|data| data.task_id.clone());
        let db_path = crate::data::loader::database_path();
        let search = cx.new(|cx| SessionSearch::new(task, db_path, window, cx));
        let events = cx.subscribe_in(&search, window, |shell, _, event, _, cx| match event {
            SessionSearchEvent::Closed => {
                shell.session_search = None;
                cx.notify();
            }
            SessionSearchEvent::Picked(result) => {
                shell.open_search_result(result.as_ref().clone(), cx)
            }
        });
        self.session_search = Some((search, events));
        cx.notify();
    }

    fn open_search_result(&mut self, result: SessionSearchResult, cx: &mut Context<Self>) {
        self.session_search = None;
        cx.notify();
        let archived = result.archived_at.is_some();
        self.open_session(result.workspace_id.0, result.session_id.0, archived, cx);
    }

    /// `handleSelectSessionSearchResult`: the conversation's task opens on
    /// it, restored first when it was archived.
    pub(super) fn open_session(
        &mut self,
        task: String,
        session: String,
        archived: bool,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            if archived {
                let id = session.clone();
                let restored = cx
                    .background_executor()
                    .spawn(async move {
                        DesktopApi::load()
                            .ok_or(ApiError::DesktopClosed)?
                            .restore_session(&id)
                    })
                    .await;
                if let Err(error) = restored {
                    let _ = this.update(cx, |_, cx| Toaster::error(error.to_string(), cx));
                    return;
                }
            }
            let _ = this.update(cx, |shell, cx| {
                shell
                    .sidebar
                    .update(cx, |sidebar, cx| sidebar.reveal_task(&task, cx));
                shell.conversation.update(cx, |conversation, cx| {
                    conversation.open(&task, Some(session), cx)
                });
                shell.show(Surface::Conversation, cx);
            });
        })
        .detach();
    }
}
