//! Under each task of the report (Compozy's changed files, Monocode's git
//! card): the files it changed — three, then "mais N arquivos" opening up
//! to thirty in place, the rest left to "Mudanças" — and the delivery
//! footer (`room_delivery`).

use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::room::{RoomEvent, TeamRoom};
use crate::data::task_changes::{ChangedFile, SHOWN, visible};
use crate::data::task_reports::TaskReport;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::changed_files::changed_files;

/// The files line up with the task's title, past its provider and mascot.
const INDENT: f32 = 38.0;

impl TeamRoom {
    /// The task's files and its delivery, under its row of the report.
    pub(super) fn change_block(
        &self,
        index: usize,
        row: usize,
        task: &TaskReport,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let task_id = task.task_id.clone()?;
        let changes = self.task_changes(task, cx);
        let files = changes
            .files
            .filter(|files| !files.is_empty())
            .map(|files| self.file_rows(index, row, &task_id, &files, cx));
        let footer = self.delivery_footer(index, row, task, &task_id, changes.read.as_deref(), cx);
        if files.is_none() && footer.is_none() {
            return None;
        }
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .pl(px(INDENT))
                .pr(px(8.0))
                .pb(px(6.0))
                .children(files)
                .children(footer)
                .into_any_element(),
        )
    }

    fn file_rows(
        &self,
        index: usize,
        row: usize,
        task_id: &str,
        files: &[ChangedFile],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let expanded = self.changes.borrow().is_expanded(task_id);
        let (shown, more) = visible(files.len(), expanded);
        let list = changed_files(
            &format!("room-report-file-{index}-{row}"),
            files
                .iter()
                .take(shown)
                .map(|file| (file.path.as_str(), file.additions, file.deletions)),
            (tw::EMERALD_600.hsla(), tw::ROSE_600.hsla()),
            cx,
        );
        let link = |id: String, text: SharedString| {
            div()
                .id(SharedString::from(id.clone()))
                .debug_selector(move || id.clone())
                .cursor_pointer()
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .hover(|link| link.text_color(colors.foreground))
                .child(text)
        };
        let count = Args::count(more as i64);
        let toggle_id = task_id.to_string();
        let toggle = (!expanded && more > 0 || expanded && files.len() > SHOWN).then(|| {
            let text = if expanded {
                cx.t("agents.report.changes.less")
            } else {
                cx.t_with("agents.report.changes.more", &count)
            };
            link(format!("room-report-more-{index}-{row}"), text)
                .on_click(cx.listener(move |room, _, _, cx| room.toggle_changes(&toggle_id, cx)))
        });
        // Past thirty, the rest is the task's own "Mudanças".
        let open_id = task_id.to_string();
        let rest =
            (expanded && more > 0).then(|| {
                link(
                    format!("room-report-rest-{index}-{row}"),
                    cx.t_with("agents.report.changes.rest", &count),
                )
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.emit(RoomEvent::OpenTaskChanges(open_id.clone()))
                }))
            });
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(list)
            .when(toggle.is_some() || rest.is_some(), |block| {
                block.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .children(rest)
                        .children(toggle),
                )
            })
            .into_any_element()
    }
}
