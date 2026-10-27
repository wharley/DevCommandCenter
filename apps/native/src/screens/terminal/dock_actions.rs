//! What the dock's bar does (`WorkspaceTerminalDrawer`'s handlers): the
//! scope picked, expanding over the conversation, renaming a tab, closing
//! one with a program running, the actions menu (output to the agent,
//! clear, interrupt, restart, the system's Terminal) and the local server.

use gpui::{AppContext, Context, Entity, Subscription, Window};

use super::agent::{OutputOrigin, RECENT_LINES, agent_content, evidence};
use super::dev_server::Detected;
use super::dock::{DockEvent, ScopeKind, TerminalDock};
use super::tabs::owner_key;
use crate::i18n::Translate;
use crate::ui::text_input::{TextInput, TextInputEvent};

/// "Renomear terminal" for one tab.
pub struct Renaming {
    pub tab: String,
    pub input: Entity<TextInput>,
    _events: Subscription,
}

/// `shell_open_terminal_at_path`: the system's terminal in that folder.
fn open_system_terminal(cwd: String) {
    std::thread::spawn(move || {
        use std::process::Command;
        let path = std::path::Path::new(&cwd);
        if !path.is_dir() {
            return;
        }
        #[cfg(target_os = "macos")]
        let _ = Command::new("open")
            .args(["-a", "Terminal"])
            .arg(path)
            .status();
        #[cfg(target_os = "linux")]
        for (bin, flag) in [
            ("x-terminal-emulator", "--working-directory"),
            ("gnome-terminal", "--working-directory"),
            ("konsole", "--workdir"),
            ("xfce4-terminal", "--working-directory"),
        ] {
            if Command::new(bin)
                .arg(flag)
                .arg(path)
                .status()
                .is_ok_and(|status| status.success())
            {
                break;
            }
        }
        #[cfg(target_os = "windows")]
        let _ = Command::new("cmd")
            .args(["/C", "start", "cmd"])
            .current_dir(path)
            .status();
    });
}

impl TerminalDock {
    pub(super) fn scope_label(&self, kind: ScopeKind, cx: &gpui::App) -> gpui::SharedString {
        match kind {
            ScopeKind::Worktree => cx.t("terminalDock.scopes.worktree"),
            ScopeKind::Project => cx.t("terminalDock.scopes.project"),
        }
    }

    /// `onScopeChange`: the other scope's tab shows, focused.
    pub fn set_kind(&mut self, kind: ScopeKind, window: &mut Window, cx: &mut Context<Self>) {
        if kind != self.kind {
            crate::data::ux_metrics::record("terminal_scope_switched");
        }
        self.show(kind, window, cx);
    }

