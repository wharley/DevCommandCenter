//! The changes of a task that runs on another machine, to read: what is not
//! committed there and what its branch has over its base, each file with its
//! counts; a click opens its diff on the stage, read from the machine's
//! git. Nothing here writes: staging,
//! committing and the delivery stay on the machine for now. Read when the
//! column opens on the task, when a turn of it ends, and on "Atualizar" —
//! no timer.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, img, prelude::*, px};

use super::changes::view::{added_color, status_color};
use super::{CodeColumn, CodeColumnEvent};
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::data::machines::{RemoteChange, RemoteChanges};
use crate::data::tasks::path_leaf;
use crate::i18n::Translate;
use crate::project::file_icon::file_icon;
use crate::screens::stage::DiffRequest;
use crate::theme::ActiveTheme;
use crate::ui::{Icon, spinner, tabular_nums};
use dcc_core::domain::workspace_git::WorkspaceGitPreviewScope;

#[derive(Default)]
pub(crate) struct Remote {
    /// The machine and the task whose changes show.
    pub(super) machine: Option<String>,
    pub(super) task: Option<String>,
    /// `None` until read; an error says why it could not be.
    pub(super) changes: Option<Result<RemoteChanges, String>>,
    pub(super) reading: bool,
    /// The task was running at the last look: its end reads again.
    running: bool,
    /// The task's pull request, from the forge on its machine.
    pub(super) pr: Option<dcc_core::domain::workspace_git::WorkspacePrStatusOutput>,
    /// The machine's default title for a new pull request.
    pub(super) request_title: Option<String>,
    /// The delivery step waiting for the person's yes, and one running.
    pub(super) confirm: Option<crate::data::remote_delivery::RemoteStep>,
    pub(super) delivering: bool,
}

pub(super) fn reason(error: ApiError) -> String {
    match error {
        ApiError::Failed(message) if !message.trim().is_empty() => message,
        other => other.to_string(),
    }
}

impl CodeColumn {
    pub(super) fn remote_api(&self) -> Option<(DesktopApi, String)> {
        let machine = self.remote.machine.as_deref()?;
        let task = self.remote.task.clone()?;
        let api = self.endpoint.clone().or_else(DesktopApi::load)?;
        Some((api.on_machine(machine), task))
    }

    /// The task on screen and the machine it runs on (none for a local
    /// one): another task starts clean and is read.
    pub(crate) fn set_remote_task(
        &mut self,
        machine: Option<String>,
        task: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let task = task.filter(|_| machine.is_some());
        if self.remote.machine == machine && self.remote.task == task {
            return;
        }
        self.remote = Remote {
            machine,
            task,
            ..Remote::default()
        };
        self.read_remote_changes(cx);
        self.load_remote_files(cx);
    }

    /// A turn that ended may have changed files: read again, once.
    pub(crate) fn remote_turn_state(&mut self, running: bool, cx: &mut Context<Self>) {
        let ended = self.remote.running && !running;
        self.remote.running = running;
        if ended && self.remote.task.is_some() {
            self.read_remote_changes(cx);
            // The turn may have created or removed files.
            self.load_remote_files(cx);
        }
    }

