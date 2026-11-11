//! Tucano's notices for the team rooms: a resident agent asking for an
//! approval, asking a question, finishing or stopping in its room
//! conversation is told while the person looks elsewhere. It rides on the
//! sidebar's own reads (`RoomWatch`), reads only a moved conversation's last
//! turn, off the UI thread, and delivers:
//! - DCC in the back: a system notification (approvals and questions bounce
//!   the Dock once); coming back to DCC within `RETURN_WINDOW`, without having
//!   moved elsewhere since, opens that room;
//! - DCC in front, the room not in sight: a toast with "Abrir sala";
//! - the room, the agent's "Conversa" or the conversation itself on screen:
//!   nothing.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dcc_core::domain::room_notice::{RoomNotice, notice_of};
use dcc_core::domain::session::SessionId;
use dcc_infra::db::SqliteSessionRepo;
use gpui::{AnyWindowHandle, App, Context, Window};

use super::{Shell, Surface};
use crate::data::agents::AgentState;
use crate::data::room_notices_pref::RoomNoticesPref;
use crate::data::room_watch::{Notice, Probe, RoomWatch};
use crate::i18n::{Args, Translate};
use crate::screens::agent_page::RoomOnScreen;
use crate::ui::Toaster;

/// Coming back to DCC this soon after a system notice opens its room.
const RETURN_WINDOW: Duration = Duration::from_secs(120);
/// How long the toast stays.
const TOAST_LIFETIME: Duration = Duration::from_secs(8);

/// Reads what a moved conversation holds. Blocking: called off the UI thread.
pub(super) type Reader = Arc<dyn Fn(&Probe) -> Option<RoomNotice> + Send + Sync>;

/// What the main area shows, to tell whether the person moved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct View {
    surface: Surface,
    room: Option<RoomOnScreen>,
    session: Option<String>,
}

/// The last notice posted to the system.
struct Away {
    room: Option<String>,
    at: Instant,
    view: View,
}

/// Whether coming back at `now` to `view` opens the room `away` told of.
fn opens_on_return(away_at: Instant, away_view: &View, now: Instant, view: &View) -> bool {
    now.duration_since(away_at) <= RETURN_WINDOW && away_view == view
}

pub(super) struct RoomNotices {
    watch: RoomWatch,
    reader: Reader,
    away: Option<Away>,
    window: AnyWindowHandle,
}

impl RoomNotices {
    pub(super) fn new(window: AnyWindowHandle) -> Self {
        let db = (!cfg!(test)).then(crate::data::loader::database_path);
        Self::with_reader(window, Arc::new(move |probe| read(db.as_ref()?, probe)))
    }

    pub(super) fn with_reader(window: AnyWindowHandle, reader: Reader) -> Self {
        Self {
            watch: RoomWatch::default(),
            reader,
            away: None,
            window,
        }
    }
}

/// The conversation's last turn, read only.
fn read(db: &PathBuf, probe: &Probe) -> Option<RoomNotice> {
    let sessions = SqliteSessionRepo::open_read_only(db).ok()?;
    let events = sessions
        .last_turn_events_sync(&SessionId(probe.session_id.clone()))
        .ok()?;
    notice_of(&events, &probe.session_id, probe.waiting)
}

impl Shell {
    /// A sidebar read: what moved in the rooms is read, and what was held
    /// back past its window goes out.
    pub(super) fn watch_rooms(&mut self, cx: &mut Context<Self>) {
        let muted = !RoomNoticesPref::enabled(cx)
            || cx
                .try_global::<crate::data::agent_triggers::AgentTriggers>()
                .is_some_and(|triggers| triggers.paused());
        let sidebar = self.sidebar.read(cx);
        let probes = self.room_notices.watch.observe(sidebar.agents(), muted);
        if self.room_notices.watch.holding() {
            let waiting = |session_id: &str| {
                sidebar.agents().iter().any(|agent| {
                    agent.sessions.iter().any(|session| {
                        session.session_id == session_id && session.state == AgentState::NeedsYou
                    })
                })
            };
            let due = self.room_notices.watch.flush(Instant::now(), waiting);
            for notice in due {
                self.deliver_room_notice(notice, cx);
            }
        }
        if probes.is_empty() {
            return;
        }
        let reader = self.room_notices.reader.clone();
        cx.spawn(async move |this, cx| {
            let found: Vec<Notice> = cx
                .background_executor()
                .spawn(async move {
                    probes
                        .into_iter()
                        .filter_map(|probe| {
                            let notice = reader(&probe)?;
                            Some(Notice { probe, notice })
                        })
                        .collect()
                })
                .await;
            let _ = this.update(cx, |shell, cx| {
                for notice in found {
                    if let Some(notice) = shell.room_notices.watch.offer(notice, Instant::now()) {
                        shell.deliver_room_notice(notice, cx);
                    }
                }
            });
        })
        .detach();
    }

