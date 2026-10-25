//! The window's frame: sidebar, resize separator and main area
//! (`App.tsx` root `<main>`). Owns the layout and follows the OS theme.

mod annotation;
mod bootstrap;
pub mod layout;
mod merge;
mod regions;
mod terminal;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use dcc_core::domain::delegation::DelegationStatus;

use gpui::{
    Context, DragMoveEvent, Entity, MouseUpEvent, Pixels, Subscription, Window, div, prelude::*, px,
};

use crate::prefs::{self, Prefs};
use crate::screens::code_column::{CodeColumn, CodeColumnEvent, CodeColumnTab};
use crate::screens::conversation::ConversationView;
use crate::screens::new_task::{NewTaskEvent, NewTaskView};
use crate::screens::open_project::{OpenProjectDialog, OpenProjectEvent, OpenRequest};
use crate::screens::sidebar::{NavPage, Sidebar, SidebarEvent};
use crate::screens::stage::{Stage, StageEvent};
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

gpui::actions!(
    dcc_shell,
    [
        /// ⌘⇧E: the code column on "Arquivos", or closed when it shows them.
        ToggleCodeFiles,
        /// ⌘⇧G: the same for "Mudanças".
        ToggleCodeChanges,
        /// ⌘J: the terminal dock (`isToggleTerminalShortcut`).
        ToggleTerminal
    ]
);

