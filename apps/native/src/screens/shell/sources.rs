//! The sources on screen (`dcc_core::domain::agent_source`): once a minute it
//! reads the agents' sources from DCC's database (a small table) into the
//! `Sources` global the room and the agent's page read, with the replies.
//! Reading them when due and waking the agent with news is the server's
//! (`dcc_server::http_api::agent_clock`), with or without this window; here
//! "Ler agora" asks it.

use std::time::Duration;

use dcc_core::domain::agent_source::AgentSource;
use gpui::{AppContext, Context, Task};

use super::Shell;
use crate::data::agent_sources::{Sources, read};
use crate::data::desktop_api::DesktopApi;
use crate::i18n::Translate;
use crate::ui::Toaster;

const EVERY: Duration = Duration::from_secs(60);

pub struct SourceClock {
    _tick: Option<Task<()>>,
}

impl SourceClock {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Tests set the sources themselves; a test never reads the real desktop.
        let tick = (!cfg!(test)).then(|| {
            cx.spawn(async move |this, cx| {
                loop {
                    let (list, replies) = cx
                        .background_executor()
                        .spawn(async { (read(), crate::data::agent_replies::read()) })
                        .await;
                    let alive = this.update(cx, |_, cx| {
                        cx.set_global(Sources { list });
                        // A reply sent from the phone leaves the desktop's room.
                        cx.set_global(crate::data::agent_replies::Replies { list: replies });
                    });
                    if alive.is_err() {
                        break;
                    }
                    cx.background_executor().timer(EVERY).await;
                }
            })
        });
        Self { _tick: tick }
    }
}

impl Shell {
    /// "Ler agora": the server reads it and wakes the agent with news,
    /// unless a hold keeps it.
    pub(super) fn read_source_now(&mut self, source: AgentSource, cx: &mut Context<Self>) {
        cx.spawn(async move |_, cx| {
            let started = cx
                .background_executor()
                .spawn(async move {
                    let api = DesktopApi::load().ok_or_else(|| "DCC is not running".to_string())?;
                    api.read_source_now(&source)
                        .map_err(|error| error.to_string())
                })
                .await;
            cx.update(|cx| match started {
                Ok(true) => {}
                Ok(false) => Toaster::warning(cx.t("agents.sources.held"), cx),
                Err(error) => Toaster::error(error, cx),
            });
        })
        .detach();
    }
}

/// The clock that keeps the list on screen.
pub(super) fn start(cx: &mut Context<Shell>) -> gpui::Entity<SourceClock> {
    cx.new(SourceClock::new)
}
