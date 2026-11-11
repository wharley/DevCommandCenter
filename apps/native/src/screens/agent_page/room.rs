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

use dcc_core::domain::agent_conversation::home_project_id;
use dcc_core::domain::team_room::{RouteReason, Teammate, route};
use gpui::{AppContext, Context, Entity, EventEmitter, Subscription, Window};

use crate::data::agent_conversation::ConversationStart;
use crate::data::agents::{AgentRecord, AgentSession, AgentState};
use crate::data::desktop_api::DesktopApi;
use crate::data::team_room::{HomeFeed, RoomLine, Speaker, last_answer, merged};
use crate::i18n::{Args, Translate};
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::ui::{TextArea, TextAreaEvent, Toaster};

pub enum RoomEvent {
    /// An agent's whole conversation, with its tools.
    OpenSession { task_id: String, session_id: String },
    /// A pull request review's code tab.
    OpenPullRequest(String),
    /// A task from the day's recap.
    OpenTask(String),
    /// A task on its "Mudanças" (the change card).
    OpenTaskChanges(String),
    /// The change card's step: the task opens and its own dock shows the
    /// commit's or the pull request's dialog; nothing runs before the
    /// person confirms there.
    DeliverTask {
        task_id: String,
        step: crate::data::task_delivery::DeliveryStep,
    },
    /// An agent's page ("Ajustar" on its limit card).
    OpenAgent(String),
    /// "Criar" on an agent proposed: its conversation, the first request
    /// in the field.
    AgentCreated {
        agent_id: String,
        first_request: String,
    },
    /// "Editar antes" on an agent proposed.
    EditProposal(dcc_core::domain::agent_proposal::AgentProposal),
}

/// A message on its way: shown until the agent's conversation has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    pub agent_id: String,
    pub text: String,
    pub at: String,
    /// Queued behind the turn at work instead of sent (`room_queue`).
    pub queued: Option<super::room_queue::Queued>,
}

pub struct TeamRoom {
    pub(super) sidebar: Entity<Sidebar>,
    pub(super) composer: Entity<Composer>,
    pub(crate) api: Option<DesktopApi>,
    pub(crate) db_path: PathBuf,
    pub(crate) feeds: Vec<HomeFeed>,
    reading: bool,
    pub(crate) input: Entity<TextArea>,
    pub(crate) pending: Vec<Pending>,
    /// Why a message went to its agent, by agent and text (this run only).
    pub(crate) reasons: HashMap<(String, String), RouteReason>,
    pub(crate) sending: bool,
    /// The suggestions to pass the turn the person already acted on.
    pub(crate) handoffs: crate::data::room_handoffs::Handoffs,
    /// The conversation's rows, virtualized (`room_list`).
    pub(crate) list: super::room_list::RoomList,
    /// Answers' markdown, parsed once per line (`room_markdown`).
    pub(super) markdown: std::cell::RefCell<super::room_markdown::LineMarkdown>,
    /// The task answers opened in full ("Ler tudo"), by key.
    pub(super) opened_answers: std::cell::RefCell<std::collections::HashSet<String>>,
    /// The change cards' files, read as they show (`room_changes`).
    pub(crate) changes: std::cell::RefCell<super::room_changes::RoomChanges>,
    /// The day's recap, read when its card opens.
    pub(crate) recap: super::room_recap::RecapRead,
    pub(crate) recap_open: bool,
    /// The team above the field while an `@` is typed.
    pub(crate) mention: Option<super::room_mentions::AgentMention>,
    /// The agents with a limit that are past it now (`room_limits`).
    pub(crate) limits: Vec<String>,
    /// The weekly bulletin, read when its card opens (`room_bulletin`).
    pub(crate) bulletin: super::room_bulletin::BulletinRead,
    pub(crate) bulletin_open: bool,
    /// When the completed tasks' space shows, and where that is kept
    /// (none in tests).
    pub(crate) cleanup: crate::data::cleanup_alert::CleanupAlert,
    pub(crate) cleanup_path: Option<PathBuf>,
    /// The one agent shown, on its page's "Conversa" (`room_focus`).
    pub(crate) focus: Option<String>,
    /// The project whose room this is; none: the Geral room.
    pub(crate) room: Option<String>,
    /// Bumped on each switch of room, so a read of the room before is dropped.
    pub(crate) room_generation: u64,
    /// The projects that have a room, for where each card shows.
    pub(crate) rooms: Vec<String>,
    /// Approvals being answered from the room (`room_approvals`).
    pub(crate) answering: std::collections::HashSet<String>,
    /// "Recusar com motivo" open on a request, with its field.
    pub(crate) approval_reason: Option<(String, Entity<crate::ui::TextInput>)>,
    /// Questions being answered in the room's dock (`room_question`).
    pub(crate) questions: super::room_question::RoomQuestions,
    /// Replies agents proposed being edited or sent (`room_replies`).
    pub(crate) replies: super::room_replies::RoomReplies,
    /// The answers whose steps are listed open, with those steps read when
    /// they opened (`room_steps`).
    pub(crate) steps_open:
        HashMap<String, std::rc::Rc<Vec<dcc_core::domain::team_room_steps::Step>>>,
    /// Agents proposed that the person answered here, gone at once.
    pub(crate) answered_proposals: Vec<String>,
    /// 👍 / 👎 changed here, by answer, before the feed reads them again.
    pub(crate) reacted: HashMap<String, Option<dcc_core::domain::agent_reaction::Reaction>>,
    /// "Guardar como regra?" after a 👎 (`room_reactions`).
    pub(crate) rule: Option<super::room_reactions::RuleDraft>,
    /// A turn at work shown as it goes (`room_live_poll`, `room_live_turn`).
    pub(crate) live: super::room_live_poll::RoomLive,
    /// Each room's unsent words, kept while switching (`room_drafts`).
    pub(crate) drafts: super::room_drafts::RoomDrafts,
    /// The new lines under the person scrolled up (`room_jump`).
    pub(crate) jump: super::room_jump::Jump,
    /// The agent card open on a face or a name (`agent_card`).
    pub(crate) card: super::agent_card::AgentCards,
    _subscriptions: [Subscription; 8],
}

