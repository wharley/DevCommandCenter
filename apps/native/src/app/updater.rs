//! The updater the windows read (`useAppUpdate`): one per app, checking at
//! launch, every 30 minutes and when the window comes back to the front (at
//! most every 5 minutes); an update found stays offered while a later check
//! fails. "Instalar" downloads it, checks it, puts it in place and restarts
//! the app (`data::updater`).

use std::time::{Duration, Instant};

use gpui::{App, AppContext, Context, Entity, Global, Task};

use crate::data::updater::{self, Update};
use crate::i18n::{Args, Translate};
use crate::ui::Toaster;

/// `UPDATE_CHECK_INTERVAL_MS`
const INTERVAL: Duration = Duration::from_secs(30 * 60);
/// `UPDATE_CHECK_FOCUS_COOLDOWN_MS`
const COOLDOWN: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    Idle,
    Checking,
    UpToDate,
    Available(Update),
    Installing(Update),
}

pub struct Updater {
    pub current: String,
    pub stage: Stage,
    /// The last check's failure, shown in Settings.
    pub error: Option<String>,
    started: Option<Instant>,
    _check: Option<Task<()>>,
    _install: Option<Task<()>>,
    _beat: Option<Task<()>>,
}

struct AppUpdater(Entity<Updater>);

impl Global for AppUpdater {}

/// The app's updater (none in tests).
pub fn updater(cx: &App) -> Option<Entity<Updater>> {
    cx.try_global::<AppUpdater>().map(|global| global.0.clone())
}

/// At launch: the first check, then one every 30 minutes.
pub fn start(cx: &mut App) {
    let updater = cx.new(|cx| {
        let mut updater = Updater::new(crate::data::feedback::version());
        updater._beat = Some(cx.spawn(async move |this: gpui::WeakEntity<Updater>, cx| {
            loop {
                cx.background_executor().timer(INTERVAL).await;
                if this
                    .update(cx, |updater: &mut Updater, cx| updater.check(false, cx))
                    .is_err()
                {
                    return;
                }
            }
        }));
        updater.check(true, cx);
        updater
    });
    cx.set_global(AppUpdater(updater));
}

/// The window came to the front.
pub fn window_active(cx: &mut App) {
    if let Some(updater) = updater(cx) {
        updater.update(cx, |updater, cx| updater.check(false, cx));
    }
}

impl Updater {
    pub fn new(current: String) -> Self {
        Self {
            current,
            stage: Stage::Idle,
            error: None,
            started: None,
            _check: None,
            _install: None,
            _beat: None,
        }
    }

    /// The update on offer.
    pub fn available(&self) -> Option<&Update> {
        match &self.stage {
            Stage::Available(update) | Stage::Installing(update) => Some(update),
            _ => None,
        }
    }

    /// `force`: "Verificar atualizações"; otherwise not within the cooldown.
    pub fn check(&mut self, force: bool, cx: &mut Context<Self>) {
        if matches!(self.stage, Stage::Checking | Stage::Installing(_)) {
            return;
        }
        if !force && self.started.is_some_and(|at| at.elapsed() < COOLDOWN) {
            return;
        }
        self.started = Some(Instant::now());
        let before = std::mem::replace(&mut self.stage, Stage::Checking);
        let current = self.current.clone();
        cx.notify();
        self._check = Some(cx.spawn(async move |this, cx| {
            let checked = cx
                .background_executor()
                .spawn(async move { updater::check(&current) })
                .await;
            let _ = this.update(cx, |updater, cx| {
                updater.finish_check(before, checked);
                cx.notify();
            });
        }));
    }

    fn finish_check(&mut self, before: Stage, checked: Result<Option<Update>, String>) {
        match checked {
            Ok(Some(update)) => {
                self.stage = Stage::Available(update);
                self.error = None;
            }
            Ok(None) => {
                self.stage = Stage::UpToDate;
                self.error = None;
            }
            // A failed check never hides an update already found.
            Err(error) => {
                self.error = Some(error);
                self.stage = match before {
                    Stage::Available(update) => Stage::Available(update),
                    _ => Stage::Idle,
                };
            }
        }
    }

    /// `app_quit_and_install`: download, check, put in place, restart.
    pub fn install(&mut self, cx: &mut Context<Self>) {
        let Stage::Available(update) = self.stage.clone() else {
            return;
        };
        self.stage = Stage::Installing(update.clone());
        cx.notify();
        self._install = Some(cx.spawn(async move |this, cx| {
            let wanted = update.clone();
            let installed = cx
                .background_executor()
                .spawn(async move {
                    let bytes = updater::download(&wanted)?;
                    updater::install(&wanted, &bytes)
                })
                .await;
            let _ = this.update(cx, |updater, cx| match installed {
                Ok(()) => cx.restart(),
                Err(message) => {
                    updater.stage = Stage::Available(update);
                    let text = cx.t_with(
                        "updater.installFailed",
                        &Args::default().with("message", message),
                    );
                    Toaster::error(text, cx);
                    cx.notify();
                }
            });
        }));
    }
}

/// An updater offering `update` (tests: no network, no loop).
#[cfg(test)]
pub fn offer_for_tests(current: &str, update: Update, cx: &mut App) -> Entity<Updater> {
    let updater = cx.new(|_| {
        let mut updater = Updater::new(current.into());
        updater.stage = Stage::Available(update);
        updater
    });
    cx.set_global(AppUpdater(updater.clone()));
    updater
}

#[cfg(test)]
#[path = "updater_tests.rs"]
mod screen_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::updater::Asset;

    fn update() -> Update {
        Update {
            version: "0.3.0".into(),
            notes: None,
            date: None,
            asset: Asset {
                signature: "s".into(),
                url: "https://x".into(),
            },
        }
    }

    #[test]
    fn a_failed_check_keeps_the_update_already_found() {
        let mut updater = Updater::new("0.2.0".into());
        updater.finish_check(Stage::Idle, Ok(Some(update())));
        assert_eq!(updater.available(), Some(&update()));
        updater.finish_check(updater.stage.clone(), Err("offline".into()));
        assert_eq!(updater.available(), Some(&update()));
        assert_eq!(updater.error.as_deref(), Some("offline"));
        updater.finish_check(updater.stage.clone(), Ok(None));
        assert_eq!(updater.stage, Stage::UpToDate);
        assert_eq!(updater.error, None);
        updater.finish_check(Stage::UpToDate, Err("offline".into()));
        assert_eq!(updater.stage, Stage::Idle);
    }
}
