//! The changes of a task that runs on another machine, to read: what is not
//! committed there and what its branch has over its base, each file with its
//! counts, and its diff under it on a click. Nothing here writes: staging,
//! committing and the delivery stay on the machine for now. Read when the
//! column opens on the task, when a turn of it ends, and on "Atualizar" —
//! no timer.

use std::collections::HashMap;

use gpui::{AnyElement, Context, FontWeight, SharedString, div, img, prelude::*, px};

use super::CodeColumn;
use super::changes::view::{added_color, status_color};
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::data::machines::{RemoteChange, RemoteChanges};
use crate::data::pr_review::{PatchRow, RowKind, parse_patch};
use crate::data::tasks::path_leaf;
use crate::i18n::Translate;
use crate::project::file_icon::file_icon;
use crate::theme::ActiveTheme;
use crate::ui::{Icon, spinner, tabular_nums};

/// A diff's lines shown at most; the rest is read on the machine.
const MAX_PATCH_ROWS: usize = 400;

/// A file's diff, once asked for.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Patch {
    Reading,
    Read { rows: Vec<PatchRow>, cut: bool },
    Failed(String),
}

#[derive(Default)]
pub(crate) struct Remote {
    /// The machine and the task whose changes show.
    pub(super) machine: Option<String>,
    pub(super) task: Option<String>,
    /// `None` until read; an error says why it could not be.
    pub(super) changes: Option<Result<RemoteChanges, String>>,
    pub(super) reading: bool,
    /// The file whose diff is open.
    pub(super) open: Option<String>,
    pub(super) patches: HashMap<String, Patch>,
    /// The task was running at the last look: its end reads again.
    running: bool,
}

fn reason(error: ApiError) -> String {
    match error {
        ApiError::Failed(message) if !message.trim().is_empty() => message,
        other => other.to_string(),
    }
}

impl CodeColumn {
    fn remote_api(&self) -> Option<(DesktopApi, String)> {
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
    }