impl EventEmitter<RoomEvent> for TeamRoom {}

/// Every conversation an agent has in `room` (none: the Geral room).
fn home_sessions<'a>(
    agent: &'a AgentRecord,
    room: Option<&'a str>,
) -> impl Iterator<Item = &'a AgentSession> {
    let home = home_project_id(&agent.id);
    agent
        .sessions
        .iter()
        .filter(move |session| session.project_id == home && session.room.as_deref() == room)
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
            // The check-in's cards, as `PrWatch` finds them.
            cx.observe_global::<crate::data::check_ins::CheckIns>(|_, cx| cx.notify()),
            // The routines proposed, as the clock reads them.
            cx.observe_global::<crate::data::routines::Routines>(|_, cx| cx.notify()),
            // The sources proposed, as their clock reads them.
            cx.observe_global::<crate::data::agent_sources::Sources>(|_, cx| cx.notify()),
            // A frame the shared clock alone asked for keeps the rows.
            // So does one the live poll asked for when only a turn at work
            // moved (`room_live_poll`): the tail is drawn fresh anyway.
            cx.observe_self(|room, cx| {
                let tick = crate::ui::pulse::took_tick(cx.entity_id(), cx);
                let quiet = std::mem::take(&mut room.live.quiet);
                room.list.notified(tick || quiet);
            }),
            // Read by the rows but kept apart from the sidebar: a trigger
            // run (cards, a held trigger) and a pull request review.
            cx.observe_global::<crate::data::agent_triggers::AgentTriggers>(|_, cx| cx.notify()),
            cx.observe_global::<crate::data::pr_review_jobs::PrReviewJobs>(|_, cx| cx.notify()),
            cx.subscribe(&input, |room, _, event, cx| match event {
                TextAreaEvent::Submitted => room.send(cx),
                TextAreaEvent::Changed => {
                    room.tint_mentions(cx);
                    room.update_mention(cx);
                    cx.notify();
                }
                TextAreaEvent::MenuUp => room.move_mention(-1, cx),
                TextAreaEvent::MenuDown => room.move_mention(1, cx),
                TextAreaEvent::MenuAccept => room.accept_mention(None, cx),
                TextAreaEvent::Cancelled => room.close_mention(cx),
                TextAreaEvent::PastedNonText => room.paste_image(cx),
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
            handoffs: crate::data::room_handoffs::Handoffs::load(
                (!cfg!(test)).then(|| crate::prefs::native_file("room-handoffs.json")),
            ),
            list: Default::default(),
            markdown: Default::default(),
            opened_answers: Default::default(),
            changes: Default::default(),
            recap: Default::default(),
            recap_open: false,
            mention: None,
            limits: Vec::new(),
            bulletin: Default::default(),
            bulletin_open: false,
            cleanup: Default::default(),
            cleanup_path: (!cfg!(test)).then(|| crate::prefs::native_file("cleanup-alert.json")),
            focus: None,
            room: None,
            room_generation: 0,
            rooms: Vec::new(),
            answering: Default::default(),
            approval_reason: None,
            questions: Default::default(),
            replies: Default::default(),
            steps_open: HashMap::new(),
            answered_proposals: Vec::new(),
            reacted: HashMap::new(),
            rule: None,
            live: Default::default(),
            drafts: Default::default(),
            jump: Default::default(),
            card: Default::default(),
            _subscriptions: subscriptions,
        };
        room.reload_cleanup();
        room.read_limits(cx);
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
            .flat_map(|agent| {
                home_sessions(agent, self.room.as_deref()).map(|session| {
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
        let generation = self.room_generation;
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
                // Read for the room before a switch: the new one reads its own.
                if room.room_generation != generation {
                    room.sync(cx);
                    return;
                }
                // A turn moved: the cards it may have proposed show now.
                if !read.is_empty() {
                    room.refresh_proposed(cx);
                }
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
                // A live poll may have read further meanwhile: kept.
                Some(old) if feed.last_sequence() < old.last_sequence() => {}
                Some(old) => *old = feed,
                None => self.feeds.push(feed),
            }
        }
        self.forget_settled();
        self.drop_unqueued();
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

    /// The agents at work or waiting on the person in their conversation.
    pub(crate) fn busy(&self, cx: &gpui::App) -> Vec<(AgentRecord, AgentState)> {
        self.sidebar
            .read(cx)
            .agents()
            .iter()
            .filter_map(|agent| {
                let state = home_sessions(agent, self.room.as_deref())
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
            .and_then(|agent| super::conversation::room_session(agent, self.room.as_deref()))
        else {
            return;
        };
        cx.emit(RoomEvent::OpenSession {
            task_id: session.workspace_id.clone(),
            session_id: session.session_id.clone(),
        });
    }

    pub(crate) fn send(&mut self, cx: &mut Context<Self>) {
        // Its images go as `@/path.png`, as the composer sends them.
        let text = self.prompt_text(cx);
        if text.is_empty() || self.sending {
            return;
        }
        // On an agent's own "Conversa", it goes straight to that agent.
        let picked = match self.focused(cx) {
            Some(agent) => Some((agent, None)),
            None => self
                .pick(&text, cx)
                .map(|(agent, reason)| (agent, Some(reason))),
        };
        let Some((agent, reason)) = picked else {
            Toaster::error(cx.t("agents.room.noAgent"), cx);
            return;
        };
        if self.deliver(agent, text, reason, cx) {
            self.input.update(cx, |area, cx| area.set_text("", cx));
        }
    }

    /// Sends `text` to `agent`'s own conversation (made the first time);
    /// false when it cannot go now (told to the person).
    pub(crate) fn deliver(
        &mut self,
        agent: AgentRecord,
        text: String,
        reason: Option<RouteReason>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.sending {
            return false;
        }
        let (provider, model, options) = match self.agent_turn(&agent, cx) {
            Ok(turn) => turn,
            Err(reason) => {
                Toaster::error(reason, cx);
                return false;
            }
        };
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return false;
        };
        let existing = super::conversation::room_session(&agent, self.room.as_deref())
            .map(|session| session.session_id.clone());
        // A turn at work there: it waits for it (`room_queue`).
        let queued = existing
            .clone()
            .filter(|session| self.queues_for(&agent.id, session, cx))
            .map(|session_id| super::room_queue::Queued {
                session_id,
                id: None,
            });
        let queue = queued.is_some();
        let room = self.room.clone();
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
        // A project's room names its conversations after the project too.
        let title = match self.room_name(cx) {
            Some(project) => format!("{title} · {project}"),
            None => title,
        };
        let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        if let Some(reason) = reason {
            self.reasons
                .insert((agent.id.clone(), text.clone()), reason);
        }
        self.pending.push(Pending {
            agent_id: agent.id.clone(),
            text: text.clone(),
            at,
            queued,
        });
        self.sending = true;
        // What the person just sent shows: back to the bottom.
        self.list.follow();
        // Talking to the agent lifts its brake (`agent_brake`).
        if cx.has_global::<crate::data::agent_triggers::AgentTriggers>() {
            gpui::BorrowAppContext::update_global::<crate::data::agent_triggers::AgentTriggers, _>(
                cx,
                |triggers, _| triggers.attend(&agent.id, chrono::Utc::now()),
            );
        }
        cx.notify();
        let runtime = options.provider_runtime.clone();
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
                                    runtime: runtime.clone(),
                                    room: room.as_deref(),
                                })?
                                .1
                            }
                        };
                        if queue {
                            api.queue_turn_for_id(&session, &text, &options).map(Some)
                        } else {
                            api.send_turn(&session, &text, None, &options).map(|_| None)
                        }
                    }
                })
                .await;
            let _ = this.update(cx, |room, cx| {
                room.sending = false;
                if let Ok(Some(id)) = &sent {
                    room.mark_queued(&text, id.clone());
                }
                if let Err(error) = sent {
                    room.pending.retain(|pending| pending.text != text);
                    let error = error.to_string();
                    if super::room_limits::is_limit_error(&error) {
                        // The card says it and offers "Continuar".
                        room.read_limits(cx);
                    } else {
                        Toaster::error(error, cx);
                    }
                } else if !cfg!(test) {
                    room.sidebar
                        .update(cx, |sidebar, cx| sidebar.reload_now(cx));
                }
                cx.notify();
            });
        })
        .detach();
        true
    }
}
