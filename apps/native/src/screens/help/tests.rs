//! The topics against the desktop's catalogs, the `[Rótulo]` chips, and
//! the dialog driven headlessly: it opens on the asked topic, the search
//! narrows the list and moves the card, a click picks, Esc closes.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, WindowAppearance};

use super::topics::{self, TopicCopy};
use super::{Help, HelpEvent, TOPICS};
use crate::i18n::{I18n, Locale};
use crate::theme::{Theme, ThemePreference};

#[test]
fn every_topic_has_its_copy_and_icon_in_both_languages() {
    for locale in [Locale::PtBr, Locale::En] {
        let i18n = I18n::new(locale);
        for id in TOPICS {
            let copy = TopicCopy::read(id, &i18n);
            assert_ne!(copy.label.as_ref(), format!("help.topics.{id}.label"));
            assert!(!copy.what_is.is_empty(), "{id}");
            assert!(crate::assets::has_icon(topics::icon(id)), "{id}");
        }
    }
    let i18n = I18n::new(Locale::PtBr);
    let workspaces = TopicCopy::read("workspaces", &i18n);
    assert!(workspaces.steps.len() > 1);
    // Some topics have no tip: it reads as nothing, not as the key.
    assert!(
        TOPICS
            .iter()
            .any(|id| TopicCopy::read(id, &i18n).tip.is_none())
    );
}

#[test]
fn the_cards_shortcut_is_the_surfaces_own() {
    let command = crate::ui::command_key();
    assert_eq!(topics::shortcut("workspaces"), Some(vec![command, "K"]));
    assert_eq!(topics::shortcut("search"), Some(vec![command, "P"]));
    assert_eq!(topics::shortcut("plan"), None);
}

#[test]
fn bracketed_labels_become_chips() {
    let (text, chips) = topics::chips("Clique em [Nova tarefa] e depois [Enviar].");
    assert_eq!(text, "Clique em Nova tarefa e depois Enviar.");
    assert_eq!(&text[chips[0].clone()], "Nova tarefa");
    assert_eq!(&text[chips[1].clone()], "Enviar");
    let (text, chips) = topics::chips("um [] vazio e um [aberto");
    assert_eq!(text, "um [] vazio e um [aberto");
    assert!(chips.is_empty());
}

type Events = Rc<RefCell<Vec<&'static str>>>;

fn open<'a>(
    topic: Option<&'static str>,
    cx: &'a mut TestAppContext,
) -> (Entity<Help>, &'a mut VisualTestContext, Events) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        I18n::init(Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let (help, cx) = cx.add_window_view(move |window, cx| Help::new(topic, window, cx));
    let events: Events = Rc::default();
    let sink = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&help, move |_, event: &HelpEvent, _| {
            sink.borrow_mut().push(match event {
                HelpEvent::Closed => "closed",
                HelpEvent::OpenShortcuts => "shortcuts",
            })
        })
        .detach();
    });
    cx.run_until_parked();
    (help, cx, events)
}

fn active(help: &Entity<Help>, cx: &mut VisualTestContext) -> &'static str {
    cx.update(|_, cx| help.read(cx).active)
}

#[gpui::test]
fn it_opens_on_the_asked_topic_and_the_search_moves_the_card(cx: &mut TestAppContext) {
    let (help, cx, events) = open(Some("terminal"), cx);
    assert_eq!(active(&help, cx), "terminal");
    assert!(cx.debug_bounds("help-dialog").is_some());
    assert!(cx.debug_bounds("help-topic-workspaces").is_some());
    // A word only "Navegador" has: the list narrows and the card follows.
    let browser = cx.update(|_, cx| {
        TopicCopy::read("browser", cx.global::<I18n>())
            .label
            .to_string()
    });
    cx.simulate_input(&browser.to_lowercase());
    cx.run_until_parked();
    assert_eq!(active(&help, cx), "browser");
    assert!(cx.debug_bounds("help-topic-workspaces").is_none());
    // Nothing matches: the list says so and the card asks for another word.
    cx.simulate_input(" zzzz");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| help.read(cx).visible(cx).is_empty()));
    cx.update(|_, cx| {
        help.update(cx, |help, cx| {
            help.input.update(cx, |input, cx| input.set_text("", cx))
        })
    });
    cx.run_until_parked();
    let bounds = cx.debug_bounds("help-topic-composer").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(active(&help, cx), "composer");
    let bounds = cx.debug_bounds("help-open-shortcuts").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), ["shortcuts", "closed"]);
}

#[gpui::test]
fn an_unknown_topic_opens_the_default(cx: &mut TestAppContext) {
    let (help, cx, _) = open(Some("nope"), cx);
    assert_eq!(active(&help, cx), super::DEFAULT_TOPIC);
}
