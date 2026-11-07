//! The agents' mascots as the agents are: the mood each shows (`Mood`) and
//! the blink of the ones watching. The blink is one face swap of 160 ms
//! every few seconds, only while DCC is in front and some agent watches;
//! only the team's screen follows it (`MascotBlink`), never the sidebar.

use std::time::Duration;

use dcc_core::domain::agent::REVIEWER_PRESET;
use gpui::{App, Context, Task};

use super::Sidebar;
use crate::agent::{MascotBlink, Mood};
use crate::data::agent_triggers::AgentTriggers;
use crate::data::agents::{AgentRecord, AgentState};
use crate::data::pr_review_jobs::{JobStatus, PrReviewJob};

/// Between blinks.
const BLINK_EVERY: Duration = Duration::from_millis(4500);
/// How long the eyes stay shut.
const BLINK_FOR: Duration = Duration::from_millis(160);

fn watching(agent: &AgentRecord, cx: &App) -> bool {
    cx.try_global::<AgentTriggers>()
        .is_some_and(|triggers| triggers.any_on(&agent.id))
}

/// The mood of `agent` in `state`: watching with a trigger on, reading
/// while its only work is a pull request review.
pub fn agent_mood(
    agent: &AgentRecord,
    state: AgentState,
    pr_reviews: &[PrReviewJob],
    cx: &App,
) -> Mood {
    let reviewing = agent.preset.as_deref() == Some(REVIEWER_PRESET)
        && pr_reviews
            .iter()
            .any(|job| job.status == JobStatus::Running);
    let session_working = agent
        .sessions
        .iter()
        .any(|session| session.state == AgentState::Working);
    Mood::of(state, watching(agent, cx), reviewing && !session_working)
}

impl Sidebar {
    /// The mood of the agent `id` (the team's screen).
    pub fn mood(&self, id: &str, cx: &App) -> Option<Mood> {
        let views = self.agent_views();
        let view = views.iter().find(|view| view.agent.id == id)?;
        Some(agent_mood(view.agent, view.state, &self.pr_reviews, cx))
    }

    pub(super) fn blink_loop(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            if cfg!(test) {
                return;
            }
            loop {
                cx.background_executor().timer(BLINK_EVERY).await;
                let Ok(blink) = this.update(cx, |sidebar, cx| {
                    crate::data::foreground::is_foreground(cx)
                        && sidebar
                            .state
                            .data
                            .agents
                            .iter()
                            .any(|agent| sidebar.mood(&agent.id, cx) == Some(Mood::Watching))
                }) else {
                    return;
                };
                if !blink {
                    continue;
                }
                for shut in [true, false] {
                    let alive = this.update(cx, |_, cx| cx.set_global(MascotBlink(shut)));
                    if alive.is_err() {
                        return;
                    }
                    if shut {
                        cx.background_executor().timer(BLINK_FOR).await;
                    }
                }
            }
        })
    }
}
