//! `conversation-scroll-jump`: away from the thread's end, a round button
//! over its bottom-right takes the person back down and follows again. A dot
//! says something new arrived meanwhile. While a turn runs it widens into
//! "Trabalhando · 14m 16s", so the work stays in sight however far up the
//! person reads.

use dcc_core::domain::timeline::Role;
use dcc_core::domain::timeline::activity::format_elapsed;
use gpui::{AnyElement, BoxShadow, Context, FollowMode, SharedString, div, point, prelude::*, px};

use super::ConversationView;
use crate::data::time;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Icon, Tooltip, thinking_indicator};

/// `52rem`: the thread's reading column the button sits beside.
const COLUMN: f32 = 832.0;

impl ConversationView {
    /// Whether the person has left the thread's end (the list stopped
    /// following it).
    pub(super) fn away_from_end(&self) -> bool {
        !self.list.is_following_tail()
    }

    /// Back to the end, following it again.
    pub(super) fn jump_to_end(&mut self, cx: &mut Context<Self>) {
        self.new_activity = false;
        self.list.set_follow_mode(FollowMode::Tail);
        cx.notify();
    }

    /// When the running turn started: its assistant row's start, else the
    /// prompt that began it.
    fn running_since(&self) -> Option<i64> {
        let data = self.data.as_ref()?;
        let last = data.rows.last()?;
        last.turn_started_at
            .as_deref()
            .or_else(|| {
                data.rows
                    .iter()
                    .rev()
                    .find(|row| row.role == Role::User)
                    .and_then(|row| row.created_at.as_deref())
            })
            .and_then(time::parse_ms)
    }

    /// The button, or nothing at the end or without a thread.
    pub(super) fn scroll_jump(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let data = self.data.as_ref()?;
        if data.rows.is_empty() || !self.away_from_end() {
            self.new_activity = false;
            return None;
        }
        let running = data.running;
        let colors = cx.theme().colors.clone();
        let label = if self.new_activity {
            cx.t("conversation.newActivity")
        } else {
            cx.t("conversation.scrollToBottom")
        };
        // `right: max(0.75rem, (100cqw - 52rem) / 2 - 2.75rem)`
        let right = ((self.viewport_width - COLUMN) / 2.0 - 44.0).max(12.0);
        let (hover, foreground) = (colors.accent, colors.accent_foreground);
        let button = div()
            .id("conversation-scroll-jump")
            .debug_selector(|| "conversation-scroll-jump".into())
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .h(px(32.0))
            .when(!running, |button| button.w(px(32.0)))
            .when(running, |button| button.pl(px(10.0)).pr(px(8.0)))
            .rounded_full()
            .border_1()
            .border_color(colors.border.opacity(0.75))
            .bg(colors.card)
            .text_color(colors.muted_foreground)
            .shadow(vec![BoxShadow {
                color: gpui::Hsla::black().opacity(0.08),
                offset: point(px(0.0), px(1.0)),
                blur_radius: px(3.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .cursor_pointer()
            .hover(move |style| style.bg(hover).text_color(foreground))
            .on_click(cx.listener(|view, _, _, cx| view.jump_to_end(cx)))
            .when(running, |button| {
                let clock = self
                    .running_since()
                    .map(|since| format_elapsed(self.now_ms() - since));
                let text: SharedString = match clock {
                    Some(clock) => {
                        format!("{} · {clock}", cx.t("conversation.activity.turn.working")).into()
                    }
                    None => cx.t("conversation.activity.turn.working"),
                };
                button
                    .child(thinking_indicator("conversation-scroll-jump-working", 13.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .whitespace_nowrap()
                            .child(text),
                    )
            })
            .child(Icon::new("chevron-down").size(16.0).stroke(2.0))
            // `[data-new-activity]::after`: the calm accent dot.
            .when(self.new_activity, |button| {
                button.child(
                    div()
                        .absolute()
                        .top(px(1.0))
                        .right(px(1.0))
                        .size(px(8.0))
                        .rounded_full()
                        .border_2()
                        .border_color(colors.card)
                        .bg(colors.accent_calm),
                )
            });
        Some(
            div()
                .absolute()
                .bottom(px(12.0))
                .right(px(right))
                .child(Tooltip::text("conversation-scroll-jump-tip", button, label))
                .into_any_element(),
        )
    }
}
