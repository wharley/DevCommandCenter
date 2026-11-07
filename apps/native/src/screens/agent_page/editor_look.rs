//! The mascot part of "Editar …": the shape, the color, what it wears and,
//! on the octopus, its arms; the eyes for every shape. Each tile is the
//! mascot as it would look, so the choice reads at a glance.

use dcc_core::domain::agent::{
    AGENT_AVATAR_ACCESSORIES, AGENT_AVATAR_COLORS, AGENT_AVATAR_EYES, AGENT_AVATAR_MAX_ARMS,
    AGENT_AVATAR_MIN_ARMS, AGENT_AVATAR_SHAPES, AgentAvatar,
};
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::editor::AgentEditor;
use crate::agent::AgentAvatarView;
use crate::theme::ActiveTheme;

impl AgentEditor {
    fn look_choice(
        &self,
        id: String,
        selected: bool,
        avatar: AgentAvatar,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let chosen = avatar.clone();
        div()
            .id(SharedString::from(id.clone()))
            .debug_selector(move || id.clone())
            .flex()
            .size(px(44.0))
            .items_center()
            .justify_center()
            .rounded(px(8.0))
            .border_1()
            .cursor_pointer()
            .map(|tile| {
                if selected {
                    tile.border_color(colors.ring).bg(colors.accent)
                } else {
                    tile.border_color(colors.border.opacity(0.7))
                        .hover(|tile| tile.bg(colors.accent.opacity(0.5)))
                }
            })
            .on_click(cx.listener(move |editor, _, _, cx| {
                editor.avatar = chosen.clone();
                cx.notify();
            }))
            .child(AgentAvatarView::new(
                SharedString::from(format!(
                    "{}-{}-{}-{}-{}",
                    avatar.shape, avatar.color, avatar.arms, avatar.eyes, avatar.accessory
                )),
                &avatar,
                None,
                28.0,
            ))
            .into_any_element()
    }

    /// One row of tiles, each the current mascot with one thing changed.
    fn look_row<T: Copy>(
        &self,
        prefix: &str,
        values: impl IntoIterator<Item = T>,
        label: impl Fn(T) -> String,
        selected: impl Fn(&AgentAvatar, T) -> bool,
        apply: impl Fn(&mut AgentAvatar, T),
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        values
            .into_iter()
            .map(|value| {
                let mut avatar = self.avatar.clone();
                apply(&mut avatar, value);
                self.look_choice(
                    format!("agent-editor-{prefix}-{}", label(value)),
                    selected(&self.avatar, value),
                    avatar,
                    cx,
                )
            })
            .collect()
    }

    /// The four rows: shape, color, accessory, then arms (octopus) and eyes.
    pub(super) fn look_rows(&self, cx: &mut Context<Self>) -> AnyElement {
        let shapes = self.look_row(
            "shape",
            AGENT_AVATAR_SHAPES.iter().copied(),
            str::to_string,
            |avatar, shape| avatar.shape == shape,
            |avatar, shape| avatar.shape = shape.to_string(),
            cx,
        );
        let colors = self.look_row(
            "color",
            AGENT_AVATAR_COLORS.iter().copied(),
            str::to_string,
            |avatar, color| avatar.color == color,
            |avatar, color| avatar.color = color.to_string(),
            cx,
        );
        let accessories = self.look_row(
            "accessory",
            AGENT_AVATAR_ACCESSORIES.iter().copied(),
            str::to_string,
            |avatar, item| avatar.accessory == item,
            |avatar, item| avatar.accessory = item.to_string(),
            cx,
        );
        let mut details = if self.avatar.shape == "octopus" {
            self.look_row(
                "arms",
                AGENT_AVATAR_MIN_ARMS..=AGENT_AVATAR_MAX_ARMS,
                |arms| arms.to_string(),
                |avatar, arms| avatar.arms == arms,
                |avatar, arms| avatar.arms = arms,
                cx,
            )
        } else {
            Vec::new()
        };
        details.extend(self.look_row(
            "eyes",
            AGENT_AVATAR_EYES.iter().copied(),
            str::to_string,
            |avatar, eyes| avatar.eyes == eyes,
            |avatar, eyes| avatar.eyes = eyes.to_string(),
            cx,
        ));
        let row = |tiles: Vec<AnyElement>| div().flex().flex_wrap().gap(px(6.0)).children(tiles);
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(row(shapes))
            .child(row(colors))
            .child(row(accessories))
            .child(row(details))
            .into_any_element()
    }
}