    fn deliver_room_notice(&mut self, notice: Notice, cx: &mut Context<Self>) {
        let (title, body) = notice_text(&notice, self.room_name(&notice.probe.room, cx), cx);
        if !crate::data::foreground::is_foreground(cx) {
            if notice.notice.asks() {
                crate::ui::attention::request_attention(&title, &body);
            } else {
                crate::ui::attention::notify_away(&title, &body);
            }
            self.room_notices.away = Some(Away {
                room: notice.probe.room.clone(),
                at: Instant::now(),
                view: self.view(cx),
            });
            return;
        }
        if self.in_sight(&notice.probe, cx) {
            return;
        }
        let (shell, window) = (cx.entity().downgrade(), self.room_notices.window);
        let room = notice.probe.room.clone();
        Toaster::info_action(
            title,
            body,
            cx.t("agents.notices.openRoom"),
            TOAST_LIFETIME,
            move |cx: &mut App| {
                let room = room.clone();
                let shell = shell.clone();
                let _ = window.update(cx, |_, window, cx| {
                    let _ = shell.update(cx, |shell, cx| {
                        shell.open_room_from_notice(room, window, cx)
                    });
                });
            },
            cx,
        );
    }

    /// DCC came back to the front: the room of the last system notice
    /// opens, when it came recently and the person has not moved since.
    pub(super) fn room_notice_returned(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(away) = self.room_notices.away.take() else {
            return;
        };
        if opens_on_return(away.at, &away.view, Instant::now(), &self.view(cx)) {
            self.open_room_from_notice(away.room, window, cx);
        }
    }

    /// The team's screen on `room` (none: Geral).
    fn open_room_from_notice(
        &mut self,
        room: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_agents(window, cx);
        if let Some((page, _)) = &self.agent_page {
            page.update(cx, |page, cx| page.open_room_of(room, cx));
        }
    }

    fn view(&self, cx: &App) -> View {
        View {
            surface: self.surface,
            room: (self.surface == Surface::Agent)
                .then_some(self.agent_page.as_ref())
                .flatten()
                .and_then(|(page, _)| page.read(cx).room_on_screen(cx)),
            session: (self.surface == Surface::Conversation)
                .then(|| self.conversation.read(cx).shown_session())
                .flatten(),
        }
    }

    /// Whether the person already looks at that agent's room conversation.
    fn in_sight(&self, probe: &Probe, cx: &App) -> bool {
        let view = self.view(cx);
        view.session.as_deref() == Some(probe.session_id.as_str())
            || view
                .room
                .is_some_and(|room| room.shows(&probe.agent_id, probe.room.as_deref()))
    }

    fn room_name(&self, room: &Option<String>, cx: &App) -> Option<String> {
        let id = room.as_deref()?;
        Some(
            self.sidebar
                .read(cx)
                .projects()
                .iter()
                .find(|project| project.project_id == id)
                .map(|project| project.display_name.clone())
                .unwrap_or_else(|| id.to_string()),
        )
    }

    #[cfg(test)]
    pub(super) fn set_room_reader(&mut self, reader: Reader) {
        self.room_notices = RoomNotices::with_reader(self.room_notices.window, reader);
    }
}

/// The notice's title and body, in the person's language.
fn notice_text(notice: &Notice, room_name: Option<String>, cx: &App) -> (String, String) {
    let agent = Args::default().with("agent", notice.probe.agent_name.clone());
    let place = match room_name {
        Some(room) => cx.t_with("agents.notices.inRoom", &Args::default().with("room", room)),
        None => cx.t("agents.notices.inGeral"),
    }
    .to_string();
    match &notice.notice {
        RoomNotice::Approval { tool, detail, .. } => (
            cx.t_with("agents.notices.approval", &agent).to_string(),
            match detail {
                Some(detail) => format!("{tool} · {detail}"),
                None => tool.clone(),
            },
        ),
        RoomNotice::Question { text, .. } => (
            cx.t_with("agents.notices.question", &agent).to_string(),
            text.clone(),
        ),
        RoomNotice::Done { first_line, .. } => (
            cx.t_with("agents.notices.done", &agent.with("where", place))
                .to_string(),
            first_line.clone(),
        ),
        RoomNotice::Stopped { reason, .. } => (
            cx.t_with("agents.notices.stopped", &agent).to_string(),
            match reason {
                Some(reason) => format!("{place} · {reason}"),
                None => place,
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coming_back_soon_to_the_same_screen_opens_the_room() {
        let view = View {
            surface: Surface::Conversation,
            room: None,
            session: Some("task".into()),
        };
        let at = Instant::now();
        assert!(opens_on_return(
            at,
            &view,
            at + Duration::from_secs(60),
            &view
        ));
        assert!(!opens_on_return(
            at,
            &view,
            at + RETURN_WINDOW + Duration::from_secs(1),
            &view
        ));
        let moved = View {
            surface: Surface::PullRequests,
            ..view.clone()
        };
        assert!(!opens_on_return(at, &view, at, &moved));
    }
}
