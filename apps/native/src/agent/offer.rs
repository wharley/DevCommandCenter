//! `OfferThought`: the mascot thinking out loud. Dots first (900ms), then
//! the question written letter by letter (28ms each), then the answer
//! buttons; the balloon rises from the mascot's head through two small
//! bubbles, and the mascot floats (`dcc-agent-float`, 2.6s).

use std::time::Duration;

use gpui::{
    AnimationExt, Context, EventEmitter, FontWeight, HighlightStyle, IntoElement, Render,
    SharedString, StyledText, Task, Window, div, prelude::*, px,
};

use super::AgentAvatarView;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon};
use dcc_core::domain::agent::AgentAvatar;

const THINKING: Duration = Duration::from_millis(900);
const PER_CHAR: Duration = Duration::from_millis(28);

pub enum OfferEvent {
    Accept,
    Dismiss,
}

pub struct OfferThought {
    id: SharedString,
    name: String,
    avatar: AgentAvatar,
    question: String,
    accept: SharedString,
    /// Characters written; `None` while thinking.
    typed: Option<usize>,
    _typing: Task<()>,
}

impl EventEmitter<OfferEvent> for OfferThought {}

impl OfferThought {
    pub fn new(
        id: impl Into<SharedString>,
        name: String,
        avatar: AgentAvatar,
        question: String,
        accept: SharedString,
        cx: &mut Context<Self>,
    ) -> Self {
        let total = question.chars().count();
        let typing = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(THINKING).await;
            for typed in 0..=total {
                if this
                    .update(cx, |offer, cx| {
                        offer.typed = Some(typed);
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
                if typed < total {
                    cx.background_executor().timer(PER_CHAR).await;
                }
            }
        });
        Self {
            id: id.into(),
            name,
            avatar,
            question,
            accept,
            typed: None,
            _typing: typing,
        }
    }

    pub fn ready(&self) -> bool {
        self.typed
            .is_some_and(|typed| typed >= self.question.chars().count())
    }
}

impl Render for OfferThought {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let edge = colors.border.opacity(0.7);
        let ready = self.ready();
        let id = self.id.clone();
        let words: AnyThought = match self.typed {
            None => AnyThought::Thinking,
            Some(typed) => {
                let written: String = self.question.chars().take(typed).collect();
                AnyThought::Text(format!("{}: {written}", self.name))
            }
        };
        let bold = self.name.len() + 1;
        let thought = match words {
            AnyThought::Thinking => div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .py(px(8.0))
                .pr(px(10.0))
                .children((0..3).map(|dot| {
                    div()
                        .size(px(6.0))
                        .rounded_full()
                        .bg(colors.muted_foreground)
                        .with_animation(
                            SharedString::from(format!("{id}-dot-{dot}")),
                            crate::ui::looping(Duration::from_millis(1000)),
                            move |dot_el, delta| {
                                // `dcc-agent-thinking-dot`, each 150ms later.
                                let phase = (delta - dot as f32 * 0.15).rem_euclid(1.0);
                                let lift = if phase < 0.3 {
                                    phase / 0.3
                                } else if phase < 0.6 {
                                    (0.6 - phase) / 0.3
                                } else {
                                    0.0
                                };
                                dot_el.opacity(0.3 + 0.7 * lift).mt(px(-3.0 * lift))
                            },
                        )
                        .into_any_element()
                }))
                .into_any_element(),
            AnyThought::Text(text) => div()
                .flex()
                .items_end()
                .py(px(4.0))
                .text_size(px(13.0))
                .text_color(colors.foreground)
                .child(StyledText::new(SharedString::from(text)).with_highlights([(
                    0..bold,
                    HighlightStyle {
                        font_weight: Some(FontWeight::MEDIUM),
                        ..Default::default()
                    },
                )]))
                .when(!ready, |line| {
                    line.child(
                        div()
                            .ml(px(1.0))
                            .mb(px(3.0))
                            .w(px(1.5))
                            .h(px(13.0))
                            .bg(colors.foreground)
                            .with_animation(
                                SharedString::from(format!("{id}-caret")),
                                crate::ui::looping(Duration::from_millis(800)),
                                |caret, delta| caret.opacity(if delta < 0.5 { 1.0 } else { 0.0 }),
                            ),
                    )
                })
                .into_any_element(),
        };
        let actions = ready.then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .pl(px(4.0))
                .child(
                    Button::new(SharedString::from(format!("{id}-accept")))
                        .size(ButtonSize::Sm)
                        .text_size(12.0)
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(OfferEvent::Accept)))
                        .child(self.accept.clone()),
                )
                .child(
                    Button::new(SharedString::from(format!("{id}-dismiss")))
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconSm)
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(OfferEvent::Dismiss)))
                        .child(Icon::new("x").size(14.0)),
                )
        });
        let bubble = |size: f32| {
            div()
                .size(px(size))
                .rounded_full()
                .border_1()
                .border_color(edge)
                .bg(colors.card)
        };
        div()
            .debug_selector({
                let id = id.clone();
                move || id.to_string()
            })
            .flex()
            .flex_col()
            .items_end()
            .child(
                div()
                    .flex()
                    .min_h(px(40.0))
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(20.0))
                    .border_1()
                    .border_color(edge)
                    .bg(colors.card)
                    .py(px(6.0))
                    .pl(px(16.0))
                    .pr(px(6.0))
                    .shadow_sm()
                    .child(thought)
                    .children(actions),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(48.0))
                    .items_center()
                    .gap(px(4.0))
                    .py(px(4.0))
                    .child(bubble(10.0))
                    .child(bubble(6.0)),
            )
            .child(
                div().relative().size(px(48.0)).child(
                    div()
                        .absolute()
                        .child(AgentAvatarView::new(
                            SharedString::from(format!("{id}-avatar")),
                            &self.avatar,
                            None,
                            48.0,
                        ))
                        .with_animation(
                            SharedString::from(format!("{id}-float")),
                            crate::ui::looping(Duration::from_millis(2600)),
                            // `dcc-agent-bob`: 6% down → 8% up → 6% down.
                            |frame, delta| {
                                let wave = (1.0 - (delta * std::f32::consts::TAU).cos()) / 2.0;
                                frame.top(px(48.0 * (0.06 - 0.14 * wave)))
                            },
                        ),
                ),
            )
    }
}

enum AnyThought {
    Thinking,
    Text(String),
}
