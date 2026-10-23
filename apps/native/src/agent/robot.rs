//! `AgentMascot`: the agent's pixel robot beside its model label in the
//! conversation (`@dcc/mascots` agent poses). It faces you while idle, walks
//! while the agent works and waves when it needs you.

use gpui::{App, Hsla, IntoElement, RenderOnce, Window, div, prelude::*, px, rgb};

use crate::project::{MascotRows, sprite};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RobotState {
    Idle,
    Working,
    Waiting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RobotKey {
    Claude,
    Codex,
    Gemini,
    Cursor,
    Droid,
    Grok,
    Generic,
}

fn brand(value: Option<&str>) -> Option<RobotKey> {
    let value = value?.to_lowercase();
    let has = |needle: &str| value.contains(needle);
    Some(if has("claude") || has("anthropic") {
        RobotKey::Claude
    } else if has("codex") || has("openai") {
        RobotKey::Codex
    } else if has("gemini") || has("google") || has("antigravity") {
        RobotKey::Gemini
    } else if has("grok") || has("xai") {
        RobotKey::Grok
    } else if has("cursor") {
        RobotKey::Cursor
    } else if has("droid") || has("factory") {
        RobotKey::Droid
    } else {
        return None;
    })
}

/// `resolveAgentMascotKey`: the provider wins, then the model.
pub fn robot_key(provider: Option<&str>, model: Option<&str>) -> RobotKey {
    brand(provider)
        .or_else(|| brand(model))
        .unwrap_or(RobotKey::Generic)
}

fn antenna(key: RobotKey) -> [&'static str; 2] {
    match key {
        RobotKey::Claude => ["..+.+.+...", "...###...."],
        RobotKey::Codex => ["....++....", ".....#...."],
        RobotKey::Gemini => ["....+.....", "...+#+...."],
        RobotKey::Cursor => ["...+++....", "....#....."],
        RobotKey::Droid => ["..+...+...", "...#.#...."],
        RobotKey::Grok => [".....+....", "....#....."],
        RobotKey::Generic => ["....+.....", "....#....."],
    }
}

const HEAD: [&str; 3] = ["..######..", "..####.#..", "..######.."];

#[derive(Clone, Copy)]
enum Pose {
    Front,
    FrontHandUp,
    FrontHandHigh,
    Stand,
    Stride,
}

fn pose(pose: Pose) -> [&'static str; 8] {
    match pose {
        Pose::Front => [
            "..######..",
            "..#.##.#..",
            "..######..",
            "..+####+..",
            "..+####+..",
            "...####...",
            "...#..#...",
            "..##..##..",
        ],
        Pose::FrontHandUp => [
            "..######..",
            "..#.##.#.+",
            "..######+.",
            "..+####+..",
            "..+####...",
            "...####...",
            "...#..#...",
            "..##..##..",
        ],
        Pose::FrontHandHigh => [
            "..######.+",
            "..#.##.#.+",
            "..######+.",
            "..+####+..",
            "..+####...",
            "...####...",
            "...#..#...",
            "..##..##..",
        ],
        Pose::Stand => [
            HEAD[0],
            HEAD[1],
            HEAD[2],
            "...#+##...",
            "...#+##...",
            "...####...",
            "...#..#...",
            "...##.##..",
        ],
        Pose::Stride => [
            HEAD[0],
            HEAD[1],
            HEAD[2],
            "...##+#...",
            "...####+..",
            "...####...",
            "..#....#..",
            "..##...##.",
        ],
    }
}

/// The frame to draw: `rest` or, every other beat while active, `move`.
pub fn robot_frame(key: RobotKey, state: RobotState, moving: bool) -> MascotRows {
    let (rest, step) = match state {
        RobotState::Idle => (Pose::Front, Pose::Front),
        RobotState::Working => (Pose::Stand, Pose::Stride),
        RobotState::Waiting => (Pose::FrontHandUp, Pose::FrontHandHigh),
    };
    let top = antenna(key);
    let body = pose(if moving && state != RobotState::Idle {
        step
    } else {
        rest
    });
    [
        top[0], top[1], body[0], body[1], body[2], body[3], body[4], body[5], body[6], body[7],
    ]
}

/// Tile border, background and sprite color for each robot.
fn palette(
    key: RobotKey,
    state: RobotState,
    appearance: Appearance,
    cx: &App,
) -> (Hsla, Hsla, Hsla) {
    let dark = appearance == Appearance::Dark;
    let colors = &cx.theme().colors;
    if state == RobotState::Waiting {
        let border = if dark {
            tw::AMBER_400.hsla()
        } else {
            tw::AMBER_500.hsla()
        };
        let text = if dark {
            tw::AMBER_300.hsla()
        } else {
            tw::AMBER_700.hsla()
        };
        return (border, tw::AMBER_500.alpha(0.15), text);
    }
    let brand = |base: u32, light: u32, dark_text: u32| {
        let base: Hsla = rgb(base).into();
        let text: Hsla = rgb(if dark { dark_text } else { light }).into();
        (base.opacity(0.25), base.opacity(0.12), text)
    };
    match key {
        RobotKey::Claude => brand(0xD97757, 0xC2603F, 0xE8936F),
        RobotKey::Codex => brand(0x3B7BF6, 0x2F6BE0, 0x7AA5FA),
        RobotKey::Gemini => (
            tw::VIOLET_500.alpha(0.25),
            tw::VIOLET_500.alpha(0.12),
            if dark {
                tw::VIOLET_300.hsla()
            } else {
                tw::VIOLET_600.hsla()
            },
        ),
        RobotKey::Generic => (
            colors.border,
            colors.muted.opacity(0.5),
            colors.foreground.opacity(0.6),
        ),
        _ => (
            colors.border,
            colors.muted.opacity(0.5),
            colors.foreground.opacity(0.75),
        ),
    }
}

#[derive(IntoElement)]
pub struct AgentMascot {
    key: RobotKey,
    state: RobotState,
    moving: bool,
}

impl AgentMascot {
    pub fn new(provider: Option<&str>, model: Option<&str>, state: RobotState) -> Self {
        Self {
            key: robot_key(provider, model),
            state,
            moving: false,
        }
    }

    /// The second frame of the beat (the owner flips it every 450ms).
    pub fn moving(mut self, moving: bool) -> Self {
        self.moving = moving;
        self
    }
}

impl RenderOnce for AgentMascot {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let appearance = cx.theme().appearance;
        let (border, background, color) = palette(self.key, self.state, appearance, cx);
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(24.0))
            .rounded(cx.theme().radius.md)
            .border_1()
            .border_color(border)
            .bg(background)
            .child(sprite(
                robot_frame(self.key, self.state, self.moving),
                color,
                appearance,
                20.0,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robots_pick_the_brand_and_walk() {
        assert_eq!(robot_key(Some("claude-code"), None), RobotKey::Claude);
        assert_eq!(
            robot_key(None, Some("gpt-6.1-sol via codex")),
            RobotKey::Codex
        );
        assert_eq!(robot_key(Some("x"), Some("y")), RobotKey::Generic);
        let rest = robot_frame(RobotKey::Codex, RobotState::Working, false);
        let step = robot_frame(RobotKey::Codex, RobotState::Working, true);
        assert_ne!(rest, step);
        assert_eq!(rest[0], "....++....");
        assert!(rest.iter().all(|row| row.len() == 10));
        assert_eq!(
            robot_frame(RobotKey::Claude, RobotState::Idle, true),
            robot_frame(RobotKey::Claude, RobotState::Idle, false)
        );
    }
}
