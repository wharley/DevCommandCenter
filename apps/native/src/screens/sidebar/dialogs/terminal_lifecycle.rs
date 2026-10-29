//! "Concluir/Mover tarefa com processos ativos?" (`pendingTerminalLifecycle`
//! in `App.tsx`): the programs running in the task's terminals go on in the
//! background, or end first; either way the task then moves.

use gpui::{AnyElement, Context, Window, prelude::*};

use super::SidebarDialog;
use crate::data::desktop_api::DesktopApi;
use crate::i18n::{Args, Translate};
use crate::screens::sidebar::Sidebar;
use crate::screens::sidebar::actions::PendingAction;
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, dialog_footer, dialog_header, spinner};

pub struct TerminalLifecycleDialog {
    pub task_id: String,
    pub action: PendingAction,
    pub count: usize,
    /// The shells are being ended.
    pub busy: bool,
}

impl Sidebar {
    pub(in crate::screens::sidebar) fn ask_terminal_lifecycle(
        &mut self,
        task_id: &str,
        action: PendingAction,
        count: usize,
        cx: &mut Context<Self>,
    ) {
        self.dialog = Some(SidebarDialog::TerminalLifecycle(TerminalLifecycleDialog {
            task_id: task_id.to_string(),
            action,
            count,
            busy: false,
        }));
        cx.notify();
    }

    fn lifecycle_dialog(&self) -> Option<&TerminalLifecycleDialog> {
        match self.dialog.as_ref()? {
            SidebarDialog::TerminalLifecycle(dialog) => Some(dialog),
            _ => None,
        }
    }

    /// "Manter executando": the task moves, its programs go on.
    fn keep_terminals(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.lifecycle_dialog() else {
            return;
        };
        let (task_id, action) = (dialog.task_id.clone(), dialog.action);
        self.dialog = None;
        cx.notify();
        self.perform_row_action(&task_id, action, window, cx);
    }

    /// "Encerrar e continuar": the task's shells end, then it moves.
    fn end_terminals(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(SidebarDialog::TerminalLifecycle(dialog)) = self.dialog.as_mut() else {
            return;
        };
        if dialog.busy {
            return;
        }
        dialog.busy = true;
        let (task_id, action) = (dialog.task_id.clone(), dialog.action);
        let mut ids = vec![task_id.clone()];
        if let Some(task) = self.state.task(&task_id) {
            ids.extend(
                task.member_workspace_ids
                    .iter()
                    .filter(|id| **id != task_id)
                    .cloned(),
            );
        }
        let api = self.terminal_api.clone().or_else(DesktopApi::load);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let ended = cx
                .background_executor()
                .spawn(async move { api.map(|api| api.end_task_terminals(&ids)) })
                .await;
            if let Some(Err(error)) = ended {
                eprintln!("[dcc-native] ending the task's terminals: {error}");
            }
            let _ = this.update_in(cx, |sidebar, window, cx| {
                sidebar.dialog = None;
                cx.notify();
                sidebar.perform_row_action(&task_id, action, window, cx);
            });
        })
        .detach();
    }
}

