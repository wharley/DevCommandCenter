//! Getting files into the prompt: the + menu's picker, a pasted image (saved
//! as `dcc_img_<uuid>` in the temp folder, like the desktop), a drop, and the
//! `@arquivo` popup. Paths inside the task's checkout go in relative.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use gpui::{ClipboardEntry, Context, ImageFormat, PathPromptOptions};

use super::Composer;
use super::mentions::{self, FileList, MentionMenu};
use super::prompt::relative_to_workspace;
use crate::data::prompt::is_image_path;
use crate::i18n::Translate;
use crate::ui::Toaster;
use crate::ui::text_area::Chip;

fn extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Gif => "gif",
        ImageFormat::Webp => "webp",
        ImageFormat::Svg => "svg",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tiff",
        _ => "png",
    }
}

impl Composer {
    fn chip_for(&self, path: &std::path::Path) -> Chip {
        let stored = relative_to_workspace(self.session.workspace_path.as_deref(), path);
        if is_image_path(&stored) {
            Chip::Image(stored)
        } else {
            Chip::File(stored)
        }
    }

    /// Files picked or dropped go in at the caret, in order.
    pub(crate) fn add_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if !self.session.usable() || paths.is_empty() {
            return;
        }
        let chips: Vec<Chip> = paths.iter().map(|path| self.chip_for(path)).collect();
        self.input.update(cx, |input, cx| {
            for chip in chips {
                input.insert_chip(chip, None, cx);
            }
        });
    }

    /// "Adicionar anexo": the system's file picker, several at once.
    pub(crate) fn open_picker(&mut self, cx: &mut Context<Self>) {
        let prompt = cx.t("composer.attachments.pickerTitle");
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(prompt),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| this.add_paths(paths, cx));
        })
        .detach();
    }

    /// An image on the clipboard: saved to the temp folder, then a chip at
    /// the end of the prompt.
    pub(crate) fn paste_image(&mut self, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let images: Vec<(Vec<u8>, &'static str)> = item
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                ClipboardEntry::Image(image) => {
                    Some((image.bytes.clone(), extension(image.format)))
                }
                _ => None,
            })
            .collect();
        if images.is_empty() || !self.session.usable() {
            return;
        }
        let failure = cx.t("composer.attachments.error");
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move {
                    images
                        .into_iter()
                        .map(|(bytes, ext)| {
                            dcc_infra::prompt_attachments::save_temp_image(&bytes, ext)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                for result in saved {
                    match result {
                        Ok(path) => {
                            let chip = this.chip_for(&path);
                            this.input
                                .update(cx, |input, cx| input.append_chip(chip, cx));
                        }
                        Err(error) => Toaster::error_with(failure.clone(), error.to_string(), cx),
                    }
                }
            });
        })
        .detach();
    }

    /// Opens, filters or closes the popup for the `@query` before the caret.
    pub(crate) fn update_mention(&mut self, cx: &mut Context<Self>) {
        let (before, has_selection) = {
            let input = self.input.read(cx);
            (
                input.text()[..input.cursor()].to_string(),
                input.has_selection(),
            )
        };
        let Some(root) = self.session.workspace_path.clone() else {
            return self.close_mention(cx);
        };
        let Some((range, query)) = (!has_selection)
            .then(|| mentions::trigger(&before))
            .flatten()
        else {
            return self.close_mention(cx);
        };
        let files = match &self.files {
            Some(list) if list.root == root => list.files.clone(),
            _ => Arc::new(Vec::new()),
        };
        if !self
            .files
            .as_ref()
            .is_some_and(|list| list.fresh_for(&root))
        {
            self.load_files(root, cx);
        }
        let options = mentions::filter(&files, &query);
        if options.is_empty() {
            return self.close_mention(cx);
        }
        let selected = self
            .mention
            .as_ref()
            .filter(|menu| menu.range.start == range.start)
            .map_or(0, |menu| menu.selected.min(options.len() - 1));
        self.mention = Some(MentionMenu {
            range,
            options,
            selected,
        });
        self.input.update(cx, |input, _| input.set_menu_open(true));
        cx.notify();
    }

    fn load_files(&mut self, root: String, cx: &mut Context<Self>) {
        if self.files_loading {
            return;
        }
        self.files_loading = true;
        cx.spawn(async move |this, cx| {
            let folder = root.clone();
            let files = cx
                .background_executor()
                .spawn(async move {
                    dcc_infra::workspace_git::git_tracked_files(&folder).unwrap_or_default()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.files_loading = false;
                this.files = Some(FileList {
                    root,
                    files: Arc::new(files),
                    read_at: Instant::now(),
                });
                this.update_mention(cx);
            });
        })
        .detach();
    }

    pub(crate) fn close_mention(&mut self, cx: &mut Context<Self>) {
        if self.mention.take().is_some() {
            self.input.update(cx, |input, _| input.set_menu_open(false));
            cx.notify();
        }
    }

    pub(crate) fn move_mention(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(menu) = self.mention.as_mut() {
            menu.move_by(delta);
            cx.notify();
        }
    }

    pub(crate) fn highlight_mention(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(menu) = self.mention.as_mut()
            && menu.selected != index
            && index < menu.options.len()
        {
            menu.selected = index;
            cx.notify();
        }
    }

    /// The pick replaces the typed `@query` with a chip and a space.
    pub(crate) fn accept_mention(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        let Some(menu) = self.mention.take() else {
            return;
        };
        let Some(path) = menu.options.get(index.unwrap_or(menu.selected)).cloned() else {
            return;
        };
        self.input.update(cx, |input, cx| {
            input.set_menu_open(false);
            input.insert_chip(Chip::File(path), Some(menu.range.clone()), cx);
            input.insert(" ", cx);
        });
        cx.notify();
    }
}
