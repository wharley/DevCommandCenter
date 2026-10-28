//! The first-run guide over the whole window (`isOnboardingOpen`): on its
//! own once, from the palette any time. Finishing or skipping it keeps it
//! from coming back (`dcc.onboarding.complete`); Esc only closes it.

use gpui::{AnyElement, AppContext, Context, Entity, Focusable, IntoElement, Subscription, Window};

use super::Shell;
use crate::data::onboarding::{OVERRIDE_ENV, opens_on_start};
use crate::screens::onboarding::{Onboarding, OnboardingEvent};

pub(super) type OpenOnboarding = (Entity<Onboarding>, Subscription);

impl Shell {
    /// `resolveInitialOnboardingOpen`, never under test.
    pub(super) fn onboarding_on_start(&self) -> bool {
        !cfg!(test)
            && opens_on_start(
                self.prefs.onboarding_complete,
                cfg!(debug_assertions),
                std::env::var(OVERRIDE_ENV).ok().as_deref(),
            )
    }

    pub(super) fn open_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let onboarding = match &self.onboarding {
            Some((onboarding, _)) => onboarding.clone(),
            None => {
                let onboarding = cx.new(Onboarding::new);
                let events = cx.subscribe_in(&onboarding, window, |shell, _, event, window, cx| {
                    shell.onboarding = None;
                    match event {
                        OnboardingEvent::Closed => {}
                        OnboardingEvent::Finished => shell.finish_onboarding(cx),
                        OnboardingEvent::FinishedIntoHelp => {
                            shell.finish_onboarding(cx);
                            shell.open_help(None, window, cx);
                        }
                    }
                    cx.notify();
                });
                self.onboarding = Some((onboarding.clone(), events));
                onboarding
            }
        };
        window.focus(&onboarding.focus_handle(cx), cx);
        cx.notify();
    }

    /// `handleCompleteOnboarding`
    fn finish_onboarding(&mut self, cx: &mut Context<Self>) {
        if self.prefs.onboarding_complete {
            return;
        }
        self.prefs.onboarding_complete = true;
        if let Err(error) = self.prefs.save(&self.prefs_path) {
            eprintln!("[dcc-native] could not save the onboarding: {error}");
        }
        cx.notify();
    }

    pub(super) fn onboarding_overlay(&self) -> Option<AnyElement> {
        self.onboarding
            .as_ref()
            .map(|(onboarding, _)| onboarding.clone().into_any_element())
    }
}
