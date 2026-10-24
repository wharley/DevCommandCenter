//! `BranchToolbar`, shown when the branch is behind its base (or while it
//! syncs): the branch and its folder, "Atualizar · N atrás" right there,
//! and a menu that copies the branch's name.

use gpui::{AnyElement, ClipboardItem, Context, FontWeight, SharedString, div, prelude::*, px};

use super::super::CodeColumn;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Icon, Menu, MenuAlign, MenuEntry, MenuItem, Toaster,
};

/// The bar, when the branch is behind or syncing (`px-2 pt-2` around it).
pub(crate) fn toolbar(column: &CodeColumn, cx: &mut Context<CodeColumn>) -> Option<AnyElement> {
    let behind = column
        .status
        .as_ref()
        .map_or(0, |status| status.behind_of_remote_count);
    let syncing = column.delivery.syncing;
    if behind == 0 && !syncing {
        return None;
    }
    let colors = cx.theme().colors.clone();
    let branch = column.current_branch().unwrap_or_default();
    let base = column.task_branch();
    let detached = branch.is_empty();
    let name: SharedString = match (&base, detached) {
        (Some(base), true) => cx.t_with(
            "branchToolbar.detachedFrom",
            &Args::default().with("branch", base.clone()),
        ),
        (_, true) => cx.t("branchToolbar.noBranch"),
        _ => branch.clone().into(),
    };
    let badge = cx.t(if detached {
        "branchToolbar.detachedBadge"
    } else {
        "branchToolbar.activeBadge"
    });
    let path: SharedString = column
        .root
        .clone()
        .map(SharedString::from)
        .unwrap_or_else(|| cx.t("branchToolbar.workspacePathUnavailable"));
    let sync_label = if syncing {
        cx.t("branchToolbar.syncingBase")
    } else {
        cx.t_with(
            "branchToolbar.syncBaseBehind",
            &Args::count(i64::from(behind)),
        )
    };
    let sync = div()
        .id("branch-toolbar-sync")
        .debug_selector(|| "branch-toolbar-sync".into())
        .flex()
        .h(px(28.0))
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .rounded(px(8.0))
        .px(px(10.0))
        .bg(colors.secondary)
        .text_color(colors.secondary_foreground)
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .map(|button| {
            if syncing {
                button.opacity(0.5)
            } else {
                let hover = colors.secondary.opacity(0.8);
                button
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(cx.listener(|column, _, _, cx| column.ask_sync_base(cx)))
            }
        })
        .child(if syncing {
            crate::ui::spinner("branch-toolbar-syncing", 14.0).into_any_element()
        } else {
            Icon::new("refresh-cw").size(14.0).into_any_element()
        })
        .child(sync_label);
    let copied = cx.t("branchToolbar.copyBranchNameSuccess");
    let copy_label = cx.t("branchToolbar.copyBranchName");
    let menu = Menu::dropdown(
        "branch-toolbar-menu",
        |_| {
            Button::new("branch-toolbar-menu-trigger")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconXs)
                .muted()
                .child(Icon::new("chevron-down").size(14.0))
                .into_any_element()
        },
        move |_, _| {
            let (name, copied) = (branch.clone(), copied.clone());
            vec![MenuEntry::from(
                MenuItem::new(copy_label.clone(), move |_, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(name.clone()));
                    Toaster::success(copied.clone(), cx);
                })
                .icon(Icon::new("copy").size(16.0))
                .disabled(detached),
            )]
        },
    )
    .align(MenuAlign::End)
    .side_offset(6.0)
    .min_width(176.0);
    Some(
        div()
            .px(px(8.0))
            .pt(px(8.0))
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.6))
                    .bg(colors.muted.opacity(0.2))
                    .px(px(8.0))
                    .py(px(6.0))
                    .child(
                        Icon::new("git-branch")
                            .size(14.0)
                            .color(colors.muted_foreground),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .min_w_0()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(colors.foreground)
                                            .child(name),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .h(px(20.0))
                                            .flex_shrink_0()
                                            .items_center()
                                            .rounded_full()
                                            .border_1()
                                            .border_color(colors.border)
                                            .px(px(6.0))
                                            .text_size(px(10.0))
                                            .text_color(colors.foreground)
                                            .child(badge),
                                    ),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .child(path),
                            ),
                    )
                    .child(sync)
                    .child(menu),
            )
            .into_any_element(),
    )
}
