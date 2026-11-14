//! The open file, with the code taking the screen: one bar on top (the
//! file and which of the conflicted ones it is, the blocks' position, the
//! agent, "Comparar" and the rarer actions in a menu), the file's own
//! editor under it with each block where it is (`blocks_inline`), and one
//! bar at the bottom with what is left and "Salvar e marcar resolvido".
//! The file is the result: there is no separate result area, and the
//! comparison with a side opens beside the editor, never in its place.

use dcc_core::domain::merge_conflict::{
    WorkspaceGitConflictSide, conflict_result_suspiciously_empty, conflict_text_unavailable,
    result_line_count,
};
use gpui::{AnyElement, Context, FontWeight, canvas, div, prelude::*, px};

use super::files_view::kind_label;
use super::view::dark;
use super::{Busy, MergeResolver, ResultView};
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Icon, Menu, MenuAlign, MenuEntry, MenuItem, spinner,
};

pub(crate) fn workspace(resolver: &MergeResolver, cx: &mut Context<MergeResolver>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    div()
        .flex()
        .flex_1()
        .min_h_0()
        .bg(colors.muted.opacity(0.025))
        .children(resolver.open.as_ref().map(|_| section(resolver, cx)))
        .into_any_element()
}

/// A quiet trigger for the bar's menus.
fn trigger(
    selector: &'static str,
    label: Option<String>,
    icon: &'static str,
    cx: &Context<MergeResolver>,
) -> impl Fn(bool) -> AnyElement + 'static {
    let colors = cx.theme().colors.clone();
    move |_| {
        div()
            .debug_selector(move || selector.into())
            .flex()
            .h(px(26.0))
            .items_center()
            .gap(px(4.0))
            .rounded(px(6.0))
            .px(px(6.0))
            .text_size(px(11.0))
            .text_color(colors.muted_foreground)
            .cursor_pointer()
            .hover(|row| row.bg(colors.accent).text_color(colors.foreground))
            .children(label.clone())
            .child(Icon::new(icon).size(13.0))
            .into_any_element()
    }
}

/// "arquivo 2 de 3": the conflicted files, to go to another.
fn file_menu(resolver: &MergeResolver, cx: &mut Context<MergeResolver>) -> Option<AnyElement> {
    let files: Vec<(String, String)> = resolver
        .conflicts()
        .iter()
        .map(|entry| (entry.path.clone(), kind_label(entry, cx).to_string()))
        .collect();
    if files.len() < 2 {
        return None;
    }
    let open = resolver.open.as_ref()?.entry.path.clone();
    let at = files
        .iter()
        .position(|(path, _)| *path == open)
        .unwrap_or(0);
    let label = cx
        .t_with(
            "mergeConflict.bar.file",
            &Args::default()
                .with("current", (at + 1).to_string())
                .with("total", files.len().to_string()),
        )
        .to_string();
    let this = cx.entity().downgrade();
    Some(
        Menu::dropdown(
            "merge-files-menu",
            trigger("merge-files", Some(label), "chevron-down", cx),
            move |_, _| {
                files
                    .iter()
                    .map(|(path, kind)| {
                        let (this, picked) = (this.clone(), path.clone());
                        MenuEntry::Item(
                            MenuItem::new(path.clone(), move |_, cx| {
                                let _ = this.update(cx, |resolver, cx| {
                                    resolver.request_open(picked.clone(), cx)
                                });
                            })
                            .detail(kind.clone()),
                        )
                    })
                    .collect()
            },
        )
        .min_width(320.0)
        .into_any_element(),
    )
}

