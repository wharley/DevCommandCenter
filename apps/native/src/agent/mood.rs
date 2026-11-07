//! The mascot as the agent itself is (the sidebar's strip, the agent's
//! page), not one of its sessions: asleep while nothing wakes it, watching
//! once a trigger is on — blinking now and then — reading while it reads a
//! pull request, and the session states otherwise (working, needs you,
//! result ready).
//!
//! What moves costs little: watching swaps its face for a blink a few times
//! a minute (`MascotBlink`, only while DCC is in front); reading looks
//! around only while it reads, at the loops' capped rate; asleep is still.

use std::time::Duration;

use dcc_core::domain::agent::AgentAvatar;
use gpui::{App, Global, Window, div, img, prelude::*, px};

use super::avatar::{Face, badge, draw, eyes, image};
use crate::data::agents::AgentState;

/// Where the eyes look while reading, in a cycle (left to right and back).
const LOOKS: [f32; 6] = [-3.0, -1.5, 0.0, 1.5, 3.0, 0.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mood {
    /// Nothing wakes it: no trigger on, nothing going on.
    Asleep,
    /// A trigger is on and nothing is going on.
    Watching,
    /// It reads a pull request (a review in the background).
    Reading,
    Working,
    NeedsYou,
    Done,
}

impl Mood {
    /// The agent's state, with what only the agent knows: whether a
    /// trigger is on and whether it is reading.
    pub fn of(state: AgentState, watching: bool, reading: bool) -> Self {
        match state {
            AgentState::NeedsYou => Self::NeedsYou,
            AgentState::Done => Self::Done,
            AgentState::Working if reading => Self::Reading,
            AgentState::Working => Self::Working,
            AgentState::Idle if watching => Self::Watching,
            AgentState::Idle => Self::Asleep,
        }
    }

    /// `agents.mood.*`
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Asleep => "agents.mood.asleep",
            Self::Watching => "agents.mood.watching",
            Self::Reading => "agents.mood.reading",
            Self::Working => "agents.state.working",
            Self::NeedsYou => "agents.state.needsYou",
            Self::Done => "agents.state.done",
        }
    }
}

/// Open eyes looking `dx` across and `dy` down.
fn looking(dx: f32, dy: f32) -> String {
    format!(
        r##"<ellipse cx="39" cy="46" rx="7.5" ry="8.5" fill="#fff"/><ellipse cx="61" cy="46" rx="7.5" ry="8.5" fill="#fff"/><circle cx="{}" cy="{}" r="3.8" fill="#0b1220"/><circle cx="{}" cy="{}" r="3.8" fill="#0b1220"/>"##,
        39.0 + dx,
        46.0 + dy,
        61.0 + dx,
        46.0 + dy
    )
}

/// Closed eyes, curved down, and two small "z" over the head.
fn sleeping() -> (String, String) {
    (
        r##"<path d="M32 45q7 6 14 0M54 45q7 6 14 0" fill="none" stroke="#0b1220" stroke-width="4.5" stroke-linecap="round"/>"##.to_string(),
        r##"<path d="M78 14h8l-8 9h8M88 4h5l-5 6h5" fill="none" stroke="#94a3b8" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>"##.to_string(),
    )
}

/// The face for `mood`; `blink` closes a watching agent's eyes, `look` is
/// where a reading agent's eyes are.
pub fn mood_svg(avatar: &AgentAvatar, mood: Mood, blink: bool, look: f32) -> String {
    let state = match mood {
        Mood::NeedsYou => Some(AgentState::NeedsYou),
        Mood::Done => Some(AgentState::Done),
        _ => None,
    };
    let (face_eyes, extra, opacity) = match mood {
        Mood::Asleep => {
            let (eyes, z) = sleeping();
            (eyes, z, 0.6)
        }
        Mood::Watching if blink => (eyes("resting"), String::new(), 1.0),
        Mood::Watching => (looking(3.0, 0.0), String::new(), 1.0),
        Mood::Reading => (looking(look, 3.0), String::new(), 1.0),
        Mood::Working => (eyes("focus"), String::new(), 1.0),
        Mood::NeedsYou => (eyes("round"), String::new(), 1.0),
        Mood::Done => (eyes("smile"), String::new(), 1.0),
    };
    draw(
        avatar,
        Face {
            eyes: face_eyes,
            opacity,
            raised_arm: mood == Mood::NeedsYou,
            badge: badge(state),
            extra,
        },
    )
}

