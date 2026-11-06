//! A DCC agent's page (`AgentPage`), in the center: its mascot, state and
//! what it runs on; how it is called; its conversations in every project
//! (opening one goes to that task); the person's extra instructions; and
//! its role, folded. It reads the sidebar, which reads the agents every few
//! seconds; "Editar" changes what the person owns, the Reporter's page is
//! its recap, and the Researcher's starts a new idea.

use gpui::{
    AnyElement, Context, Entity, EventEmitter, FontWeight, IntoElement, Render, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::agent::AgentAvatarView;
use crate::data::agents::{AgentRecord, AgentState, CHRONICLER_PRESET};
use crate::data::composer_settings::clamp_effort;
use crate::data::time::medium_date_time;
use crate::i18n::{I18n, Locale, Translate};
use crate::screens::composer::Composer;
use crate::screens::sidebar::{AgentPageData, Sidebar};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Icon, Tracked};

mod editor;
mod idea;
mod memory;
mod memory_history;
mod picker;
mod pr_reviews;
mod recap;
#[cfg(test)]
mod memory_tests;
#[cfg(test)]
mod tests;

pub const RESEARCHER_PRESET: &str = "researcher";

pub enum AgentPageEvent {
    /// A conversation of the agent: its task, on that session.
    OpenSession { task_id: String, session_id: String },
    /// A task from the Reporter's recap.
    OpenTask(String),
    /// A new idea's task, on its conversation with the Researcher: the
    /// composer takes the focus.
    IdeaStarted { task_id: String, session_id: String },
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
    /// Where the Reporter's cleanup alert is kept (tests use their own).
    pub(crate) cleanup_path: std::path::PathBuf,
    /// The Reporter's recap for its current window.
    pub(crate) recap: recap::RecapRead,
    /// The database the recap is read from (tests use their own).
    pub(crate) db_path: std::path::PathBuf,
    /// "Nova ideia" while it runs.
    pub(crate) starting_idea: bool,
    /// The desktop the idea goes through (tests point it elsewhere).
    pub(crate) api: Option<crate::data::desktop_api::DesktopApi>,
    /// "Memória", made on the first render (its fields need the window).
    pub(crate) memory: Option<Entity<memory::MemoryCard>>,
    _sidebar: Subscription,
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
            cleanup_path: crate::prefs::native_file("cleanup-alert.json"),
            recap: recap::RecapRead::default(),
            db_path: crate::data::loader::database_path(),
            starting_idea: false,
            api: if cfg!(test) {
                None
            } else {
                crate::data::desktop_api::DesktopApi::load()
            },
            memory: None,
            _sidebar: observe,
        }
    }

    pub fn show(&mut self, agent_id: String, cx: &mut Context<Self>) {
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

    /// "Editar": the stored values, read when it opens.
    pub(crate) fn open_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(data) = self.sidebar.read(cx).agent_page(&self.agent_id) else {
            return;
        };
        let catalog = self
            .composer
            .read(cx)
            .catalog
            .as_deref()
            .cloned()
            .unwrap_or_default();
        let path = self.cleanup_path.clone();
        let editor = cx.new(|cx| editor::AgentEditor::new(data.agent, catalog, path, window, cx));
        let events = cx.subscribe(&editor, |page, _, event, cx| match event {
            editor::EditorEvent::Closed => {
                page.editor = None;
                cx.notify();
            }
            // The sidebar reads the agents again within its next pass.
            editor::EditorEvent::Saved => cx.notify(),
        });
        self.editor = Some((editor, events));
        cx.notify();
    }

    fn memory_card(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (agent_id, api) = (self.agent_id.clone(), self.api.clone());
        self.memory
            .get_or_insert_with(|| cx.new(|cx| memory::MemoryCard::new(agent_id, api, window, cx)))
            .clone()
            .into_any_element()
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
        let researcher = agent.preset.as_deref() == Some(RESEARCHER_PRESET);
        let rows: Vec<AnyElement> = agent
            .sessions
            .iter()
            .zip(&data.unread)
            // The Researcher's list is its ideas in progress.
            .filter(|(session, _)| !researcher || data.idea_tasks.contains(&session.workspace_id))
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
        let title = cx.t(if researcher {
            "agents.page.researcherSessions"
        } else {
            "agents.page.sessions"
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
                    .child(cx.t(if researcher {
                        "agents.page.researcherNoSessions"
                    } else {
                        "agents.page.noSessions"
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

impl Render for AgentPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        let agent = data.agent.clone();
        let runs_on = runs_on(&agent, self.composer.read(cx), cx);
        let chronicler = agent.preset.as_deref() == Some(CHRONICLER_PRESET);
        let researcher = agent.preset.as_deref() == Some(RESEARCHER_PRESET);
        let header = div()
            .flex()
            .items_center()
            .gap(px(20.0))
            .child(AgentAvatarView::new(
                "agent-page-avatar",
                &agent.avatar,
                Some(data.state),
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
                            .child(format!("{} · {runs_on}", cx.t(data.state.i18n_key()))),
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
        // The Reviewer needs a diff, so it is called from a task; the
        // Researcher is the opposite: an idea starts here.
        let how_to_call = (!chronicler && !researcher).then(|| {
            Self::card(cx)
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
                        .child(cx.t("agents.page.howToCallReviewer")),
                )
        });
        let new_idea = researcher.then(|| self.new_idea_card(cx));
        let sessions = (!chronicler).then(|| self.sessions(&data, cx));
        let pr_reviews = self.pr_reviews(&data, cx);
        // The Reporter has no conversations: its page is the recap.
        let recap = chronicler.then(|| self.recap_card(&data, cx));
        // It runs no model either, so it has no memory.
        let memory = (!chronicler).then(|| self.memory_card(window, cx));
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
                    .children(recap)
                    .children(new_idea)
                    .children(how_to_call)
                    .children(sessions)
                    .children(pr_reviews)
                    .children(memory)
                    .children(extra)
                    .child(role),
            )
            .children(
                self.editor
                    .as_ref()
                    .map(|(editor, _)| editor.clone().into_any_element()),
            )
            .into_any_element()
    }
}
