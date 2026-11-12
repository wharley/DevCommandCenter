//! "Executar" on a command the agent wrote in the conversation: it runs in a
//! new tab of the task's terminal, in the task's checkout, and the dock
//! shows it — as Claude's app runs a shell block in its terminal. Nothing
//! runs until the person clicks.

use gpui::{Context, Window};

use super::dock::{ScopeKind, TerminalDock};

impl TerminalDock {
    pub fn run_command(&mut self, command: &str, window: &mut Window, cx: &mut Context<Self>) {
        let command = command.trim();
        if command.is_empty() {
            return;
        }
        let Some(scope) = self
            .place
            .as_ref()
            .map(|place| place.scope(ScopeKind::Worktree))
            .filter(|scope| scope.cwd.is_some())
        else {
            return;
        };
        // A tab of its own, or the last one when the scope is at its cap.
        let tab = self.tabs.add(&scope.key).or_else(|| {
            self.tabs
                .scope(&scope.key)
                .active()
                .map(|tab| tab.id.clone())
        });
        let Some(tab) = tab else {
            return;
        };
        self.tabs.set_active(&scope.key, &tab);
        let Some(session) = self.session_for(&scope.key, &tab, cx) else {
            return;
        };
        session.update(cx, |session, cx| session.run_line(command, cx));
        self.kind = scope.kind;
        self.open = true;
        self.show_active(cx);
        let focus = session.read(cx).focus_handle();
        window.focus(&focus, cx);
        cx.notify();
    }
}
