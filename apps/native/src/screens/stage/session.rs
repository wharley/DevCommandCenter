//! The "Sessão" tab on the stage (`SessionStageSurface`): the Activity chip
//! next to the Chat, with its X on hover, over the session panel. A
//! delegation's file opens on the stage in its place.

use gpui::{AnyElement, AppContext, Context, Entity, Subscription, Window, div, prelude::*, px};

use super::{Stage, StageEvent};
use crate::i18n::Translate;
use crate::screens::conversation::ConversationView;
use crate::screens::session_panel::{SessionPanel, SessionPanelEvent, SessionTab};
use crate::theme::ActiveTheme;
use crate::ui::Icon;

pub(crate) struct StageSession {
    pub(crate) panel: Entity<SessionPanel>,
    _events: Subscription,
}

impl Stage {
    /// `openSessionTab(tab)`: the tab in front, on the one asked.
    pub fn open_session(
        &mut self,
        conversation: Entity<ConversationView>,
        tab: Option<SessionTab>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &self.session {
            Some(session) => {
                if let Some(tab) = tab {
                    session.panel.update(cx, |panel, cx| panel.set_tab(tab, cx));
                }
            }
            None => {
                let panel =
                    cx.new(|cx| SessionPanel::new(conversation, tab.unwrap_or_default(), cx));
                let events =
                    cx.subscribe_in(&panel, window, |stage, _, event, window, cx| match event {
                        SessionPanelEvent::OpenSession(id) => {
                            cx.emit(StageEvent::OpenSession(id.clone()))
                        }
                        SessionPanelEvent::OpenDiff(request) => {
                            stage.open_diff((**request).clone(), window, cx)
                        }
                    });
                self.diff = None;
                self.files = None;
                self.session = Some(StageSession {
                    panel,
                    _events: events,
                });
            }
        }
        self.chat_active = false;
        cx.notify();
    }

    pub fn session_open(&self) -> bool {
        self.session.is_some()
    }
}

/// The chip: `Activity` and "Sessão", the X on hover.
pub(super) fn session_tab(stage: &Stage, cx: &mut Context<Stage>) -> Option<AnyElement> {
    stage.session.as_ref()?;
    let colors = cx.theme().colors.clone();
    let foreground = colors.foreground;
    Some(
        super::view::tab_base("stage-session-tab", !stage.chat_active, cx)
            .group("stage-session-tab")
            .px(px(8.0))
            .on_click(cx.listener(|stage, _, window, cx| stage.show_content(window, cx)))
            .child(Icon::new("activity").size(14.0).stroke(1.8))
            .child(div().child(cx.t("sessionStage.title")))
            .child(
                div()
                    .id("stage-session-close")
                    .debug_selector(|| "stage-session-close".into())
                    .flex_shrink_0()
                    .rounded(px(4.0))
                    .p(px(2.0))
                    .text_color(colors.muted_foreground)
                    .invisible()
                    .group_hover("stage-session-tab", |style| style.visible())
                    .hover(move |style| style.text_color(foreground))
                    .on_click(cx.listener(|stage, _, _, cx| {
                        cx.stop_propagation();
                        stage.close(cx)
                    }))
                    .child(Icon::new("x").size(12.0).stroke(2.0)),
            )
            .into_any_element(),
    )
}

pub(super) fn session_body(stage: &Stage) -> Option<AnyElement> {
    stage.session.as_ref().map(|session| {
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(session.panel.clone())
            .into_any_element()
    })
}
