//! "Atalhos": the quick composer's and Appshots' global keys (the desktop
//! registers them; click to record a new one, Esc cancels, "Desativar"
//! clears it), opening the quick composer, and the keyboard shortcuts card
//! with the cheatsheet.

use gpui::{
    AnyElement, Context, FocusHandle, FontWeight, Keystroke, SharedString, Subscription, Window,
    div, prelude::*, px,
};

use super::{Settings, SettingsEvent};
use crate::data::shortcuts::{Global, ShortcutStatus, display, error_key, from_keystroke};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Toaster, command_key};

pub(super) struct Shortcuts {
    pub(super) quick: Option<ShortcutStatus>,
    appshots: Option<ShortcutStatus>,
    /// The card listening for a combination.
    pub(super) recording: Option<Global>,
    busy: bool,
    focus: FocusHandle,
    /// While recording, keys come here before the app's own bindings (so
    /// ⌘K is recorded, not the palette opened).
    interceptor: Option<Subscription>,
}

impl Shortcuts {
    pub(super) fn new(cx: &mut gpui::App) -> Self {
        Self {
            quick: None,
            appshots: None,
            recording: None,
            busy: false,
            focus: cx.focus_handle(),
            interceptor: None,
        }
    }
}

impl Settings {
    pub(super) fn load_shortcuts(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    (
                        api.global_shortcut(Global::QuickComposer),
                        api.global_shortcut(Global::Appshots),
                    )
                })
                .await;
            let _ = this.update(cx, |settings, cx| {
                (settings.shortcuts.quick, settings.shortcuts.appshots) = read;
                cx.notify();
            });
        })
        .detach();
    }

    fn record(&mut self, which: Global, window: &mut Window, cx: &mut Context<Self>) {
        self.shortcuts.recording = Some(which);
        window.focus(&self.shortcuts.focus, cx);
        let this = cx.weak_entity();
        let handle = window.window_handle();
        self.shortcuts.interceptor = Some(cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() == handle {
                let _ = this.update(cx, |settings, cx| {
                    settings.recorder_key(&event.keystroke, cx)
                });
            }
        }));
        cx.notify();
    }

    fn stop_recording(&mut self, cx: &mut Context<Self>) {
        self.shortcuts.interceptor = None;
        if self.shortcuts.recording.take().is_some() {
            cx.notify();
        }
    }

    /// `save`: the key (or none) goes to the desktop, then is read back.
    fn save_shortcut(&mut self, which: Global, shortcut: Option<String>, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        self.shortcuts.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api.set_global_shortcut(which, shortcut.as_deref())?;
                    Ok::<_, crate::data::desktop_api::ApiError>(api.global_shortcut(which))
                })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.shortcuts.busy = false;
                match result {
                    Ok(status) => {
                        match which {
                            Global::QuickComposer => settings.shortcuts.quick = status,
                            Global::Appshots => settings.shortcuts.appshots = status,
                        }
                        settings.stop_recording(cx);
                    }
                    Err(error) => {
                        let key = error_key(&error.to_string());
                        Toaster::error(Translate::t(&**cx, &key), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn recorder_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        let Some(which) = self.shortcuts.recording else {
            return;
        };
        cx.stop_propagation();
        if keystroke.key == "escape" {
            self.stop_recording(cx);
            return;
        }
        if self.shortcuts.busy {
            return;
        }
        if let Some(shortcut) = from_keystroke(keystroke) {
            self.save_shortcut(which, Some(shortcut), cx);
        }
    }

    fn shortcut_card(
        &self,
        which: Global,
        status: &ShortcutStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let (title, hint, id) = match which {
            Global::QuickComposer => (
                "quickComposer.title",
                "quickComposer.shortcutHint",
                "quick-composer",
            ),
            Global::Appshots => (
                "appshots.shortcutTitle",
                "appshots.shortcutHint",
                "appshots",
            ),
        };
        let recording = self.shortcuts.recording == Some(which);
        let label: SharedString = if recording {
            cx.t("appshots.pressShortcut")
        } else {
            status
                .shortcut
                .as_deref()
                .map(|shortcut| display(shortcut).into())
                .unwrap_or_else(|| cx.t("appshots.shortcutDisabled"))
        };
        let recorder = Button::new(SharedString::from(format!("settings-{id}-shortcut")))
            .variant(ButtonVariant::Outline)
            .size(ButtonSize::Sm)
            .disabled(self.shortcuts.busy)
            .on_click(
                cx.listener(move |settings, _, window, cx| settings.record(which, window, cx)),
            )
            .child(label);
        let disable = status.shortcut.is_some().then(|| {
            Button::new(SharedString::from(format!("settings-{id}-disable")))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .disabled(self.shortcuts.busy)
                .on_click(
                    cx.listener(move |settings, _, _, cx| settings.save_shortcut(which, None, cx)),
                )
                .child(cx.t("appshots.disableShortcut"))
        });
        let open = (which == Global::QuickComposer).then(|| {
            Button::new("settings-quick-composer-open")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .on_click(cx.listener(|settings, _, _, cx| {
                    let Some(api) = settings.general.api() else {
                        return;
                    };
                    cx.spawn(async move |_, cx| {
                        let opened = cx
                            .background_executor()
                            .spawn(async move { api.toggle_quick_composer() })
                            .await;
                        if let Err(error) = opened {
                            cx.update(|cx| {
                                let key = error_key(&error.to_string());
                                Toaster::error(Translate::t(cx, &key), cx)
                            });
                        }
                    })
                    .detach();
                }))
                .child(cx.t("quickComposer.open"))
        });
        div()
            .debug_selector(move || format!("settings-{id}"))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(14.0))
            .border_1()
            .border_color(colors.border)
            .p(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(cx.t(title)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(recorder)
                            .children(disable)
                            .children(open),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t(hint)),
            )
            .when(status.shortcut_error, |card| {
                card.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(colors.destructive)
                        .child(cx.t("appshots.errors.shortcutUnavailable")),
                )
            })
            .into_any_element()
    }

    pub(super) fn shortcuts_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let command = command_key();
        let quick = self
            .shortcuts
            .quick
            .clone()
            .filter(|status| status.supported)
            .map(|status| self.shortcut_card(Global::QuickComposer, &status, cx));
        let appshots = self
            .shortcuts
            .appshots
            .clone()
            .filter(|status| status.supported)
            .map(|status| self.shortcut_card(Global::Appshots, &status, cx));
        let badges = [
            format!("{command}+K"),
            format!("{command}+Shift+L"),
            format!("{command}+J"),
            format!("{command}+O"),
            "Esc".to_string(),
        ]
        .into_iter()
        .map(|keys| {
            Badge::new(keys, BadgeVariant::Secondary)
                .height(32.0)
                .padding_x(12.0)
                .weight(FontWeight::NORMAL)
        });
        div()
            .id("settings-shortcuts")
            .track_focus(&self.shortcuts.focus)
            .on_mouse_down_out(cx.listener(|settings, _, _, cx| settings.stop_recording(cx)))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .children(quick)
            .children(appshots)
            .child(
                div()
                    .rounded(px(14.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.6))
                    .p(px(16.0))
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .justify_between()
                            .gap(px(16.0))
                            .child(
                                div()
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(colors.foreground)
                                            .child(cx.t("settings.shortcuts.keyboardShortcuts")),
                                    )
                                    .child(
                                        div()
                                            .mt(px(4.0))
                                            .text_size(px(12.0))
                                            .text_color(colors.muted_foreground)
                                            .child(
                                                cx.t("settings.shortcuts.keyboardShortcutsHint"),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_end()
                                    .gap(px(8.0))
                                    .child(
                                        div().text_color(colors.muted_foreground).child(
                                            // `activeCount: 11`, fixed on the desktop too.
                                            Badge::new(
                                                cx.t_with(
                                                    "settings.shortcuts.activeCount",
                                                    &Args::count(11),
                                                ),
                                                BadgeVariant::Outline,
                                            )
                                            .height(32.0)
                                            .padding_x(12.0)
                                            .weight(FontWeight::NORMAL),
                                        ),
                                    )
                                    .child(
                                        Button::new("settings-open-cheatsheet")
                                            .variant(ButtonVariant::Outline)
                                            .size(ButtonSize::Sm)
                                            .on_click(cx.listener(|_, _, _, cx| {
                                                cx.emit(SettingsEvent::OpenShortcuts)
                                            }))
                                            .child(cx.t("settings.shortcuts.openCheatsheet")),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(12.0))
                            .text_size(px(12.0))
                            .line_height(px(19.5))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("settings.shortcuts.comingSoonBody")),
                    )
                    .child(
                        div()
                            .mt(px(16.0))
                            .flex()
                            .flex_wrap()
                            .gap(px(8.0))
                            .children(badges),
                    ),
            )
            .into_any_element()
    }
}
