//! A DCC agent's page (`AgentPage`), in the center: its mascot, state and
//! what it runs on; how it is called; its conversations in every project
//! (opening one goes to that task); the person's extra instructions; and
//! its role, folded. It reads the sidebar, which reads the agents every few
//! seconds; "Editar" changes what the person owns.

use gpui::{
    AnyElement, Context, Entity, EventEmitter, FontWeight, IntoElement, SharedString, Subscription,
    Window, div, prelude::*, px,
};

use crate::agent::AgentAvatarView;
use crate::data::agents::{AgentRecord, AgentState};
use crate::data::composer_settings::clamp_effort;
use crate::data::time::medium_date_time;
use crate::i18n::{I18n, Locale, Translate};
use crate::screens::composer::Composer;
use crate::screens::sidebar::{AgentPageData, Sidebar};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Icon, Tracked};

mod conversation;
#[cfg(test)]
mod conversation_tests;
mod editing;
mod editor;
mod editor_access;
mod editor_budget;
mod editor_look;
mod editor_projects;
mod editor_role;
mod editor_templates;
mod execution;
mod memory;
mod memory_history;
mod memory_layers;
#[cfg(test)]
mod memory_tests;
mod page_tabs;
mod picker;
mod pr_reviews;
mod room;
mod room_bulletin;
mod room_cards;
mod room_checkins;
mod room_cleanup;
mod room_focus;
mod room_handoff;
mod room_limits;
mod room_mentions;
mod room_recap;
mod room_routines;
#[cfg(test)]
mod room_tests;
mod room_timeline;
mod room_view;
mod routines;
mod spend;
mod team;
#[cfg(test)]
mod tests;
mod triggers;
#[cfg(test)]
mod triggers_tests;
mod triggers_wake;

pub enum AgentPageEvent {
    /// A conversation of the agent: its task, on that session.
    OpenSession { task_id: String, session_id: String },
    /// A task from the day's recap in the team room.
    OpenTask(String),
    /// One of the reviewer's pull request reviews: the hub on its code tab.
    OpenPullRequest(String),
}

pub struct AgentPage {
    pub agent_id: String,
    sidebar: Entity<Sidebar>,
    composer: Entity<Composer>,
    pub(crate) role_open: bool,
    /// "Editar …", while open.
    pub(crate) editor: Option<(Entity<editor::AgentEditor>, Subscription)>,
    /// The shown agent's month (`spend`), read when it shows.
    pub(crate) spend: spend::SpendRead,
    /// "Nova rotina", while open (`routines`).
    pub(crate) routine_form: Option<routines::RoutineForm>,
    /// The desktop the memory goes through (tests point it elsewhere).
    pub(crate) api: Option<crate::data::desktop_api::DesktopApi>,
    /// "Memória", made on the first render (its fields need the window).
    pub(crate) memory: Option<Entity<memory::MemoryCard>>,
    /// The team room, made the first time it shows.
    pub(crate) room: Option<(Entity<room::TeamRoom>, Subscription)>,
    /// The room is on screen instead of an agent's page.
    pub(crate) room_open: bool,
    /// The agent's page on its conversation or its settings (`page_tabs`).
    pub(crate) tab: page_tabs::AgentTab,
    /// What the room's field starts with when it opens ("@Revisor ").
    room_draft: Option<String>,
    _sidebar: Subscription,
    /// Its mascot follows the blink and the triggers turned on or off.
    _mascot: [Subscription; 2],
}

impl EventEmitter<AgentPageEvent> for AgentPage {}

/// `runsOn`: provider · model · effort, or "the composer's".
pub fn runs_on(agent: &AgentRecord, composer: &Composer, cx: &gpui::App) -> SharedString {
    let provider = agent.provider_id.as_deref().and_then(|id| {
        composer
            .catalog
            .as_deref()?
            .iter()
            .find(|provider| provider.id.0 == id)
    });
    let Some(provider) = provider else {
        return cx.t("agents.page.runsOnCurrent");
    };
    let model = agent
        .model
        .as_deref()
        .and_then(|id| provider.models.iter().find(|model| model.id == id));
    // `agentEffortForModel`
    let effort = match (agent.effort.as_deref(), model) {
        (Some(effort), Some(model)) if !model.effort_levels.is_empty() => {
            Some(clamp_effort(effort, &model.effort_levels))
        }
        _ => None,
    };
    let parts: Vec<String> = [
        Some(provider.label.clone()),
        model.map(|model| model.label.clone()),
        effort.map(|effort| cx.t(&format!("composer.effort.{effort}")).to_string()),
    ]
    .into_iter()
    .flatten()
    .collect();
    parts.join(" · ").into()
}