/// The blink every watching mascot shows for a moment.
#[derive(Clone, Copy, Debug, Default)]
pub struct MascotBlink(pub bool);

impl Global for MascotBlink {}

/// Whether watching mascots have their eyes closed this instant.
pub fn blinking(cx: &App) -> bool {
    cx.try_global::<MascotBlink>().is_some_and(|blink| blink.0)
}

#[derive(IntoElement)]
pub struct MoodAvatarView {
    id: gpui::ElementId,
    avatar: AgentAvatar,
    mood: Mood,
    size: f32,
}

impl MoodAvatarView {
    pub fn new(
        id: impl Into<gpui::ElementId>,
        avatar: &AgentAvatar,
        mood: Mood,
        size: f32,
    ) -> Self {
        Self {
            id: id.into(),
            avatar: avatar.clone(),
            mood,
            size,
        }
    }
}

impl RenderOnce for MoodAvatarView {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let size = self.size;
        let (avatar, mood) = (self.avatar, self.mood);
        let face = move |avatar: &AgentAvatar, look: f32, blink: bool| {
            img(image(mood_svg(avatar, mood, blink, look)))
                .size(px(size))
                .flex_shrink_0()
        };
        match mood {
            // Looks along the lines, a step at a time.
            Mood::Reading => crate::ui::animated(
                div().relative().size(px(size)),
                self.id,
                crate::ui::looping(Duration::from_millis(2400)),
                cx,
                move |frame, delta| {
                    let step = ((delta * LOOKS.len() as f32) as usize).min(LOOKS.len() - 1);
                    frame.child(face(&avatar, LOOKS[step], false))
                },
            ),
            // `dcc-agent-bob`, as a working session's mascot.
            Mood::Working => crate::ui::animated(
                div()
                    .relative()
                    .size(px(size))
                    .child(face(&avatar, 0.0, false)),
                self.id,
                crate::ui::looping(Duration::from_millis(1100)),
                cx,
                move |frame, delta| {
                    let wave = (1.0 - (delta * std::f32::consts::TAU).cos()) / 2.0;
                    frame.top(px(size * (0.06 - 0.14 * wave)))
                },
            ),
            _ => div()
                .relative()
                .size(px(size))
                .child(face(&avatar, 0.0, blinking(cx)))
                .into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn avatar() -> AgentAvatar {
        AgentAvatar {
            shape: "octopus".to_string(),
            accessory: "none".to_string(),
            color: "violet".into(),
            arms: 3,
            eyes: "round".into(),
        }
    }

    #[test]
    fn the_mood_follows_the_agent_its_triggers_and_its_reading() {
        assert_eq!(Mood::of(AgentState::Idle, false, false), Mood::Asleep);
        assert_eq!(Mood::of(AgentState::Idle, true, false), Mood::Watching);
        assert_eq!(Mood::of(AgentState::Working, true, true), Mood::Reading);
        assert_eq!(Mood::of(AgentState::Working, false, false), Mood::Working);
        assert_eq!(Mood::of(AgentState::NeedsYou, true, true), Mood::NeedsYou);
        assert_eq!(Mood::of(AgentState::Done, false, true), Mood::Done);

        let asleep = mood_svg(&avatar(), Mood::Asleep, false, 0.0);
        assert!(asleep.contains("M78 14h8"), "the z over its head");
        assert!(asleep.contains(r##"opacity="0.6""##));
        let open = mood_svg(&avatar(), Mood::Watching, false, 0.0);
        let shut = mood_svg(&avatar(), Mood::Watching, true, 0.0);
        assert_ne!(open, shut, "a blink closes the eyes");
        assert_ne!(
            mood_svg(&avatar(), Mood::Reading, false, -3.0),
            mood_svg(&avatar(), Mood::Reading, false, 3.0)
        );
        for mood in [
            Mood::Asleep,
            Mood::Watching,
            Mood::Reading,
            Mood::Working,
            Mood::NeedsYou,
            Mood::Done,
        ] {
            let svg = mood_svg(&avatar(), mood, false, 0.0);
            assert!(
                resvg::usvg::Tree::from_str(&svg, &Default::default()).is_ok(),
                "{mood:?}"
            );
        }
    }
}
