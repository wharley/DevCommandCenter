//! `AgentAvatar`: the mascot drawn from the same SVG the desktop builds, as
//! an image (gradient-free, but built per agent and state). It bobs while
//! the agent works (`dcc-agent-bob`, 1.1s).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use dcc_core::domain::agent::AgentAvatar;
use gpui::{AnimationExt, App, Image, ImageFormat, Window, div, img, prelude::*, px};

use crate::data::agents::AgentState;

const INK: &str = "#0b1220";

fn color(name: &str) -> &'static str {
    match name {
        "cyan" => "#20BEFF",
        "violet" => "#8B5CF6",
        "amber" => "#F2A93B",
        "green" => "#34C28A",
        "pink" => "#F472B6",
        _ => "#356EFF",
    }
}

fn eyes(style: &str) -> String {
    match style {
        "smile" => format!(
            r##"<path d="M32 48q7 -9 14 0M54 48q7 -9 14 0" fill="none" stroke="{INK}" stroke-width="4.5" stroke-linecap="round"/>"##
        ),
        "resting" => format!(
            r##"<path d="M32 46h14M54 46h14" fill="none" stroke="{INK}" stroke-width="4.5" stroke-linecap="round"/>"##
        ),
        "visor" => format!(
            r##"<rect x="27" y="38" width="46" height="16" rx="8" fill="{INK}"/><circle cx="40" cy="46" r="3.2" fill="#fff"/><circle cx="60" cy="46" r="3.2" fill="#fff"/>"##
        ),
        _ => {
            let (dx, y) = if style == "focus" {
                (0.0, 50.0)
            } else {
                (1.5, 47.0)
            };
            format!(
                r##"<ellipse cx="39" cy="46" rx="7.5" ry="8.5" fill="#fff"/><ellipse cx="61" cy="46" rx="7.5" ry="8.5" fill="#fff"/><circle cx="{}" cy="{y}" r="3.8" fill="{INK}"/><circle cx="{}" cy="{y}" r="3.8" fill="{INK}"/>"##,
                39.0 + dx,
                61.0 + dx
            )
        }
    }
}

/// The desktop's SVG, element for element.
pub fn avatar_svg(avatar: &AgentAvatar, state: Option<AgentState>) -> String {
    let fill = color(&avatar.color);
    let arms = avatar.arms.clamp(3, 5) as usize;
    let gap = 3.0;
    let width = (68.0 - gap * (arms as f32 - 1.0)) / arms as f32;
    let eye_style = match state {
        Some(AgentState::Idle) => "resting",
        Some(AgentState::Working) => "focus",
        Some(AgentState::Done) => "smile",
        Some(AgentState::NeedsYou) => "round",
        None => avatar.eyes.as_str(),
    };
    let mut body = String::new();
    if state == Some(AgentState::NeedsYou) {
        body.push_str(&format!(
            r##"<rect x="78" y="14" width="13" height="40" rx="6.5" fill="{fill}" transform="rotate(24 84 52)"/>"##
        ));
    }
    for index in 0..arms {
        let height = if index % 2 == 0 { 26 } else { 20 };
        body.push_str(&format!(
            r##"<rect x="{}" y="56" width="{width}" height="{height}" rx="{}" fill="{fill}"/>"##,
            16.0 + index as f32 * (width + gap),
            width / 2.0
        ));
    }
    body.push_str(&format!(
        r##"<path d="M16 60V52C16 29 32 14 50 14s34 15 34 38v8z" fill="{fill}"/>"##
    ));
    body.push_str(r##"<ellipse cx="36" cy="27" rx="9" ry="5" fill="#fff" opacity="0.22" transform="rotate(-28 36 27)"/>"##);
    body.push_str(&eyes(eye_style));
    let opacity = if state == Some(AgentState::Idle) {
        0.6
    } else {
        1.0
    };
    let badge = match state {
        Some(AgentState::NeedsYou) => format!(
            r##"<circle cx="82" cy="80" r="15" fill="#F2A93B"/><path d="M82 72v9" stroke="{INK}" stroke-width="4" stroke-linecap="round"/><circle cx="82" cy="87" r="2.3" fill="{INK}"/>"##
        ),
        Some(AgentState::Done) => format!(
            r##"<circle cx="82" cy="80" r="15" fill="#34C28A"/><path d="M75.5 80.5l4.5 4.5 8-9" fill="none" stroke="{INK}" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>"##
        ),
        _ => String::new(),
    };
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" viewBox="0 0 100 100"><g opacity="{opacity}">{body}</g>{badge}</svg>"##
    )
}

/// One image per distinct face, kept for the app's life (a handful).
fn image(svg: String) -> Arc<Image> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Image>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut cache = cache.lock().expect("avatar cache");
    cache
        .entry(svg)
        .or_insert_with_key(|svg| {
            Arc::new(Image::from_bytes(ImageFormat::Svg, svg.as_bytes().to_vec()))
        })
        .clone()
}

#[derive(IntoElement)]
pub struct AgentAvatarView {
    id: gpui::ElementId,
    avatar: AgentAvatar,
    state: Option<AgentState>,
    size: f32,
}

impl AgentAvatarView {
    pub fn new(
        id: impl Into<gpui::ElementId>,
        avatar: &AgentAvatar,
        state: Option<AgentState>,
        size: f32,
    ) -> Self {
        Self {
            id: id.into(),
            avatar: avatar.clone(),
            state,
            size,
        }
    }
}

impl RenderOnce for AgentAvatarView {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let size = self.size;
        let face = img(image(avatar_svg(&self.avatar, self.state)))
            .size(px(size))
            .flex_shrink_0();
        let frame = div().relative().size(px(size)).child(face);
        if self.state != Some(AgentState::Working) {
            return frame.into_any_element();
        }
        // `dcc-agent-bob`: 6% down → 8% up → 6% down, eased both ways.
        frame
            .with_animation(
                self.id,
                crate::ui::looping(Duration::from_millis(1100)),
                move |frame, delta| {
                    let wave = (1.0 - (delta * std::f32::consts::TAU).cos()) / 2.0;
                    frame.top(px(size * (0.06 - 0.14 * wave)))
                },
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn avatar(arms: u8) -> AgentAvatar {
        AgentAvatar {
            color: "violet".into(),
            arms,
            eyes: "round".into(),
        }
    }

    #[test]
    fn the_svg_follows_arms_and_state() {
        let idle = avatar_svg(&avatar(4), Some(AgentState::Idle));
        assert_eq!(idle.matches(r##"y="56""##).count(), 4);
        assert!(idle.contains(r##"opacity="0.6""##));
        let waiting = avatar_svg(&avatar(9), Some(AgentState::NeedsYou));
        assert_eq!(
            waiting.matches(r##"y="56""##).count(),
            5,
            "at most five arms"
        );
        assert!(waiting.contains("#F2A93B"));
        assert!(resvg::usvg::Tree::from_str(&waiting, &Default::default()).is_ok());
    }
}
