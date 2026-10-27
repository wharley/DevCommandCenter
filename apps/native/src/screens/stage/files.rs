//! The stage's file tabs (`FileTabsSurface`): a file opens as a preview
//! (italic) that the next preview replaces; a double-click pins it. Closing
//! a tab activates its neighbour, and the last one closes the files. Only
//! the active tab keeps its file read (`shouldKeepFileSurfaceMounted`); a
//! tab read again comes from the disk.

use gpui::{AppContext, Context, Entity, Task, Window};

use super::image::{ImageView, is_viewable_image};
use super::{Stage, ThemeDark};
use crate::screens::file_view::{FileView, FileViewEvent};

pub(crate) enum FileLoad {
    Loading(#[allow(dead_code)] Task<()>),
    Text(Entity<FileView>),
    Image(Entity<ImageView>),
    Failed(String),
}

pub(crate) struct OpenFile {
    pub path: String,
    pub name: String,
    /// A preview tab: the next preview replaces it.
    pub preview: bool,
    pub focus_line: Option<u32>,
    pub load: Option<FileLoad>,
    /// "Salvar" running.
    pub saving: bool,
    _edited: Option<gpui::Subscription>,
}

impl OpenFile {
    /// Unsaved changes in its editor.
    pub fn is_dirty(&self, cx: &gpui::App) -> bool {
        matches!(&self.load, Some(FileLoad::Text(view)) if view.read(cx).is_dirty())
    }
}

pub(crate) struct FileTabs {
    pub root: String,
    pub files: Vec<OpenFile>,
    pub active: usize,
}

impl FileTabs {
    pub fn active(&self) -> Option<&OpenFile> {
        self.files.get(self.active)
    }
}

fn name_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

impl Stage {
    /// A file on the stage (`file-edit`): its tab comes forward (at `line`
    /// when given); a new one is a preview in place of the last preview.
    pub fn open_file(
        &mut self,
        root: String,
        path: String,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.diff = None;
        self.session = None;
        self.chat_active = false;
        let tabs = self.files.get_or_insert_with(|| FileTabs {
            root: root.clone(),
            files: Vec::new(),
            active: 0,
        });
        if tabs.root != root {
            *tabs = FileTabs {
                root,
                files: Vec::new(),
                active: 0,
            };
        }
        let at = match tabs.files.iter().position(|file| file.path == path) {
            Some(at) => {
                if line.is_some() {
                    tabs.files[at].focus_line = line;
                }
                at
            }
            None => {
                tabs.files.retain(|file| !file.preview);
                tabs.files.push(OpenFile {
                    name: name_of(&path),
                    path,
                    preview: true,
                    focus_line: line,
                    load: None,
                    saving: false,
                    _edited: None,
                });
                tabs.files.len() - 1
            }
        };
        self.activate_file(at, window, cx);
    }

