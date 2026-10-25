//! The stage's file tabs (`FileTabsSurface`): a file opens as a preview
//! (italic) that the next preview replaces; a double-click pins it. Closing
//! a tab activates its neighbour, and the last one closes the files. Only
//! the active tab keeps its file read (`shouldKeepFileSurfaceMounted`); a
//! tab read again comes from the disk.

use gpui::{AppContext, Context, Entity, Task, Window};

use super::image::{ImageView, is_viewable_image};
use super::{Stage, ThemeDark};
use crate::screens::file_view::FileView;

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
        if tabs.active != at
            && let Some(leaving) = tabs.files.get_mut(tabs.active)
        {
            leaving.load = None;
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
                    let relative =
                        dcc_infra::workspace_file_preview::validate_git_relative_path(&read_path)?;
                    dcc_infra::workspace_file_preview::read_worktree_file_text(&root, &relative)?
                        .ok_or_else(|| "file not found".to_string())
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
                    Ok(text) => {
                        let line = file.focus_line.take();
                        let name = file.path.clone();
                        FileLoad::Text(cx.new(|cx| {
                            let mut view = FileView::new(name, text, dark, cx);
                            view.focus(line, cx);
                            view
                        }))
                    }
                    Err(error) => FileLoad::Failed(error),
                });
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

    /// A tab's X: its neighbour comes forward; the last one closes the files.
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

    /// The files' theme followed: a file colored for the other theme is read again.
    pub(crate) fn follow_files_theme(&mut self, cx: &mut Context<Self>) {
        let dark = cx.theme_dark();
        let stale = self
            .files
            .as_ref()
            .and_then(FileTabs::active)
            .and_then(|file| match &file.load {
                Some(FileLoad::Text(view)) => Some(view.read(cx).dark != dark),
                _ => None,
            })
            .unwrap_or(false);
        if stale {
            if let Some(file) = self
                .files
                .as_mut()
                .and_then(|tabs| tabs.files.get_mut(tabs.active))
            {
                file.load = None;
            }
            self.load_active_file(cx);
        }
    }
}