pub fn render(dialog: &TerminalLifecycleDialog, cx: &mut Context<Sidebar>) -> AnyElement {
    let busy = dialog.busy;
    let title = cx.t(if dialog.action == PendingAction::Archive {
        "sidebar.terminalLifecycle.archiveTitle"
    } else {
        "sidebar.terminalLifecycle.completeTitle"
    });
    let description = cx.t_with(
        "sidebar.terminalLifecycle.description",
        &Args::default().with("count", dialog.count.to_string()),
    );
    let footer = dialog_footer(
        [
            Button::new("terminal-lifecycle-cancel")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Default)
                .disabled(busy)
                .on_click(cx.listener(|sidebar, _, _, cx| sidebar.close_dialog(cx)))
                .child(cx.t("sidebar.terminalLifecycle.cancel"))
                .into_any_element(),
            Button::new("terminal-lifecycle-keep")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Default)
                .disabled(busy)
                .on_click(cx.listener(|sidebar, _, window, cx| sidebar.keep_terminals(window, cx)))
                .child(cx.t("sidebar.terminalLifecycle.keep"))
                .into_any_element(),
            Button::new("terminal-lifecycle-terminate")
                .variant(ButtonVariant::Destructive)
                .size(ButtonSize::Default)
                .disabled(busy)
                .when(busy, |button| {
                    button.child(spinner("terminal-lifecycle-busy", 14.0))
                })
                .on_click(cx.listener(|sidebar, _, window, cx| sidebar.end_terminals(window, cx)))
                .child(cx.t("sidebar.terminalLifecycle.terminate"))
                .into_any_element(),
        ],
        cx,
    );
    let close = cx.entity().downgrade();
    Dialog::new("terminal-lifecycle-dialog", move |_: &mut Window, cx| {
        let _ = close.update(cx, |sidebar, cx| sidebar.close_dialog(cx));
    })
    .max_width(448.0)
    .close_button(!busy)
    .child(dialog_header(title, Some(description), cx))
    .child(footer)
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, WindowAppearance};

    use super::*;
    use crate::data::loader::SidebarData;
    use crate::data::tasks::{TaskStatus, TaskSummary};
    use crate::data::terminal_api::{ActivityStatus, TerminalActivity};
    use crate::theme::{Theme, ThemePreference};

    fn task(id: &str) -> TaskSummary {
        TaskSummary {
            id: id.into(),
            name: id.into(),
            branch: "main".into(),
            base_branch: "main".into(),
            status: TaskStatus::Ready,
            project_id: "p".into(),
            root_path: "/repo".into(),
            worktree_path: Some(format!("/repo/.dcc/{id}")),
            pinned_at: None,
            snoozed_until: None,
            created_at: "2026-10-23T12:00:00Z".into(),
            updated_at: "2026-10-23T12:00:00Z".into(),
            bundle_id: None,
            member_workspace_ids: vec![],
            member_names: vec![],
            member_project_names: vec![],
            context_folders: vec![],
        }
    }

    #[gpui::test]
    fn completing_a_task_with_running_programs_asks_first(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
            crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        });
        let dir = tempfile::tempdir().unwrap();
        let seen = dir.path().join("seen.json");
        let (sidebar, cx) =
            cx.add_window_view(move |_, cx| Sidebar::new(false, false, false, seen, cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            sidebar.update(cx, |sidebar, cx| {
                sidebar.apply(
                    SidebarData {
                        tasks: vec![task("w-1"), task("w-2")],
                        ..SidebarData::default()
                    },
                    cx,
                );
                sidebar.set_terminals(
                    std::rc::Rc::new(vec![TerminalActivity {
                        pty_id: "pty-1".into(),
                        status: ActivityStatus::Running,
                        process_label: Some("npm run dev".into()),
                        pty_owner_key: Some("terminal:worktree:w-1:t1".into()),
                    }]),
                    cx,
                );
                assert_eq!(sidebar.active_terminals("w-1"), 1);
                assert_eq!(sidebar.active_terminals("w-2"), 0);
                sidebar.run_row_action("w-1", PendingAction::Complete, window, cx);
            })
        });
        cx.run_until_parked();
        let asked = cx.update(|_, cx| {
            let sidebar = sidebar.read(cx);
            let dialog = sidebar
                .lifecycle_dialog()
                .map(|dialog| (dialog.count, dialog.action));
            (dialog, sidebar.pending.contains_key("w-1"))
        });
        assert_eq!(
            asked,
            (Some((1, PendingAction::Complete)), false),
            "nothing ran yet"
        );
        assert!(cx.debug_bounds("terminal-lifecycle-keep").is_some());
        assert!(cx.debug_bounds("sidebar-active-terminals").is_some());
        let cancel = cx.debug_bounds("terminal-lifecycle-cancel").unwrap();
        cx.simulate_click(cancel.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        let closed = cx.update(|_, cx| sidebar.read(cx).dialog.is_none());
        assert!(closed);
    }
}
