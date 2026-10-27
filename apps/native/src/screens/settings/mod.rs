//! Settings (`SettingsDialog`): a navigation of sections in three groups
//! with a search, the chosen section's title and its content. The sections
//! arrive one by one (screen 9g); the navigation only lists the ones the
//! native app has.

pub(crate) mod account;
mod appearance;
mod computer;
mod connections;
mod connectors;
mod decision;
mod experimental;
mod general;
mod git;
mod memory;
mod nav;
mod preview;
mod providers;
mod shortcuts;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_account;
#[cfg(test)]
mod tests_computer;
#[cfg(test)]
mod tests_connections;
#[cfg(test)]
mod tests_connectors;
#[cfg(test)]
mod tests_mcp_advanced;
#[cfg(test)]
mod tests_providers;
mod view;

use gpui::{AppContext, Context, Entity, EventEmitter, ScrollHandle, Subscription, Window};

use crate::i18n::Locale;
use crate::theme::{Density, ThemePreference};
use crate::ui::text_input::{TextInput, TextInputEvent};

pub enum SettingsEvent {
    Closed,
    /// "Abrir atalhos": the cheatsheet over Settings.
    OpenShortcuts,
    Theme(ThemePreference),
    Locale(Locale),
    Density(Density),
    /// A model of "Provedores" is the default for new tasks.
    DefaultPick(crate::data::provider_selection::Pick),
    /// A provider turned on or off, rechecked or launched another way: the
    /// composer reads the catalog again.
    ProvidersChanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Workspace,
    Services,
    Advanced,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Workspace, Group::Services, Group::Advanced];

    pub fn key(self) -> &'static str {
        match self {
            Group::Workspace => "workspace",
            Group::Services => "services",
            Group::Advanced => "advanced",
        }
    }
}

/// `SettingsSectionId`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionId {
    General,
    Appearance,
    Model,
    DecisionProvider,
    AiMemory,
    Integrations,
    ComputerUse,
    Connections,
    Shortcuts,
    Git,
    Experimental,
    Account,
}

impl SectionId {
    /// The desktop's id (`settings.sections.<id>`, `keywords.<id>`).
    pub fn key(self) -> &'static str {
        match self {
            SectionId::General => "general",
            SectionId::Appearance => "appearance",
            SectionId::Model => "model",
            SectionId::DecisionProvider => "decisionProvider",
            SectionId::AiMemory => "aiMemory",
            SectionId::Integrations => "integrations",
            SectionId::ComputerUse => "computerUse",
            SectionId::Connections => "connections",
            SectionId::Shortcuts => "shortcuts",
            SectionId::Git => "git",
            SectionId::Experimental => "experimental",
            SectionId::Account => "account",
        }
    }

    pub fn group(self) -> Group {
        match self {
            SectionId::General | SectionId::Appearance | SectionId::Shortcuts => Group::Workspace,
            SectionId::Git | SectionId::Experimental => Group::Advanced,
            _ => Group::Services,
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            SectionId::General => "wrench",
            SectionId::Appearance => "sun-medium",
            SectionId::Model => "sparkles",
            SectionId::DecisionProvider => "brain-circuit",
            SectionId::AiMemory => "database",
            SectionId::Integrations => "cable",
            SectionId::ComputerUse => "monitor-cog",
            SectionId::Connections => "server",
            SectionId::Shortcuts => "keyboard",
            SectionId::Git => "git-branch",
            SectionId::Experimental => "package",
            SectionId::Account => "circle-user-round",
        }
    }
}

/// The sections, in the desktop's array order (all of them now).
pub const AVAILABLE: [SectionId; 12] = [
    SectionId::General,
    SectionId::Appearance,
    SectionId::Model,
    SectionId::DecisionProvider,
    SectionId::AiMemory,
    SectionId::Integrations,
    SectionId::ComputerUse,
    SectionId::Connections,
    SectionId::Shortcuts,
    SectionId::Git,
    SectionId::Experimental,
    SectionId::Account,
];

pub struct Settings {
    active: SectionId,
    search: Entity<TextInput>,
    /// The open task's name and checkout, for the navigation's footer.
    workspace: (Option<String>, Option<String>),
    /// The open session, for the advanced MCP panel.
    session: crate::data::mcp_advanced::SessionContext,
    /// What `prefs` holds now (the dialog shows it, the shell keeps it).
    theme: ThemePreference,
    locale: Locale,
    density: Density,
    metrics: std::collections::HashMap<String, crate::data::ux_metrics::Metric>,
    scroll: ScrollHandle,
    general: general::General,
    shortcuts: shortcuts::Shortcuts,
    providers: providers::Providers,
    decision: decision::Decision,
    memory: memory::Memory,
    account: account::Account,
    connectors: connectors::Connectors,
    computer: computer::Computer,
    connections: connections::Connections,
    /// "Automação do projeto" over Settings.
    automation: Option<(
        Entity<crate::screens::automation::ProjectAutomation>,
        Subscription,
    )>,
    _search_events: Subscription,
}

impl EventEmitter<SettingsEvent> for Settings {}

