//! The files and the open one: the list on the left; on the right the
//! file's actions, the block panel, the result's header with its views,
//! the editor or the comparison, and the footer with what is left and
//! "Salvar e marcar resolvido".

use dcc_core::domain::merge_conflict::{
    WorkspaceGitConflictSide, conflict_result_suspiciously_empty, conflict_text_unavailable,
    result_line_count,
};
use gpui::{AnyElement, Context, FontWeight, Hsla, canvas, div, prelude::*, px};

use super::files_view::kind_label;
use super::view::dark;
use super::{Busy, MergeResolver, ResultView};
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tracked, spinner};

fn tone(hunks: usize, empty: bool, destructive: Hsla) -> Hsla {
    if empty {
        destructive
    } else if hunks > 0 {
        tw::AMBER_500.hsla()
    } else {
        tw::EMERALD_500.hsla()
    }
}

pub(crate) fn workspace(resolver: &MergeResolver, cx: &mut Context<MergeResolver>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    div()
        .flex()
        .flex_1()
        .min_h_0()
        .bg(colors.muted.opacity(0.025))
        .child(super::files_view::files(resolver, cx))
        .children(resolver.open.as_ref().map(|_| section(resolver, cx)))
        .into_any_element()
}

fn section(resolver: &MergeResolver, cx: &mut Context<MergeResolver>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let open = resolver.open.as_ref().expect("an open file");
    let entry = &open.entry;
    let ((current, _), (incoming, _)) = resolver.labels(cx);
    let busy = resolver.busy;
    let busy_any = busy.is_some();
    let side_button = |id: &'static str,
                       label: String,
                       side: WorkspaceGitConflictSide,
                       cx: &mut Context<MergeResolver>| {
        Button::new(id)
            .variant(ButtonVariant::Outline)
            .size(ButtonSize::Xs)
            .disabled(busy_any)
            .on_click(cx.listener(move |resolver, _, _, cx| resolver.use_whole_side(side, cx)))
            .child(div().max_w(px(200.0)).truncate().child(label))
    };
    let actions = div()
        .flex()
        .flex_shrink_0()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .border_b_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.background.opacity(0.95))
        .px(px(16.0))
        .py(px(10.0))
        .child(
            div()
                .mr_auto()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .font_family(fonts::MONO)
                        .text_size(px(12.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(entry.path.clone()),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(colors.muted_foreground)
                        .child(kind_label(entry, cx)),
                ),
        )
        .child(side_button(
            "merge-use-current",
            cx.t_with(
                "mergeConflict.fileActions.useFile",
                &Args::default().with("branch", current.clone()),
            )
            .to_string(),
            WorkspaceGitConflictSide::Current,
            cx,
        ))
        .child(side_button(
            "merge-use-incoming",
            cx.t_with(
                "mergeConflict.fileActions.useFile",
                &Args::default().with("branch", incoming.clone()),
            )
            .to_string(),
            WorkspaceGitConflictSide::Incoming,
            cx,
        ))
        .child(
            Button::new("merge-delete-result")
                .variant(ButtonVariant::Destructive)
                .size(ButtonSize::Xs)
                .disabled(busy_any)
                .on_click(cx.listener(|resolver, _, _, cx| resolver.request_delete(cx)))
                .child(Icon::new("trash-2").size(14.0))
                .child(cx.t("mergeConflict.fileActions.deleteResult")),
        );
    if conflict_text_unavailable(entry) {
        let reason = cx.t(if entry.result.truncated {
            "mergeConflict.unavailable.tooLarge"
        } else {
            "mergeConflict.unavailable.unsupported"
        });
        let text = format!("{reason} {}", cx.t("mergeConflict.unavailable.instruction"));
        return div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .bg(colors.background)
            .child(actions)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .p(px(32.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .max_w(px(448.0))
                            .child(
                                div()
                                    .text_color(tw::AMBER_500.hsla())
                                    .child(Icon::new("file-warning").size(32.0)),
                            )
                            .child(
                                div()
                                    .mt(px(12.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(cx.t("mergeConflict.unavailable.title")),
                            )
                            .child(
                                div()
                                    .mt(px(8.0))
                                    .text_size(px(14.0))
                                    .text_center()
                                    .text_color(colors.muted_foreground)
                                    .child(text),
                            ),
                    ),
            )
            .into_any_element();
    }

    let hunks = open.hunks();
    let panel = super::blocks_view::blocks_panel(resolver, &current, &incoming, cx);

    let empty = conflict_result_suspiciously_empty(entry, &open.buffer);
    let dirty = open.dirty();
    let view = open.view;
    let toggle = |id: &'static str,
                  key: &str,
                  mode: ResultView,
                  disabled: bool,
                  cx: &mut Context<MergeResolver>| {
        Button::new(id)
            .variant(if view == mode {
                ButtonVariant::Secondary
            } else {
                ButtonVariant::Ghost
            })
            .size(ButtonSize::Xs)
            .disabled(disabled)
            .on_click(cx.listener(move |resolver, _, _, cx| resolver.set_view(mode, cx)))
            .child(cx.t(key))
    };
    let result_header = div()
        .flex()
        .flex_shrink_0()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .border_b_1()
        .border_color(colors.border.opacity(0.5))
        .bg(colors.muted.opacity(0.08))
        .px(px(16.0))
        .py(px(6.0))
        .text_size(px(10.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.muted_foreground)
        .child(
            div()
                .size(px(6.0))
                .rounded_full()
                .bg(tone(hunks.len(), empty, colors.destructive)),
        )
        .child(div().mr_auto().child(Tracked::new(
            cx.t("mergeConflict.result.title").to_uppercase(),
            10.0,
            0.08,
        )))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .font_weight(FontWeight::NORMAL)
                .child(toggle(
                    "merge-view-edit",
                    "mergeConflict.result.views.edit",
                    ResultView::Edit,
                    false,
                    cx,
                ))
                .child(toggle(
                    "merge-view-current",
                    "mergeConflict.result.views.compareCurrent",
                    ResultView::CompareCurrent,
                    entry.current.text.is_none(),
                    cx,
                ))
                .child(toggle(
                    "merge-view-incoming",
                    "mergeConflict.result.views.compareIncoming",
                    ResultView::CompareIncoming,
                    entry.incoming.text.is_none(),
                    cx,
                ))
                .child(
                    Button::new("merge-restore")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Xs)
                        .disabled(!dirty || busy_any)
                        .on_click(cx.listener(|resolver, _, _, cx| resolver.restore(cx)))
                        .child(Icon::new("rotate-ccw").size(12.0))
                        .child(cx.t("mergeConflict.result.restore")),
                ),
        );
    let warning = empty.then(|| {
        div()
            .flex()
            .flex_shrink_0()
            .items_start()
            .gap(px(8.0))
            .border_b_1()
            .border_color(colors.destructive.opacity(0.2))
            .bg(colors.destructive.opacity(0.06))
            .px(px(16.0))
            .py(px(8.0))
            .text_size(px(11.0))
            .text_color(colors.destructive)
            .child(
                div()
                    .mt(px(2.0))
                    .child(Icon::new("triangle-alert").size(14.0)),
            )
            .child(cx.t("mergeConflict.result.emptyWarning"))
    });
    let width = resolver.width.clone();
    let body: AnyElement = match (&open.compare, view) {
        (Some(diff), ResultView::CompareCurrent | ResultView::CompareIncoming) => {
            let label = if view == ResultView::CompareCurrent {
                current.clone()
            } else {
                incoming.clone()
            };
            let inline = !diff.read(cx).split;
            let final_label = cx.t("mergeConflict.result.finalLabel").to_string();
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .overflow_hidden()
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(8.0))
                        .border_b_1()
                        .border_color(colors.border.opacity(0.5))
                        .bg(colors.muted.opacity(0.08))
                        .px(px(if inline { 12.0 } else { 16.0 }))
                        .py(px(6.0))
                        .text_size(px(10.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.muted_foreground)
                        .child(
                            div()
                                .when(!inline, |label| label.flex_1().pr(px(12.0)))
                                .truncate()
                                .child(Tracked::new(label.to_uppercase(), 10.0, 0.06)),
                        )
                        .when(inline, |row| {
                            row.child(
                                div()
                                    .text_color(colors.muted_foreground.opacity(0.5))
                                    .child("→"),
                            )
                        })
                        .child(
                            div()
                                .when(!inline, |label| label.flex_1().pl(px(12.0)))
                                .truncate()
                                .text_color(if inline {
                                    colors.foreground.opacity(0.8)
                                } else {
                                    colors.muted_foreground
                                })
                                .child(Tracked::new(final_label.to_uppercase(), 10.0, 0.06)),
                        ),
                )
                .child(div().flex().flex_1().min_h_0().child(diff.clone()))
                .into_any_element()
        }
        _ => div()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .child(open.editor.clone())
            .into_any_element(),
    };
    let status = if !hunks.is_empty() {
        cx.t_with(
            "mergeConflict.hunk.pending",
            &Args::count(hunks.len() as i64),
        )
    } else if dirty {
        cx.t("mergeConflict.result.changed")
    } else {
        cx.t("mergeConflict.result.ready")
    };
    let stats = cx.t_with(
        "mergeConflict.result.stats",
        &Args::default()
            .with("lines", result_line_count(&open.buffer).to_string())
            .with("characters", open.buffer.chars().count().to_string()),
    );
    let footer = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .border_t_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.background.opacity(0.95))
        .px(px(16.0))
        .py(px(10.0))
        .child(
            div()
                .min_w_0()
                .child(
                    div()
                        .debug_selector(|| "merge-result-status".into())
                        .text_size(px(11.0))
                        .text_color(if hunks.is_empty() {
                            tw::EMERALD_600.hsla()
                        } else if dark(cx) {
                            tw::AMBER_300.hsla()
                        } else {
                            tw::AMBER_700.hsla()
                        })
                        .child(status),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(if empty {
                            colors.destructive
                        } else {
                            colors.muted_foreground
                        })
                        .child(stats),
                ),
        )
        .when(!hunks.is_empty(), |row| {
            row.child(
                div()
                    .flex_1()
                    .text_right()
                    .text_size(px(10.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("mergeConflict.result.resolveHint")),
            )
        })
        .child(
            Button::new("merge-save")
                .size(ButtonSize::Sm)
                .disabled(busy_any || !hunks.is_empty())
                .on_click(cx.listener(|resolver, _, _, cx| resolver.save(false, false, cx)))
                .child(if busy == Some(Busy::Save) {
                    spinner("merge-save-busy", 16.0).into_any_element()
                } else {
                    Icon::new("check").size(16.0).into_any_element()
                })
                .child(cx.t("mergeConflict.result.save")),
        );
    div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .bg(colors.background)
        .child(
            canvas(
                move |bounds, _, _| width.set(f32::from(bounds.size.width)),
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        )
        .child(actions)
        .child(panel)
        .child(result_header)
        .children(warning)
        .child(body)
        .child(footer)
        .into_any_element()
}