    /// The expand button: over the conversation, or back to the split.
    pub fn set_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        if self.expanded != expanded {
            self.expanded = expanded;
            cx.emit(DockEvent::Expanded(expanded));
            cx.notify();
        }
    }

    /// Esc steps back one level: collapse first, then close.
    pub fn step_back(&mut self, cx: &mut Context<Self>) {
        if self.expanded {
            self.set_expanded(false, cx);
        } else {
            self.hide(cx);
        }
    }

    pub(super) fn start_rename(&mut self, tab: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(scope) = self.scope() else {
            return;
        };
        let Some(title) = self
            .tabs
            .scope(&scope.key)
            .tabs
            .iter()
            .find(|candidate| candidate.id == tab)
            .map(|tab| tab.title.clone())
        else {
            return;
        };
        let input = cx.new(|cx| TextInput::new(window, cx).with_max_len(80));
        input.update(cx, |input, cx| {
            input.set_text(title, cx);
            input.focus_and_select_all(window, cx);
        });
        let events = cx.subscribe_in(&input, window, |dock, _, event, _, cx| match event {
            TextInputEvent::Submitted => dock.submit_rename(cx),
            TextInputEvent::Cancelled => dock.cancel_rename(cx),
            TextInputEvent::Changed => cx.notify(),
            TextInputEvent::Blurred => {}
        });
        self.renaming = Some(Renaming {
            tab: tab.to_string(),
            input,
            _events: events,
        });
        cx.notify();
    }

    pub(super) fn submit_rename(&mut self, cx: &mut Context<Self>) {
        let (Some(scope), Some(renaming)) = (self.scope(), self.renaming.as_ref()) else {
            return;
        };
        let title = renaming.input.read(cx).text().to_string();
        let tab = renaming.tab.clone();
        if self.tabs.rename(&scope.key, &tab, &title) {
            self.renaming = None;
            cx.notify();
        }
    }

    pub(super) fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.renaming = None;
        cx.notify();
    }

    /// The tab's ×: a program still running asks first.
    pub(super) fn request_close(&mut self, tab: &str, cx: &mut Context<Self>) {
        let Some(scope) = self.scope() else {
            return;
        };
        let session = self.sessions.get(&owner_key(&scope.key, tab)).cloned();
        let running = session
            .as_ref()
            .is_some_and(|session| session.read(cx).busy());
        if !running {
            return self.close_tab(tab, cx);
        }
        let label = session
            .and_then(|session| {
                session
                    .read(cx)
                    .activity
                    .as_ref()
                    .and_then(|activity| activity.process_label.clone())
            })
            .or_else(|| {
                self.tabs
                    .scope(&scope.key)
                    .tabs
                    .iter()
                    .find(|candidate| candidate.id == tab)
                    .map(|tab| tab.title.clone())
            })
            .unwrap_or_else(|| cx.t("terminalDock.process").to_string());
        self.closing = Some((tab.to_string(), label));
        cx.notify();
    }

    /// "Encerrar e fechar"
    pub(super) fn confirm_close(&mut self, cx: &mut Context<Self>) {
        if let Some((tab, _)) = self.closing.take() {
            self.close_tab(&tab, cx);
        }
        cx.notify();
    }

    /// "Apenas ocultar": the program goes on; the dock hides.
    pub(super) fn hide_instead(&mut self, cx: &mut Context<Self>) {
        self.closing = None;
        self.hide(cx);
    }

    pub(super) fn cancel_close(&mut self, cx: &mut Context<Self>) {
        self.closing = None;
        cx.notify();
    }

    /// "Analisar saída com agente" and "Enviar seleção ao agente": the
    /// selection, else the recent output, as evidence for the composer.
    pub(super) fn send_to_agent(&mut self, cx: &mut Context<Self>) {
        let (Some(scope), Some(session)) = (self.scope(), self.active_session(cx)) else {
            return;
        };
        let Some(cwd) = scope.cwd.clone() else {
            return;
        };
        let (content, selection_only) = {
            let session = session.read(cx);
            let selection = session.selection_text().unwrap_or_default();
            let recent = session
                .emulator
                .recent_text(RECENT_LINES, super::agent::MAX_CHARS);
            agent_content(&selection, &recent)
        };
        if content.is_empty() {
            return;
        }
        let source = if selection_only {
            cx.t("terminalDock.agentContext.selection")
        } else {
            cx.t("terminalDock.agentContext.recentOutput")
        };
        let origin = OutputOrigin {
            project_label: scope.project_label.clone(),
            scope_label: self.scope_label(scope.kind, cx).to_string(),
            branch: scope.branch.clone(),
            cwd,
        };
        session.update(cx, |session, cx| {
            session.emulator.clear_selection();
            cx.notify();
        });
        cx.emit(DockEvent::SendToAgent(evidence(&origin, &source, &content)));
    }

    pub(super) fn clear_active(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.active_session(cx) {
            session.update(cx, |session, cx| {
                session.emulator.clear_selection();
                session.clear(cx);
            });
        }
    }

    pub(super) fn interrupt_active(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.active_session(cx) {
            session.update(cx, |session, _| session.interrupt());
        }
    }

    pub(super) fn restart_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.active_session(cx) else {
            return;
        };
        let ended = session.update(cx, |session, cx| session.restart(cx));
        if let Some(ended) = ended {
            self.by_pty.remove(&ended);
        }
        let focus = session.read(cx).focus_handle();
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(super) fn open_external(&mut self) {
        if let Some(cwd) = self.scope().and_then(|scope| scope.cwd) {
            open_system_terminal(cwd);
        }
    }

    /// The newest local server across the scope's tabs.
    pub(super) fn detected_server(&self, cx: &gpui::App) -> Option<Detected> {
        let scope = self.scope()?;
        self.tabs
            .scope(&scope.key)
            .tabs
            .iter()
            .filter_map(|tab| self.sessions.get(&owner_key(&scope.key, &tab.id)))
            .flat_map(|session| session.read(cx).dev_servers.found.clone())
            .max_by_key(|server| server.at)
    }
}