pub struct SettingsState {
    pub theme: ThemePreference,
    pub locale: Locale,
    pub density: Density,
    /// The task's name and checkout.
    pub workspace: (Option<String>, Option<String>),
    /// New tasks' provider and model.
    pub default_pick: Option<crate::data::provider_selection::Pick>,
    /// The open task's project and session.
    pub session: crate::data::mcp_advanced::SessionContext,
}

impl Settings {
    /// `section` is the one asked for (`openSettingsSection`); one the
    /// native app does not have yet opens the first it has.
    pub fn new(
        state: SettingsState,
        section: Option<SectionId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = crate::i18n::Translate::t(&**cx, "settings.navigation.search");
        let search = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
        let events = cx.subscribe(&search, |settings, input, event, cx| match event {
            TextInputEvent::Changed => cx.notify(),
            // A search with text clears first; Esc again closes.
            TextInputEvent::Cancelled => {
                if input.read(cx).text().is_empty() {
                    cx.emit(SettingsEvent::Closed);
                } else {
                    input.update(cx, |input, cx| input.set_text("", cx));
                    cx.notify();
                }
            }
            TextInputEvent::Submitted => settings.select_first_match(cx),
            TextInputEvent::Blurred => {}
        });
        let active = section
            .filter(|section| AVAILABLE.contains(section))
            .unwrap_or(AVAILABLE[0]);
        let mut settings = Self {
            active,
            search,
            workspace: state.workspace,
            session: state.session,
            theme: state.theme,
            locale: state.locale,
            density: state.density,
            metrics: crate::data::ux_metrics::read(),
            scroll: ScrollHandle::new(),
            general: general::General::new(if cfg!(test) {
                None
            } else {
                crate::data::desktop_api::DesktopApi::load()
            }),
            shortcuts: shortcuts::Shortcuts::new(cx),
            providers: providers::Providers::new(state.default_pick, window, cx),
            decision: decision::Decision::new(window, cx),
            memory: memory::Memory::new(window, cx),
            account: account::Account::new(window, cx),
            connectors: connectors::Connectors::new(window, cx),
            computer: computer::Computer::default(),
            connections: connections::Connections::new(window, cx),
            automation: None,
            _search_events: events,
        };
        settings.load_general(cx);
        settings.load_shortcuts(cx);
        settings.load_providers(cx);
        settings.load_decision(cx);
        settings.load_memory(cx);
        if settings.active == SectionId::Account {
            settings.ensure_account(cx);
        }
        if settings.active == SectionId::Integrations {
            settings.ensure_connectors(cx);
        }
        if settings.active == SectionId::ComputerUse {
            settings.ensure_computer(cx);
        }
        if settings.active == SectionId::Connections {
            settings.ensure_connections(cx);
        }
        settings
    }

    #[cfg(test)]
    pub fn set_api(
        &mut self,
        api: crate::data::desktop_api::DesktopApi,
        data_dir: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.providers
            .set_runtime_path(data_dir.join("provider-runtime.json"));
        self.general.set_api(api, data_dir);
        self.load_general(cx);
        self.load_shortcuts(cx);
        self.load_providers(cx);
        self.load_decision(cx);
        self.load_memory(cx);
        self.account.reset();
        if self.active == SectionId::Account {
            self.ensure_account(cx);
        }
        self.connectors.reset();
        if self.active == SectionId::Integrations {
            self.ensure_connectors(cx);
        }
        self.computer = computer::Computer::default();
        if self.active == SectionId::ComputerUse {
            self.ensure_computer(cx);
        }
        self.connections.reset();
        if self.active == SectionId::Connections {
            self.ensure_connections(cx);
        }
    }

    pub fn search_input(&self) -> Entity<TextInput> {
        self.search.clone()
    }

    fn select(&mut self, section: SectionId, cx: &mut Context<Self>) {
        self.active = section;
        if section == SectionId::Account {
            self.ensure_account(cx);
        }
        if section == SectionId::Integrations {
            self.ensure_connectors(cx);
        }
        if section == SectionId::ComputerUse {
            self.ensure_computer(cx);
        }
        if section == SectionId::Connections {
            self.ensure_connections(cx);
        }
        // A new section starts at its top.
        self.scroll
            .set_offset(gpui::point(gpui::px(0.0), gpui::px(0.0)));
        cx.notify();
    }

    fn select_first_match(&mut self, cx: &mut Context<Self>) {
        if let Some(first) = self.matches(cx).first().copied() {
            self.select(first, cx);
        }
    }

    pub(super) fn set_theme(&mut self, theme: ThemePreference, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.emit(SettingsEvent::Theme(theme));
        cx.notify();
    }

    pub(super) fn set_locale(&mut self, locale: Locale, cx: &mut Context<Self>) {
        self.locale = locale;
        cx.emit(SettingsEvent::Locale(locale));
        cx.notify();
    }

    pub(super) fn set_density(&mut self, density: Density, cx: &mut Context<Self>) {
        self.density = density;
        cx.emit(SettingsEvent::Density(density));
        cx.notify();
    }

    pub(super) fn reset_metrics(&mut self, cx: &mut Context<Self>) {
        crate::data::ux_metrics::reset();
        self.metrics.clear();
        cx.notify();
    }
}

/// A removal waiting for its confirmation, for the tests.
#[cfg(test)]
fn connectors_confirm_remove(view: crate::data::connectors::View) -> connectors::Confirm {
    connectors::Confirm::Remove(view)
}
