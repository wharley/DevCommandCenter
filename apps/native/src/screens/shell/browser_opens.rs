//! The agents' Browser opens, once allowed (`handleBrowserOpenApproved`):
//! the shell goes to the task's conversation and, once it shows, opens its
//! Browser on the request's page; when the desktop knows that Browser, its
//! lifecycle token confirms the open (`handleBrowserOpened`). One waiting
//! open per conversation: a newer one replaces it (`enqueueBrowserOpen`).

use std::rc::Rc;

use gpui::{AppContext, Context, Entity, Subscription, Window};

use super::{Shell, Surface};
use crate::data::browser_opens::OpenRequest;
use crate::screens::browser::approval::{ApprovalEvent, BrowserApprovals, TitleOf};
use crate::screens::conversation::ConversationView;
use crate::screens::sidebar::Sidebar;

/// The card, and the shell following what it allows and what shows.
pub(super) fn new_approvals(
    sidebar: &Entity<Sidebar>,
    conversation: &Entity<ConversationView>,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> (Entity<BrowserApprovals>, [Subscription; 2]) {
    let (titles, names) = (conversation.downgrade(), sidebar.downgrade());
    // The conversation's own title when its task is open, else the task's.
    let title_of: TitleOf = Rc::new(move |request, cx| {
        let open = titles.upgrade().and_then(|view| {
            let data = view.read(cx).data()?;
            (data.task_id == request.workspace_id).then(|| {
                crate::data::delegation_lineage::session_title(
                    &data.sessions,
                    Some(&request.session_id),
                )
                .map(str::to_string)
            })?
        });
        open.or_else(|| names.upgrade()?.read(cx).task_name(&request.workspace_id))
    });
    let api = if cfg!(test) {
        None
    } else {
        crate::data::desktop_api::DesktopApi::load()
    };
    let approvals = cx.new(|cx| BrowserApprovals::new(api, title_of, cx));
    let events = cx.subscribe_in(&approvals, window, |shell, _, event, window, cx| {
        let ApprovalEvent::Open(request) = event;
        shell.open_approved(request.clone(), window, cx);
    });
    let shown = cx.observe_in(conversation, window, |shell, conversation, window, cx| {
        let session = conversation.read(cx).shown_session();
        shell.browser_approvals.update(cx, |approvals, cx| {
            approvals.set_visible_session(session, cx)
        });
        shell.open_waiting_browser(window, cx);
    });
    (approvals, [events, shown])
}

impl Shell {
    /// To the task's conversation; its Browser opens once it shows.
    pub(super) fn open_approved(
        &mut self,
        request: OpenRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let waiting = &mut self.browser.waiting;
        waiting.retain(|open| {
            (open.workspace_id.as_str(), open.session_id.as_str())
                != (request.workspace_id.as_str(), request.session_id.as_str())
        });
        waiting.push(request.clone());
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.reveal_task(&request.workspace_id, cx)
        });
        let (task, session) = (request.workspace_id, request.session_id);
        self.conversation.update(cx, |conversation, cx| {
            conversation.open(&task, Some(session), cx)
        });
        self.show(Surface::Conversation, cx);
        self.open_waiting_browser(window, cx);
    }

    /// The conversation on screen has an allowed open waiting: its Browser.
    fn open_waiting_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.browser.waiting.is_empty() {
            return;
        }
        let (task, session) = {
            let view = self.conversation.read(cx);
            (
                view.data().map(|data| data.task_id.clone()),
                view.shown_session(),
            )
        };
        let (Some(task), Some(session)) = (task, session) else {
            return;
        };
        let Some(index) = self
            .browser
            .waiting
            .iter()
            .position(|open| open.workspace_id == task && open.session_id == session)
        else {
            return;
        };
        let request = self.browser.waiting.remove(index);
        self.open_browser(Some(request.url.clone()), window, cx);
        if let Some(open) = self.browser.open.as_mut() {
            open.approval = Some(request);
        }
    }

    /// The desktop knows the Browser: an open it was for is confirmed.
    pub(super) fn browser_ready(&mut self, token: u64, cx: &mut Context<Self>) {
        let Some(request) = self
            .browser
            .open
            .as_mut()
            .and_then(|open| open.approval.take())
        else {
            return;
        };
        self.browser_approvals
            .update(cx, |approvals, cx| approvals.confirm(request, token, cx));
    }
}