impl AgentPage {
    pub fn new(
        agent_id: String,
        sidebar: Entity<Sidebar>,
        composer: Entity<Composer>,
        cx: &mut Context<Self>,
    ) -> Self {
        let observe = cx.observe(&sidebar, |_, _, cx| cx.notify());
        Self {
            agent_id,
            sidebar,
            composer,
            role_open: false,
            editor: None,
            api: if cfg!(test) {
                None
            } else {
                crate::data::desktop_api::DesktopApi::load()
            },
            spend: Default::default(),
            routine_form: None,
            memory: None,
            room: None,
            room_open: false,
            tab: Default::default(),
            room_draft: None,
            _sidebar: observe,
            _mascot: [
                cx.observe_global::<crate::agent::MascotBlink>(|_, cx| cx.notify()),
                cx.observe_global::<crate::data::agent_triggers::AgentTriggers>(|_, cx| {
                    cx.notify()
                }),
            ],
        }
    }

    pub fn show(&mut self, agent_id: String, cx: &mut Context<Self>) {
        self.room_open = false;
        self.tab = page_tabs::AgentTab::Settings;
        // Read again: the month moves while the page is away.
        self.spend = Default::default();
        self.routine_form = None;
        if self.agent_id != agent_id {
            self.agent_id = agent_id;
            self.role_open = false;
            self.editor = None;
        }
        if let Some(memory) = &self.memory {
            let agent_id = self.agent_id.clone();
            memory.update(cx, |memory, cx| memory.show(&agent_id, cx));
        }
        cx.notify();
    }

    fn memory_card(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (agent_id, api) = (self.agent_id.clone(), self.api.clone());
        let card = self
            .memory
            .get_or_insert_with(|| cx.new(|cx| memory::MemoryCard::new(agent_id, api, window, cx)))
            .clone();
        // An agent keeps a layer per project it works in.
        let sidebar = self.sidebar.read(cx);
        let layers = sidebar
            .agents()
            .iter()
            .find(|agent| agent.id == self.agent_id)
            .map(|agent| {
                editor_projects::project_options(sidebar.projects())
                    .into_iter()
                    .filter(|(id, _)| agent.works_in(id))
                    .collect()
            })
            .unwrap_or_default();
        card.update(cx, |card, cx| card.set_layers(layers, cx));
        card.into_any_element()
    }

