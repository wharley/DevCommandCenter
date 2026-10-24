//! The window's frame: sidebar, resize separator and main area
//! (`App.tsx` root `<main>`). Owns the layout and follows the OS theme.

mod bootstrap;
pub mod layout;
mod regions;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use gpui::{
    Context, DragMoveEvent, Entity, MouseUpEvent, Pixels, Subscription, Window, div, prelude::*, px,
};

use crate::prefs::{self, Prefs};
use crate::screens::conversation::ConversationView;
use crate::screens::new_task::{NewTaskEvent, NewTaskView};
use crate::screens::open_project::{OpenProjectDialog, OpenProjectEvent, OpenRequest};
use crate::screens::sidebar::{NavPage, Sidebar, SidebarEvent};
use crate::theme::{ActiveTheme, Theme};
use crate::ui::{ResizeHandle, Toaster};
use layout::ShellLayout;
use regions::{MainRegion, SidebarRegion};

/// What the main area shows (`globalSurface`, else the selected task).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Conversation,
    NewTask,
}

/// Drag payload of the sidebar separator.
#[derive(Clone, Copy)]
struct SidebarResize;

pub struct Shell {
    layout: ShellLayout,
    prefs: Prefs,
    prefs_path: PathBuf,
    sidebar: Entity<Sidebar>,
    conversation: Entity<ConversationView>,
    new_task: Entity<NewTaskView>,
    pub(crate) surface: Surface,
    /// The task the desktop just created: its composer takes the focus.
    focus_on_open: Option<String>,
    /// "Provedor atual" on the no-task screen, read when it shows.
    bootstrap_provider: Option<(String, Option<String>)>,
    /// "Abrir projeto", while open.
    open_project: Option<(Entity<OpenProjectDialog>, Subscription)>,
    _appearance: Subscription,
    _sidebar_events: Subscription,
    _sidebar_data: Subscription,
    _new_task_events: Subscription,
}

