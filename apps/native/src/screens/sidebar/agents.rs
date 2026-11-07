//! `AgentsSidebarSection`: the resident agents as a row of mascots above the
//! task list, the one that needs the person speaking for the row. In the
//! collapsed rail, one button per agent.

use std::path::Path;

use gpui::{AnyElement, ClickEvent, Context, Hsla, SharedString, Window, div, prelude::*, px};

use super::{Sidebar, SidebarEvent};
use crate::data::agents::{
    AgentRecord, AgentState, CHRONICLER_PRESET, REVIEWER_PRESET, aggregate, lead,
};
use crate::data::daily_recap::DailyRecap;
use crate::data::pr_review_jobs::{JobStatus, PrReviewJob};
use crate::data::seen::SeenResults;
use crate::i18n::Translate;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::tooltip::Side;
use crate::ui::{Button, ButtonSize, ButtonVariant, Tooltip};

/// The reading markers the agents' states depend on.
pub struct AgentMarkers {
    /// Finished agent results the person opened (`agent-seen-results.ts`).
    pub seen: SeenResults,
    /// The Reporter's recap window (`daily-recap.ts`).
    pub recap: DailyRecap,
}

impl AgentMarkers {
    pub fn load(dir: &Path) -> Self {
        let now = chrono::Local::now();
        Self {
            seen: SeenResults::load(
                &dir.join("agent-seen-results.json"),
                &now.to_utc().to_rfc3339(),
            ),
            recap: DailyRecap::load(&dir.join("daily-recap.json"), now),
        }
    }
}

/// One agent and the state its mascot shows.
pub struct AgentView<'a> {
    pub agent: &'a AgentRecord,
    pub state: AgentState,
}

impl Sidebar {
    /// `useAgentsQuery`: each agent's state across its sessions; the
    /// reviewer's pull request reviews count too, and the Reporter's result
    /// is the recap.
    pub(super) fn agent_views(&self) -> Vec<AgentView<'_>> {
        self.state
            .data
            .agents
            .iter()
            .map(|agent| {
                let mut sessions: Vec<(AgentState, bool)> = agent
                    .sessions
                    .iter()
                    .map(|session| {
                        let unread = session.state == AgentState::Done
                            && self
                                .agent_markers
                                .seen
                                .is_unread(&session.session_id, session.completed_at.as_deref());
                        (session.state, unread)
                    })
                    .collect();
                if agent.preset.as_deref() == Some(REVIEWER_PRESET) {
                    sessions.extend(self.pr_reviews.iter().map(|job| {
                        let state = if job.status == JobStatus::Running {
                            AgentState::Working
                        } else {
                            AgentState::Done
                        };
                        (state, job.unread())
                    }));
                }
                if agent.preset.as_deref() == Some(CHRONICLER_PRESET)
                    && self.agent_markers.recap.is_unread()
                {
                    sessions.push((AgentState::Done, true));
                }
                AgentView {
                    agent,
                    state: aggregate(&sessions),
                }
            })
            .collect()
    }
}

/// What the agent's page reads (`AgentView` in `use-agents.ts`).
#[derive(Clone, Debug, PartialEq)]
pub struct AgentPageData {
    pub agent: AgentRecord,
    pub state: AgentState,
    /// Per session, whether its finished result is unread.
    pub unread: Vec<bool>,
    /// Task names by id (`workspaceNames`), idea tasks included.
    pub task_names: std::collections::HashMap<String, String>,
    /// Project names by project id (`projectLabels`).
    pub project_labels: std::collections::HashMap<String, String>,
    /// The tasks that are still ideas (`ideaWorkspaceIds`).
    pub idea_tasks: std::collections::HashSet<String>,
    /// The reviewer's pull request reviews (`prReviews`).
    pub pr_reviews: Vec<PrReviewJob>,
    /// The agent's own conversation task, once it has one.
    pub home_task: Option<String>,
}