/// The shell's own shortcuts (`isCodeColumnShortcut`, `isInspectorGitModeShortcut`).
pub fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("cmd-shift-e", ToggleCodeFiles, None),
        gpui::KeyBinding::new("cmd-shift-g", ToggleCodeChanges, None),
        gpui::KeyBinding::new("cmd-j", ToggleTerminal, None),
    ]);
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
    code_column: Entity<CodeColumn>,
    /// The stage over the conversation (a change's diff, …).
    stage: Entity<Stage>,
    /// The task's checkout the stage's content belongs to.
    stage_root: Option<String>,
    /// Holds the focus when no field does, so the shell's shortcuts still
    /// reach it (with nothing focused GPUI only asks the window's root).
    focus: gpui::FocusHandle,
    pub(crate) surface: Surface,
    /// The task the desktop just created: its composer takes the focus.
    focus_on_open: Option<String>,
    /// "Provedor atual" on the no-task screen, read when it shows.
    bootstrap_provider: Option<(String, Option<String>)>,
    /// "Abrir projeto", while open.
    open_project: Option<(Entity<OpenProjectDialog>, Subscription)>,
    /// The merge conflict resolver, while open.
    merge: Option<(Entity<crate::screens::merge::MergeResolver>, Subscription)>,
    /// "Enviar trecho ao agente", while open.
    annotation: Option<(
        Entity<crate::screens::annotation::card::AnnotationCard>,
        Subscription,
    )>,
    _appearance: Subscription,
    _sidebar_events: Subscription,
    _sidebar_data: Subscription,
    _new_task_events: Subscription,
    _conversation_events: Subscription,
    _code_column_events: Subscription,
    _conversation_root: Subscription,
    _stage_events: Subscription,
    /// The terminal dock under the conversation (screen 7).
    pub(crate) terminal: Entity<crate::screens::terminal::TerminalDock>,
    _terminal_events: Subscription,
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
        let conversation_events =
            cx.subscribe_in(&conversation, window, Self::on_conversation_event);
        let code_column = cx.new(|cx| CodeColumn::new(prefs.code_column_tab, cx));
        let code_column_events = cx.subscribe_in(&code_column, window, Self::on_code_column_event);
        let stage = cx.new(Stage::new);
        let stage_events = cx.subscribe_in(&stage, window, Self::on_stage_event);
        // A file link in the conversation opens on the stage.
        let links = cx.entity().downgrade();
        cx.set_global(crate::markdown::LinkHandler(std::rc::Rc::new(
            move |target, window, cx| {
                let _ = links.update(cx, |shell, cx| shell.open_link(target, window, cx));
            },
        )));
        // The column follows the open task's checkout.
        let conversation_root = cx.observe(&conversation, |this, conversation, cx| {
            let view = conversation.read(cx);
            let place = terminal::place_of(view, cx);
            let root = view.workspace_path();
            let session = view.shown_session();
            let waiting = view
                .data()
                .map(crate::screens::code_column::reviewables)
                .unwrap_or_default();
            let (running, pending) = view.data().map_or((false, 0), |data| {
                let pending = data
                    .delegations
                    .values()
                    .filter(|record| record.status == DelegationStatus::ReviewPending)
                    .count();
                (data.running, pending)
            });
            // Another task: the stage starts empty; the agent's turn marks the Chat tab.
            if root != this.stage_root {
                this.stage_root = root.clone();
                this.stage.update(cx, |stage, cx| stage.clear(cx));
                this.merge = None;
                this.code_column
                    .update(cx, |column, cx| column.set_open_diff(None, cx));
            }
            this.stage
                .update(cx, |stage, cx| stage.set_agent_working(running, cx));
            this.terminal
                .update(cx, |dock, cx| dock.set_place(place, cx));
            this.code_column.update(cx, |column, cx| {
                column.set_root(root, cx);
                column.set_reviews(session, waiting, cx);
                column.set_conversation_state(running, pending, cx);
            });
        });
        let (terminal, terminal_events) = terminal::new_dock(&prefs, window, cx);
        Self {
            terminal,
            _terminal_events: terminal_events,
            layout: ShellLayout::new(prefs.sidebar_width, prefs.sidebar_collapsed),
            prefs,
            prefs_path,
            sidebar,
            conversation,
            new_task,
            code_column,
            stage,
            stage_root: None,
            focus: cx.focus_handle(),
            surface: Surface::Conversation,
            focus_on_open: None,
            open_project: None,
            merge: None,
            annotation: None,
            bootstrap_provider: None,
            _appearance: appearance,
            _sidebar_events: sidebar_events,
            _sidebar_data: sidebar_data,
            _new_task_events: new_task_events,
            _conversation_events: conversation_events,
            _code_column_events: code_column_events,
            _conversation_root: conversation_root,
            _stage_events: stage_events,
        }
    }

    /// `handleLinkClick`: a file of the task opens on the stage at its line;
    /// another local file opens with the system; web links in the browser.
    fn open_link(&mut self, target: &str, window: &mut Window, cx: &mut Context<Self>) {
        use dcc_core::domain::file_reference::{is_local_file_href, parse_file_reference};
        let root = self.stage_root.clone();
        if let Some(reference) = root
            .as_deref()
            .and_then(|root| parse_file_reference(target, root))
        {
            let root = root.unwrap_or_default();
            let path = reference.path.clone();
            self.code_column.update(cx, |column, cx| {
                column.set_open_diff(None, cx);
                column.reveal(Some(path), cx);
            });
            self.stage.update(cx, |stage, cx| {
                stage.open_file(root, reference.path, reference.line, window, cx)
            });
            return;
        }
        if is_local_file_href(target) {
            let path = target.trim().trim_start_matches("file://");
            cx.open_with_system(std::path::Path::new(path));
            return;
        }
        if target.starts_with("http://")
            || target.starts_with("https://")
            || target.starts_with("mailto:")
        {
            cx.open_url(target);
        }
    }

    /// The stage's content closed: back to "Mudanças" when it came from there.
    fn on_stage_event(
        &mut self,
        _: &Entity<Stage>,
        event: &StageEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            StageEvent::Annotate(request) => {
                self.open_annotation(request.clone(), true, window, cx)
            }
            StageEvent::ThreadToAgent {
                annotation,
                instruction,
                send,
            } => self.thread_to_agent(annotation, instruction, *send, window, cx),
            StageEvent::Closed { to_changes } => {
                self.code_column.update(cx, |column, cx| {
                    column.set_open_diff(None, cx);
                    column.reveal(None, cx);
                });
                if *to_changes {
                    self.set_code_column(true, Some(CodeColumnTab::Changes), cx);
                }
            }
        }
    }

    /// `handleReviewDelegation`: "Mudanças" opens on the delegation's review;
    /// from a file chip, that file's diff opens on the stage too.
    fn on_conversation_event(
        &mut self,
        _: &Entity<ConversationView>,
        event: &crate::screens::conversation::ConversationEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            crate::screens::conversation::ConversationEvent::ReviewDelegation { id, path } => {
                self.set_code_column(true, Some(CodeColumnTab::Changes), cx);
                let request = self.code_column.update(cx, |column, cx| {
                    column.review_delegation(id, cx);
                    path.as_deref()
                        .and_then(|path| column.delegation_file_request(id, path))
                });
                if let Some(request) = request {
                    let open = Some((request.scope.clone(), request.path.clone()));
                    self.code_column.update(cx, |column, cx| {
                        column.set_open_diff(open, cx);
                        column.reveal(None, cx);
                    });
                    self.stage
                        .update(cx, |stage, cx| stage.open_diff(request, window, cx));
                }
            }
        }
    }

    fn on_code_column_event(
        &mut self,
        _: &Entity<CodeColumn>,
        event: &CodeColumnEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            // A file picked in the tree opens on the stage, marked in the tree.
            CodeColumnEvent::OpenFile(path) => {
                let path = path.clone();
                self.code_column.update(cx, |column, cx| {
                    column.set_open_diff(None, cx);
                    column.reveal(Some(path.clone()), cx);
                });
                if let Some(root) = self.code_column.read(cx).root.clone() {
                    self.stage.update(cx, |stage, cx| {
                        stage.open_file(root, path, None, window, cx)
                    });
                }
            }
            CodeColumnEvent::Annotate(request) => {
                self.open_annotation(request.clone(), false, window, cx)
            }
            CodeColumnEvent::OpenMergeResolver(request) => {
                self.open_merge_resolver(request.clone(), cx)
            }
            CodeColumnEvent::OpenDiff(request) => {
                let open = Some((request.scope.clone(), request.path.clone()));
                self.code_column.update(cx, |column, cx| {
                    column.set_open_diff(open, cx);
                    column.reveal(None, cx);
                });
                let request = request.clone();
                self.stage
                    .update(cx, |stage, cx| stage.open_diff(request, window, cx));
            }
            CodeColumnEvent::Mention(path) => {
                // Mentioning a file is writing to the agent: the Chat comes forward.
                self.stage.update(cx, |stage, cx| stage.show_chat(cx));
                let composer = self.conversation.read(cx).composer.clone();
                composer.update(cx, |composer, cx| composer.mention_file(path, window, cx));
            }
            CodeColumnEvent::AskAgent { prompt, delivered } => {
                let composer = self.conversation.read(cx).composer.clone();
                let sent =
                    composer.update(cx, |composer, cx| composer.append_text(prompt, window, cx));
                if sent {
                    crate::ui::Toaster::success(delivered.clone(), cx);
                }
            }
            CodeColumnEvent::OpenSession(session) => {
                let session = session.clone();
                self.conversation.update(cx, |conversation, cx| {
                    conversation.open_session(session, cx)
                });
            }
            CodeColumnEvent::Close => self.set_code_column(false, None, cx),
            CodeColumnEvent::TabChanged(tab) => self.set_code_column(true, Some(*tab), cx),
        }
    }

    /// `setCodeColumnOpen` / `setCodeColumnTab`, kept with the layout.
    fn set_code_column(&mut self, open: bool, tab: Option<CodeColumnTab>, cx: &mut Context<Self>) {
        let tab = tab.unwrap_or(self.prefs.code_column_tab);
        self.code_column
            .update(cx, |column, cx| column.set_tab(tab, cx));
        if self.prefs.code_column_open != open || self.prefs.code_column_tab != tab {
            self.prefs.code_column_open = open;
            self.prefs.code_column_tab = tab;
            if let Err(error) = self.prefs.save(&self.prefs_path) {
                eprintln!("[dcc-native] could not save the layout: {error}");
            }
        }
        cx.notify();
    }

    /// `toggleCodeColumnTab`: shows one list, or closes when it already shows.
    fn toggle_code_column(&mut self, tab: CodeColumnTab, cx: &mut Context<Self>) {
        if self.prefs.code_column_open && self.prefs.code_column_tab == tab {
            self.set_code_column(false, None, cx);
        } else {
            self.set_code_column(true, Some(tab), cx);
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

    /// `globalSurface === null && selectedLocalWorkspacePath && codeColumnOpen`.
    fn code_column_shown(&self, cx: &gpui::App) -> bool {
        self.prefs.code_column_open
            && self.merge.is_none()
            && self.surface == Surface::Conversation
            && self.code_column.read(cx).root.is_some()
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

impl Shell {
    /// The conversation, with the stage over it while it has content: the
    /// conversation stays mounted below the strip, and the Chat tab shrinks
    /// the stage to it.
    fn conversation_with_stage(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let stage = self.stage.read(cx);
        if !stage.has_content() {
            return self.conversation.clone().into_any_element();
        }
        let chat = stage.chat_active();
        div()
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex_col()
                    .min_h_0()
                    .min_w_0()
                    .pt(px(36.0))
                    .child(self.conversation.clone()),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .flex()
                    .flex_col()
                    .map(|layer| {
                        if chat {
                            layer.h(px(36.0)).overflow_hidden()
                        } else {
                            layer.bottom_0()
                        }
                    })
                    .child(self.stage.clone()),
            )
            .into_any_element()
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if window.focused(cx).is_none() {
            window.focus(&self.focus, cx);
        }
        let theme = cx.theme();
        let shell = cx.entity().downgrade();

        div()
            .id("shell")
            .track_focus(&self.focus)
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
            .on_action(cx.listener(|this, _: &ToggleCodeFiles, _, cx| {
                this.toggle_code_column(CodeColumnTab::Files, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ToggleTerminal, window, cx| {
                    this.toggle_terminal(window, cx)
                }),
            )
            // Esc outside the terminal hides its dock (the drawer's window listener).
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.terminal.update(cx, |dock, cx| dock.step_back(cx));
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleCodeChanges, _, cx| {
                this.toggle_code_column(CodeColumnTab::Changes, cx)
            }))
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
            .children(self.code_column_shown(cx).then(|| self.code_column.clone()))
            .child(MainRegion::default().child(match self.surface {
                // No task at all: `WorkspaceBootstrapState`.
                Surface::Conversation if self.sidebar.read(cx).has_open_task() == Some(false) => {
                    bootstrap::render(self, cx)
                }
                Surface::Conversation => {
                    let content = match self.merge_resolver() {
                        Some(resolver) => resolver.into_any_element(),
                        None => self.conversation_with_stage(cx),
                    };
                    self.with_terminal(content, cx)
                }
                Surface::NewTask => self.new_task.clone().into_any_element(),
            }))
            .children(self.open_project.as_ref().map(|(dialog, _)| dialog.clone()))
            .children(self.annotation.as_ref().map(|(card, _)| card.clone()))
            .children(Toaster::handle(cx))
    }
}
