//! The team's screen ("Agentes" in the sidebar): the agents down the left,
//! each with its mascot as it is and its mood, and "Novo agente"; the page
//! of the one picked in the center. The Sala comes in this column next.

use gpui::{AnyElement, Context, IntoElement, Render, SharedString, Window, div, prelude::*, px};

use super::AgentPage;
use crate::agent::{Mood, MoodAvatarView};
use crate::data::agents::AgentState;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::Icon;

/// The column's width.
const TEAM_WIDTH: f32 = 224.0;

impl AgentPage {
    fn team_row(
        &self,
        index: usize,
        id: String,
        name: String,
        avatar: &dcc_core::domain::agent::AgentAvatar,
        mood: Mood,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let current = self.agent_id == id && self.editor_is_for_agent(cx);
        let mood_text = cx.t(mood.i18n_key());
        let urgent = mood == Mood::NeedsYou;
        div()
            .id(SharedString::from(format!("team-agent-{index}")))
            .debug_selector(move || format!("team-agent-{index}"))
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(8.0))
            .py(px(6.0))
            .rounded(cx.theme().radius.control)
            .cursor_pointer()
            .when(current, |row| row.bg(colors.accent))
            .when(!current, |row| {
                row.hover(move |style| style.bg(colors.accent.opacity(0.5)))
            })
            .on_click(cx.listener(move |page, _, _, cx| page.show(id.clone(), cx)))
            .child(MoodAvatarView::new(
                SharedString::from(format!("team-face-{index}")),
                avatar,
                mood,
                30.0,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(name),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(if urgent {
                                crate::theme::tailwind::AMBER_600.hsla()
                            } else {
                                colors.muted_foreground
                            })
                            .child(mood_text),
                    ),
            )
            .into_any_element()
    }

    /// The agent picked is shown (not "Novo agente" in the editor).
    fn editor_is_for_agent(&self, cx: &gpui::App) -> bool {
        self.editor
            .as_ref()
            .is_none_or(|(editor, _)| !editor.read(cx).is_new())
    }

    fn team_column(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let sidebar = self.sidebar.read(cx);
        let agents: Vec<_> = sidebar
            .agents()
            .iter()
            .map(|agent| {
                let mood = sidebar
                    .mood(&agent.id, cx)
                    .unwrap_or_else(|| Mood::of(AgentState::Idle, false, false));
                (
                    agent.id.clone(),
                    agent.name.clone(),
                    agent.avatar.clone(),
                    mood,
                )
            })
            .collect();
        let rows: Vec<AnyElement> = agents
            .into_iter()
            .enumerate()
            .map(|(index, (id, name, avatar, mood))| {
                self.team_row(index, id, name, &avatar, mood, cx)
            })
            .collect();
        let creating = !self.editor_is_for_agent(cx);
        let new_agent = div()
            .id("team-new-agent")
            .debug_selector(|| "team-new-agent".into())
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(8.0))
            .h(px(34.0))
            .rounded(cx.theme().radius.control)
            .cursor_pointer()
            .text_size(px(13.0))
            .text_color(colors.muted_foreground)
            .when(creating, |row| {
                row.bg(colors.accent).text_color(colors.foreground)
            })
            .when(!creating, |row| {
                row.hover(move |style| {
                    style
                        .bg(colors.accent.opacity(0.5))
                        .text_color(colors.foreground)
                })
            })
            .on_click(cx.listener(|page, _, window, cx| page.open_new_agent(window, cx)))
            .child(
                div()
                    .flex()
                    .size(px(30.0))
                    .items_center()
                    .justify_center()
                    .child(Icon::new("plus").size(16.0)),
            )
            .child(cx.t("agents.team.new"));
        div()
            .id("team-column")
            .debug_selector(|| "team-column".into())
            .flex()
            .flex_col()
            .flex_none()
            .w(px(TEAM_WIDTH))
            .h_full()
            .min_h_0()
            .overflow_y_scroll()
            .gap(px(2.0))
            .px(px(8.0))
            .py(px(16.0))
            .border_r_1()
            .border_color(colors.border.opacity(0.7))
            .child(
                div()
                    .px(px(8.0))
                    .child(AgentPage::section_title(cx.t("agents.team.title"), cx)),
            )
            .children(rows)
            .child(new_agent)
            .into_any_element()
    }
}

impl Render for AgentPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = self.page_body(window, cx);
        div()
            .flex()
            .size_full()
            .min_h_0()
            .bg(cx.theme().colors.background)
            .child(self.team_column(cx))
            .child(div().flex_1().min_w_0().h_full().child(body))
    }
}
