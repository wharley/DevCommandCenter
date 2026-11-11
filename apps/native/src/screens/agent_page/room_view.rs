//! How the team room draws: the conversation (the person's messages to the
//! right with who took them, each answer with its agent's mascot and
//! receipt), who is at work or waiting, and the field with the team's names
//! to address one and who would answer now.

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, Render, SharedString, Window, div, list,
    prelude::*, px,
};

use super::room::TeamRoom;
use crate::agent::AgentAvatarView;
use crate::data::agents::AgentRecord;
use crate::data::team_room::RoomLine;
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::markdown::{MarkdownStyle, render_markdown};
use crate::theme::ActiveTheme;

/// The face's side, and so the column the words line up in.
const FACE: f32 = 32.0;

pub(super) fn english(cx: &gpui::App) -> bool {
    cx.try_global::<I18n>()
        .is_some_and(|i18n| i18n.locale() == Locale::En)
}

impl TeamRoom {
    /// "→ Revisor · pelo papel".
    pub(super) fn routed(
        &self,
        agent: &str,
        text: &str,
        name: &str,
        cx: &gpui::App,
    ) -> SharedString {
        let to = cx.t_with(
            "agents.room.to",
            &Args::default().with("agent", name.to_string()),
        );
        match self.reasons.get(&(agent.to_string(), text.to_string())) {
            Some(reason) => format!("{to} · {}", cx.t(reason.i18n_key())).into(),
            None => to,
        }
    }

    /// An answer as a flat row: the agent's face, bold name and the time on
    /// one line, the words under; the next answers of a group (`first` is
    /// false) sit under the face with the words alone.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn agent_line(
        &self,
        index: usize,
        (first, steps): (bool, Option<&std::rc::Rc<super::room_steps::StepsRow>>),
        line: &RoomLine,
        agent: &AgentRecord,
        model: &super::room_rows::RoomModel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let blocks = self
            .markdown
            .borrow_mut()
            .get(&line.key, &line.text, &model.mentions);
        let when = first.then(|| super::room_flow::clock(&line.at, english(cx)));
        // The row is the hover group of its receipt and reactions.
        let group = SharedString::from(format!("room-answer-g-{index}"));
        div()
            .group(group.clone())
            .debug_selector(move || format!("room-answer-{index}"))
            .flex()
            .items_start()
            .gap(px(10.0))
            .child(match first {
                true => div()
                    .debug_selector(move || format!("room-face-{index}"))
                    .flex_none()
                    .child(AgentAvatarView::new(
                        SharedString::from(format!("room-face-{index}")),
                        &agent.avatar,
                        None,
                        FACE,
                    ))
                    .into_any_element(),
                // The face's column, empty: the words keep their line.
                false => div().w(px(FACE)).flex_none().into_any_element(),
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .children(when.map(|when| {
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(colors.foreground)
                                    .child(agent.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .child(when),
                            )
                    }))
                    .children(steps.map(|steps| self.steps_line(index, line, steps, cx)))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(colors.foreground)
                            .child(render_markdown(
                                &blocks,
                                MarkdownStyle {
                                    size: 13.0,
                                    ..MarkdownStyle::reply()
                                },
                                &format!("room-{}", line.key),
                                window,
                                cx,
                            )),
                    )
                    .child(self.answer_footer(index, line, &group, steps.is_some(), cx))
                    .children(self.report_card(index, line, window, cx))
                    .children(self.rule_field(line, cx)),
            )
            .into_any_element()
    }
}

impl Render for TeamRoom {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        // The rows are read again only when the room's data moved, not on a
        // frame the shared clock alone asked for (`room_list`).
        let scope = self.scope();
        // A conversation at work is read as it goes while this draws.
        self.watch_live(cx);
        if self.list.needs_build(&scope) {
            let model = self.build_rows(cx);
            {
                let mut markdown = self.markdown.borrow_mut();
                markdown.roster(&model.mentions);
                markdown.retain(model.answers());
            }
            self.list.take(model);
        }
        // The questions and approvals wait on the field, not in the list.
        let question = self.question_card(self.questions(&self.list.model().agents), cx);
        let dock = self.approval_dock(self.approvals(&self.list.model().agents), question, cx);
        let whole = self.focus.is_none();
        let unread_bar = self.unread_bar(self.list.model().unread, cx);
        let header = whole.then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .px(px(24.0))
                .pt(px(20.0))
                .pb(px(12.0))
                .border_b_1()
                .border_color(colors.border.opacity(0.7))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(16.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(colors.foreground)
                                .debug_selector(|| "room-title".into())
                                .child(match self.room_name(cx) {
                                    Some(project) => cx.t_with(
                                        "agents.room.projectTitle",
                                        &Args::default().with("project", project),
                                    ),
                                    None => cx.t("agents.room.title"),
                                }),
                        )
                        .children(self.pause_button(cx)),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(colors.muted_foreground)
                        .child(match self.room_name(cx) {
                            Some(project) => cx.t_with(
                                "agents.room.projectSubtitle",
                                &Args::default().with("project", project),
                            ),
                            None => cx.t("agents.room.subtitle"),
                        }),
                )
                .children(unread_bar)
        });
        // Away from the end: back down, with the lines new since.
        let jump = self.jump_button(cx);
        let room = cx.entity();
        div()
            .debug_selector(|| "team-room".into())
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(colors.background)
            // On one agent, its page draws the head.
            .children(header)
            .child(
                div()
                    .id("room-lines")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        list(self.list.state.clone(), move |index, window, cx| {
                            room.update(cx, |room, cx| room.render_item(index, window, cx))
                        })
                        .size_full(),
                    )
                    .children(jump),
            )
            .child(self.composer(dock, window, cx))
    }
}