    fn section_title(text: SharedString, cx: &gpui::App) -> AnyElement {
        div()
            .mb(px(8.0))
            .text_size(px(11.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().colors.muted_foreground)
            .child(Tracked::new(text.to_uppercase(), 11.0, 0.08))
            .into_any_element()
    }

    fn card(cx: &gpui::App) -> gpui::Div {
        let colors = &cx.theme().colors;
        div()
            .rounded(px(18.0))
            .border_1()
            .border_color(colors.border.opacity(0.7))
            .bg(colors.card)
    }

    fn sessions(&self, data: &AgentPageData, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let dark = cx.theme().appearance == Appearance::Dark;
        let english = cx.global::<I18n>().locale() == Locale::En;
        let agent = &data.agent;
        let home = dcc_core::domain::agent_conversation::home_project_id(&agent.id);
        let rows: Vec<AnyElement> = agent
            .sessions
            .iter()
            .zip(&data.unread)
            // Its own conversation is the "Conversa" tab.
            .filter(|(session, _)| session.project_id != home)
            .enumerate()
            .map(|(index, (session, unread))| {
                let read = session.state == AgentState::Done && !unread;
                let name = data
                    .task_names
                    .get(&session.workspace_id)
                    .cloned()
                    .or_else(|| session.title.clone())
                    .unwrap_or_else(|| session.session_id.clone());
                let line: Vec<String> = [
                    data.project_labels.get(&session.project_id).cloned(),
                    Some(medium_date_time(&session.updated_at, english)),
                ]
                .into_iter()
                .flatten()
                .collect();
                let (state_text, state_color, weight) = if read {
                    (
                        cx.t("agents.state.finished"),
                        colors.muted_foreground,
                        FontWeight::NORMAL,
                    )
                } else if session.state == AgentState::NeedsYou {
                    (
                        cx.t(session.state.i18n_key()),
                        if dark { tw::AMBER_300 } else { tw::AMBER_700 }.hsla(),
                        FontWeight::NORMAL,
                    )
                } else if *unread {
                    (
                        cx.t(session.state.i18n_key()),
                        if dark {
                            tw::EMERALD_300
                        } else {
                            tw::EMERALD_700
                        }
                        .hsla(),
                        FontWeight::MEDIUM,
                    )
                } else {
                    (
                        cx.t(session.state.i18n_key()),
                        colors.muted_foreground,
                        FontWeight::NORMAL,
                    )
                };
                let (task_id, session_id) =
                    (session.workspace_id.clone(), session.session_id.clone());
                div()
                    .id(SharedString::from(format!("agent-session-{index}")))
                    .debug_selector(move || format!("agent-session-{index}"))
                    .flex()
                    .w_full()
                    .items_center()
                    .gap(px(12.0))
                    .px(px(16.0))
                    .py(px(12.0))
                    .cursor_pointer()
                    .hover(|row| row.bg(colors.accent.opacity(0.5)))
                    .when(index > 0, |row| {
                        row.border_t_1().border_color(colors.border.opacity(0.6))
                    })
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(AgentPageEvent::OpenSession {
                            task_id: task_id.clone(),
                            session_id: session_id.clone(),
                        })
                    }))
                    // A result already read shows the plain mascot.
                    .child(AgentAvatarView::new(
                        SharedString::from(format!("agent-session-avatar-{index}")),
                        &agent.avatar,
                        (!read).then_some(session.state),
                        28.0,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(name),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .child(line.join(" · ")),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(11.0))
                            .font_weight(weight)
                            .text_color(state_color)
                            .child(state_text),
                    )
                    .into_any_element()
            })
            .collect();
        let reviewer = agent.preset.as_deref() == Some(crate::data::agents::REVIEWER_PRESET);
        let title = cx.t(if reviewer {
            "agents.page.sessions"
        } else {
            "agents.page.agentSessions"
        });
        div()
            .child(Self::section_title(title, cx))
            .child(if rows.is_empty() {
                div()
                    .rounded(px(18.0))
                    .border_1()
                    .border_dashed()
                    .border_color(colors.border.opacity(0.7))
                    .px(px(20.0))
                    .py(px(24.0))
                    .text_size(px(13.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t(if reviewer {
                        "agents.page.noSessions"
                    } else {
                        "agents.page.agentNoSessions"
                    }))
                    .into_any_element()
            } else {
                Self::card(cx)
                    .debug_selector(|| "agent-sessions".into())
                    .overflow_hidden()
                    .children(rows)
                    .into_any_element()
            })
            .into_any_element()
    }
}

