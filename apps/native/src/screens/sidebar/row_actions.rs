//! The buttons a row shows on hover (`group/actions` in
//! `workspace-rail-row.tsx`): the ⋯ menu (pin, snooze, rename), then
//! complete and put on hold, or restore for tasks out of the list.

use gpui::{AnyElement, App, Context, Hsla, SharedString, div, prelude::*, px};

use super::Sidebar;
use super::actions::PendingAction;
use crate::data::rail::is_snoozed;
use crate::data::snooze::SnoozeOption;
use crate::data::tasks::{TaskStatus, TaskSummary};
use crate::i18n::Translate;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::tooltip::Side;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Icon, ItemStyle, Menu, MenuAlign, MenuEntry, MenuItem,
    Tooltip, spinner,
};

/// The CSS group the row's hover effects hang on (`group/dccRailRow`).
pub const ROW_GROUP: &str = "dcc-rail-row";

/// How far the title steps aside on hover so the buttons never cover it.
pub fn title_inset(task: &TaskSummary) -> f32 {
    // `onRenameWorkspace` is always given, so the menu is always there.
    if task.status == TaskStatus::Ready {
        72.0
    } else {
        48.0
    }
}

fn icon_button(id: SharedString, icon: &'static str, hover: Hsla, cx: &App) -> Button {
    Button::new(id)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconXs)
        .text_color(cx.theme().colors.muted_foreground.opacity(0.6))
        .hover_text_color(hover)
        .child(Icon::new(icon).size(14.0))
}

/// `hover:text-emerald-600 dark:hover:text-emerald-400`, and amber alike.
fn hover_tones(cx: &App) -> (Hsla, Hsla) {
    match cx.theme().appearance {
        Appearance::Light => (tw::EMERALD_600.hsla(), tw::AMBER_600.hsla()),
        Appearance::Dark => (tw::EMERALD_400.hsla(), tw::AMBER_400.hsla()),
    }
}

fn menu_entries(
    sidebar: &Sidebar,
    task: &TaskSummary,
    cx: &mut Context<Sidebar>,
) -> Vec<MenuEntry> {
    let entity = cx.entity().downgrade();
    let id = task.id.clone();
    let selectable = task.status.is_selectable();
    let pinned = task.pinned_at.is_some();
    let snoozed = is_snoozed(task, sidebar.state.now_ms);
    let mut entries = Vec::new();
    if selectable {
        let (pin_entity, pin_id) = (entity.clone(), id.clone());
        let (icon, label) = if pinned {
            ("pin-off", "sidebar.unpinWorkspace")
        } else {
            ("pin", "sidebar.pinWorkspace")
        };
        entries.push(
            MenuItem::new(cx.t(label), move |window, cx| {
                let _ = pin_entity.update(cx, |sidebar, cx| {
                    sidebar.set_pinned(&pin_id, !pinned, window, cx)
                });
            })
            .icon(Icon::new(icon).stroke(1.9))
            .into(),
        );
        if snoozed {
            let (entity, id) = (entity.clone(), id.clone());
            entries.push(
                MenuItem::new(cx.t("sidebar.snooze.wake"), move |window, cx| {
                    let _ = entity.update(cx, |sidebar, cx| sidebar.snooze(&id, None, window, cx));
                })
                .icon(Icon::new("alarm-clock-off").stroke(1.9))
                .into(),
            );
        } else {
            let options = SnoozeOption::ALL
                .iter()
                .map(|&option| {
                    let (entity, id) = (entity.clone(), id.clone());
                    MenuItem::new(cx.t(option.i18n_key()), move |window, cx| {
                        let _ = entity.update(cx, |sidebar, cx| {
                            sidebar.snooze(&id, Some(option), window, cx)
                        });
                    })
                    .into()
                })
                .collect();
            entries.push(MenuEntry::Submenu {
                label: cx.t("sidebar.snooze.menu"),
                icon: Some(Icon::new("alarm-clock").stroke(1.9)),
                entries: options,
            });
        }
    }
    entries.push(
        MenuItem::new(cx.t("sidebar.renameWorkspace"), move |window, cx| {
            let _ = entity.update(cx, |sidebar, cx| sidebar.start_rename(&id, window, cx));
        })
        .icon(Icon::new("pencil"))
        .into(),
    );
    entries
}

