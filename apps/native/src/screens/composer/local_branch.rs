//! `LocalBranchPicker`: in a local-direct task the branch under the
//! composer opens the checkout's branches — a search, the local ones, the
//! remote ones (fetched on open) and a new one from the current — and moves
//! the checkout there before the conversation starts. Once it starts, or
//! while an agent works in the folder, it only shows the branch, locked.

use dcc_tauri::commands::local_branches::{LocalBranchEntry, LocalBranchesOutput};
use gpui::{
    AnyElement, AppContext, Context, Entity, Focusable, FontWeight, SharedString, Task, Window,
    div, prelude::*, px,
};

use crate::data::desktop_api::DesktopApi;
use crate::data::local_branches::BranchTarget;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::popover::{Popover, PopoverAlign, PopoverSide};
use crate::ui::{Icon, TextInput, TextInputEvent, Tooltip, spinner};

impl super::Composer {
    /// The picker of the task shown, when it runs on the project's own
    /// checkout; a protected worktree keeps the plain branch label.
    pub(super) fn sync_local_branch(&mut self, cx: &mut Context<Self>) {
        let session = &self.session;
        let local = session.context.as_ref().filter(|context| !context.isolated);
        let (Some(context), Some(task_id)) = (local, session.task_id.clone()) else {
            self.local_branch = None;
            return;
        };
        let fallback = context
            .current_branch
            .clone()
            .or_else(|| context.base_branch.clone());
        let started = session.start.is_none() || session.running;
        if self
            .local_branch
            .as_ref()
            .is_none_or(|picker| picker.read(cx).task_id() != task_id)
        {
            let api = if cfg!(test) { None } else { DesktopApi::load() };
            self.local_branch = Some(cx.new(|cx| LocalBranchPicker::new(task_id, api, cx)));
        }
        if let Some(picker) = &self.local_branch {
            picker.update(cx, |picker, cx| picker.set_state(fallback, started, cx));
        }
    }
}

pub struct LocalBranchPicker {
    task_id: String,
    api: Option<DesktopApi>,
    /// The branch the conversation reports, until the list is read.
    fallback: Option<String>,
    /// The conversation started (or its first turn is on its way).
    started: bool,
    data: Option<LocalBranchesOutput>,
    load_failed: bool,
    open: bool,
    search: Option<Entity<TextInput>>,
    /// The new branch's name, while one is being made.
    name: Option<Entity<TextInput>>,
    refreshing: bool,
    switching: bool,
    error: Option<String>,
    _read: Option<Task<()>>,
    _switch: Option<Task<()>>,
}

impl LocalBranchPicker {
    pub fn new(task_id: String, api: Option<DesktopApi>, cx: &mut Context<Self>) -> Self {
        let mut picker = Self {
            task_id,
            api,
            fallback: None,
            started: false,
            data: None,
            load_failed: false,
            open: false,
            search: None,
            name: None,
            refreshing: false,
            switching: false,
            error: None,
            _read: None,
            _switch: None,
        };
        picker.read(false, cx);
        picker
    }

    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// What the conversation knows: its branch, and whether it started.
    pub fn set_state(&mut self, fallback: Option<String>, started: bool, cx: &mut Context<Self>) {
        if self.fallback == fallback && self.started == started {
            return;
        }
        self.fallback = fallback;
        if self.started != started {
            self.started = started;
            self.open = false;
            self.read(false, cx);
        }
        cx.notify();
    }

    fn locked(&self) -> bool {
        self.started || self.data.as_ref().is_some_and(|data| data.has_conversation)
    }

    fn disabled(&self) -> bool {
        self.locked() || self.switching || self.data.as_ref().is_none_or(|data| data.agent_running)
    }