impl Sidebar {
    pub fn agent_page(&self, id: &str) -> Option<AgentPageData> {
        let view = self
            .agent_views()
            .into_iter()
            .find(|view| view.agent.id == id)?;
        let unread = view
            .agent
            .sessions
            .iter()
            .map(|session| {
                session.state == AgentState::Done
                    && self
                        .agent_markers
                        .seen
                        .is_unread(&session.session_id, session.completed_at.as_deref())
            })
            .collect();
        let data = &self.state.data;
        let mut task_names: std::collections::HashMap<String, String> = data
            .tasks
            .iter()
            .map(|task| (task.id.clone(), task.name.clone()))
            .collect();
        task_names.extend(data.idea_tasks.clone());
        Some(AgentPageData {
            agent: view.agent.clone(),
            state: view.state,
            unread,
            task_names,
            project_labels: data
                .projects
                .iter()
                .map(|project| (project.project_id.clone(), project.display_name.clone()))
                .collect(),
            idea_tasks: data.idea_tasks.keys().cloned().collect(),
            home_task: data.agent_homes.get(id).cloned(),
            pr_reviews: if view.agent.preset.as_deref() == Some(REVIEWER_PRESET) {
                self.pr_reviews.clone()
            } else {
                Vec::new()
            },
        })
    }

    /// The built-in reviewer, for the pull requests hub.
    pub fn reviewer(&self) -> Option<AgentRecord> {
        self.state
            .data
            .agents
            .iter()
            .find(|agent| agent.preset.as_deref() == Some(REVIEWER_PRESET))
            .cloned()
    }

    /// The Reporter's current recap window.
    pub fn recap_window(&self) -> Option<crate::data::daily_recap::RecapWindow> {
        self.agent_markers.recap.current.clone()
    }

    pub fn recap_unread(&self) -> bool {
        self.agent_markers.recap.is_unread()
    }

    /// `markDailyRecapSeen`: its page is on screen.
    pub fn mark_recap_seen(&mut self, cx: &mut Context<Self>) {
        if self.agent_markers.recap.mark_seen() {
            cx.notify();
        }
    }

    /// `RECAP_BLOCKING_REASONS`: what blocks the person now (an unread result
    /// is "moved" instead).
    pub fn recap_blocked(&self) -> Vec<dcc_core::domain::attention::AttentionItem> {
        use dcc_core::domain::attention::AttentionReason as R;
        self.state
            .attention()
            .into_iter()
            .filter(|item| {
                matches!(
                    item.reason,
                    R::Permission
                        | R::Input
                        | R::Conflicts
                        | R::PrConflicts
                        | R::ChecksFailing
                        | R::DelegatedReview
                        | R::Setup
                )
            })
            .collect()
    }

    /// A listed task's project id.
    pub fn task_project(&self, id: &str) -> Option<String> {
        self.state.task(id).map(|task| task.project_id.clone())
    }

    pub fn set_active_agent(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        self.active_agent_id = id;
        cx.notify();
    }
}

fn state_text(state: AgentState, cx: &Context<Sidebar>) -> SharedString {
    cx.t(state.i18n_key())
}

/// `amber-500` for "needs you", `sky-500` for an unread result.
fn dot_color(state: AgentState) -> Option<Hsla> {
    match state {
        AgentState::NeedsYou => Some(tw::AMBER_500.hsla()),
        AgentState::Done => Some(tw::SKY_500.hsla()),
        _ => None,
    }
}

fn open(
    id: String,
    cx: &mut Context<Sidebar>,
) -> impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static {
    cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::OpenAgent(id.clone())))
}

