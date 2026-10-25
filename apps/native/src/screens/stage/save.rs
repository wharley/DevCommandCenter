//! Saving the active file (`handleSave`): ⌘S or "Salvar" writes through
//! the desktop only when the disk still has what the tab last read; when an
//! agent changed it meanwhile, the reconcile dialog shows the disk beside
//! the edit to overwrite, take the disk, or cancel. Closing an unsaved tab
//! asks first.

use gpui::{AppContext, Context, Entity, Window};

use super::files::FileLoad;
use super::{Discard, Reconcile, Stage};
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::i18n::{Args, Translate};
use crate::screens::diff_view::DiffView;
use crate::screens::file_view::FileView;
use crate::ui::Toaster;

impl Stage {
    fn active_editor(&self) -> Option<(String, String, Entity<FileView>)> {
        let tabs = self.files.as_ref()?;
        let file = tabs.active()?;
        match &file.load {
            Some(FileLoad::Text(view)) => {
                Some((tabs.root.clone(), file.path.clone(), view.clone()))
            }
            _ => None,
        }
    }

    fn set_saving(&mut self, path: &str, saving: bool) {
        if let Some(file) = self
            .files
            .as_mut()
            .and_then(|tabs| tabs.files.iter_mut().find(|file| file.path == path))
        {
            file.saving = saving;
        }
    }

    /// ⌘S / "Salvar": nothing to do when the text is what was read.
    pub(crate) fn save(&mut self, cx: &mut Context<Self>) {
        let Some((root, path, view)) = self.active_editor() else {
            return;
        };
        let (mine, base) = {
            let view = view.read(cx);
            (view.text().to_string(), view.base.clone())
        };
        if mine == base {
            return;
        }
        if self
            .files
            .as_ref()
            .and_then(|tabs| tabs.active())
            .is_some_and(|file| file.saving)
        {
            return;
        }
        self.write(root, path, view, mine, Some(base), cx);
    }

    fn write(
        &mut self,
        root: String,
        path: String,
        view: Entity<FileView>,
        mine: String,
        expected: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.set_saving(&path, true);
        let api = self
            .endpoint
            .clone()
            .or_else(DesktopApi::load)
            .ok_or(ApiError::DesktopClosed);
        cx.notify();
        let (written, read_path) = (mine.clone(), path.clone());
        self._save = Some(cx.spawn(async move |this, cx| {
            let result =
                cx.background_executor()
                    .spawn(async move {
                        api?.write_file(&root, &read_path, &written, expected.as_deref())
                    })
                    .await;
            let _ = this.update(cx, |stage, cx| {
                stage.set_saving(&path, false);
                let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                match result {
                    Ok(Ok(())) => {
                        view.update(cx, |view, cx| view.mark_saved(mine, cx));
                        stage.reconcile = None;
                        Toaster::success(
                            cx.t_with("fileSurface.saved", &Args::default().with("name", name)),
                            cx,
                        );
                    }
                    Ok(Err(disk)) => {
                        let dark = view.read(cx).dark;
                        let diff = cx.new(|cx| {
                            DiffView::side_by_side(
                                path.clone(),
                                disk.clone(),
                                mine.clone(),
                                dark,
                                cx,
                            )
                        });
                        stage.reconcile = Some(Reconcile {
                            path: path.clone(),
                            name,
                            disk,
                            mine,
                            view: view.clone(),
                            diff,
                        });
                    }
                    Err(ApiError::DesktopClosed) => {
                        Toaster::error(cx.t("native.desktopClosed"), cx)
                    }
                    Err(ApiError::Failed(message)) => Toaster::error(message, cx),
                    Err(_) => Toaster::error(cx.t("fileSurface.saveFailed"), cx),
                }
                cx.notify();
            });
        }));
    }

    /// "Sobrescrever com a minha": past the guard, as chosen.
    pub(crate) fn overwrite(&mut self, cx: &mut Context<Self>) {
        let Some(reconcile) = self.reconcile.as_ref() else {
            return;
        };
        let Some(root) = self.files.as_ref().map(|tabs| tabs.root.clone()) else {
            return;
        };
        let (path, view, mine) = (
            reconcile.path.clone(),
            reconcile.view.clone(),
            reconcile.mine.clone(),
        );
        self.write(root, path, view, mine, None, cx);
    }

    /// "Recarregar do disco": the disk's text in the tab.
    pub(crate) fn take_disk(&mut self, cx: &mut Context<Self>) {
        let Some(reconcile) = self.reconcile.take() else {
            return;
        };
        reconcile
            .view
            .update(cx, |view, cx| view.take_disk(reconcile.disk, cx));
        cx.notify();
    }

    pub(crate) fn cancel_reconcile(&mut self, cx: &mut Context<Self>) {
        if self.saving_reconcile(cx) {
            return;
        }
        self.reconcile = None;
        cx.notify();
    }

    pub(crate) fn saving_reconcile(&self, _: &gpui::App) -> bool {
        self.reconcile.as_ref().is_some_and(|reconcile| {
            self.files
                .as_ref()
                .and_then(|tabs| tabs.files.iter().find(|file| file.path == reconcile.path))
                .is_some_and(|file| file.saving)
        })
    }

    /// The unsaved changes go: the tab (or every tab) closes.
    pub(crate) fn confirm_discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.discard.take() {
            Some(Discard::Tab(at)) => self.close_file(at, window, cx),
            Some(Discard::All) => self.close(cx),
            Some(Discard::ForDiff(request)) => self.replace_with_diff(request, window, cx),
            None => {}
        }
        cx.notify();
    }

    pub(crate) fn cancel_discard(&mut self, cx: &mut Context<Self>) {
        self.discard = None;
        cx.notify();
    }
}
