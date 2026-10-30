//! `AppshotsDialog`: + → "Capturar janela de aplicativo". The other apps'
//! windows, six to a page, each captured once (one at a time, this page
//! only) so the person sees exactly what goes; up to six chosen are saved
//! and attached to the message as images. Without Screen Recording, the way
//! to grant it, and a recheck when the app comes back to the front.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    AnyElement, Context, EventEmitter, FontWeight, Image, ImageFormat, SharedString, Subscription,
    Task, Window, div, img, prelude::*, px,
};

use crate::data::appshots::{self, Availability, ComputerTarget, MAX_IMAGES};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, Icon, dialog_footer, spinner};

/// Where the windows, their captures and the saved files come from: the
/// system in the app, fakes in tests.
#[derive(Clone)]
pub struct Source {
    pub availability: fn() -> Availability,
    pub targets: fn() -> Result<Vec<ComputerTarget>, String>,
    pub capture: fn(&ComputerTarget) -> Result<Vec<u8>, String>,
    pub folder: std::path::PathBuf,
}

impl Source {
    pub fn system() -> Self {
        Self {
            availability: appshots::availability,
            targets: appshots::targets,
            capture: appshots::capture,
            folder: appshots::folder(),
        }
    }
}

pub enum AppshotsEvent {
    /// The chosen captures, saved, to attach in order.
    Attach(Vec<std::path::PathBuf>),
    Close,
}

enum Stage {
    Loading,
    Unsupported,
    Permission,
    Ready(Vec<ComputerTarget>),
    Failed(String),
}

enum Preview {
    Ready {
        image: Arc<Image>,
        bytes: Arc<Vec<u8>>,
    },
    Failed(String),
}

pub struct AppshotsDialog {
    source: Source,
    stage: Stage,
    page: usize,
    previews: HashMap<String, Preview>,
    selected: Vec<String>,
    busy: bool,
    error: Option<String>,
    _capturing: Option<Task<()>>,
    _activation: Subscription,
}

impl EventEmitter<AppshotsEvent> for AppshotsDialog {}

