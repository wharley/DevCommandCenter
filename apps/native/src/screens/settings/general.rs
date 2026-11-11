//! "Geral": the interface's diagnostics to copy, the installed version,
//! keeping the Mac awake (the desktop's hold), going on alone when the plan's
//! limit runs out, the completed tasks' space, the Browser's data, and three
//! notes on how the shell opens.

use gpui::{AnyElement, Context, FontWeight, SharedString, Task, div, prelude::*, px};

use super::Settings;
use crate::data::desktop_api::DesktopApi;
use crate::data::keep_awake::{Mode, Status, reason_key};
use crate::i18n::{Args, I18n, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Dialog, Icon, Switch, ToggleGroup,
    Tracked, dialog_footer, dialog_header, spinner,
};

/// How often the hold is read again while "Geral" shows (the desktop
/// listens for `keep-awake-changed`).
const KEEP_AWAKE_POLL: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Default)]
pub(super) struct General {
    api: Option<DesktopApi>,
    /// Where `usage-limit-settings.json` lives (the database's folder).
    data_dir: Option<std::path::PathBuf>,
    copied: bool,
    keep_awake: Option<Status>,
    keep_awake_saving: bool,
    keep_awake_error: Option<String>,
    auto_resume: bool,
    usage_error: Option<String>,
    browser: crate::screens::browser::data_store::Usage,
    /// Which clearing runs: the caches (`false`) or everything.
    clearing: Option<bool>,
    cleared: bool,
    browser_error: Option<String>,
    confirm_everything: bool,
    _poll: Option<Task<()>>,
}

/// `rounded-xl border border-border/60 bg-muted/15 p-4`
pub(super) fn card(cx: &Context<Settings>) -> gpui::Div {
    let colors = &cx.theme().colors;
    div()
        .rounded(px(14.0))
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.muted.opacity(0.15))
        .p(px(16.0))
}

/// The title (14px medium) and its line.
pub(super) fn heading(
    title: SharedString,
    body: SharedString,
    cx: &Context<Settings>,
) -> gpui::Div {
    let colors = &cx.theme().colors;
    div()
        .min_w_0()
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.foreground)
                .child(title),
        )
        .child(
            div()
                .mt(px(4.0))
                .text_size(px(12.0))
                .line_height(px(19.5))
                .text_color(colors.muted_foreground)
                .child(body),
        )
}

fn note(text: SharedString, color: gpui::Hsla) -> gpui::Div {
    div().text_size(px(12.0)).text_color(color).child(text)
}

impl General {
    pub(super) fn new(api: Option<DesktopApi>) -> Self {
        Self {
            api,
            data_dir: crate::data::loader::database_path()
                .parent()
                .map(std::path::Path::to_path_buf),
            ..Default::default()
        }
    }

    pub(super) fn api(&self) -> Option<DesktopApi> {
        self.api.clone()
    }

    #[cfg(test)]
    pub(super) fn set_api(&mut self, api: DesktopApi, data_dir: std::path::PathBuf) {
        self.api = Some(api);
        self.data_dir = Some(data_dir);
    }
}