    /// A turn that ended may have changed files: read again, once.
    pub(crate) fn remote_turn_state(&mut self, running: bool, cx: &mut Context<Self>) {
        let ended = self.remote.running && !running;
        self.remote.running = running;
        if ended && self.remote.task.is_some() {
            self.read_remote_changes(cx);
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
            let read = cx
                .background_executor()
                .spawn(async move { api.remote_changes(&asked) })
                .await;
            let _ = this.update(cx, |column, cx| {
                // Another task was opened meanwhile.
                if column.remote.task.as_deref() != Some(task.as_str()) {
                    return;
                }
                column.remote.reading = false;
                column.remote.changes = Some(read.map_err(reason));
                // What was read before may be stale now.
                let open = column.remote.open.clone();
                column.remote.patches.clear();
                if let Some(path) = open {
                    column.read_remote_patch(path, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn toggle_remote_file(&mut self, path: String, cx: &mut Context<Self>) {
        if self.remote.open.as_deref() == Some(path.as_str()) {
            self.remote.open = None;
            cx.notify();
            return;
        }
        self.remote.open = Some(path.clone());
        if !self.remote.patches.contains_key(&path) {
            self.read_remote_patch(path, cx);
        }
        cx.notify();
    }

    fn read_remote_patch(&mut self, path: String, cx: &mut Context<Self>) {
        let Some((api, task)) = self.remote_api() else {
            return;
        };
        self.remote.patches.insert(path.clone(), Patch::Reading);
        cx.spawn(async move |this, cx| {
            let (asked_task, asked_path) = (task.clone(), path.clone());
            let read = cx
                .background_executor()
                .spawn(async move { api.remote_patch(&asked_task, &asked_path) })
                .await;
            let _ = this.update(cx, |column, cx| {
                if column.remote.task.as_deref() != Some(task.as_str()) {
                    return;
                }
                let patch = match read {
                    Ok((text, truncated)) => {
                        let mut rows = parse_patch(Some(&text));
                        let cut = truncated || rows.len() > MAX_PATCH_ROWS;
                        rows.truncate(MAX_PATCH_ROWS);
                        Patch::Read { rows, cut }
                    }
                    Err(error) => Patch::Failed(reason(error)),
                };
                column.remote.patches.insert(path, patch);
                cx.notify();
            });
        })
        .detach();
    }
}

fn patch_lines(patch: &Patch, cx: &Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let mono = cx.theme().fonts.mono.clone();
    let quiet = |text: SharedString| {
        div()
            .px(px(12.0))
            .py(px(8.0))
            .text_size(px(11.0))
            .text_color(colors.muted_foreground)
            .child(text)
            .into_any_element()
    };
    match patch {
        Patch::Reading => div()
            .px(px(12.0))
            .py(px(8.0))
            .child(spinner("remote-patch-reading", 14.0))
            .into_any_element(),
        Patch::Failed(error) => quiet(error.clone().into()),
        Patch::Read { rows, .. } if rows.is_empty() => quiet(cx.t("codeColumn.remote.noDiff")),
        Patch::Read { rows, cut } => {
            let added = added_color(cx);
            let lines = rows.iter().map(|row| {
                let (mark, color, background) = match row.kind {
                    RowKind::Addition => ("+", added, Some(added.opacity(0.1))),
                    RowKind::Deletion => (
                        "−",
                        colors.destructive,
                        Some(colors.destructive.opacity(0.1)),
                    ),
                    RowKind::Hunk => ("", colors.muted_foreground, Some(colors.muted.opacity(0.4))),
                    RowKind::Meta => ("", colors.muted_foreground, None),
                    RowKind::Context => (" ", colors.foreground.opacity(0.8), None),
                };
                div()
                    .flex()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .when_some(background, |line, background| line.bg(background))
                    .text_color(color)
                    .child(div().flex_none().w(px(8.0)).child(mark))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(row.content.clone()),
                    )
            });
            div()
                .debug_selector(|| "remote-patch".into())
                .py(px(4.0))
                .font_family(mono)
                .text_size(px(11.0))
                .line_height(px(17.0))
                .children(lines)
                .when(*cut, |patch| {
                    patch.child(
                        div()
                            .px(px(12.0))
                            .py(px(6.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("codeColumn.remote.cut")),
                    )
                })
                .into_any_element()
        }
    }
}

fn file_row(
    column: &CodeColumn,
    index: usize,
    change: &RemoteChange,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let open = column.remote.open.as_deref() == Some(change.path.as_str());
    let name = path_leaf(&change.path).unwrap_or_else(|| change.path.clone());
    let folder = super::changes::tree::short_folder(super::changes::view::dirname(&change.path));
    let hover = colors.accent.opacity(0.55);
    let path = change.path.clone();
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
        .on_click(cx.listener(move |column, _, _, cx| column.toggle_remote_file(path.clone(), cx)))
        .child(
            Icon::new("chevron-right")
                .size(12.0)
                .rotate(if open { 90.0 } else { 0.0 }),
        )
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
    let patch = open
        .then(|| column.remote.patches.get(&change.path))
        .flatten()
        .map(|patch| patch_lines(patch, cx));
    div()
        .w_full()
        .px(px(4.0))
        .child(row)
        .children(patch)
        .into_any_element()
}

fn group(
    column: &CodeColumn,
    title: SharedString,
    changes: &[RemoteChange],
    offset: usize,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    if changes.is_empty() {
        return None;
    }
    let colors = cx.theme().colors.clone();
    let rows: Vec<AnyElement> = changes
        .iter()
        .enumerate()
        .map(|(index, change)| file_row(column, offset + index, change, cx))
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
                cx,
            );
            let branch = group(
                column,
                cx.t("codeColumn.remote.onBranch"),
                &changes.branch_changes,
                changes.changes.len(),
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
                .child(
                    div()
                        .px(px(12.0))
                        .pt(px(12.0))
                        .text_size(px(11.0))
                        .line_height(px(17.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("codeColumn.remote.readOnly")),
                )
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
    // The fake desktop matches the whole address, query included.
    const PATCH: &str =
        "/api/v1/machines/m-1/proxy/api/v1/mobile/workspaces/w-1/patch?path=src%2Fapp%2Fpay.ts";

    #[gpui::test]
    fn a_task_on_a_machine_lists_its_changes_and_opens_a_files_diff(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
            crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        });
        let changes = r#"{"workspaceId":"w-1","branch":"dcc/checkout","baseBranch":"main",
            "changes":[{"path":"src/app/pay.ts","status":"M","insertions":4,"deletions":1,"staged":false}],
            "branchChanges":[{"path":"README.md","status":"A","insertions":9,"deletions":0,"staged":false}]}"#;
        let patch = r#"{"path":"src/app/pay.ts","truncated":false,
            "patch":"diff --git a/src/app/pay.ts b/src/app/pay.ts\n@@ -1,2 +1,2 @@\n-old\n+new\n same\n"}"#;
        let (port, desktop) = fake_desktop_routes(vec![
            (DELIVERY, 200, changes.into()),
            (PATCH, 200, patch.into()),
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
        cx.simulate_click(file.center(), Modifiers::none());
        cx.run_until_parked();
        column.read_with(cx, |column, _| {
            let Some(Patch::Read { rows, cut }) = column.remote.patches.get("src/app/pay.ts")
            else {
                panic!("the file's diff");
            };
            assert!(!cut);
            let kinds: Vec<RowKind> = rows.iter().map(|row| row.kind).collect();
            assert!(kinds.contains(&RowKind::Addition) && kinds.contains(&RowKind::Deletion));
        });
        assert!(cx.debug_bounds("remote-patch").is_some());
        let asked = desktop.join().unwrap();
        assert!(
            asked[1].0.contains("patch?path=src%2Fapp%2Fpay.ts"),
            "{:?}",
            asked[1].0
        );
        // A local task again: the column is this computer's.
        column.update(cx, |column, cx| {
            column.set_root(None, cx);
            column.set_remote_task(None, None, cx);
            assert!(!column.on_machine && column.remote.task.is_none());
        });
    }
}