/// "Comparar": the result against a side, beside the editor.
fn compare_menu(resolver: &MergeResolver, cx: &mut Context<MergeResolver>) -> AnyElement {
    let open = resolver.open.as_ref().expect("an open file");
    let view = open.view;
    let sides = [
        (
            ResultView::CompareCurrent,
            "mergeConflict.result.views.compareCurrent",
            open.entry.current.text.is_some(),
        ),
        (
            ResultView::CompareIncoming,
            "mergeConflict.result.views.compareIncoming",
            open.entry.incoming.text.is_some(),
        ),
        (
            ResultView::Edit,
            "mergeConflict.bar.compareOff",
            view != ResultView::Edit,
        ),
    ];
    let label = cx.t("mergeConflict.bar.compare").to_string();
    let this = cx.entity().downgrade();
    Menu::dropdown(
        "merge-compare-menu",
        trigger("merge-compare", Some(label), "chevron-down", cx),
        move |_, cx| {
            sides
                .iter()
                .filter(|(_, _, offered)| *offered)
                .map(|(mode, key, _)| {
                    let (this, mode) = (this.clone(), *mode);
                    MenuEntry::Item(MenuItem::new(cx.t(key), move |_, cx| {
                        let _ = this.update(cx, |resolver, cx| resolver.set_view(mode, cx));
                    }))
                })
                .collect()
        },
    )
    .align(MenuAlign::End)
    .min_width(240.0)
    .into_any_element()
}

/// "⋯": what is rarely asked of the file.
fn more_menu(resolver: &MergeResolver, cx: &mut Context<MergeResolver>) -> AnyElement {
    let ((current, _), (incoming, _)) = resolver.labels(cx);
    let dirty = resolver.open.as_ref().is_some_and(super::Open::dirty);
    let use_file = |branch: &str, cx: &gpui::App| {
        cx.t_with(
            "mergeConflict.fileActions.useFile",
            &Args::default().with("branch", branch.to_string()),
        )
    };
    let (use_current, use_incoming) = (use_file(&current, cx), use_file(&incoming, cx));
    let several = resolver.conflicts().len() > 1;
    let this = cx.entity().downgrade();
    Menu::dropdown(
        "merge-more-menu",
        trigger("merge-more", None, "ellipsis", cx),
        move |_, cx| {
            let side = |label: gpui::SharedString, side: WorkspaceGitConflictSide| {
                let this = this.clone();
                MenuEntry::Item(MenuItem::new(label, move |_, cx| {
                    let _ = this.update(cx, |resolver, cx| resolver.use_whole_side(side, cx));
                }))
            };
            let mut entries = Vec::new();
            // Every conflicted file to the agent, one after the other.
            if several {
                let this = this.clone();
                entries.push(MenuEntry::Item(
                    MenuItem::new(cx.t("mergeConflict.agent.resolveAll"), move |_, cx| {
                        let _ = this.update(cx, |resolver, cx| resolver.resolve_all_with_agent(cx));
                    })
                    .icon(Icon::new("sparkles").size(14.0)),
                ));
            }
            entries.push(side(use_current.clone(), WorkspaceGitConflictSide::Current));
            entries.push(side(
                use_incoming.clone(),
                WorkspaceGitConflictSide::Incoming,
            ));
            if dirty {
                let this = this.clone();
                entries.push(MenuEntry::Item(
                    MenuItem::new(cx.t("mergeConflict.result.restore"), move |_, cx| {
                        let _ = this.update(cx, |resolver, cx| resolver.restore(cx));
                    })
                    .icon(Icon::new("rotate-ccw").size(14.0)),
                ));
            }
            let this = this.clone();
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    cx.t("mergeConflict.fileActions.deleteResult"),
                    move |_, cx| {
                        let _ = this.update(cx, |resolver, cx| resolver.request_delete(cx));
                    },
                )
                .icon(Icon::new("trash-2").size(14.0))
                .destructive(),
            ));
            entries
        },
    )
    .align(MenuAlign::End)
    .min_width(260.0)
    .into_any_element()
}