impl AgentPage {
    /// The agent's page, or its editor, beside the team (`team`).
    fn page_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.room_open && self.editor.is_none() {
            return self.room_element(None, window, cx);
        }
        // "Novo agente" has no conversation yet.
        if self.editor_is_for_agent(cx) {
            return self.tabbed_body(window, cx);
        }
        self.settings_body(window, cx)
    }

    /// "Configuração": the page as it is, or its editor.
    fn settings_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let Some(data) = self.sidebar.read(cx).agent_page(&self.agent_id) else {
            return div()
                .debug_selector(|| "agent-page-missing".into())
                .flex()
                .size_full()
                .items_center()
                .justify_center()
                .bg(colors.background)
                .text_size(px(13.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("agents.page.missing"))
                .into_any_element();
        };
        // "Editar" takes the page, with a way back to it.
        if let Some((editor, _)) = &self.editor {
            return div()
                .id("agent-editor-page")
                .size_full()
                .min_h_0()
                .overflow_y_scroll()
                .bg(colors.background)
                .child(
                    div()
                        .mx_auto()
                        .max_w(px(640.0))
                        .px(px(32.0))
                        .py(px(32.0))
                        .child(editor.clone()),
                )
                .into_any_element();
        }
        let agent = data.agent.clone();
        let runs_on = runs_on(&agent, self.composer.read(cx), cx);
        let mood = self
            .sidebar
            .read(cx)
            .mood(&agent.id, cx)
            .unwrap_or_else(|| crate::agent::Mood::of(data.state, false, false));
        let header = div()
            .flex()
            .items_center()
            .gap(px(20.0))
            .child(crate::agent::MoodAvatarView::new(
                "agent-page-avatar",
                &agent.avatar,
                mood,
                88.0,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(22.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.foreground)
                            .child(agent.name.clone()),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(12.0))
                            .text_color(colors.muted_foreground)
                            .debug_selector(|| "agent-page-mood".into())
                            .child(format!("{} · {runs_on}", cx.t(mood.i18n_key()))),
                    ),
            )
            .child(
                crate::ui::Button::new("agent-page-edit")
                    .variant(crate::ui::ButtonVariant::Outline)
                    .size(crate::ui::ButtonSize::Default)
                    .text_size(14.0)
                    .on_click(cx.listener(|page, _, window, cx| page.open_editor(window, cx)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(Icon::new("pencil").size(16.0))
                            .child(cx.t("agents.page.edit")),
                    ),
            );
        // An agent works on a task's changes, so it is called from the task.
        let reviewer = agent.preset.as_deref() == Some(crate::data::agents::REVIEWER_PRESET);
        let how_to_call = Self::card(cx)
            .p(px(20.0))
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(cx.t("agents.page.howToCall")),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(12.0))
                    .line_height(px(19.5))
                    .text_color(colors.muted_foreground)
                    .child(cx.t(if reviewer {
                        "agents.page.howToCallReviewer"
                    } else {
                        "agents.page.howToCallAgent"
                    })),
            );
        let triggers = self.triggers_card(&data, cx);
        let spend = self.spend_card(agent.monthly_token_limit, cx);
        let routines = self.routines_card(cx);
        let sessions = self.sessions(&data, cx);
        let pr_reviews = self.pr_reviews(&data, cx);
        let memory = self.memory_card(window, cx);
        let extra = (!agent.extra_instructions.is_empty()).then(|| {
            div()
                .child(Self::section_title(cx.t("agents.editor.extra"), cx))
                .child(
                    Self::card(cx)
                        .px(px(20.0))
                        .py(px(16.0))
                        .text_size(px(13.0))
                        .line_height(px(21.1))
                        .text_color(colors.foreground)
                        .child(agent.extra_instructions.clone()),
                )
        });
        let open = self.role_open;
        let role = div()
            .child(
                div()
                    .id("agent-role-toggle")
                    .debug_selector(|| "agent-role-toggle".into())
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .cursor_pointer()
                    .text_color(colors.muted_foreground)
                    .hover(|toggle| toggle.text_color(colors.foreground))
                    .on_click(cx.listener(|page, _, _, cx| {
                        page.role_open = !page.role_open;
                        cx.notify();
                    }))
                    .child(
                        Icon::new(if open {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        })
                        .size(14.0),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(cx.t("agents.page.showRole")),
                    ),
            )
            .when(open, |section| {
                section
                    .child(
                        div()
                            .mt(px(8.0))
                            .text_size(px(11.0))
                            .line_height(px(17.9))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("agents.page.roleHint")),
                    )
                    .child(
                        Self::card(cx)
                            .debug_selector(|| "agent-role".into())
                            .mt(px(8.0))
                            .px(px(20.0))
                            .py(px(16.0))
                            .text_size(px(12.0))
                            .line_height(px(19.5))
                            .text_color(colors.foreground)
                            .child(agent.role.clone()),
                    )
            });
        div()
            .id("agent-page")
            .debug_selector(|| "agent-page".into())
            .size_full()
            .min_h_0()
            .overflow_y_scroll()
            .bg(colors.background)
            .child(
                div()
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .gap(px(32.0))
                    .max_w(px(768.0))
                    .px(px(32.0))
                    .py(px(32.0))
                    .child(header)
                    .children(triggers)
                    .children(spend)
                    .child(routines)
                    .child(how_to_call)
                    .child(sessions)
                    .children(pr_reviews)
                    .child(memory)
                    .children(extra)
                    .child(role),
            )
            .into_any_element()
    }
}