    /// The list (`refresh` fetches the remotes first).
    fn read(&mut self, refresh: bool, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            return;
        };
        let task = self.task_id.clone();
        self.refreshing = refresh;
        self._read = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.local_branches(&task, refresh) })
                .await;
            let _ = this.update(cx, |picker, cx| {
                picker.refreshing = false;
                match read {
                    Ok(data) => {
                        picker.data = Some(data);
                        picker.load_failed = false;
                    }
                    Err(_) => picker.load_failed = picker.data.is_none(),
                }
                if picker.disabled() {
                    picker.open = false;
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if open && (self.open || self.disabled()) {
            return;
        }
        self.open = open;
        self.name = None;
        self.error = None;
        if open {
            let placeholder = cx.t("localBranch.search");
            let search = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
            cx.subscribe(&search, |_, _, event: &TextInputEvent, cx| {
                if matches!(event, TextInputEvent::Changed) {
                    cx.notify();
                }
            })
            .detach();
            window.focus(&search.read(cx).focus_handle(cx), cx);
            self.search = Some(search);
            self.read(true, cx);
        } else {
            self.search = None;
        }
        cx.notify();
    }

    fn start_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.query(cx);
        let name = cx.new(|cx| {
            let mut input = TextInput::new(window, cx).with_placeholder("feature/minha-branch");
            input.set_text(typed, cx);
            input
        });
        cx.subscribe(
            &name,
            |picker, input, event: &TextInputEvent, cx| match event {
                TextInputEvent::Submitted => {
                    let name = input.read(cx).text().trim().to_string();
                    if !name.is_empty() {
                        picker.switch(BranchTarget::New(name), cx);
                    }
                }
                TextInputEvent::Changed => cx.notify(),
                _ => {}
            },
        )
        .detach();
        window.focus(&name.read(cx).focus_handle(cx), cx);
        self.name = Some(name);
        cx.notify();
    }

    fn switch(&mut self, target: BranchTarget, cx: &mut Context<Self>) {
        let (Some(api), Some(data)) = (self.api.clone(), self.data.as_ref()) else {
            return;
        };
        if self.disabled() {
            return;
        }
        let expected = data.current_branch.clone();
        let task = self.task_id.clone();
        self.switching = true;
        self.error = None;
        self._switch = Some(cx.spawn(async move |this, cx| {
            let switched = cx
                .background_executor()
                .spawn(async move { api.switch_local_branch(&task, expected.as_deref(), &target) })
                .await;
            let _ = this.update(cx, |picker, cx| {
                picker.switching = false;
                match switched {
                    Ok(data) => {
                        picker.data = Some(data);
                        picker.open = false;
                        picker.search = None;
                        picker.name = None;
                    }
                    Err(error) => {
                        picker.error = Some(error.text(cx));
                        picker.read(false, cx);
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn query(&self, cx: &gpui::App) -> String {
        self.search
            .as_ref()
            .map(|search| search.read(cx).text().trim().to_string())
            .unwrap_or_default()
    }

    fn label(&self, cx: &gpui::App) -> Option<String> {
        match &self.data {
            Some(data) => Some(
                data.current_branch
                    .clone()
                    .unwrap_or_else(|| cx.t("localBranch.detached").to_string()),
            ),
            None => self.fallback.clone(),
        }
    }

    fn row(&self, entry: &LocalBranchEntry, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let current = !entry.remote
            && self
                .data
                .as_ref()
                .and_then(|data| data.current_branch.as_deref())
                == Some(entry.name.as_str());
        let reference = entry.reference.clone();
        div()
            .id(SharedString::from(format!(
                "local-branch-{}",
                entry.reference
            )))
            .debug_selector({
                let reference = entry.reference.clone();
                move || format!("local-branch-{reference}")
            })
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(6.0))
            .px(px(8.0))
            .py(px(6.0))
            .text_size(px(11.0))
            .cursor_pointer()
            .hover(move |style| style.bg(colors.muted))
            .on_click(cx.listener(move |picker, _, _, cx| {
                picker.switch(BranchTarget::Existing(reference.clone()), cx)
            }))
            .child(Icon::new("git-branch").size(14.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(entry.name.clone()),
            )
            .when(current, |row| row.child(Icon::new("check").size(14.0)))
            .into_any_element()
    }

    fn panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let query = self.query(cx).to_lowercase();
        let entries: Vec<LocalBranchEntry> = self
            .data
            .as_ref()
            .map(|data| {
                data.branches
                    .iter()
                    .filter(|entry| entry.name.to_lowercase().contains(&query))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let small = |text: SharedString, color: gpui::Hsla| {
            div()
                .px(px(8.0))
                .py(px(4.0))
                .text_size(px(10.0))
                .text_color(color)
                .child(text)
        };
        let mut rows = Vec::new();
        for remote in [false, true] {
            let group: Vec<&LocalBranchEntry> = entries
                .iter()
                .filter(|entry| entry.remote == remote)
                .collect();
            if group.is_empty() {
                continue;
            }
            let heading = if remote {
                "localBranch.remote"
            } else {
                "localBranch.local"
            };
            rows.push(small(cx.t(heading), colors.muted_foreground).into_any_element());
            for entry in group {
                rows.push(self.row(entry, cx));
            }
        }
        if entries.is_empty() {
            rows.push(small(cx.t("localBranch.empty"), colors.muted_foreground).into_any_element());
        }
        let refresh = div()
            .id("local-branch-refresh")
            .flex()
            .size(px(28.0))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .cursor_pointer()
            .hover(|style| style.bg(colors.muted))
            .on_click(cx.listener(|picker, _, _, cx| {
                if !picker.refreshing {
                    picker.read(true, cx);
                }
            }))
            .child(if self.refreshing {
                spinner("local-branch-refreshing", 14.0).into_any_element()
            } else {
                Icon::new("refresh-cw").size(14.0).into_any_element()
            });
        let new_branch = match &self.name {
            Some(name) => {
                let from = cx.t_with(
                    "localBranch.from",
                    &Args::default().with("branch", self.label(cx).unwrap_or_default()),
                );
                let typed = name.read(cx).text().trim().to_string();
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(small(from, colors.muted_foreground))
                    .child(
                        div()
                            .h(px(30.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(colors.border)
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .id("local-branch-create")
                            .debug_selector(|| "local-branch-create".into())
                            .flex()
                            .h(px(28.0))
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            .bg(colors.primary)
                            .text_color(colors.primary_foreground)
                            .text_size(px(11.0))
                            .font_weight(FontWeight::MEDIUM)
                            .when(typed.is_empty(), |button| button.opacity(0.5))
                            .when(!typed.is_empty(), |button| button.cursor_pointer())
                            .on_click(cx.listener(move |picker, _, _, cx| {
                                if !typed.is_empty() {
                                    picker.switch(BranchTarget::New(typed.clone()), cx);
                                }
                            }))
                            .child(cx.t("localBranch.create")),
                    )
                    .into_any_element()
            }
            None => div()
                .id("local-branch-new")
                .debug_selector(|| "local-branch-new".into())
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded(px(6.0))
                .px(px(8.0))
                .py(px(6.0))
                .text_size(px(11.0))
                .cursor_pointer()
                .hover(|style| style.bg(colors.muted))
                .on_click(cx.listener(|picker, _, window, cx| picker.start_new(window, cx)))
                .child(Icon::new("plus").size(14.0))
                .child(cx.t("localBranch.create"))
                .into_any_element(),
        };
        let refresh_failed = self
            .data
            .as_ref()
            .is_some_and(|data| data.refresh_error.is_some());
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        div()
                            .flex_1()
                            .h(px(30.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(colors.border)
                            .children(self.search.clone()),
                    )
                    .child(refresh),
            )
            .when(refresh_failed, |panel| {
                panel.child(small(
                    cx.t("localBranch.refreshFailed"),
                    tw::AMBER_600.hsla(),
                ))
            })
            .child(
                div()
                    .id("local-branch-list")
                    .max_h(px(256.0))
                    .overflow_y_scroll()
                    .children(rows),
            )
            .child(
                div()
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(colors.border)
                    .child(new_branch),
            )
            .children(self.error.clone().map(|error| {
                div()
                    .px(px(8.0))
                    .text_size(px(10.5))
                    .text_color(colors.destructive)
                    .child(error)
            }))
            .into_any_element()
    }
}

impl Render for LocalBranchPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let Some(label) = self.label(cx) else {
            return div().into_any_element();
        };
        let disabled = self.disabled();
        let busy = self.switching || (self.data.is_none() && !self.load_failed);
        let hover = colors.muted.opacity(0.45);
        let trigger = div()
            .id("local-branch-trigger")
            .debug_selector(|| "local-branch-trigger".into())
            .flex()
            .min_w_0()
            .items_center()
            .gap(px(6.0))
            .rounded(px(6.0))
            .px(px(6.0))
            .py(px(4.0))
            .when(!disabled, move |trigger| {
                trigger.cursor_pointer().hover(move |style| style.bg(hover))
            })
            .child(if busy {
                spinner("local-branch-busy", 14.0).into_any_element()
            } else {
                Icon::new("git-branch").size(14.0).into_any_element()
            })
            .child(div().min_w_0().truncate().child(label))
            .when(self.locked(), |trigger| {
                trigger.child(Icon::new("lock-keyhole").size(12.0))
            });
        let control = if disabled {
            let reason = if self.locked() {
                cx.t("localBranch.locked")
            } else if self.data.as_ref().is_some_and(|data| data.agent_running) {
                cx.t("localBranch.agentRunning")
            } else {
                cx.t("localBranch.choose")
            };
            Tooltip::text("local-branch-tip", trigger, reason)
                .side(crate::ui::tooltip::Side::Top)
                .into_any_element()
        } else {
            let content = cx.entity().downgrade();
            let toggle = cx.entity().downgrade();
            Popover::new(
                "local-branch-picker",
                self.open,
                trigger,
                move |_, cx| {
                    content
                        .update(cx, |picker, cx| picker.panel(cx))
                        .unwrap_or_else(|_| div().into_any_element())
                },
                move |open, window, cx| {
                    let _ = toggle.update(cx, |picker, cx| picker.set_open(open, window, cx));
                },
            )
            .side(PopoverSide::Top)
            .align(PopoverAlign::End)
            .width(320.0)
            .padding(8.0)
            .into_any_element()
        };
        let mismatch = self.data.as_ref().filter(|data| {
            data.has_conversation && data.current_branch.as_deref() != Some(&data.task_branch)
        });
        div()
            .flex()
            .min_w_0()
            .flex_col()
            .items_end()
            .gap(px(4.0))
            .child(control)
            .children(mismatch.map(|data| {
                let actual = data
                    .current_branch
                    .clone()
                    .unwrap_or_else(|| cx.t("localBranch.detached").to_string());
                div()
                    .max_w(px(360.0))
                    .text_right()
                    .text_size(px(10.5))
                    .text_color(tw::AMBER_600.hsla())
                    .child(
                        cx.t_with(
                            "localBranch.mismatch",
                            &Args::default()
                                .with("expected", data.task_branch.clone())
                                .with("actual", actual),
                        ),
                    )
            }))
            .when(self.load_failed, |column| {
                column.child(
                    div()
                        .id("local-branch-retry")
                        .cursor_pointer()
                        .text_size(px(10.5))
                        .text_color(colors.destructive)
                        .on_click(cx.listener(|picker, _, _, cx| picker.read(false, cx)))
                        .child(cx.t("localBranch.retry")),
                )
            })
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "local_branch_tests.rs"]
mod tests;