impl AppshotsDialog {
    pub fn new(source: Source, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Screen Recording is granted in System Settings: read again on return.
        let activation = cx.observe_window_activation(window, |dialog, window, cx| {
            if window.is_window_active() && matches!(dialog.stage, Stage::Permission) {
                dialog.refresh(cx);
            }
        });
        let mut dialog = Self {
            source,
            stage: Stage::Loading,
            page: 0,
            previews: HashMap::new(),
            selected: Vec::new(),
            busy: false,
            error: None,
            _capturing: None,
            _activation: activation,
        };
        dialog.refresh(cx);
        dialog
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.stage = Stage::Loading;
        self.page = 0;
        self.previews.clear();
        self.selected.clear();
        self.error = None;
        self._capturing = None;
        cx.notify();
        let source = self.source.clone();
        cx.spawn(async move |this, cx| {
            let stage = cx
                .background_executor()
                .spawn(async move {
                    match (source.availability)() {
                        Availability::Unsupported => Stage::Unsupported,
                        Availability::Permission => Stage::Permission,
                        Availability::Ready => match (source.targets)() {
                            Ok(targets) => Stage::Ready(targets),
                            Err(code) => Stage::Failed(appshots::error_key(&code)),
                        },
                    }
                })
                .await;
            let _ = this.update(cx, |dialog, cx| {
                dialog.stage = stage;
                dialog.capture_page(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn visible(&self) -> &[ComputerTarget] {
        match &self.stage {
            Stage::Ready(targets) => {
                let start = (self.page * MAX_IMAGES).min(targets.len());
                &targets[start..(start + MAX_IMAGES).min(targets.len())]
            }
            _ => &[],
        }
    }

    /// Captures this page's windows not seen yet, one after another.
    fn capture_page(&mut self, cx: &mut Context<Self>) {
        let pending: Vec<ComputerTarget> = self
            .visible()
            .iter()
            .filter(|target| !self.previews.contains_key(&appshots::key(target)))
            .cloned()
            .collect();
        if pending.is_empty() {
            return;
        }
        let capture = self.source.capture;
        self._capturing = Some(cx.spawn(async move |this, cx| {
            for target in pending {
                let key = appshots::key(&target);
                let preview = cx
                    .background_executor()
                    .spawn(async move { capture(&target) })
                    .await
                    .map_or_else(
                        |code| Preview::Failed(appshots::error_key(&code)),
                        |bytes| Preview::Ready {
                            image: Arc::new(Image::from_bytes(ImageFormat::Png, bytes.clone())),
                            bytes: Arc::new(bytes),
                        },
                    );
                if this
                    .update(cx, |dialog, cx| {
                        dialog.previews.insert(key, preview);
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
            }
        }));
    }

    fn turn_page(&mut self, page: usize, cx: &mut Context<Self>) {
        self.page = page;
        self.capture_page(cx);
        cx.notify();
    }

    fn toggle(&mut self, key: String, cx: &mut Context<Self>) {
        if let Some(at) = self.selected.iter().position(|chosen| *chosen == key) {
            self.selected.remove(at);
        } else if self.selected.len() < MAX_IMAGES {
            self.selected.push(key);
        }
        cx.notify();
    }

    /// The chosen captures with their app's name, once every one is ready.
    fn chosen(&self) -> Option<Vec<(String, Arc<Vec<u8>>)>> {
        let Stage::Ready(targets) = &self.stage else {
            return None;
        };
        if self.selected.is_empty() {
            return None;
        }
        self.selected
            .iter()
            .map(|key| {
                let Some(Preview::Ready { bytes, .. }) = self.previews.get(key) else {
                    return None;
                };
                let name = targets
                    .iter()
                    .find(|target| appshots::key(target) == *key)
                    .map(|target| target.name.clone())
                    .unwrap_or_default();
                Some((name, bytes.clone()))
            })
            .collect()
    }

    fn attach(&mut self, cx: &mut Context<Self>) {
        let Some(chosen) = self.chosen() else {
            return;
        };
        self.busy = true;
        self.error = None;
        cx.notify();
        let folder = self.source.folder.clone();
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move {
                    let mut paths = Vec::new();
                    for (name, bytes) in chosen {
                        match appshots::save(&folder, &name, &bytes) {
                            Ok(path) => paths.push(path),
                            Err(code) => {
                                // A batch goes whole or not at all.
                                for path in paths {
                                    let _ = std::fs::remove_file(path);
                                }
                                return Err(code);
                            }
                        }
                    }
                    Ok(paths)
                })
                .await;
            let _ = this.update(cx, |dialog, cx| match saved {
                Ok(paths) => cx.emit(AppshotsEvent::Attach(paths)),
                Err(code) => {
                    dialog.busy = false;
                    dialog.error = Some(appshots::error_key(&code));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn request_access(&mut self, cx: &mut Context<Self>) {
        cx.background_executor()
            .spawn(async { appshots::request_access() })
            .detach();
        cx.open_url(appshots::SCREEN_RECORDING_SETTINGS);
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        if !self.busy {
            cx.emit(AppshotsEvent::Close);
        }
    }
}

fn card(
    dialog: &AppshotsDialog,
    index: usize,
    target: &ComputerTarget,
    cx: &mut Context<AppshotsDialog>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let radius = cx.theme().radius;
    let key = appshots::key(target);
    let preview = dialog.previews.get(&key);
    let checked = dialog.selected.contains(&key);
    let failed = matches!(preview, Some(Preview::Failed(_)));
    let disabled = dialog.busy || failed || (!checked && dialog.selected.len() >= MAX_IMAGES);
    let shot: AnyElement = match preview {
        Some(Preview::Ready { image, .. }) => img(image.clone())
            .size_full()
            .object_fit(gpui::ObjectFit::Contain)
            .into_any_element(),
        Some(Preview::Failed(message)) => div()
            .p(px(12.0))
            .text_size(px(12.0))
            .text_color(colors.muted_foreground)
            .child(cx.t(message))
            .into_any_element(),
        None => {
            spinner(SharedString::from(format!("appshot-loading-{index}")), 20.0).into_any_element()
        }
    };
    let title = if target.title.trim().is_empty() {
        cx.t("appshots.untitled")
    } else {
        SharedString::from(target.title.clone())
    };
    div()
        .id(("appshot-window", index))
        .debug_selector(move || format!("appshot-window-{index}"))
        .relative()
        .overflow_hidden()
        .rounded(radius.xl)
        .border_1()
        .map(|card| {
            if checked {
                card.border_color(colors.primary)
            } else {
                card.border_color(colors.border)
            }
        })
        .bg(colors.muted.opacity(0.3))
        .when(disabled, |card| card.opacity(0.5))
        .when(!disabled, |card| {
            let hover = colors.muted_foreground.opacity(0.5);
            card.cursor_pointer()
                .when(!checked, |card| {
                    card.hover(move |style| style.border_color(hover))
                })
                .on_click(cx.listener(move |dialog, _, _, cx| dialog.toggle(key.clone(), cx)))
        })
        .child(
            div()
                .flex()
                .w_full()
                .h(px(112.0))
                .items_center()
                .justify_center()
                .bg(gpui::black().opacity(0.05))
                .child(shot),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .px(px(12.0))
                .py(px(8.0))
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(SharedString::from(target.name.clone())),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .child(title),
                ),
        )
        .when(checked, |card| {
            card.child(
                div()
                    .absolute()
                    .top(px(8.0))
                    .right(px(8.0))
                    .rounded_full()
                    .bg(colors.primary)
                    .p(px(4.0))
                    .child(
                        Icon::new("check")
                            .size(12.0)
                            .color(colors.primary_foreground),
                    ),
            )
        })
        .into_any_element()
}

impl Render for AppshotsDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // `max-h-[min(90vh,850px)]`, the header and footer outside the scroll.
        let body_height = (f32::from(window.viewport_size().height) * 0.9).min(850.0) - 200.0;
        let colors = cx.theme().colors.clone();
        let muted = colors.muted_foreground;
        let body: AnyElement = match &self.stage {
            Stage::Loading => div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .py(px(32.0))
                .text_color(muted)
                .child(spinner("appshots-loading", 16.0))
                .child(cx.t("appshots.loading"))
                .into_any_element(),
            Stage::Unsupported => div()
                .py(px(24.0))
                .text_size(px(14.0))
                .text_color(muted)
                .child(cx.t("appshots.errors.unsupported"))
                .into_any_element(),
            Stage::Failed(message) => div()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(12.0))
                .py(px(16.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(colors.destructive)
                        .child(cx.t(message)),
                )
                .child(
                    Button::new("appshots-retry")
                        .variant(ButtonVariant::Outline)
                        .on_click(cx.listener(|dialog, _, _, cx| dialog.refresh(cx)))
                        .child(cx.t("appshots.refresh")),
                )
                .into_any_element(),
            Stage::Permission => div()
                .debug_selector(|| "appshots-permission".into())
                .flex()
                .flex_col()
                .items_start()
                .gap(px(12.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(colors.border)
                .p(px(20.0))
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t("appshots.permissionTitle")),
                )
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(muted)
                        .child(cx.t("appshots.permissionHint")),
                )
                .child(
                    Button::new("appshots-grant")
                        .on_click(cx.listener(|dialog, _, _, cx| dialog.request_access(cx)))
                        .child(cx.t("appshots.grantPermission")),
                )
                .into_any_element(),
            Stage::Ready(targets) => {
                let pages = targets.len().div_ceil(MAX_IMAGES);
                let page = self.page;
                let cards: Vec<AnyElement> = self
                    .visible()
                    .iter()
                    .enumerate()
                    .map(|(index, target)| card(self, page * MAX_IMAGES + index, target, cx))
                    .collect();
                let mut rows = Vec::new();
                let mut cards = cards.into_iter();
                while let Some(first) = cards.next() {
                    rows.push(
                        div()
                            .flex()
                            .gap(px(12.0))
                            .child(div().flex_1().min_w_0().child(first))
                            .child(div().flex_1().min_w_0().children(cards.next()))
                            .child(div().flex_1().min_w_0().children(cards.next())),
                    );
                }
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .text_size(px(12.0))
                            .text_color(muted)
                            .child(
                                cx.t_with(
                                    "appshots.selectionCount",
                                    &Args::default()
                                        .with("count", self.selected.len().to_string())
                                        .with("max", MAX_IMAGES.to_string()),
                                ),
                            )
                            .child(
                                Button::new("appshots-refresh")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::Sm)
                                    .disabled(self.busy)
                                    .on_click(cx.listener(|dialog, _, _, cx| dialog.refresh(cx)))
                                    .child(Icon::new("refresh-cw").size(14.0))
                                    .child(cx.t("appshots.refresh")),
                            ),
                    )
                    .when(targets.is_empty(), |list| {
                        list.child(
                            div()
                                .py(px(32.0))
                                .text_center()
                                .text_size(px(14.0))
                                .text_color(muted)
                                .child(cx.t("appshots.empty")),
                        )
                    })
                    .children(rows)
                    .when(pages > 1, |list| {
                        list.child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .gap(px(12.0))
                                .child(
                                    Button::new("appshots-previous")
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::IconSm)
                                        .disabled(self.busy || page == 0)
                                        .on_click(cx.listener(move |dialog, _, _, cx| {
                                            dialog.turn_page(page.saturating_sub(1), cx)
                                        }))
                                        .child(Icon::new("chevron-left").size(16.0)),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(muted)
                                        .child(format!("{} / {pages}", page + 1)),
                                )
                                .child(
                                    Button::new("appshots-next")
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::IconSm)
                                        .disabled(self.busy || page + 1 >= pages)
                                        .on_click(cx.listener(move |dialog, _, _, cx| {
                                            dialog.turn_page(page + 1, cx)
                                        }))
                                        .child(Icon::new("chevron-right").size(16.0)),
                                ),
                        )
                    })
                    .into_any_element()
            }
        };
        let can_attach = !self.busy && self.chosen().is_some();
        let count = self.selected.len().to_string();
        let this = cx.entity().downgrade();
        Dialog::new("appshots", move |_, cx| {
            let _ = this.update(cx, |dialog, cx| dialog.close(cx));
        })
        .max_width(672.0)
        .close_button(!self.busy)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .pr(px(28.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(18.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(Icon::new("camera").size(16.0))
                        .child(cx.t("appshots.title")),
                )
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(muted)
                        .child(cx.t("appshots.description")),
                ),
        )
        .child(
            div()
                .id("appshots-body")
                .debug_selector(|| "appshots".into())
                .flex()
                .flex_col()
                .gap(px(16.0))
                .max_h(px(body_height.max(160.0)))
                .overflow_y_scroll()
                .children(self.error.as_ref().map(|message| {
                    div()
                        .text_size(px(14.0))
                        .text_color(colors.destructive)
                        .child(cx.t(message))
                }))
                .child(body),
        )
        .child(dialog_footer(
            [
                Button::new("appshots-cancel")
                    .variant(ButtonVariant::Outline)
                    .disabled(self.busy)
                    .on_click(cx.listener(|dialog, _, _, cx| dialog.close(cx)))
                    .child(cx.t("appshots.cancel"))
                    .into_any_element(),
                Button::new("appshots-attach")
                    .disabled(!can_attach)
                    .on_click(cx.listener(|dialog, _, _, cx| dialog.attach(cx)))
                    .when(self.busy, |button| {
                        button.child(spinner("appshots-attaching", 16.0))
                    })
                    .child(cx.t_with("appshots.attach", &Args::default().with("count", count)))
                    .into_any_element(),
            ],
            cx,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use gpui::{Modifiers, TestAppContext, VisualTestContext, WindowAppearance};

    use super::*;
    use crate::theme::{Theme, ThemePreference};

    fn window(pid: i32, name: &str) -> ComputerTarget {
        ComputerTarget {
            bundle_id: format!("app.{name}"),
            name: name.into(),
            pid,
            window_id: 1,
            title: format!("{name} — janela"),
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        }
    }

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbImage::new(8, 6)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    fn open(
        cx: &mut TestAppContext,
        source: Source,
    ) -> (gpui::Entity<AppshotsDialog>, &mut VisualTestContext) {
        cx.update(|cx| {
            cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
            crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        });
        let (dialog, cx) =
            cx.add_window_view(move |window, cx| AppshotsDialog::new(source.clone(), window, cx));
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        (dialog, cx)
    }

    #[gpui::test]
    fn chosen_windows_are_saved_and_attached_in_order(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let source = Source {
            availability: || Availability::Ready,
            targets: || {
                Ok(vec![
                    window(10, "Figma"),
                    window(20, "Safari"),
                    window(30, "Notes"),
                ])
            },
            capture: |target| {
                if target.pid == 30 {
                    Err("captureFailed".into())
                } else {
                    Ok(png())
                }
            },
            folder: dir.path().join("appshots"),
        };
        let (dialog, cx) = open(cx, source);
        let attached = std::rc::Rc::new(Mutex::new(None));
        let seen = attached.clone();
        cx.update(|_, cx| {
            cx.subscribe(&dialog, move |_, event: &AppshotsEvent, _| {
                if let AppshotsEvent::Attach(paths) = event {
                    *seen.lock().unwrap() = Some(paths.clone());
                }
            })
            .detach();
        });
        for card in ["appshot-window-1", "appshot-window-0"] {
            let bounds = cx.debug_bounds(card).expect(card);
            cx.simulate_click(bounds.center(), Modifiers::none());
            cx.run_until_parked();
        }
        // The window that could not be captured cannot be chosen.
        let notes = cx.debug_bounds("appshot-window-2").expect("notes");
        cx.simulate_click(notes.center(), Modifiers::none());
        cx.run_until_parked();
        dialog.read_with(cx, |dialog, _| {
            assert_eq!(dialog.selected, ["20:1", "10:1"]);
        });
        let attach = cx.debug_bounds("appshots-attach").expect("attach");
        cx.simulate_click(attach.center(), Modifiers::none());
        cx.run_until_parked();
        let paths = attached.lock().unwrap().clone().expect("attached");
        let names: Vec<String> = paths
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names[0].starts_with("Appshot-Safari-"), "{names:?}");
        assert!(names[1].starts_with("Appshot-Figma-"), "{names:?}");
        assert!(paths.iter().all(|path| path.exists()));
    }

    #[gpui::test]
    fn without_screen_recording_it_asks_for_it(cx: &mut TestAppContext) {
        let source = Source {
            availability: || Availability::Permission,
            targets: || Err("permission".into()),
            capture: |_| Err("permission".into()),
            folder: std::env::temp_dir(),
        };
        let (_, cx) = open(cx, source);
        assert!(cx.debug_bounds("appshots-permission").is_some());
        assert!(cx.debug_bounds("appshot-window-0").is_none());
    }
}