pub fn render(sidebar: &Sidebar, cx: &mut Context<Sidebar>) -> Option<AnyElement> {
    let views = sidebar.agent_views();
    if views.is_empty() {
        return None;
    }
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let radius = theme.radius;
    let dark = theme.appearance == Appearance::Dark;
    let ring = colors.sidebar;
    let mut buttons = Vec::new();
    for view in &views {
        let agent = view.agent;
        let active = sidebar.active_agent_id.as_deref() == Some(agent.id.as_str());
        let hover = colors.accent.opacity(0.5);
        let tip = if view.state == AgentState::Idle {
            SharedString::from(agent.name.clone())
        } else {
            SharedString::from(format!("{} · {}", agent.name, state_text(view.state, cx)))
        };
        let button = div()
            .id(SharedString::from(format!("agent-{}", agent.id)))
            .relative()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(32.0))
            .rounded(radius.lg)
            .cursor_pointer()
            .when(active, |button| button.bg(colors.accent))
            .when(!active, |button| button.hover(move |style| style.bg(hover)))
            .on_click(open(agent.id.clone(), cx))
            .child(face_piece(sidebar, &agent.id, false, cx))
            .when_some(dot_color(view.state), |button, dot| {
                button.child(
                    div()
                        .absolute()
                        .right(px(2.0))
                        .top(px(2.0))
                        .size(px(8.0))
                        .rounded_full()
                        .border_2()
                        .border_color(ring)
                        .bg(dot),
                )
            });
        buttons.push(
            Tooltip::text(
                SharedString::from(format!("tip-agent-{}", agent.id)),
                button,
                tip,
            )
            .side(Side::Bottom)
            .into_any_element(),
        );
    }
    let speaker = lead(&views, |view| view.state).map(|view| {
        let color = match (view.state, dark) {
            (AgentState::NeedsYou, false) => tw::AMBER_700.hsla(),
            (AgentState::NeedsYou, true) => tw::AMBER_300.hsla(),
            (AgentState::Done, false) => tw::SKY_700.hsla(),
            (AgentState::Done, true) => tw::SKY_300.hsla(),
            _ => colors.muted_foreground,
        };
        let text = format!(
            "{}: {}",
            view.agent.name,
            state_text(view.state, cx).to_lowercase()
        );
        div()
            .id("agents-lead")
            .ml(px(4.0))
            .min_w_0()
            .truncate()
            .cursor_pointer()
            .text_size(px(11.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(color)
            .on_click(open(view.agent.id.clone(), cx))
            .child(SharedString::from(text))
    });
    Some(
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(2.0))
            .px(px(10.0))
            .pb(px(12.0))
            .children(buttons)
            .children(speaker)
            .into_any_element(),
    )
}

/// The collapsed rail: a ghost button per agent, its name beside it.
pub fn rail_buttons(sidebar: &Sidebar, cx: &mut Context<Sidebar>) -> Vec<AnyElement> {
    let views = sidebar.agent_views();
    let mut buttons = Vec::new();
    for view in &views {
        let agent = view.agent;
        let active = sidebar.active_agent_id.as_deref() == Some(agent.id.as_str());
        let button = Button::new(SharedString::from(format!("rail-agent-{}", agent.id)))
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::IconXs)
            .current(active)
            .child(face_piece(sidebar, &agent.id, true, cx))
            .on_click(open(agent.id.clone(), cx));
        buttons.push(
            Tooltip::text(
                SharedString::from(format!("tip-rail-agent-{}", agent.id)),
                button,
                agent.name.clone(),
            )
            .side(Side::Right)
            .into_any_element(),
        );
    }
    buttons
}

fn face_size(rail: bool) -> f32 {
    if rail { 20.0 } else { 22.0 }
}

/// The agent's face in its own piece: its bob while it works repaints only it.
fn face_piece(
    sidebar: &Sidebar,
    agent_id: &str,
    rail: bool,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let side = px(face_size(rail));
    sidebar.pieces.element(
        super::piece::Piece::AgentFace {
            agent_id: agent_id.to_string(),
            rail,
        },
        gpui::StyleRefinement::default().size(side).flex_shrink_0(),
        cx,
    )
}

/// The agent's face, as its piece draws it.
pub(super) fn face(
    sidebar: &Sidebar,
    agent_id: &str,
    rail: bool,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let views = sidebar.agent_views();
    let Some(view) = views.iter().find(|view| view.agent.id == agent_id) else {
        return div().into_any_element();
    };
    let mood = super::mascot::agent_mood(view.agent, view.state, &sidebar.pr_reviews, cx);
    let prefix = if rail {
        "rail-agent-face"
    } else {
        "agent-face"
    };
    crate::agent::MoodAvatarView::new(
        SharedString::from(format!("{prefix}-{agent_id}")),
        &view.agent.avatar,
        mood,
        face_size(rail),
    )
    .into_any_element()
}
