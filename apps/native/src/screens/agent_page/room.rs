//! The team room (Sala): one conversation with the residents, at the top of
//! the team's column. The person writes once; the rule in
//! `dcc_core::domain::team_room` picks who answers (`@`, the role, who
//! spoke last, the Reviewer) and the message goes to that agent's own
//! conversation, started the first time, where it answers with its memory
//! and tools. The room reads every agent's conversation back (only what is
//! new, when the sidebar sees it change) and shows them together, each
//! answer with its receipt. The whole conversation, with every tool, opens
//! from there.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use dcc_core::domain::agent_conversation::home_project_id;
use dcc_core::domain::team_room::{RouteReason, Teammate, route, talks};
use gpui::{AppContext, Context, Entity, EventEmitter, ScrollHandle, Subscription, Window};

use super::idea::agent_execution;
use crate::data::agent_conversation::ConversationStart;
use crate::data::agents::{AgentRecord, AgentSession, AgentState};
use crate::data::desktop_api::{DesktopApi, TurnOptions};
use crate::data::team_room::{HomeFeed, RoomLine, Speaker, last_answer, merged};
use crate::i18n::{Args, Translate};
use crate::markdown::Block;
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::ui::{TextArea, TextAreaEvent, Toaster};

pub enum RoomEvent {
    /// An agent's whole conversation, with its tools.
    OpenSession { task_id: String, session_id: String },
    /// A pull request review's code tab.
    OpenPullRequest(String),
}

/// A message on its way: shown until the agent's conversation has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    pub agent_id: String,
    pub text: String,
    pub at: String,
}

pub struct TeamRoom {
    pub(super) sidebar: Entity<Sidebar>,
    composer: Entity<Composer>,
    pub(crate) api: Option<DesktopApi>,
    pub(crate) db_path: PathBuf,
    pub(crate) feeds: Vec<HomeFeed>,
    reading: bool,
    pub(crate) input: Entity<TextArea>,
    pub(crate) pending: Vec<Pending>,
    /// Why a message went to its agent, by agent and text (this run only).
    pub(crate) reasons: HashMap<(String, String), RouteReason>,
    pub(crate) sending: bool,
    pub(super) scroll: ScrollHandle,
    /// How many lines it showed, to follow new ones to the bottom.
    pub(super) shown: usize,
    pub(super) markdown: std::cell::RefCell<HashMap<String, Rc<Vec<Block>>>>,
    _subscriptions: [Subscription; 2],
}

impl EventEmitter<RoomEvent> for TeamRoom {}

/// Every conversation an agent has in its home.
fn home_sessions(agent: &AgentRecord) -> impl Iterator<Item = &AgentSession> {
    let home = home_project_id(&agent.id);
    agent
        .sessions
        .iter()
        .filter(move |session| session.project_id == home)
}

impl TeamRoom {
    pub fn new(
        sidebar: Entity<Sidebar>,
        composer: Entity<Composer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx
            .new(|cx| TextArea::new(window, cx).with_placeholder(cx.t("agents.room.placeholder")));
        let subscriptions = [
            // Who works and the inbox's cards follow the sidebar as it reads.
            cx.observe(&sidebar, |room, _, cx| {
                room.sync(cx);
                cx.notify();
            }),
            cx.subscribe(&input, |room, _, event, cx| match event {
                TextAreaEvent::Submitted => room.send(cx),
                TextAreaEvent::Changed => cx.notify(),
                _ => {}
            }),
        ];
        let mut room = Self {
            sidebar,
            composer,
            api: if cfg!(test) { None } else { DesktopApi::load() },
            // Tests feed the room themselves, never DCC's own database.
            db_path: if cfg!(test) {
                PathBuf::new()
            } else {
                crate::data::loader::database_path()
            },
            feeds: Vec::new(),
            reading: false,
            input,
            pending: Vec::new(),
            reasons: HashMap::new(),
            sending: false,
            scroll: ScrollHandle::new(),
            shown: 0,
            markdown: Default::default(),
            _subscriptions: subscriptions,
        };
        room.sync(cx);
        room
    }

