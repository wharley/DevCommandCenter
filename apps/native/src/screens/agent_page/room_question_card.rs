//! The question card docked on the room's field (`room_question`): the
//! agent's face and "Dev pergunta", one decision at a time ("1 de 3") with
//! the agent's options and "Outra coisa", then a short review of the answers
//! and "Enviar". Calm on purpose: the same amber edge as the approvals it
//! sits above, one decision on screen, nothing that scrolls the list.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use super::room_question::RoomQuestion;
use super::room_question_steps::{Pick, QuestionDraft};
use crate::agent::AgentAvatarView;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant};

impl TeamRoom {
    /// The oldest question waiting, with how many more wait behind it.
    pub(super) fn question_card(
        &self,
        mut questions: Vec<RoomQuestion>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if questions.is_empty() {
            return None;
        }
        let more = questions.len() - 1;
        let question = questions.remove(0);
        let colors = cx.theme().colors.clone();
        let draft = self.questions.draft(&question);
        let request = question.request.request_id.clone();
        let total = draft.total();
        let sending = self.questions.sending.contains(&request);
        let reviewing = draft.reviewing();
        let step = draft.step.min(total.saturating_sub(1));
        let answered = |index: usize| {
            draft
                .answer(index, &self.questions.other_text(&request, index, cx))
                .is_some()
        };
        let ready = if reviewing {
            (0..total).all(answered)
        } else {
            answered(step)
        };
        let head = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(div().flex_none().child(AgentAvatarView::new(
                "room-question-face",
                &question.agent.avatar,
                None,
                22.0,
            )))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .child(cx.t_with(
                        "agents.room.question.asks",
                        &Args::default().with("agent", question.agent.name.clone()),
                    )),
            )
            .when(total > 1 && !reviewing, |head| {
                head.child(
                    div()
                        .debug_selector(|| "room-question-step".into())
                        .flex_none()
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .child(
                            cx.t_with(
                                "agents.room.question.step",
                                &Args::default()
                                    .with("current", (step + 1).to_string())
                                    .with("total", total.to_string()),
                            ),
                        ),
                )
            });
        let body = if reviewing {
            self.question_review(&question, &draft, cx)
        } else {
            self.question_step(&question, &draft, step, sending, cx)
        };
        // The main button: "Próxima", "Revisar" before the review, or
        // "Enviar" (a single decision sends from its own step).
        let (label, sends) = if reviewing || total == 1 {
            (cx.t("agents.room.question.send"), true)
        } else if step + 1 == total {
            (cx.t("agents.room.question.review"), false)
        } else {
            (cx.t("agents.room.question.next"), false)
        };
        let label = if sending {
            cx.t("agents.room.question.sending")
        } else {
            label
        };
        let (main_question, back_question) = (question.clone(), question.clone());
        let actions = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .when(draft.step > 0, |row| {
                row.child(
                    Button::new("room-question-back")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .disabled(sending)
                        .child(cx.t("agents.room.question.back"))
                        .on_click(cx.listener(move |room, _, _, cx| {
                            room.step_question(&back_question, false, cx)
                        })),
                )
            })
            .child(
                Button::new("room-question-main")
                    .variant(ButtonVariant::Default)
                    .size(ButtonSize::Sm)
                    .disabled(sending || !ready)
                    .child(label)
                    .on_click(cx.listener(move |room, _, _, cx| {
                        if sends {
                            room.send_question(&main_question, cx);
                        } else {
                            room.step_question(&main_question, true, cx);
                        }
                    })),
            );
        Some(
            div()
                .debug_selector(|| "room-question".into())
                .flex()
                .flex_col()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(10.0))
                .child(head)
                .child(body)
                .child(actions)
                .when(more > 0, |card| {
                    card.child(
                        div()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(
                                cx.t_with("agents.room.question.more", &Args::count(more as i64)),
                            ),
                    )
                })
                .into_any_element(),
        )
    }

    /// One decision: its header, the question, the agent's options and
    /// "Outra coisa" with its field once picked.
    fn question_step(
        &self,
        question: &RoomQuestion,
        draft: &QuestionDraft,
        index: usize,
        sending: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let amber = crate::theme::tailwind::AMBER_500.hsla();
        let asked = &question.request.questions[index];
        let picked = draft.picks.get(index).cloned().flatten();
        let choice = |id: String, label: SharedString, detail: Option<String>, on: bool| {
            div()
                .id(SharedString::from(id.clone()))
                .debug_selector(move || id.clone())
                .flex()
                .items_start()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(if on {
                    amber.opacity(0.8)
                } else {
                    colors.border
                })
                .bg(if on {
                    amber.opacity(0.1)
                } else {
                    colors.background
                })
                .when(!sending, |row| row.cursor_pointer())
                .child(
                    div()
                        .flex_none()
                        .mt(px(3.0))
                        .size(px(10.0))
                        .rounded_full()
                        .border_1()
                        .border_color(if on { amber } else { colors.muted_foreground })
                        .when(on, |dot| dot.bg(amber)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(colors.foreground)
                                .child(label),
                        )
                        .children(detail.filter(|detail| !detail.trim().is_empty()).map(
                            |detail| {
                                div()
                                    .text_size(px(11.5))
                                    .text_color(colors.muted_foreground)
                                    .child(detail)
                            },
                        )),
                )
        };
        let mut options: Vec<AnyElement> = Vec::new();
        for (option_index, option) in asked.options.iter().enumerate() {
            let on = picked.as_ref() == Some(&Pick::Option(option.label.clone()));
            let (target, label) = (question.clone(), option.label.clone());
            options.push(
                choice(
                    format!("room-question-option-{option_index}"),
                    SharedString::from(option.label.clone()),
                    Some(option.description.clone()),
                    on,
                )
                .when(!sending, |row| {
                    row.on_click(cx.listener(move |room, _, window, cx| {
                        room.pick_answer(&target, index, Pick::Option(label.clone()), window, cx)
                    }))
                })
                .into_any_element(),
            );
        }
        let other_on = picked == Some(Pick::Other);
        let target = question.clone();
        options.push(
            choice(
                "room-question-other".into(),
                cx.t("agents.room.question.other"),
                None,
                other_on,
            )
            .when(!sending, |row| {
                row.on_click(cx.listener(move |room, _, window, cx| {
                    room.pick_answer(&target, index, Pick::Other, window, cx)
                }))
            })
            .into_any_element(),
        );
        let field = other_on
            .then(|| {
                self.questions
                    .other_field(&question.request.request_id, index)
            })
            .flatten()
            .map(|input| {
                div()
                    .debug_selector(|| "room-question-field".into())
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(colors.input)
                    .bg(colors.background)
                    .child(input)
            });
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .when(!asked.header.trim().is_empty(), |step| {
                step.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .child(asked.header.clone()),
                )
            })
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(colors.foreground)
                    .child(asked.question.clone()),
            )
            .children(options)
            .children(field)
            .into_any_element()
    }

    /// "Suas respostas": each decision and what it got, one line each.
    fn question_review(
        &self,
        question: &RoomQuestion,
        draft: &QuestionDraft,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let request = &question.request.request_id;
        let rows: Vec<AnyElement> = question
            .request
            .questions
            .iter()
            .enumerate()
            .map(|(index, asked)| {
                let answer = draft
                    .answer(index, &self.questions.other_text(request, index, cx))
                    .unwrap_or_default();
                let what = if asked.header.trim().is_empty() {
                    asked.question.clone()
                } else {
                    asked.header.clone()
                };
                div()
                    .flex()
                    .gap(px(6.0))
                    .text_size(px(12.5))
                    .child(
                        div()
                            .flex_none()
                            .max_w(px(220.0))
                            .truncate()
                            .text_color(colors.muted_foreground)
                            .child(what),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(colors.foreground)
                            .child(answer),
                    )
                    .into_any_element()
            })
            .collect();
        div()
            .debug_selector(|| "room-question-review".into())
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("agents.room.question.reviewTitle")),
            )
            .children(rows)
            .into_any_element()
    }
}