    pub(super) fn read_remote_changes(&mut self, cx: &mut Context<Self>) {
        let Some((api, task)) = self.remote_api() else {
            return;
        };
        if self.remote.reading {
            return;
        }
        self.remote.reading = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let asked = task.clone();
            let (read, forge) = cx
                .background_executor()
                .spawn(async move {
                    let read = api.remote_changes(&asked);
                    // The pull request only matters once the list was read.
                    let forge = read
                        .as_ref()
                        .ok()
                        .and_then(|_| api.remote_forge(&asked).ok());
                    (read, forge)
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                // Another task was opened meanwhile.
                if column.remote.task.as_deref() != Some(task.as_str()) {
                    return;
                }
                column.remote.reading = false;
                column.remote.changes = Some(read.map_err(reason));
                if let Some(forge) = forge {
                    column.remote.pr = forge.pull_request;
                    column.remote.request_title = forge.request_title;
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A change's diff on the stage, as a local change opens.
    fn open_remote_diff(
        &mut self,
        change: &RemoteChange,
        scope: WorkspaceGitPreviewScope,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.machine_root.clone() else {
            return;
        };
        let base = self
            .remote
            .changes
            .as_ref()
            .and_then(|read| read.as_ref().ok())
            .map(|changes| changes.base_branch.clone())
            .filter(|base| !base.is_empty());
        cx.emit(CodeColumnEvent::OpenDiff(DiffRequest {
            root,
            name: path_leaf(&change.path).unwrap_or_else(|| change.path.clone()),
            path: change.path.clone(),
            status: change.status.clone(),
            base: base.filter(|_| scope == WorkspaceGitPreviewScope::Committed),
            scope,
            from_changes: true,
            review: None,
        }));
    }
}

fn file_row(
    column: &CodeColumn,
    index: usize,
    change: &RemoteChange,
    scope: &WorkspaceGitPreviewScope,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    // The one whose diff is on the stage.
    let open = column
        .open_diff
        .as_ref()
        .is_some_and(|(open, path)| open == scope && *path == change.path);
    let name = path_leaf(&change.path).unwrap_or_else(|| change.path.clone());
    let folder = super::changes::tree::short_folder(super::changes::view::dirname(&change.path));
    let hover = colors.accent.opacity(0.55);
    let (opened, opened_scope) = (change.clone(), scope.clone());
    let row = div()
        .id(("remote-file", index))
        .debug_selector(move || format!("remote-file-{index}"))
        .flex()
        .w_full()
        .min_h(px(32.0))
        .items_center()
        .gap(px(8.0))
        .rounded(px(6.0))
        .px(px(8.0))
        .text_color(colors.muted_foreground)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .when(open, |row| row.bg(colors.primary.opacity(0.08)))
        .on_click(cx.listener(move |column, _, _, cx| {
            column.open_remote_diff(&opened, opened_scope.clone(), cx)
        }))
        .child(img(file_icon(&name)).size(px(14.0)).flex_shrink_0())
        .child(
            div()
                .min_w_0()
                .max_w(gpui::relative(0.58))
                .truncate()
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.foreground)
                .child(name),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_right()
                .text_size(px(10.0))
                .child(folder),
        )
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(4.0))
                .text_size(px(10.0))
                .font_features(tabular_nums())
                .when(change.insertions > 0, |stats| {
                    stats.child(
                        div()
                            .text_color(added_color(cx))
                            .child(format!("+{}", change.insertions)),
                    )
                })
                .when(change.deletions > 0, |stats| {
                    stats.child(
                        div()
                            .text_color(colors.destructive)
                            .child(format!("−{}", change.deletions)),
                    )
                })
                .child(
                    div()
                        .min_w(px(12.0))
                        .text_center()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(status_color(&change.status, cx))
                        .child(change.status.clone()),
                ),
        );
    div().w_full().px(px(4.0)).child(row).into_any_element()
}

fn group(
    column: &CodeColumn,
    title: SharedString,
    changes: &[RemoteChange],
    offset: usize,
    committed: bool,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    if changes.is_empty() {
        return None;
    }
    let colors = cx.theme().colors.clone();
    let rows: Vec<AnyElement> = changes
        .iter()
        .enumerate()
        .map(|(index, change)| {
            let scope = match (committed, change.staged) {
                (true, _) => WorkspaceGitPreviewScope::Committed,
                (false, true) => WorkspaceGitPreviewScope::Staged,
                (false, false) => WorkspaceGitPreviewScope::Unstaged,
            };
            file_row(column, offset + index, change, &scope, cx)
        })
        .collect();
    Some(
        div()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(12.0))
                    .pt(px(10.0))
                    .pb(px(4.0))
                    .text_size(px(11.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.muted_foreground)
                    .child(title)
                    .child(
                        div()
                            .font_weight(FontWeight::NORMAL)
                            .child(changes.len().to_string()),
                    ),
            )
            .children(rows)
            .into_any_element(),
    )
}

