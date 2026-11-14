//! A task's delivery in the room (`dcc_core::domain::task_shipped`): the
//! message DCC sent the agent the work started with reads as one line —
//! "Entrega · Corrigir CEP · em produção" — and the agent's answer under it
//! closes the subject with the person.

use gpui::{AnyElement, Context, div, prelude::*, px};

use super::room::TeamRoom;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::Icon;

impl TeamRoom {
    /// The delivery of `title`, as it stands (`live`, `merged`,
    /// `deploy failed`).
    pub(super) fn shipped_row(
        &self,
        index: usize,
        title: &str,
        state: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let (key, tone, icon) = match state {
            "live" => ("agents.room.shipped.live", colors.success, "rocket"),
            "deploy failed" => (
                "agents.room.shipped.deployFailed",
                colors.destructive,
                "triangle-alert",
            ),
            _ => (
                "agents.room.shipped.merged",
                colors.muted_foreground,
                "git-merge",
            ),
        };
        let text = cx.t_with(key, &Args::default().with("task", title.to_string()));
        div()
            .debug_selector(move || format!("room-shipped-{index}"))
            .flex()
            .justify_center()
            .child(
                div()
                    .max_w(px(560.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(px(10.0))
                    .bg(tone.opacity(0.1))
                    .text_size(px(11.5))
                    .text_color(tone)
                    .child(Icon::new(icon).size(12.0).color(tone))
                    .child(div().truncate().child(text)),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use dcc_core::domain::task_origin::TaskOrigin;
    use dcc_core::domain::task_shipped::{Outcome, shipped_prompt};
    use gpui::TestAppContext;

    use super::super::room_list_tests::room_with_lines;
    use crate::data::agents::AgentState;
    use crate::data::team_room::{RoomLine, Speaker};

    #[gpui::test]
    fn a_delivery_is_one_line_not_the_persons_words(cx: &mut TestAppContext) {
        let line = |index: usize, speaker: Speaker, text: String| RoomLine {
            key: format!("l-{index}"),
            agent_id: "dev".into(),
            session_id: "s-geral".into(),
            speaker,
            text,
            at: format!("2026-11-10T10:0{index}:00Z"),
            receipt: None,
            tasks: Vec::new(),
            steps: Default::default(),
        };
        let told = shipped_prompt(
            "Corrigir CEP",
            "#1014",
            &Outcome::Live {
                environment: "Production".into(),
                at: None,
            },
            &TaskOrigin::default(),
        );
        let (_page, room, cx) = room_with_lines(
            cx,
            vec![
                line(0, Speaker::Person, told),
                line(
                    1,
                    Speaker::Agent,
                    "O ajuste do CEP está em produção.".into(),
                ),
            ],
            AgentState::Idle,
        );
        assert!(cx.debug_bounds("room-shipped-0").is_some());
        assert!(cx.debug_bounds("room-said-0").is_none());
        assert!(cx.debug_bounds("room-answer-1").is_some());
        // The search finds it by the task's name.
        let model = room.read_with(cx, |room, _| room.list.model());
        let texts = super::super::room_find::texts(&model);
        assert!(
            texts
                .iter()
                .any(|(id, text)| id == "l-0" && text == "Corrigir CEP")
        );
    }
}
