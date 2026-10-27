//! "Ajuda" (`HelpDialog`): the topics on the left with a search, the
//! chosen one on the right (what it is, when to use it, the steps, a
//! prompt to try, a tip). It opens on a topic when the caller names one
//! ("Como funciona" opens "workspaces"); the footer opens the shortcuts.

#[cfg(test)]
mod tests;
mod topics;
mod view;

use gpui::{AppContext, Context, Entity, EventEmitter, Subscription, Window};

use topics::TopicCopy;
pub use topics::{DEFAULT_TOPIC, TOPICS};

use crate::i18n::I18n;
use crate::ui::text_input::{TextInput, TextInputEvent};

pub enum HelpEvent {
    Closed,
    /// "Atalhos de teclado": this closes and the shortcuts open.
    OpenShortcuts,
}

pub struct Help {
    input: Entity<TextInput>,
    topics: Vec<TopicCopy>,
    active: &'static str,
    _input_events: Subscription,
}

impl EventEmitter<HelpEvent> for Help {}

impl Help {
    /// `topic` is one of [`TOPICS`]; anything else opens the default.
    pub fn new(topic: Option<&str>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let i18n = cx.global::<I18n>();
        let topics: Vec<TopicCopy> = TOPICS.iter().map(|id| TopicCopy::read(id, i18n)).collect();
        let placeholder = i18n.t("help.searchPlaceholder");
        let input = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
        let events = cx.subscribe(&input, |help, _, event, cx| match event {
            TextInputEvent::Changed => help.follow_search(cx),
            TextInputEvent::Cancelled => cx.emit(HelpEvent::Closed),
            TextInputEvent::Submitted | TextInputEvent::Blurred => {}
        });
        let active = TOPICS
            .iter()
            .copied()
            .find(|id| Some(*id) == topic)
            .unwrap_or(DEFAULT_TOPIC);
        Self {
            input,
            topics,
            active,
            _input_events: events,
        }
    }

    pub fn input(&self) -> Entity<TextInput> {
        self.input.clone()
    }

    fn visible(&self, cx: &gpui::App) -> Vec<&TopicCopy> {
        let query = self.input.read(cx).text();
        self.topics
            .iter()
            .filter(|topic| topic.matches(query))
            .collect()
    }

    /// The card follows the list: a search never leaves a topic on the
    /// right that is missing on the left.
    fn follow_search(&mut self, cx: &mut Context<Self>) {
        let first = {
            let visible = self.visible(cx);
            (!visible.iter().any(|topic| topic.id == self.active))
                .then(|| visible.first().map(|topic| topic.id))
                .flatten()
        };
        if let Some(first) = first {
            self.active = first;
        }
        cx.notify();
    }

    fn select(&mut self, topic: &'static str, cx: &mut Context<Self>) {
        self.active = topic;
        cx.notify();
    }
}
