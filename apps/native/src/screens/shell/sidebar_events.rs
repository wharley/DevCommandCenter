//! What the sidebar asks of the shell: collapse, open a task or one of its
//! sessions, start a task, open or clone a project, the help, the code
//! column.

use gpui::{Context, Entity, Window};

use super::{Shell, Surface};
use crate::screens::open_project::OpenRequest;
use crate::screens::sidebar::{Sidebar, SidebarEvent};

impl Shell {
    /// Screens the sidebar opens arrive one by one; until then their events
    /// only matter here when they change the frame.
    pub(super) fn on_sidebar_event(
        &mut self,
        sidebar: &Entity<Sidebar>,
        event: &SidebarEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SidebarEvent::ToggleCollapsed => {
                self.layout.toggle_sidebar();
                let collapsed = self.layout.sidebar_collapsed;
                sidebar.update(cx, |sidebar, cx| sidebar.set_collapsed(collapsed, cx));
                self.save_layout(cx);
                cx.notify();
            }
            SidebarEvent::OpenTask(task_id) => {
                let task_id = task_id.clone();
                let focus = self.focus_on_open.as_deref() == Some(task_id.as_str());
                if focus {
                    self.focus_on_open = None;
                }
                self.conversation.update(cx, |conversation, cx| {
                    conversation.open(&task_id, None, cx);
                    if focus {
                        conversation.focus_composer_soon(cx);
                    }
                });
                self.show(Surface::Conversation, cx);
            }
            SidebarEvent::NewTask => self.show(Surface::NewTask, cx),
            SidebarEvent::OpenHelp => self.open_help(None, window, cx),
            SidebarEvent::OpenUsage => self.open_usage(cx),
            SidebarEvent::OpenSettings => self.open_settings(None, window, cx),
            SidebarEvent::OpenSkills => self.open_skills(window, cx),
            SidebarEvent::OpenFeedback => self.open_feedback(window, cx),
            // `toggleCodeColumn`
            SidebarEvent::ToggleCodeColumn => {
                let open = !self.prefs.code_column_open;
                self.set_code_column(open, None, cx);
            }
            SidebarEvent::CloneProject => {
                self.show(Surface::Conversation, cx);
                self.open_project_dialog(
                    OpenRequest {
                        mode: crate::screens::open_project::DialogMode::Clone,
                        worktree: true,
                        ..OpenRequest::default()
                    },
                    window,
                    cx,
                );
            }
            // "Abrir branch" on a project's menu: `openWorkspaceDialog("open", project)`.
            SidebarEvent::OpenBranchInProject {
                project_id,
                root_path,
                label,
                ..
            } => {
                self.show(Surface::Conversation, cx);
                self.open_project_dialog(
                    OpenRequest {
                        worktree: true,
                        context: Some(crate::screens::open_project::RepositoryContext {
                            project_id: project_id.clone(),
                            workspace_root: root_path.clone(),
                            label: label.clone(),
                        }),
                        ..OpenRequest::default()
                    },
                    window,
                    cx,
                );
            }
            // "Vários projetos" in the + menu: `openWorkspaceDialog("open")`.
            SidebarEvent::OpenProject => {
                self.show(Surface::Conversation, cx);
                self.open_project_dialog(
                    OpenRequest {
                        multi: false,
                        worktree: true,
                        pick_folder: false,
                        ..OpenRequest::default()
                    },
                    window,
                    cx,
                );
            }
            SidebarEvent::NewTaskInProject { root_path, .. } => {
                let project = sidebar
                    .read(cx)
                    .projects()
                    .iter()
                    .find(|project| project.root_path == *root_path)
                    .cloned();
                if let Some(project) = project {
                    // `handleQuickCreateTask(repository)`: protected, with the
                    // linked folders.
                    self.new_task
                        .update(cx, |new_task, cx| new_task.create(&project, true, true, cx));
                }
            }
            SidebarEvent::OpenSession {
                task_id,
                session_id,
            } => {
                let (task_id, session_id) = (task_id.clone(), session_id.clone());
                self.conversation.update(cx, |conversation, cx| {
                    conversation.open(&task_id, Some(session_id), cx)
                });
                self.show(Surface::Conversation, cx);
            }
            SidebarEvent::OpenAgents => self.open_agents(window, cx),
            SidebarEvent::OpenPullRequests => self.open_pull_requests(window, cx),
            SidebarEvent::OpenNotes => self.open_notes(cx),
            SidebarEvent::ReviewRequests(requests) => {
                let requests = requests.clone();
                self.review_requests_arrived(&requests, cx);
            }
            SidebarEvent::TasksCompleted(ids) => {
                let ids = ids.clone();
                self.notes
                    .update(cx, |notes, cx| notes.task_completed(ids, cx));
            }
            _ => self.save_layout(cx),
        }
    }
}
