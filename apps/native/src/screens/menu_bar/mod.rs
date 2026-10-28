//! The menu bar panel (`MenuBar`): "DCC Atividade" and refresh; how many run
//! and wait; the keep-awake hold; the tasks needing the person, running,
//! then the recent ones, each opening its conversation; the DCC processes'
//! CPU and memory; "Nova tarefa", "Abrir DCC" and "Sair do DCC". It reads
//! the desktop every 3s while it shows; Esc and losing the focus close it.

#[cfg(test)]
mod tests;
mod view;

use std::time::Duration;

use gpui::{Context, FocusHandle, Focusable, Subscription, Task, Window};

use crate::data::desktop_api::DesktopApi;
use crate::data::menu_bar::{Snapshot, Task as MenuTask};

pub const WIDTH: f32 = 368.0;
pub const HEIGHT: f32 = 480.0;
/// `setTimeout(poll, 3000)`
const POLL: Duration = Duration::from_secs(3);

pub struct MenuBarPanel {
    pub api: Option<DesktopApi>,
    /// `None` while the first read runs.
    pub snapshot: Option<Snapshot>,
    pub error: bool,
    pub action_error: bool,
    pub busy: bool,
    focus: FocusHandle,
    _poll: Option<Task<()>>,
    _activation: Option<Subscription>,
}

impl Focusable for MenuBarPanel {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl MenuBarPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let activation = cx.observe_window_activation(window, |_, window, cx| {
            if !window.is_window_active() {
                window.remove_window();
                crate::app::menu_bar::panel_left(cx);
            }
        });
        let mut panel = Self {
            api: if cfg!(test) { None } else { DesktopApi::load() },
            snapshot: None,
            error: false,
            action_error: false,
            busy: false,
            focus,
            _poll: None,
            _activation: Some(activation),
        };
        panel.start(cx);
        panel
    }

    /// The reads every 3s, from now.
    pub fn start(&mut self, cx: &mut Context<Self>) {
        self._poll = Some(cx.spawn(async move |this, cx| {
            loop {
                let Ok(api) = this.update(cx, |panel, _| panel.api.clone()) else {
                    return;
                };
                let read = match api {
                    Some(api) => {
                        cx.background_executor()
                            .spawn(async move { api.menu_bar_snapshot() })
                            .await
                    }
                    None => Err(crate::data::desktop_api::ApiError::DesktopClosed),
                };
                if this
                    .update(cx, |panel, cx| {
                        match read {
                            Ok(snapshot) => {
                                panel.snapshot = Some(snapshot);
                                panel.error = false;
                            }
                            // The last snapshot stays, marked stale.
                            Err(_) => panel.error = true,
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
                cx.background_executor().timer(POLL).await;
            }
        }));
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.start(cx);
    }

    pub fn running(&self) -> usize {
        self.tasks()
            .iter()
            .filter(|task| task.status == "running")
            .count()
    }

    pub fn waiting(&self) -> usize {
        self.tasks().iter().filter(|task| task.waiting()).count()
    }

    pub fn tasks(&self) -> &[MenuTask] {
        self.snapshot
            .as_ref()
            .map_or(&[], |snapshot| &snapshot.tasks)
    }

    /// The panel goes (its own window, from inside).
    fn leave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self._activation = None;
        crate::app::menu_bar::forget_panel(cx);
        window.remove_window();
    }

    /// `menu_bar_open_main`: the panel goes, the window comes, on the task.
    pub fn open(&mut self, task: Option<MenuTask>, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if cx
            .try_global::<crate::app::menu_bar::MainWindow>()
            .is_none()
        {
            self.action_error = true;
            cx.notify();
            return;
        }
        self.leave(window, cx);
        let Some(main) = crate::app::menu_bar::show_main(cx) else {
            return;
        };
        if let Some(task) = task {
            let _ = main.update(cx, |shell, _, cx| {
                shell.open_from_menu_bar(task.workspace_id, Some(task.session_id), cx)
            });
        }
    }

    /// `menu_bar_compose`: the Quick Composer (the desktop's, for now).
    pub fn compose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            self.action_error = true;
            cx.notify();
            return;
        };
        self.busy = true;
        self.action_error = false;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { api.toggle_quick_composer() })
                .await;
            let _ = this.update_in(cx, |panel, window, cx| {
                panel.busy = false;
                match done {
                    Ok(()) => panel.leave(window, cx),
                    Err(_) => {
                        panel.action_error = true;
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    pub fn quit(&mut self, cx: &mut Context<Self>) {
        cx.quit();
    }

    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.leave(window, cx);
    }
}