fn action_button(
    task_id: &str,
    pending: Option<PendingAction>,
    action: PendingAction,
    hover: Hsla,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let (icon, label, busy_label) = match action {
        PendingAction::Restore => (
            "rotate-ccw",
            "sidebar.restoreWorkspace",
            "sidebar.restoringWorkspace",
        ),
        PendingAction::Complete => (
            "circle-check",
            "sidebar.completeWorkspace",
            "sidebar.completingWorkspace",
        ),
        PendingAction::Archive => (
            "circle-pause",
            "sidebar.moveWorkspaceToWaiting",
            "sidebar.moveWorkspaceToWaiting",
        ),
    };
    // Hold keeps its icon while it runs; restore and complete spin.
    let running = pending == Some(action) && action != PendingAction::Archive;
    let id = task_id.to_string();
    let element_id = SharedString::from(format!("{label}-{task_id}"));
    let mut button = icon_button(element_id.clone(), icon, hover, cx);
    if running {
        button = Button::new(element_id.clone())
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::IconXs)
            .text_color(cx.theme().colors.muted_foreground.opacity(0.6))
            .child(spinner(
                SharedString::from(format!("{busy_label}-{task_id}")),
                14.0,
            ));
    } else if pending.is_none() {
        button = button.on_click(
            cx.listener(move |this, _, window, cx| this.run_row_action(&id, action, window, cx)),
        );
    }
    match action {
        // Only complete and hold explain themselves (`TooltipContent side="top"`).
        PendingAction::Complete | PendingAction::Archive => Tooltip::text(
            SharedString::from(format!("tip-{element_id}")),
            button,
            cx.t(label),
        )
        .side(Side::Top)
        .into_any_element(),
        PendingAction::Restore => button.into_any_element(),
    }
}

pub fn render(sidebar: &Sidebar, task: &TaskSummary, cx: &mut Context<Sidebar>) -> AnyElement {
    let pending = sidebar.pending.get(&task.id).copied();
    let menu_open = sidebar.row_menu_open.as_deref() == Some(task.id.as_str());
    let renaming = sidebar.is_renaming(&task.id);
    let busy = pending.is_some() || renaming;
    let (emerald, amber) = hover_tones(cx);
    let colors = &cx.theme().colors;
    let foreground = colors.foreground;
    let background = colors.sidebar.opacity(0.9);

    let task_id = task.id.clone();
    let entries_task = task.clone();
    let entity = cx.entity().downgrade();
    let menu_entity = entity.clone();
    let menu_id = task.id.clone();
    let muted = colors.muted_foreground.opacity(0.6);
    let menu = Menu::dropdown(
        SharedString::from(format!("row-menu-{task_id}")),
        move |_| {
            Button::new(SharedString::from(format!("row-menu-trigger-{task_id}")))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconXs)
                .text_color(muted)
                .hover_text_color(foreground)
                .disabled(busy)
                .child(Icon::new("ellipsis").size(14.0))
                .into_any_element()
        },
        move |_, cx| {
            entity
                .update(cx, |sidebar, cx| menu_entries(sidebar, &entries_task, cx))
                .unwrap_or_default()
        },
    )
    .align(MenuAlign::End)
    .side_offset(5.0)
    .item_style(ItemStyle {
        text_size: 13.0,
        gap: 8.0,
    })
    .on_open_change(move |open, _, cx| {
        let _ = menu_entity.update(cx, |sidebar, cx| {
            sidebar.set_row_menu_open(&menu_id, open, cx)
        });
    });

    let buttons: Vec<AnyElement> = match task.status {
        TaskStatus::Archived | TaskStatus::Completed => {
            // Deleting a completed task arrives with its dialog (slice 1c-b).
            vec![action_button(
                &task.id,
                pending,
                PendingAction::Restore,
                foreground,
                cx,
            )]
        }
        status => {
            let mut buttons = Vec::new();
            if status == TaskStatus::Ready {
                buttons.push(action_button(
                    &task.id,
                    pending,
                    PendingAction::Complete,
                    emerald,
                    cx,
                ));
            }
            buttons.push(action_button(
                &task.id,
                pending,
                PendingAction::Archive,
                amber,
                cx,
            ));
            buttons
        }
    };

    let shown = pending.is_some() || menu_open;
    div()
        .absolute()
        .right(px(8.0))
        .top(px(6.0))
        .flex()
        .items_center()
        .gap(px(2.0))
        .pl(px(4.0))
        .rounded(cx.theme().radius.md)
        .bg(background)
        .opacity(if shown { 1.0 } else { 0.0 })
        .group_hover(ROW_GROUP, |style| style.opacity(1.0))
        .child(menu)
        .children(buttons)
        .into_any_element()
}
