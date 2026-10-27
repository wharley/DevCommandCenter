//! Settings over the window (`setIsSettingsOpen`): the sidebar's gear, the
//! palette and Feedback's "Abrir Configurações" open it. What it changes is
//! applied at once and kept in the preferences: the theme, the language
//! (text built when a view opened keeps its language until it reopens) and
//! the density.

use gpui::{AnyElement, AppContext, Context, Entity, IntoElement, Subscription, Window};

use super::Shell;
use crate::data::mcp_advanced::SessionContext;
use crate::prefs::Prefs;
use crate::screens::settings::{SectionId, Settings, SettingsEvent, SettingsState};

pub(super) type OpenSettings = (Entity<Settings>, Subscription);

impl Shell {
    pub(super) fn open_settings(
        &mut self,
        section: Option<SectionId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.is_some() {
            return;
        }
        let conversation = self.conversation.read(cx);
        let root = conversation.workspace_path();
        let name = conversation
            .data()
            .and_then(|data| self.sidebar.read(cx).task_name(&data.task_id));
        let session = conversation
            .data()
            .map(|data| SessionContext {
                project_id: self
                    .sidebar
                    .read(cx)
                    .tasks()
                    .iter()
                    .find(|task| task.id == data.task_id)
                    .map(|task| task.project_id.clone()),
                session_id: data.session_id.clone(),
                provider_id: data.provider_id.clone(),
                created_at: data.session_id.as_ref().and_then(|id| {
                    data.sessions
                        .iter()
                        .find(|row| &row.id == id)
                        .map(|row| row.created_at.clone())
                }),
            })
            .unwrap_or_default();
        let state = SettingsState {
            theme: self.prefs.theme,
            locale: self.prefs.locale,
            density: self.prefs.density,
            workspace: (name, root),
            default_pick: conversation.composer.read(cx).settings.global_pick.clone(),
            session,
        };
        let settings = cx.new(|cx| Settings::new(state, section, window, cx));
        let events = cx.subscribe_in(&settings, window, |shell, _, event, window, cx| {
            match event {
                SettingsEvent::Closed => shell.settings = None,
                SettingsEvent::OpenShortcuts => shell.shortcuts_open = true,
                SettingsEvent::DefaultPick(pick) => {
                    let composer = shell.conversation.read(cx).composer.clone();
                    composer.update(cx, |composer, cx| {
                        composer.set_default_pick(pick.clone(), cx)
                    });
                }
                SettingsEvent::ProvidersChanged => {
                    let composer = shell.conversation.read(cx).composer.clone();
                    composer.update(cx, |composer, cx| composer.invalidate_catalog(cx));
                }
                SettingsEvent::Theme(theme) => {
                    shell.update_prefs(|prefs| prefs.theme = *theme);
                    crate::theme::Theme::init(*theme, cx);
                    crate::theme::Theme::sync_with_window(window, cx);
                }
                SettingsEvent::Locale(locale) => {
                    shell.update_prefs(|prefs| prefs.locale = *locale);
                    crate::i18n::I18n::init(*locale, cx);
                }
                SettingsEvent::Density(density) => {
                    shell.update_prefs(|prefs| prefs.density = *density);
                    cx.set_global(*density);
                }
            }
            cx.refresh_windows();
            cx.notify();
        });
        self.settings = Some((settings, events));
        cx.notify();
    }

    fn update_prefs(&mut self, change: impl FnOnce(&mut Prefs)) {
        let mut prefs = self.prefs.clone();
        change(&mut prefs);
        self.prefs = prefs;
        if let Err(error) = self.prefs.save(&self.prefs_path) {
            eprintln!("[dcc-native] could not save the preferences: {error}");
        }
    }

    pub(super) fn settings_overlay(&self) -> Option<AnyElement> {
        self.settings
            .as_ref()
            .map(|(settings, _)| settings.clone().into_any_element())
    }
}
