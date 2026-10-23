//! `ProjectEditDialog`: the project's name, mascot or symbol or logo, color,
//! and the folders it links into every new task. Saved through
//! `dcc_infra::project_actions`, the same checks the desktop runs.

mod view;

use std::path::PathBuf;

use dcc_core::domain::repository::{Repository, RepositoryId};
use dcc_core::ports::RepositoryRepo;
use dcc_infra::db::SqliteWorkspaceRepo;
use dcc_infra::project_actions::{self, ProjectIdentity};
use futures::executor::block_on;
use gpui::{AppContext, Context, Entity, PathPromptOptions, Subscription, Window};

use super::SidebarDialog;
use crate::data::loader;
use crate::data::project_logo;
use crate::i18n::Translate;
use crate::project::{COLOR_IDS, is_known_icon};
use crate::screens::sidebar::Sidebar;
use crate::ui::{TextInput, TextInputEvent, Toaster};

pub use view::render;

/// Nothing stored: the mascot and color come from the project path.
pub const AUTO: &str = "auto";

pub struct ProjectEditDialog {
    pub repository: Repository,
    pub name: Entity<TextInput>,
    pub icon: String,
    pub color: String,
    pub logo: Option<String>,
    pub folders: Vec<String>,
    pub saved_folders: Vec<String>,
    /// Saved linked folders without Git: they join tasks as context.
    pub context_folders: Vec<String>,
    pub saving: bool,
    _name_events: Subscription,
}

impl ProjectEditDialog {
    fn name_text(&self, cx: &gpui::App) -> String {
        self.name.read(cx).text().to_string()
    }

    /// `normalizedDisplayName`: empty or the repository's own name stores nothing.
    fn next_display_name(&self, cx: &gpui::App) -> Option<String> {
        let name = self.name_text(cx);
        let trimmed = name.trim();
        (!trimmed.is_empty()
            && trimmed.to_lowercase() != self.repository.name.trim().to_lowercase())
        .then(|| trimmed.to_string())
    }

    fn explicit(value: &str) -> Option<String> {
        (value != AUTO).then(|| value.to_string())
    }

    fn identity_dirty(&self, cx: &gpui::App) -> bool {
        let current = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        self.next_display_name(cx) != current(&self.repository.display_name)
            || Self::explicit(&self.icon) != current(&self.repository.icon)
            || Self::explicit(&self.color) != current(&self.repository.color)
            || self.logo != self.repository.logo
    }

    fn folders_dirty(&self) -> bool {
        self.folders != self.saved_folders
    }

    pub fn is_dirty(&self, cx: &gpui::App) -> bool {
        self.identity_dirty(cx) || self.folders_dirty()
    }
}