impl Shell {
    pub fn new(
        prefs: Prefs,
        prefs_path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            if Theme::sync_with_window(window, cx) {
                cx.notify();
            }
        });
        let sidebar = cx.new(|cx| {
            Sidebar::new(
                prefs.sidebar_collapsed,
                prefs.waiting_open,
                prefs.completed_open,
                prefs::native_file("seen-results.json"),
                cx,
            )
        });
        let sidebar_events = cx.subscribe_in(&sidebar, window, Self::on_sidebar_event);
        let conversation = cx.new(|cx| ConversationView::new(window, cx));
        let new_task = cx.new(|_| NewTaskView::new());
        // The new-task screen lists the projects the sidebar loads.
        let sidebar_data = cx.observe(&sidebar, |this, sidebar, cx| {
            let sidebar = sidebar.read(cx);
            let (projects, linked) = (
                sidebar.projects().to_vec(),
                sidebar.linked_folders().clone(),
            );
            if sidebar.has_open_task() == Some(false) && this.bootstrap_provider.is_none() {
                this.bootstrap_provider = bootstrap::current_provider();
            }
            this.new_task.update(cx, |new_task, cx| {
                new_task.set_projects(&projects, &linked, cx)
            });
        });
        let new_task_events = cx.subscribe_in(&new_task, window, Self::on_new_task_event);
        Self {
            layout: ShellLayout::new(prefs.sidebar_width, prefs.sidebar_collapsed),
            prefs,
            prefs_path,
            sidebar,
            conversation,
            new_task,
            surface: Surface::Conversation,
            focus_on_open: None,
            open_project: None,
            bootstrap_provider: None,
            _appearance: appearance,
            _sidebar_events: sidebar_events,
            _sidebar_data: sidebar_data,
            _new_task_events: new_task_events,
        }
    }

    fn show(&mut self, surface: Surface, cx: &mut Context<Self>) {
        self.surface = surface;
        let page = (surface == Surface::NewTask).then_some(NavPage::NewTask);
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_current_page(page, cx));
        cx.notify();
    }

    fn on_new_task_event(
        &mut self,
        _: &Entity<NewTaskView>,
        event: &NewTaskEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            NewTaskEvent::Created(task_id) => self.open_created(task_id.clone(), cx),
            NewTaskEvent::OpenProject { multi, worktree } => {
                // `setGlobalSurface(null)` first, as the desktop.
                self.show(Surface::Conversation, cx);
                self.open_project_dialog(
                    // "Abrir outro projeto…" opens protected; "vários" keeps the
                    // screen's choice.
                    OpenRequest {
                        multi: *multi,
                        worktree: !*multi || *worktree,
                        pick_folder: !*multi,
                        ..OpenRequest::default()
                    },
                    window,
                    cx,
                );
            }
        }
    }

    /// The no-task screen's "Abrir projeto" and "Clone from URL".
    fn open_from_bootstrap(&mut self, clone: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mode = if clone {
            crate::screens::open_project::DialogMode::Clone
        } else {
            crate::screens::open_project::DialogMode::Open
        };
        self.open_project_dialog(
            OpenRequest {
                mode,
                worktree: true,
                ..OpenRequest::default()
            },
            window,
            cx,
        );
    }

    /// A task the desktop just created opens once the list has it.
    fn open_created(&mut self, task_id: String, cx: &mut Context<Self>) {
        self.focus_on_open = Some(task_id.clone());
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.open_when_listed(task_id, cx));
    }

    fn open_project_dialog(
        &mut self,
        request: OpenRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sidebar = self.sidebar.read(cx);
        let (projects, linked) = (
            sidebar.projects().to_vec(),
            sidebar.linked_folders().clone(),
        );
        let dialog = cx.new(|cx| OpenProjectDialog::new(&projects, &linked, request, window, cx));
        let events = cx.subscribe(&dialog, |this, _, event: &OpenProjectEvent, cx| {
            if let OpenProjectEvent::Created(task_id) = event {
                this.open_created(task_id.clone(), cx);
            }
            this.open_project = None;
            cx.notify();
        });
        self.open_project = Some((dialog, events));
        cx.notify();
    }

    /// Screens the sidebar opens arrive one by one; until then their events
    /// only matter here when they change the frame.
    fn on_sidebar_event(
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
            _ => self.save_layout(cx),
        }
    }

    fn start_sidebar_resize(&mut self, pointer_x: Pixels, cx: &mut Context<Self>) {
        self.layout.start_resize(f32::from(pointer_x));
        cx.notify();
    }

    fn resize_sidebar(&mut self, pointer_x: Pixels, cx: &mut Context<Self>) {
        if self.layout.resize_to(f32::from(pointer_x)) {
            cx.notify();
        }
    }

    /// Applies the release position too: GPUI swallows the move that starts a
    /// drag, so a quick drag-and-release would otherwise end short.
    fn finish_sidebar_resize(&mut self, pointer_x: Pixels, cx: &mut Context<Self>) {
        self.layout.resize_to(f32::from(pointer_x));
        if self.layout.finish_resize() {
            self.save_layout(cx);
            cx.notify();
        }
    }

    fn save_layout(&mut self, cx: &mut Context<Self>) {
        let (waiting_open, completed_open) = self.sidebar.read(cx).section_state();
        let next = Prefs {
            sidebar_width: self.layout.sidebar_width,
            sidebar_collapsed: self.layout.sidebar_collapsed,
            waiting_open,
            completed_open,
            ..self.prefs.clone()
        };
        if next == self.prefs {
            return;
        }
        self.prefs = next;
        if let Err(error) = self.prefs.save(&self.prefs_path) {
            eprintln!("[dcc-native] could not save the layout: {error}");
        }
    }
}

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let shell = cx.entity().downgrade();

        div()
            .id("shell")
            .relative()
            .flex()
            .size_full()
            .overflow_hidden()
            .bg(theme.colors.background)
            .text_color(theme.colors.foreground)
            .font_family(theme.fonts.sans.clone())
            // The desktop's base text: 16px with Tailwind preflight's 1.5 line height.
            .text_size(px(16.0))
            .line_height(gpui::relative(1.5))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<SidebarResize>, _, cx| {
                    this.resize_sidebar(event.event.position.x, cx)
                }),
            )
            .capture_any_mouse_up(cx.listener(|this, event: &MouseUpEvent, _, cx| {
                this.finish_sidebar_resize(event.position.x, cx)
            }))
            .child(SidebarRegion::new(self.layout.sidebar_rail_width()).child(self.sidebar.clone()))
            .child(
                ResizeHandle::new("sidebar-resize", SidebarResize)
                    .left(px(self.layout.separator_left()))
                    .active(self.layout.is_resizing())
                    .on_start(move |pointer_x, _, cx| {
                        shell
                            .update(cx, |this, cx| this.start_sidebar_resize(pointer_x, cx))
                            .ok();
                    }),
            )
            .child(MainRegion::default().child(match self.surface {
                // No task at all: `WorkspaceBootstrapState`.
                Surface::Conversation if self.sidebar.read(cx).has_open_task() == Some(false) => {
                    bootstrap::render(self, cx)
                }
                Surface::Conversation => self.conversation.clone().into_any_element(),
                Surface::NewTask => self.new_task.clone().into_any_element(),
            }))
            .children(self.open_project.as_ref().map(|(dialog, _)| dialog.clone()))
            .children(Toaster::handle(cx))
    }
}
