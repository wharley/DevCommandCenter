//! `@` in the team room's field: the team opens above it, each agent with
//! its mascot, its name and what it does, filtered by what follows the `@`
//! (`composer::mentions::trigger`, like `@arquivo`); ↑↓ move, Enter or Tab
//! pick, Esc closes. The pick becomes "@Nome " in the text, which is what
//! the room's rule reads.

use std::ops::Range;

use gpui::{AnyElement, BoxShadow, Context, FontWeight, SharedString, div, point, prelude::*, px};

use super::room::TeamRoom;
use crate::agent::AgentAvatarView;
use crate::data::agents::{AgentRecord, REVIEWER_PRESET};
use crate::i18n::Translate;
use crate::screens::composer::mentions::trigger;
use crate::theme::ActiveTheme;

/// How much of an agent's role the list shows.
const ROLE_CHARS: usize = 80;

/// The open list.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AgentMention {
    /// The typed `@query` in the field.
    pub(crate) range: Range<usize>,
    /// The agents that match, by id.
    pub(crate) options: Vec<String>,
    pub(crate) selected: usize,
}

/// The agents whose name starts with `query` (any case), then those that
/// contain it.
pub(crate) fn matching(agents: &[AgentRecord], query: &str) -> Vec<String> {
    let query = query.to_lowercase();
    let mut starts = Vec::new();
    let mut contains = Vec::new();
    for agent in agents {
        let name = agent.name.to_lowercase();
        if name.starts_with(&query) {
            starts.push(agent.id.clone());
        } else if name.contains(&query) {
            contains.push(agent.id.clone());
        }
    }
    starts.extend(contains);
    starts
}

/// What the list says an agent does: the Reviewer in the app's words, any
/// other agent by the start of its role.
fn what_it_does(agent: &AgentRecord, cx: &gpui::App) -> SharedString {
    if agent.preset.as_deref() == Some(REVIEWER_PRESET) {
        return cx.t("agents.room.mention.reviewer");
    }
    let role = agent.role.lines().next().unwrap_or_default().trim();
    let mut short: String = role.chars().take(ROLE_CHARS).collect();
    if role.chars().count() > ROLE_CHARS {
        short.push('…');
    }
    short.into()
}

impl TeamRoom {
    /// Opens, filters or closes the list for the `@query` before the caret.
    pub(crate) fn update_mention(&mut self, cx: &mut Context<Self>) {
        let (before, selection) = {
            let input = self.input.read(cx);
            (
                input.text()[..input.cursor()].to_string(),
                input.has_selection(),
            )
        };
        let found = (!selection).then(|| trigger(&before)).flatten();
        let Some((range, query)) = found else {
            return self.close_mention(cx);
        };
        let options = matching(self.sidebar.read(cx).agents(), &query);
        if options.is_empty() {
            return self.close_mention(cx);
        }
        let selected = self
            .mention
            .as_ref()
            .filter(|menu| menu.range.start == range.start)
            .map_or(0, |menu| menu.selected.min(options.len() - 1));
        self.mention = Some(AgentMention {
            range,
            options,
            selected,
        });
        self.input.update(cx, |input, _| input.set_menu_open(true));
        cx.notify();
    }

    pub(crate) fn close_mention(&mut self, cx: &mut Context<Self>) {
        if self.mention.take().is_some() {
            self.input.update(cx, |input, _| input.set_menu_open(false));
            cx.notify();
        }
    }

    pub(crate) fn move_mention(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(menu) = &mut self.mention
            && !menu.options.is_empty()
        {
            let count = menu.options.len() as isize;
            menu.selected = ((menu.selected as isize + delta).rem_euclid(count)) as usize;
            cx.notify();
        }
    }

    /// The highlighted agent, or `index`, becomes "@Nome ".
    pub(crate) fn accept_mention(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        let Some(menu) = self.mention.clone() else {
            return;
        };
        let picked = menu.options.get(index.unwrap_or(menu.selected)).cloned();
        let name = picked.and_then(|id| {
            self.sidebar
                .read(cx)
                .agents()
                .iter()
                .find(|agent| agent.id == id)
                .map(|agent| agent.name.clone())
        });
        self.close_mention(cx);
        if let Some(name) = name {
            let text = format!("@{name} ");
            self.input
                .update(cx, |input, cx| input.replace_range(menu.range, &text, cx));
        }
    }

    pub(super) fn mention_popup(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.mention.as_ref()?;
        let colors = cx.theme().colors.clone();
        let agents = self.sidebar.read(cx).agents().to_vec();
        let rows: Vec<AnyElement> = menu
            .options
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let agent = agents.iter().find(|agent| &agent.id == id)?;
                let selected = index == menu.selected;
                Some(
                    div()
                        .id(SharedString::from(format!("room-mention-{index}")))
                        .debug_selector(move || format!("room-mention-{index}"))
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .px(px(10.0))
                        .py(px(6.0))
                        .rounded(px(8.0))
                        .cursor_pointer()
                        .when(selected, |row| row.bg(colors.muted))
                        .on_mouse_move(cx.listener(move |room, _, _, cx| {
                            if let Some(menu) = &mut room.mention
                                && menu.selected != index
                            {
                                menu.selected = index;
                                cx.notify();
                            }
                        }))
                        .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                            window.prevent_default()
                        })
                        .on_click(
                            cx.listener(move |room, _, _, cx| room.accept_mention(Some(index), cx)),
                        )
                        .child(AgentAvatarView::new(
                            SharedString::from(format!("room-mention-face-{index}")),
                            &agent.avatar,
                            None,
                            24.0,
                        ))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(colors.foreground)
                                        .child(format!("@{}", agent.name)),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(11.0))
                                        .text_color(colors.muted_foreground)
                                        .child(what_it_does(agent, cx)),
                                ),
                        )
                        .into_any_element(),
                )
            })
            .collect();
        let list = div()
            .debug_selector(|| "room-mentions".into())
            .p(px(4.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.background)
            .shadow(vec![BoxShadow {
                color: gpui::black().opacity(0.2),
                offset: point(px(0.0), px(16.0)),
                blur_radius: px(40.0),
                spread_radius: px(-12.0),
                inset: false,
            }])
            .child(
                div()
                    .px(px(8.0))
                    .py(px(4.0))
                    .text_size(px(11.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.muted_foreground)
                    .child(cx.t("agents.room.mention.title")),
            )
            .children(rows);
        Some(
            gpui::deferred(
                div()
                    .absolute()
                    .left_0()
                    .bottom(gpui::relative(1.0))
                    .w(px(360.0))
                    .max_w_full()
                    .pb(px(6.0))
                    .child(list),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, name: &str) -> AgentRecord {
        AgentRecord {
            id: id.into(),
            name: name.into(),
            ..super::super::editor_role::blank_agent()
        }
    }

    #[test]
    fn a_name_that_starts_with_the_query_comes_first() {
        let team = [
            agent("rev", "Revisor"),
            agent("qa", "QA Mobile"),
            agent("res", "Pesquisa"),
        ];
        assert_eq!(matching(&team, ""), ["rev", "qa", "res"]);
        assert_eq!(matching(&team, "r"), ["rev"]);
        assert_eq!(matching(&team, "is"), ["rev", "res"]);
        assert_eq!(matching(&team, "QA"), ["qa"]);
        assert!(matching(&team, "zz").is_empty());
    }
}