/// The column's body for a task on another machine.
pub(super) fn remote_changes(column: &CodeColumn, cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let remote = &column.remote;
    let quiet = |text: SharedString| {
        div()
            .flex()
            .flex_1()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .px(px(24.0))
            .text_center()
            .text_size(px(12.0))
            .line_height(px(18.0))
            .text_color(colors.muted_foreground)
            .child(Icon::new("server").size(20.0).stroke(1.6))
            .child(text)
            .into_any_element()
    };
    let header = div()
        .flex()
        .flex_none()
        .h(px(36.0))
        .items_center()
        .gap(px(8.0))
        .px(px(12.0))
        .border_b_1()
        .border_color(colors.border.opacity(0.5))
        .child(
            Icon::new("server")
                .size(14.0)
                .stroke(1.8)
                .color(colors.muted_foreground),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .child(
                    match remote.changes.as_ref().and_then(|read| read.as_ref().ok()) {
                        Some(changes) => changes
                            .branch
                            .clone()
                            .unwrap_or_else(|| changes.base_branch.clone())
                            .into(),
                        None => cx.t("codeColumn.remote.title"),
                    },
                ),
        )
        .child(
            div()
                .id("remote-changes-refresh")
                .debug_selector(|| "remote-changes-refresh".into())
                .flex()
                .size(px(24.0))
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .text_color(colors.muted_foreground)
                .cursor_pointer()
                .hover(|style| style.bg(colors.muted.opacity(0.4)))
                .on_click(cx.listener(|column, _, _, cx| column.read_remote_changes(cx)))
                .child(if remote.reading {
                    spinner("remote-changes-reading", 14.0).into_any_element()
                } else {
                    Icon::new("refresh-cw").size(14.0).into_any_element()
                }),
        );
    let body = match &remote.changes {
        None => quiet(cx.t("codeColumn.remote.reading")),
        Some(Err(error)) => quiet(error.clone().into()),
        Some(Ok(changes)) if changes.is_empty() => quiet(cx.t("codeColumn.remote.none")),
        Some(Ok(changes)) => {
            let local = group(
                column,
                cx.t("codeColumn.remote.uncommitted"),
                &changes.changes,
                0,
                false,
                cx,
            );
            let branch = group(
                column,
                cx.t("codeColumn.remote.onBranch"),
                &changes.branch_changes,
                changes.changes.len(),
                true,
                cx,
            );
            div()
                .id("remote-changes-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .pb(px(12.0))
                .text_size(px(12.0))
                .children(local)
                .children(branch)
                .into_any_element()
        }
    };
    div()
        .debug_selector(|| "code-column-on-machine".into())
        .flex()
        .flex_1()
        .min_h_0()
        .flex_col()
        .child(header)
        .children(super::remote_delivery::delivery_strip(column, cx))
        .child(body)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};

    use super::*;
    use crate::screens::code_column::CodeColumnTab;
    use crate::test_support::fake_desktop_routes;
    use crate::theme::{Theme, ThemePreference};

    const DELIVERY: &str = "/api/v1/machines/m-1/proxy/api/v1/mobile/workspaces/w-1/delivery";
    const FORGE: &str = "/api/v1/machines/m-1/proxy/api/v1/mobile/workspaces/w-1/delivery/forge";
    const COMMIT: &str = "/api/v1/machines/m-1/proxy/api/v1/workspaces/w-1/delivery/commit";

    #[gpui::test]
    fn a_task_on_a_machine_lists_its_changes_and_opens_a_files_diff(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
            crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        });
        let changes = r#"{"workspaceId":"w-1","branch":"dcc/checkout","baseBranch":"main",
            "changesToken":"tok-1","commitMessage":"feat: pay",
            "changes":[{"path":"src/app/pay.ts","status":"M","insertions":4,"deletions":1,"staged":false}],
            "branchChanges":[{"path":"README.md","status":"A","insertions":9,"deletions":0,"staged":false}]}"#;
        let forge = r#"{"pullRequest":null,"pipeline":null,"requestTitle":"Checkout"}"#;
        let (port, desktop) = fake_desktop_routes(vec![
            (DELIVERY, 200, changes.into()),
            (FORGE, 200, forge.into()),
            (COMMIT, 200, r#"{"ok":true}"#.into()),
            (DELIVERY, 200, changes.into()),
            (FORGE, 200, forge.into()),
        ]);
        let (column, cx) = cx.add_window_view(|_, cx| CodeColumn::new(CodeColumnTab::Changes, cx));
        cx.simulate_resize(size(px(380.0), px(700.0)));
        column.update(cx, |column, cx| {
            column.endpoint = Some(DesktopApi::new(port, "k"));
            // A path on a machine: nothing of this computer is read.
            column.set_root(Some("dcc-machine://m-1/srv/app".into()), cx);
            assert!(column.on_machine && column.root.is_none());
            column.set_remote_task(Some("m-1".into()), Some("w-1".into()), cx);
        });
        cx.run_until_parked();
        column.read_with(cx, |column, _| {
            let read = column.remote.changes.clone().unwrap().unwrap();
            assert_eq!(read.branch.as_deref(), Some("dcc/checkout"));
            assert_eq!((read.changes.len(), read.branch_changes.len()), (1, 1));
        });
        assert!(cx.debug_bounds("code-column-on-machine").is_some());
        let file = cx.debug_bounds("remote-file-0").expect("the changed file");
        assert!(file.right() <= px(380.0));
        assert!(
            cx.debug_bounds("remote-file-1").is_some(),
            "the branch's file"
        );
        // A click asks for the file's diff on the stage, on the machine's folder.
        let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = opened.clone();
        let _events = cx.update(|_, cx| {
            cx.subscribe(&column, move |_, event: &CodeColumnEvent, _| {
                if let CodeColumnEvent::OpenDiff(request) = event {
                    seen.borrow_mut().push(request.clone());
                }
            })
        });
        cx.simulate_click(file.center(), Modifiers::none());
        let branch_file = cx.debug_bounds("remote-file-1").unwrap();
        cx.simulate_click(branch_file.center(), Modifiers::none());
        cx.run_until_parked();
        let opened = opened.borrow();
        assert_eq!(opened.len(), 2);
        assert_eq!(opened[0].root, "dcc-machine://m-1/srv/app");
        assert_eq!(
            (opened[0].path.as_str(), &opened[0].scope, &opened[0].base),
            ("src/app/pay.ts", &WorkspaceGitPreviewScope::Unstaged, &None)
        );
        // The branch's file reads against the machine's base.
        assert_eq!(
            (
                opened[1].path.as_str(),
                &opened[1].scope,
                opened[1].base.as_deref()
            ),
            (
                "README.md",
                &WorkspaceGitPreviewScope::Committed,
                Some("main")
            )
        );
        // The rail says where it stands; the next step runs on the machine
        // only after the person's yes.
        assert!(cx.debug_bounds("remote-rail").is_some());
        let step = cx
            .debug_bounds("remote-delivery-step")
            .expect("Commitar e enviar");
        cx.simulate_click(step.center(), Modifiers::none());
        cx.run_until_parked();
        column.read_with(cx, |column, _| {
            assert_eq!(
                column.remote.confirm,
                Some(crate::data::remote_delivery::RemoteStep::CommitAndPush)
            );
            assert!(!column.remote.delivering, "nothing ran yet");
        });
        column.update(cx, |column, cx| column.run_remote_step_for_tests(cx));
        cx.run_until_parked();
        let asked = desktop.join().unwrap();
        cx.run_until_parked();
        let (_, sent) = asked
            .iter()
            .find(|(request, _)| request.ends_with("/delivery/commit"))
            .expect("the commit went to the machine");
        let sent: serde_json::Value = serde_json::from_str(sent).unwrap();
        assert_eq!(
            sent,
            serde_json::json!({ "push": true, "changesToken": "tok-1" })
        );
        // A local task again: the column is this computer's.
        column.update(cx, |column, cx| {
            column.set_root(None, cx);
            column.set_remote_task(None, None, cx);
            assert!(!column.on_machine && column.remote.task.is_none());
        });
    }
}