impl Settings {
    /// Reads what "Geral" shows, and keeps the hold fresh while it shows.
    pub(super) fn load_general(&mut self, cx: &mut Context<Self>) {
        self.general.auto_resume = self
            .general
            .data_dir
            .as_deref()
            .is_some_and(|dir| dcc_infra::usage_limit_settings::load_settings(dir).auto_resume);
        self.general.browser = crate::screens::browser::data_store::usage();
        let api = self.general.api.clone();
        self.general._poll = Some(cx.spawn(async move |this, cx| {
            loop {
                let reader = api.clone();
                let status = cx
                    .background_executor()
                    .spawn(async move { reader.and_then(|api| api.keep_awake_status()) })
                    .await;
                let alive = this
                    .update(cx, |settings, cx| {
                        if settings.general.keep_awake != status {
                            settings.general.keep_awake = status;
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !alive || cfg!(test) {
                    break;
                }
                cx.background_executor().timer(KEEP_AWAKE_POLL).await;
            }
        }));
    }

    pub(super) fn set_keep_awake(&mut self, mode: Mode, cx: &mut Context<Self>) {
        let Some(api) = self.general.api.clone() else {
            return;
        };
        if self.general.keep_awake_saving
            || self
                .general
                .keep_awake
                .as_ref()
                .is_some_and(|status| status.mode == mode)
        {
            return;
        }
        self.general.keep_awake_saving = true;
        self.general.keep_awake_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move { api.set_keep_awake(mode) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.general.keep_awake_saving = false;
                match saved {
                    Ok(status) => settings.general.keep_awake = Some(status),
                    Err(error) => settings.general.keep_awake_error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn set_auto_resume(&mut self, on: bool, cx: &mut Context<Self>) {
        self.general.usage_error = None;
        let saved = match self.general.data_dir.as_deref() {
            Some(dir) => dcc_infra::usage_limit_settings::save_settings(
                dir,
                dcc_core::domain::usage_limit::UsageLimitSettings { auto_resume: on },
            ),
            None => Err("no app data folder".into()),
        };
        match saved {
            Ok(()) => self.general.auto_resume = on,
            Err(error) => self.general.usage_error = Some(error),
        }
        cx.notify();
    }

    fn clear_browser(&mut self, everything: bool, cx: &mut Context<Self>) {
        if self.general.clearing.is_some() {
            return;
        }
        self.general.confirm_everything = false;
        self.general.clearing = Some(everything);
        self.general.cleared = false;
        self.general.browser_error = None;
        cx.notify();
        let entity = cx.entity().downgrade();
        let mut async_cx = cx.to_async();
        crate::screens::browser::data_store::clear(everything, move |result| {
            let _ = entity.update(&mut async_cx, |settings, cx| {
                settings.general.clearing = None;
                match result {
                    Ok(()) => {
                        settings.general.browser = crate::screens::browser::data_store::usage();
                        settings.general.cleared = true;
                    }
                    Err(error) => settings.general.browser_error = Some(error),
                }
                cx.notify();
            });
        });
    }

    fn diagnostics_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        card(cx)
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(cx.t("frontendRecovery.settingsTitle")),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("frontendRecovery.settingsBody")),
            )
            .child(
                div().flex().child(
                    div()
                        .id("settings-copy-diagnostics")
                        .debug_selector(|| "settings-copy-diagnostics".into())
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(colors.border)
                        .px(px(12.0))
                        .py(px(8.0))
                        .text_size(px(14.0))
                        .cursor_pointer()
                        .on_click(cx.listener(|settings, _, _, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                crate::data::diagnostics::export(),
                            ));
                            settings.general.copied = true;
                            cx.notify();
                        }))
                        .child(cx.t("frontendRecovery.copy")),
                ),
            )
            .when(self.general.copied, |card| {
                card.child(
                    div()
                        .text_size(px(14.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("frontendRecovery.copied")),
                )
            })
            .into_any_element()
    }

    fn keep_awake_card(&self, status: &Status, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let entity = cx.entity().downgrade();
        let selected = Mode::ALL.iter().position(|mode| *mode == status.mode);
        let modes = ToggleGroup::new(
            "settings-keep-awake",
            Mode::ALL
                .iter()
                .map(|mode| cx.t(&format!("settings.general.keepAwake.modes.{}", mode.key())))
                .collect(),
            selected,
        )
        .accent()
        .on_select(move |index, _, cx| {
            let _ = entity.update(cx, |settings, cx| {
                settings.set_keep_awake(Mode::ALL[index], cx)
            });
        });
        let now = (status.mode != Mode::Off).then(|| {
            let line = match status.active.as_ref() {
                Some(hold) => {
                    let (key, count, time) = reason_key(&hold.reason);
                    let mut args = Args::default();
                    if let Some(count) = count {
                        args = args.with_count(count);
                    }
                    if let Some(time) = time {
                        args = args.with("time", time);
                    }
                    let reason = cx.global::<I18n>().t_with(key, &args).to_string();
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(Icon::new("coffee").size(14.0).color(colors.foreground))
                        .child(cx.t_with(
                            "settings.general.keepAwake.activeNow",
                            &Args::default().with("reason", reason),
                        ))
                }
                None => div().child(cx.t("settings.general.keepAwake.idle")),
            };
            line.mt(px(4.0))
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
        });
        card(cx)
            .debug_selector(|| "settings-keep-awake".into())
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(24.0))
                    .child(heading(
                        cx.t("settings.general.keepAwake.title"),
                        cx.t("settings.general.keepAwake.body"),
                        cx,
                    ))
                    .child(div().flex_none().child(modes)),
            )
            .child(div().mt(px(12.0)).child(note(
                cx.t(&format!(
                    "settings.general.keepAwake.modeHints.{}",
                    status.mode.key()
                )),
                colors.foreground,
            )))
            .children(now)
            .child(
                div()
                    .mt(px(12.0))
                    .text_size(px(12.0))
                    .line_height(px(19.5))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("settings.general.keepAwake.lid")),
            )
            .children(self.general.keep_awake_error.clone().map(|error| {
                div().mt(px(8.0)).child(note(
                    cx.t_with(
                        "settings.general.keepAwake.saveFailed",
                        &Args::default().with("message", error),
                    ),
                    colors.destructive,
                ))
            }))
            .into_any_element()
    }

    fn usage_limit_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let entity = cx.entity().downgrade();
        // `KeepAwakeForLimitWarning`: the Mac may sleep before the reset.
        let warning = (self.general.auto_resume
            && self
                .general
                .keep_awake
                .as_ref()
                .is_some_and(|status| status.supported && status.mode == Mode::Off))
        .then(|| {
            div()
                .debug_selector(|| "settings-limit-keep-awake".into())
                .mt(px(12.0))
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(tw::AMBER_500.hsla().opacity(0.3))
                .bg(tw::AMBER_500.hsla().opacity(0.1))
                .px(px(12.0))
                .py(px(8.0))
                .text_size(px(12.0))
                .text_color(if cx.theme().appearance == crate::theme::Appearance::Dark {
                    tw::AMBER_200.hsla()
                } else {
                    tw::AMBER_800.hsla()
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(cx.t("settings.general.usageLimit.keepAwakeWarning")),
                )
                .child(
                    Button::new("settings-enable-keep-awake")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .text_size(12.0)
                        .disabled(self.general.keep_awake_saving)
                        .on_click(cx.listener(|settings, _, _, cx| {
                            settings.set_keep_awake(Mode::DuringAgents, cx)
                        }))
                        .child(cx.t("settings.general.usageLimit.enableKeepAwake")),
                )
        });
        card(cx)
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(24.0))
                    .child(heading(
                        cx.t("settings.general.usageLimit.title"),
                        cx.t("settings.general.usageLimit.body"),
                        cx,
                    ))
                    .child(
                        Switch::new("settings-auto-resume", self.general.auto_resume).on_toggle(
                            move |on, _, cx| {
                                let _ = entity
                                    .update(cx, |settings, cx| settings.set_auto_resume(on, cx));
                            },
                        ),
                    ),
            )
            .child(
                div()
                    .mt(px(12.0))
                    .flex()
                    .text_size(px(12.0))
                    .child(cx.t("settings.general.usageLimit.toggle"))
                    .child(
                        div()
                            .text_color(colors.muted_foreground)
                            .child(format!(" · {}", cx.t("settings.general.usageLimit.hint"))),
                    ),
            )
            .children(warning)
            .children(self.general.usage_error.clone().map(|error| {
                div().mt(px(8.0)).child(note(
                    cx.t_with(
                        "settings.general.usageLimit.saveFailed",
                        &Args::default().with("message", error),
                    ),
                    colors.destructive,
                ))
            }))
            .into_any_element()
    }

    fn browser_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let locale = cx.global::<I18n>().locale();
        let usage = self.general.browser;
        let format = |bytes: u64| crate::data::disk::format_disk_bytes(bytes, locale);
        let button = |id: &'static str,
                      everything: bool,
                      label: SharedString,
                      cx: &mut Context<Self>| {
            Button::new(id)
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Sm)
                .disabled(self.general.clearing.is_some())
                .on_click(cx.listener(move |settings, _, _, cx| {
                    if everything {
                        settings.general.confirm_everything = true;
                        cx.notify();
                    } else {
                        settings.clear_browser(false, cx);
                    }
                }))
                .children((self.general.clearing == Some(everything)).then(|| spinner(id, 14.0)))
                .child(label)
        };
        card(cx)
            .debug_selector(|| "settings-browser-data".into())
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(24.0))
                    .child(
                        heading(
                            cx.t("settings.general.browserData.title"),
                            cx.t("settings.general.browserData.body"),
                            cx,
                        )
                        .child(
                            div()
                                .mt(px(12.0))
                                .font_family(fonts::MONO)
                                .text_size(px(12.0))
                                .child(
                                    cx.t_with(
                                        "settings.general.browserData.usage",
                                        &Args::default()
                                            .with("cache", format(usage.cache_bytes))
                                            .with("total", format(usage.total_bytes)),
                                    ),
                                ),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .flex_wrap()
                            .gap(px(8.0))
                            .child(button(
                                "settings-clear-cache",
                                false,
                                cx.t("settings.general.browserData.clearCache"),
                                cx,
                            ))
                            .child(button(
                                "settings-clear-everything",
                                true,
                                cx.t("settings.general.browserData.clearEverything"),
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .mt(px(12.0))
                    .text_size(px(12.0))
                    .line_height(px(19.5))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("settings.general.browserData.clearEverythingHint")),
            )
            .when(self.general.cleared, |card| {
                card.child(div().mt(px(4.0)).child(note(
                    cx.t_with(
                        "settings.general.browserData.cleared",
                        &Args::default().with("cache", format(usage.cache_bytes)),
                    ),
                    colors.foreground,
                )))
            })
            .children(self.general.browser_error.clone().map(|error| {
                div()
                    .mt(px(4.0))
                    .child(note(error.into(), colors.destructive))
            }))
            .into_any_element()
    }

    /// `window.confirm(confirmEverything)`
    pub(super) fn confirm_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.general.confirm_everything {
            return None;
        }
        let close = cx.entity().downgrade();
        Some(
            Dialog::new("settings-confirm-everything", move |_, cx| {
                let _ = close.update(cx, |settings, cx| {
                    settings.general.confirm_everything = false;
                    cx.notify();
                });
            })
            .close_button(false)
            .child(dialog_header(
                cx.t("settings.general.browserData.confirmEverything"),
                None,
                cx,
            ))
            .child(dialog_footer(
                [
                    Button::new("settings-confirm-cancel")
                        .variant(ButtonVariant::Outline)
                        .on_click(cx.listener(|settings, _, _, cx| {
                            settings.general.confirm_everything = false;
                            cx.notify();
                        }))
                        .child(cx.t("sidebar.cancel"))
                        .into_any_element(),
                    Button::new("settings-confirm-ok")
                        .variant(ButtonVariant::Destructive)
                        .on_click(
                            cx.listener(|settings, _, _, cx| settings.clear_browser(true, cx)),
                        )
                        .child(cx.t("settings.general.browserData.clearEverything"))
                        .into_any_element(),
                ],
                cx,
            ))
            .into_any_element(),
        )
    }

    fn note_card(&self, title: &str, body: &str, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        div()
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .rounded(px(14.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .p(px(16.0))
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.muted_foreground)
                    .child(Tracked::new(cx.t(title).to_uppercase(), 11.0, 0.08)),
            )
            .child(
                div()
                    .mt(px(8.0))
                    .text_size(px(13.0))
                    .text_color(colors.foreground)
                    .child(cx.t(body)),
            )
            .into_any_element()
    }

    pub(super) fn general(&self, cx: &mut Context<Self>) -> AnyElement {
        let keep_awake = self
            .general
            .keep_awake
            .clone()
            .filter(|status| status.supported)
            .map(|status| self.keep_awake_card(&status, cx));
        let browser = self
            .general
            .browser
            .supported
            .then(|| self.browser_card(cx));
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.diagnostics_card(cx))
            .child(self.version_card(cx))
            .children(keep_awake)
            .child(self.usage_limit_card(cx))
            .child(self.cleanup_card(cx))
            .child(self.room_notices_card(cx))
            .children(browser)
            .child(
                card(cx)
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(24.0))
                    .child(heading(
                        cx.t("settings.general.shellBehavior"),
                        cx.t("settings.general.shellBehaviorBody"),
                        cx,
                    ))
                    .child(
                        Badge::new(cx.t("settings.general.shellReady"), BadgeVariant::Outline)
                            .height(32.0)
                            .padding_x(12.0)
                            .weight(FontWeight::NORMAL),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(12.0))
                    .child(self.note_card(
                        "settings.general.defaultEntry",
                        "settings.general.defaultEntryBody",
                        cx,
                    ))
                    .child(self.note_card(
                        "settings.general.commandPalette",
                        "settings.general.commandPaletteBody",
                        cx,
                    )),
            )
            .into_any_element()
    }
}