    /// A tab to the front; the one leaving lets its file go.
    pub(crate) fn activate_file(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tabs) = self.files.as_mut() else {
            return;
        };
        if at >= tabs.files.len() {
            return;
        }
        // A clean tab leaving lets its file go; an unsaved one stays
        // (`shouldKeepFileSurfaceMounted`).
        let leaving_dirty = tabs
            .files
            .get(tabs.active)
            .is_some_and(|file| file.is_dirty(cx));
        let tabs = self.files.as_mut().expect("tabs");
        if tabs.active != at
            && !leaving_dirty
            && let Some(leaving) = tabs.files.get_mut(tabs.active)
        {
            leaving.load = None;
            leaving._edited = None;
        }
        tabs.active = at;
        self.chat_active = false;
        self.load_active_file(cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn load_active_file(&mut self, cx: &mut Context<Self>) {
        let dark = cx.theme_dark();
        let Some(tabs) = self.files.as_mut() else {
            return;
        };
        let root = tabs.root.clone();
        let Some(file) = tabs.files.get_mut(tabs.active) else {
            return;
        };
        if let Some(load) = &file.load {
            // Already read: a new line to show.
            if let (FileLoad::Text(view), Some(line)) = (load, file.focus_line.take()) {
                view.update(cx, |view, cx| view.focus(Some(line), cx));
            }
            return;
        }
        let path = file.path.clone();
        if is_viewable_image(&path) {
            let absolute =
                match dcc_infra::workspace_file_preview::validate_git_relative_path(&path) {
                    Ok(relative) => std::path::Path::new(&root).join(relative),
                    Err(error) => {
                        file.load = Some(FileLoad::Failed(error));
                        return;
                    }
                };
            file.load = Some(FileLoad::Image(cx.new(|cx| ImageView::new(absolute, cx))));
            return;
        }
        let read_path = path.clone();
        let task = cx.spawn(async move |this, cx| {
            let text = cx
                .background_executor()
                .spawn(async move {
                    use dcc_infra::workspace_file_preview::{
                        read_worktree_file_text, validate_git_relative_path,
                    };
                    let relative = validate_git_relative_path(&read_path)?;
                    let text = read_worktree_file_text(&root, &relative)?
                        .ok_or_else(|| "file not found".to_string())?;
                    // `useFileIndentation`: the project's rule, then the file's own.
                    let config = read_worktree_file_text(&root, ".editorconfig")
                        .ok()
                        .flatten();
                    let indentation = dcc_core::domain::indentation::resolve_file_indentation(
                        config.as_deref(),
                        &relative,
                        &text,
                    );
                    Ok::<_, String>((text, indentation))
                })
                .await;
            let _ = this.update(cx, |stage, cx| {
                let Some(file) = stage
                    .files
                    .as_mut()
                    .and_then(|tabs| tabs.files.get_mut(tabs.active))
                    .filter(|file| file.path == path)
                else {
                    return;
                };
                file.load = Some(match text {
                    Ok((text, indentation)) => {
                        let line = file.focus_line.take();
                        let name = file.path.clone();
                        let view = cx.new(|cx| {
                            let mut view =
                                FileView::new(name, text, indentation, dark, cx).annotatable();
                            view.focus(line, cx);
                            view
                        });
                        // The first edit pins a preview tab.
                        let edited_path = path.clone();
                        file._edited = Some(cx.subscribe(
                            &view,
                            move |stage, _, event: &FileViewEvent, cx| {
                                if let FileViewEvent::Annotate(request) = event {
                                    cx.emit(super::StageEvent::Annotate(request.clone()));
                                    return;
                                }
                                if let Some(file) = stage.files.as_mut().and_then(|tabs| {
                                    tabs.files.iter_mut().find(|file| file.path == edited_path)
                                }) {
                                    file.preview = false;
                                }
                                cx.notify();
                            },
                        ));
                        FileLoad::Text(view)
                    }
                    Err(error) => FileLoad::Failed(error),
                });
                stage.focus_editor = true;
                cx.notify();
            });
        });
        file.load = Some(FileLoad::Loading(task));
    }

    /// A double-click keeps the tab (`pinTab`).
    pub(crate) fn pin_file(&mut self, at: usize, cx: &mut Context<Self>) {
        if let Some(file) = self.files.as_mut().and_then(|tabs| tabs.files.get_mut(at)) {
            file.preview = false;
            cx.notify();
        }
    }

    /// A tab's X: an unsaved one asks first (`discardConfirm`).
    pub(crate) fn request_close_file(
        &mut self,
        at: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dirty = self
            .files
            .as_ref()
            .and_then(|tabs| tabs.files.get(at))
            .is_some_and(|file| file.is_dirty(cx));
        if dirty {
            self.discard = Some(super::Discard::Tab(at));
            cx.notify();
            return;
        }
        self.close_file(at, window, cx);
    }

    /// "Fechar editor": with anything unsaved, asks first.
    pub(crate) fn request_close_files(&mut self, cx: &mut Context<Self>) {
        let dirty = self
            .files
            .as_ref()
            .is_some_and(|tabs| tabs.files.iter().any(|file| file.is_dirty(cx)));
        if dirty {
            self.discard = Some(super::Discard::All);
            cx.notify();
            return;
        }
        self.close(cx);
    }

    /// The tab closes: its neighbour comes forward; the last one closes the files.
    pub(crate) fn close_file(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tabs) = self.files.as_mut() else {
            return;
        };
        if at >= tabs.files.len() {
            return;
        }
        let was_active = tabs.active == at;
        tabs.files.remove(at);
        if tabs.files.is_empty() {
            return self.close(cx);
        }
        if was_active {
            let next = at.min(tabs.files.len() - 1);
            tabs.active = usize::MAX;
            self.activate_file(next, window, cx);
        } else if tabs.active > at {
            tabs.active -= 1;
        }
        cx.notify();
    }

    /// The files follow the theme (an edit in progress stays).
    pub(crate) fn follow_files_theme(&mut self, cx: &mut Context<Self>) {
        let dark = cx.theme_dark();
        let views: Vec<Entity<FileView>> = self
            .files
            .iter()
            .flat_map(|tabs| tabs.files.iter())
            .filter_map(|file| match &file.load {
                Some(FileLoad::Text(view)) => Some(view.clone()),
                _ => None,
            })
            .collect();
        for view in views {
            if view.read(cx).dark != dark {
                view.update(cx, |view, cx| view.set_dark(dark, cx));
            }
        }
    }
}