fn section(resolver: &MergeResolver, cx: &mut Context<MergeResolver>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let open = resolver.open.as_ref().expect("an open file");
    let entry = &open.entry;
    let ((current, _), (incoming, _)) = resolver.labels(cx);
    let busy = resolver.busy;
    let busy_any = busy.is_some();
    let unavailable = conflict_text_unavailable(entry);
    let hunks = open.hunks();
    // With no conflict left, the arrows walk what the agent still offers.
    let proposed = resolver
        .proposal
        .as_ref()
        .filter(|_| hunks.is_empty())
        .and_then(|proposal| proposal.position(&open.buffer));
    let (at, total) = match proposed {
        Some((position, total)) => (position - 1, total),
        None => (
            open.active_hunk.min(hunks.len().saturating_sub(1)),
            hunks.len(),
        ),
    };
    let on_proposal = proposed.is_some();
    // The blocks' position and its arrows, while there are blocks.
    let position = (!unavailable && total > 0).then(|| {
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(2.0))
            .child(
                div()
                    .debug_selector(|| "merge-hunk-position".into())
                    .mr(px(2.0))
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(
                        cx.t_with(
                            "mergeConflict.hunk.position",
                            &Args::default()
                                .with("current", (at + 1).to_string())
                                .with("total", total.to_string()),
                        ),
                    ),
            )
            .child(
                Button::new("merge-hunk-previous")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconXs)
                    .disabled(at == 0)
                    .on_click(cx.listener(move |resolver, _, _, cx| {
                        if on_proposal {
                            resolver.step_proposed(false, cx)
                        } else {
                            resolver.step_hunk(false, cx)
                        }
                    }))
                    .child(Icon::new("chevron-up").size(16.0)),
            )
            .child(
                Button::new("merge-hunk-next")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconXs)
                    .disabled(at + 1 >= total)
                    .on_click(cx.listener(move |resolver, _, _, cx| {
                        if on_proposal {
                            resolver.step_proposed(true, cx)
                        } else {
                            resolver.step_hunk(true, cx)
                        }
                    }))
                    .child(Icon::new("chevron-down").size(16.0)),
            )
    });
    let bar = div()
        .debug_selector(|| "merge-bar".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(8.0))
        .border_b_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.background.opacity(0.95))
        .px(px(16.0))
        .py(px(7.0))
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_family(fonts::MONO)
                .text_size(px(12.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(entry.path.clone()),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(10.0))
                .text_color(colors.muted_foreground)
                .child(kind_label(entry, cx)),
        )
        .children(file_menu(resolver, cx))
        .child(div().flex_1())
        .children(position)
        .child(super::agent_view::resolve_button(resolver, cx))
        .when(!unavailable, |bar| bar.child(compare_menu(resolver, cx)))
        .child(more_menu(resolver, cx));
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
            .child(bar)
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

    let empty = conflict_result_suspiciously_empty(entry, &open.buffer);
    let dirty = open.dirty();
    let view = open.view;
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
    let editor = div()
        .flex()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .overflow_hidden()
        .child(open.editor.clone());
    // The comparison beside the editor: the file stays editable.
    let body: AnyElement = match (&open.compare, view) {
        (Some(diff), ResultView::CompareCurrent | ResultView::CompareIncoming) => {
            let side = if view == ResultView::CompareCurrent {
                current.clone()
            } else {
                incoming.clone()
            };
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .child(editor)
                .child(
                    div()
                        .debug_selector(|| "merge-compare-panel".into())
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .min_w_0()
                        .border_l_1()
                        .border_color(colors.border.opacity(0.6))
                        .child(
                            div()
                                .flex()
                                .flex_shrink_0()
                                .items_center()
                                .gap(px(8.0))
                                .border_b_1()
                                .border_color(colors.border.opacity(0.5))
                                .bg(colors.muted.opacity(0.08))
                                .px(px(12.0))
                                .py(px(5.0))
                                .text_size(px(11.0))
                                .text_color(colors.muted_foreground)
                                .child(div().flex_1().min_w_0().truncate().child(cx.t_with(
                                    "mergeConflict.bar.comparing",
                                    &Args::default().with("side", side),
                                )))
                                .child(
                                    Button::new("merge-compare-close")
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::IconXs)
                                        .on_click(cx.listener(|resolver, _, _, cx| {
                                            resolver.set_view(ResultView::Edit, cx)
                                        }))
                                        .child(Icon::new("x").size(14.0)),
                                ),
                        )
                        .child(div().flex().flex_1().min_h_0().child(diff.clone())),
                )
                .into_any_element()
        }
        _ => editor.into_any_element(),
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
        .child(bar)
        .children(
            super::agent_view::agent_panels(resolver, cx)
                .into_iter()
                .map(|panel| {
                    div()
                        .flex_shrink_0()
                        .border_b_1()
                        .border_color(colors.border.opacity(0.6))
                        .px(px(16.0))
                        .py(px(10.0))
                        .child(panel)
                }),
        )
        .children(warning)
        .child(body)
        .child(footer)
        .into_any_element()
}