    /// Reads the conversations that changed since they were read.
    pub(crate) fn sync(&mut self, cx: &mut Context<Self>) {
        if self.reading {
            return;
        }
        let wanted: Vec<(String, String, String)> = self
            .sidebar
            .read(cx)
            .agents()
            .iter()
            .filter(|agent| talks(agent.preset.as_deref()))
            .flat_map(|agent| {
                home_sessions(agent).map(|session| {
                    (
                        agent.id.clone(),
                        session.session_id.clone(),
                        session.updated_at.clone(),
                    )
                })
            })
            .collect();
        // Conversations gone (an agent deleted) leave the room.
        self.feeds.retain(|feed| {
            wanted
                .iter()
                .any(|(_, session, _)| *session == feed.session_id)
        });
        let stale: Vec<(HomeFeed, String)> = wanted
            .into_iter()
            .filter_map(|(agent, session, updated)| {
                match self.feeds.iter().find(|feed| feed.session_id == session) {
                    Some(feed) if feed.read_at == updated => None,
                    Some(feed) => Some((feed.clone(), updated)),
                    None => Some((HomeFeed::new(&agent, &session), updated)),
                }
            })
            .collect();
        if stale.is_empty() {
            return;
        }
        self.reading = true;
        let db = self.db_path.clone();
        cx.spawn(async move |this, cx| {
            let read: Vec<HomeFeed> = cx
                .background_executor()
                .spawn(async move {
                    // A conversation that cannot be read keeps what it had
                    // until it changes again.
                    stale
                        .into_iter()
                        .map(|(feed, updated)| {
                            let mut kept = feed.clone();
                            kept.read_at = updated.clone();
                            feed.refresh(&db, &updated).unwrap_or(kept)
                        })
                        .collect()
                })
                .await;
            let _ = this.update(cx, |room, cx| {
                room.reading = false;
                room.take(read);
                cx.notify();
                // It may have changed again meanwhile.
                room.sync(cx);
            });
        })
        .detach();
    }

    /// The conversations just read, in place of the old ones; a message on
    /// its way leaves once its conversation shows it.
    pub(crate) fn take(&mut self, read: Vec<HomeFeed>) {
        for feed in read {
            match self
                .feeds
                .iter_mut()
                .find(|old| old.session_id == feed.session_id)
            {
                Some(old) => *old = feed,
                None => self.feeds.push(feed),
            }
        }
        let feeds = &self.feeds;
        self.pending.retain(|pending| {
            !feeds.iter().flat_map(|feed| &feed.lines).any(|line| {
                line.speaker == Speaker::Person
                    && line.agent_id == pending.agent_id
                    && line.text == pending.text
                    && line.at >= pending.at
            })
        });
    }

    pub(crate) fn lines(&self) -> Vec<&RoomLine> {
        merged(&self.feeds)
    }

    /// Who would answer `text` now, and why.
    pub(crate) fn pick(&self, text: &str, cx: &gpui::App) -> Option<(AgentRecord, RouteReason)> {
        let agents = self.sidebar.read(cx).agents();
        let team: Vec<Teammate<'_>> = agents
            .iter()
            .map(|agent| Teammate {
                id: &agent.id,
                name: &agent.name,
                preset: agent.preset.as_deref(),
            })
            .collect();
        let lines = self.lines();
        let last = last_answer(&lines, chrono::Utc::now());
        let (mate, reason) = route(
            text,
            &team,
            last.as_ref().map(|(id, minutes)| (id.as_str(), *minutes)),
        )?;
        let agent = agents.iter().find(|agent| agent.id == mate.id)?.clone();
        Some((agent, reason))
    }

    /// "@Nome " at the start of the message, from a name above the field.
    pub(crate) fn address(&mut self, name: &str, cx: &mut Context<Self>) {
        let text = self.input.read(cx).text().to_string();
        let rest = text
            .strip_prefix('@')
            .and_then(|rest| rest.split_once(' ').map(|(_, rest)| rest))
            .unwrap_or(&text)
            .to_string();
        let text = format!("@{name} {rest}");
        self.input.update(cx, |area, cx| area.set_text(&text, cx));
        cx.notify();
    }

