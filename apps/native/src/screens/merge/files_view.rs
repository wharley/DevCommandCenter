//! The merge's files on the left (`aside`): how many are resolved, the
//! progress, and each conflicted file with its kind; the open one marked.

use dcc_core::domain::merge_conflict::{WorkspaceGitConflictEntry, WorkspaceGitConflictKind};
use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::MergeResolver;
use super::view::progress;
use crate::i18n::Translate;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::Icon;

pub(crate) fn kind_label(entry: &WorkspaceGitConflictEntry, cx: &gpui::App) -> SharedString {
    cx.t(match entry.kind {
        WorkspaceGitConflictKind::BothModified => "mergeConflict.conflictKind.bothModified",
        WorkspaceGitConflictKind::BothAdded => "mergeConflict.conflictKind.bothAdded",
        WorkspaceGitConflictKind::DeletedByCurrent => "mergeConflict.conflictKind.deletedByCurrent",
        WorkspaceGitConflictKind::DeletedByIncoming => {
            "mergeConflict.conflictKind.deletedByIncoming"
        }
        WorkspaceGitConflictKind::BothDeleted => "mergeConflict.conflictKind.bothDeleted",
        WorkspaceGitConflictKind::Other => "mergeConflict.conflictKind.other",
    })
}

pub(crate) fn files(resolver: &MergeResolver, cx: &mut Context<MergeResolver>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let (resolved, total) = resolver.resolved();
    let selected = resolver.open.as_ref().map(|open| open.entry.path.clone());
    let busy = resolver.busy.is_some();
    let hover = (colors.border.opacity(0.6), colors.accent.opacity(0.5));
    div()
        .flex()
        .flex_col()
        .w(px(256.0))
        .flex_shrink_0()
        .border_r_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.sidebar.opacity(0.6))
        .child(
            div()
                .border_b_1()
                .border_color(colors.border.opacity(0.5))
                .bg(colors.background.opacity(0.4))
                .px(px(12.0))
                .py(px(12.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t("mergeConflict.files"))
                        .child(
                            div()
                                .rounded(px(6.0))
                                .bg(colors.secondary)
                                .px(px(6.0))
                                .text_size(px(11.0))
                                .text_color(colors.secondary_foreground)
                                .child(format!("{resolved}/{total}")),
                        ),
                )
                .child(div().mt(px(8.0)).child(progress(resolved, total, cx))),
        )
        .child(
            div()
                .id("merge-files")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p(px(8.0))
                .children(resolver.conflicts().iter().map(|entry| {
                    let path = entry.path.clone();
                    let current = selected.as_deref() == Some(path.as_str());
                    let selector = format!("merge-file-{path}");
                    div()
                        .id(SharedString::from(selector.clone()))
                        .debug_selector(move || selector.clone())
                        .mb(px(4.0))
                        .flex()
                        .w_full()
                        .items_start()
                        .gap(px(10.0))
                        .rounded(px(8.0))
                        .border_1()
                        .px(px(10.0))
                        .py(px(10.0))
                        .map(|row| {
                            if current {
                                row.border_color(tw::AMBER_500.alpha(0.25))
                                    .bg(tw::AMBER_500.alpha(0.08))
                                    .shadow_sm()
                            } else {
                                row.border_color(gpui::transparent_black())
                                    .hover(move |style| style.border_color(hover.0).bg(hover.1))
                            }
                        })
                        .when(!busy, |row| {
                            row.cursor_pointer()
                                .on_click(cx.listener(move |resolver, _, _, cx| {
                                    resolver.request_open(path.clone(), cx)
                                }))
                        })
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_color(tw::AMBER_500.hsla())
                                .child(Icon::new("file-warning").size(14.0)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .min_w_0()
                                .child(
                                    div()
                                        .truncate()
                                        .font_family(fonts::MONO)
                                        .text_size(px(11.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(entry.path.clone()),
                                )
                                .child(
                                    div()
                                        .mt(px(2.0))
                                        .text_size(px(10.0))
                                        .text_color(colors.muted_foreground)
                                        .child(kind_label(entry, cx)),
                                ),
                        )
                })),
        )
        .into_any_element()
}
