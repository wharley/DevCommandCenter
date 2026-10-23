//! `SidebarProjectMenu`: the rail has no project headers, so a right click
//! on a task row opens what a project's menu offers.

use gpui::{AnyElement, Context, SharedString, prelude::*};

use super::{Sidebar, SidebarEvent};
use crate::data::tasks::Project;
use crate::i18n::Translate;
use crate::ui::{Icon, ItemStyle, Menu, MenuEntry, MenuItem};

pub fn wrap(
    task_id: &str,
    project: Option<&Project>,
    row: AnyElement,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let Some(project) = project.cloned() else {
        return row;
    };
    let entity = cx.entity().downgrade();
    Menu::context(
        SharedString::from(format!("project-menu-{task_id}")),
        row,
        move |_, cx| {
            let event = |project: &Project, branch: bool| {
                let (project_id, root_path, base_branch, label) = (
                    project.project_id.clone(),
                    project.root_path.clone(),
                    project.base_branch.clone(),
                    project.display_name.clone(),
                );
                if branch {
                    SidebarEvent::OpenBranchInProject {
                        project_id,
                        root_path,
                        base_branch,
                        label,
                    }
                } else {
                    SidebarEvent::NewTaskInProject {
                        project_id,
                        root_path,
                        base_branch,
                        label,
                    }
                }
            };
            let (new_task, open_branch) = (event(&project, false), event(&project, true));
            let (a, b, c, d) = (
                entity.clone(),
                entity.clone(),
                entity.clone(),
                entity.clone(),
            );
            let (edit_id, remove) = (project.id.clone(), project.clone());
            vec![
                MenuEntry::Label(SharedString::from(project.display_name.clone())),
                MenuItem::new(cx.t("sidebar.projectMenu.newTask"), move |_, cx| {
                    let event = new_task.clone();
                    let _ = a.update(cx, |_, cx| cx.emit(event));
                })
                .icon(Icon::new("plus").stroke(1.9))
                .into(),
                MenuItem::new(cx.t("sidebar.openBranch"), move |_, cx| {
                    let event = open_branch.clone();
                    let _ = b.update(cx, |_, cx| cx.emit(event));
                })
                .icon(Icon::new("git-branch").stroke(1.9))
                .into(),
                MenuEntry::Separator,
                MenuItem::new(cx.t("sidebar.editProject"), move |window, cx| {
                    let _ = c.update(cx, |sidebar, cx| {
                        sidebar.open_edit_project(&edit_id, window, cx)
                    });
                })
                .icon(Icon::new("settings-2").stroke(1.9))
                .into(),
                MenuItem::new(cx.t("sidebar.removeProject"), move |_, cx| {
                    let _ = d.update(cx, |sidebar, cx| sidebar.open_remove_project(&remove, cx));
                })
                .icon(Icon::new("trash-2"))
                .destructive()
                .into(),
            ]
        },
    )
    .min_width(192.0)
    .item_style(ItemStyle {
        text_size: 13.0,
        gap: 8.0,
    })
    .into_any_element()
}