    /// The agents at work or waiting on the person in their conversation.
    pub(crate) fn busy(&self, cx: &gpui::App) -> Vec<(AgentRecord, AgentState)> {
        self.sidebar
            .read(cx)
            .agents()
            .iter()
            .filter(|agent| talks(agent.preset.as_deref()))
            .filter_map(|agent| {
                let state = home_sessions(agent)
                    .map(|session| session.state)
                    .find(|state| matches!(state, AgentState::Working | AgentState::NeedsYou))?;
                Some((agent.clone(), state))
            })
            .collect()
    }

    /// The agent's latest conversation, to open whole.
    pub(crate) fn open_conversation(&mut self, agent_id: &str, cx: &mut Context<Self>) {
        let sidebar = self.sidebar.read(cx);
        let Some(session) = sidebar
            .agents()
            .iter()
            .find(|agent| agent.id == agent_id)
            .and_then(super::conversation::home_session)
        else {
            return;
        };
        cx.emit(RoomEvent::OpenSession {
            task_id: session.workspace_id.clone(),
            session_id: session.session_id.clone(),
        });
    }

    pub(crate) fn send(&mut self, cx: &mut Context<Self>) {
        let text = self.input.read(cx).text().trim().to_string();
        if text.is_empty() || self.sending {
            return;
        }
        let Some((agent, reason)) = self.pick(&text, cx) else {
            Toaster::error(cx.t("agents.room.noAgent"), cx);
            return;
        };
        let (provider, model) = match agent_execution(&agent, self.composer.read(cx), cx) {
            Ok(execution) => execution,
            Err(reason) => {
                Toaster::error(reason, cx);
                return;
            }
        };
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        let existing =
            super::conversation::home_session(&agent).map(|session| session.session_id.clone());
        let home_task = self
            .sidebar
            .read(cx)
            .agent_page(&agent.id)
            .and_then(|data| data.home_task);
        let title = cx
            .t_with(
                "agents.conversation.taskName",
                &Args::default().with("agent", agent.name.clone()),
            )
            .to_string();
        let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.reasons
            .insert((agent.id.clone(), text.clone()), reason);
        self.pending.push(Pending {
            agent_id: agent.id.clone(),
            text: text.clone(),
            at,
        });
        self.input.update(cx, |area, cx| area.set_text("", cx));
        self.sending = true;
        cx.notify();
        let effort = agent.provider_id.as_ref().and(agent.effort.clone());
        cx.spawn(async move |this, cx| {
            let sent = cx
                .background_executor()
                .spawn({
                    let text = text.clone();
                    async move {
                        let session = match existing {
                            Some(session) => session,
                            None => {
                                api.start_agent_conversation(&ConversationStart {
                                    agent_id: &agent.id,
                                    agent_name: &agent.name,
                                    home_task: home_task.as_deref(),
                                    title: &title,
                                    provider: &provider,
                                    model: model.as_deref(),
                                })?
                                .1
                            }
                        };
                        let options = TurnOptions {
                            provider_id: Some(provider.clone()),
                            model: model.clone(),
                            effort,
                            approval_policy: Some("ask".into()),
                            plan_mode: false,
                            fast_mode: false,
                            tool_instructions: None,
                            provider_runtime: (!cfg!(test))
                                .then(|| crate::data::provider_runtime::for_provider(&provider))
                                .flatten(),
                        };
                        api.send_turn(&session, &text, None, &options)
                    }
                })
                .await;
            let _ = this.update(cx, |room, cx| {
                room.sending = false;
                if let Err(error) = sent {
                    room.pending.retain(|pending| pending.text != text);
                    Toaster::error(error.to_string(), cx);
                } else if !cfg!(test) {
                    room.sidebar
                        .update(cx, |sidebar, cx| sidebar.reload_now(cx));
                }
                cx.notify();
            });
        })
        .detach();
    }
}