impl Sidebar {
    /// "Editar projeto" from a row's right-click menu; reads the project and
    /// its linked folders fresh before the dialog opens.
    pub(in crate::screens::sidebar) fn open_edit_project(
        &mut self,
        repository_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = repository_id.to_string();
        let db_path = loader::database_path();
        cx.spawn_in(window, async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    let repo =
                        SqliteWorkspaceRepo::open(&db_path).map_err(|error| error.to_string())?;
                    let repository = block_on(repo.get_repository(&RepositoryId(id.clone())))
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "project not found".to_string())?;
                    let folders = repo
                        .list_repository_linked_folders()
                        .map_err(|error| error.to_string())?
                        .into_iter()
                        .find(|(repository_id, _)| repository_id.0 == id)
                        .map(|(_, paths)| paths)
                        .unwrap_or_default();
                    Ok::<_, String>((repository, folders))
                })
                .await;
            let _ = this.update_in(cx, |sidebar, window, cx| match loaded {
                Ok((repository, folders)) => {
                    sidebar.show_edit_project(repository, folders, window, cx)
                }
                Err(error) => Toaster::error(error, cx),
            });
        })
        .detach();
    }

    fn show_edit_project(
        &mut self,
        repository: Repository,
        folders: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let display = crate::data::tasks::repository_display_name(&repository);
        let placeholder = repository.name.clone();
        let name = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_max_len(80)
                .with_placeholder(placeholder)
        });
        name.update(cx, |input, cx| {
            input.set_text(display, cx);
            input.focus_and_select_all(window, cx);
        });
        let events =
            cx.subscribe_in(
                &name,
                window,
                |this, _, event: &TextInputEvent, _, cx| match event {
                    TextInputEvent::Submitted => this.save_project(cx),
                    TextInputEvent::Changed => cx.notify(),
                    TextInputEvent::Cancelled | TextInputEvent::Blurred => {}
                },
            );
        let pick = |value: &Option<String>, known: &dyn Fn(&str) -> bool| {
            value
                .as_deref()
                .filter(|value| known(value))
                .unwrap_or(AUTO)
                .to_string()
        };
        let icon = pick(&repository.icon, &is_known_icon);
        let color = pick(&repository.color, &|value| COLOR_IDS.contains(&value));
        self.dialog = Some(SidebarDialog::EditProject(Box::new(ProjectEditDialog {
            logo: repository.logo.clone(),
            context_folders: project_actions::linked_context_paths(&folders),
            saved_folders: folders.clone(),
            folders,
            icon,
            color,
            name,
            repository,
            saving: false,
            _name_events: events,
        })));
        cx.notify();
    }

    pub(super) fn edit_project_dialog(&mut self) -> Option<&mut ProjectEditDialog> {
        match self.dialog.as_mut()? {
            SidebarDialog::EditProject(dialog) => Some(dialog),
            _ => None,
        }
    }

    fn edit_project(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut ProjectEditDialog),
    ) {
        if let Some(dialog) = self.edit_project_dialog()
            && !dialog.saving
        {
            change(dialog);
        }
        cx.notify();
    }

    fn use_repository_name(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.edit_project_dialog() else {
            return;
        };
        let name = dialog.repository.name.clone();
        let input = dialog.name.clone();
        input.update(cx, |input, cx| input.set_text(name, cx));
        cx.notify();
    }

    fn pick_logo(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let logo = cx
                .background_executor()
                .spawn(async move { project_logo::logo_from_file(&path) })
                .await;
            let _ = this.update(cx, |sidebar, cx| match logo {
                Ok(logo) => sidebar.edit_project(cx, |dialog| {
                    dialog.logo = Some(logo);
                    dialog.icon = "logo".into();
                }),
                Err(error) => {
                    eprintln!("[dcc-native] project logo: {error}");
                    Toaster::error(cx.t("projectEditor.logoError"), cx);
                }
            });
        })
        .detach();
    }

    fn add_linked_folder(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths
                .into_iter()
                .next()
                .map(|path: PathBuf| path.to_string_lossy().trim().to_string())
            else {
                return;
            };
            if path.is_empty() {
                return;
            }
            let _ = this.update(cx, |sidebar, cx| {
                sidebar.edit_project(cx, |dialog| {
                    if !dialog.folders.contains(&path) {
                        dialog.folders.push(path);
                    }
                })
            });
        })
        .detach();
    }

    fn save_project(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialog.as_ref().and_then(|dialog| match dialog {
            SidebarDialog::EditProject(dialog) => Some(dialog),
            _ => None,
        }) else {
            return;
        };
        if dialog.saving || !dialog.is_dirty(cx) {
            return;
        }
        let identity = dialog.identity_dirty(cx).then(|| ProjectIdentity {
            display_name: dialog.next_display_name(cx),
            icon: ProjectEditDialog::explicit(&dialog.icon),
            color: ProjectEditDialog::explicit(&dialog.color),
            logo: dialog.logo.clone(),
        });
        let folders = dialog.folders_dirty().then(|| dialog.folders.clone());
        let repository_id = dialog.repository.id.0.clone();
        if let Some(dialog) = self.edit_project_dialog() {
            dialog.saving = true;
        }
        cx.notify();
        let db_path = loader::database_path();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let repo =
                        SqliteWorkspaceRepo::open(&db_path).map_err(|error| error.to_string())?;
                    if let Some(identity) = identity {
                        block_on(project_actions::update_repository_identity(
                            &repo,
                            &repository_id,
                            &identity,
                        ))?;
                    }
                    if let Some(folders) = folders {
                        block_on(project_actions::set_repository_linked_folders(
                            &repo,
                            &repository_id,
                            &folders,
                        ))?;
                    }
                    Ok::<_, String>(())
                })
                .await;
            let _ = this.update(cx, |sidebar, cx| {
                match result {
                    Ok(()) => {
                        Toaster::success(cx.t("projectEditor.saved"), cx);
                        sidebar.dialog = None;
                        sidebar.reload_now(cx);
                    }
                    Err(error) => {
                        if let Some(dialog) = sidebar.edit_project_dialog() {
                            dialog.saving = false;
                        }
                        Toaster::error_with(cx.t("projectEditor.saveError"), error, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
