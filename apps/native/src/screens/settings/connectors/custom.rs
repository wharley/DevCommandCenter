//! "Adicionar conector personalizado" (`AddCustomConnectorDialog`): the
//! server's address (its name follows until the person edits it), and under
//! "Mais opções" the scopes and a logo. "Continuar" goes to the connect
//! dialog; an address in the catalog connects as that preset.

use gpui::{
    AnyElement, AppContext, Context, Entity, EventEmitter, FontWeight, PathPromptOptions, Render,
    Subscription, Window, div, prelude::*, px,
};

use super::glyph::glyph;
use crate::data::connector_custom::{default_name, parse_url, read_logo, valid_scopes};
use crate::data::connectors::{Target, preset_for_url};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, Icon, dialog_header};

pub enum CustomEvent {
    Closed,
    Submitted(Target),
}

pub struct ConnectorCustom {
    pub(in super::super) url: Entity<TextInput>,
    pub(in super::super) name: Entity<TextInput>,
    pub(in super::super) scopes: Entity<TextInput>,
    name_edited: bool,
    show_error: bool,
    more_open: bool,
    pub(in super::super) logo: Option<String>,
    logo_error: bool,
    _inputs: Vec<Subscription>,
}

impl EventEmitter<CustomEvent> for ConnectorCustom {}

impl ConnectorCustom {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url =
            cx.new(|cx| TextInput::new(window, cx).with_placeholder("https://mcp.example.com/mcp"));
        let name = cx.new(|cx| TextInput::new(window, cx));
        let scopes =
            cx.new(|cx| TextInput::new(window, cx).with_placeholder("support:read reports:read"));
        window.focus(&gpui::Focusable::focus_handle(url.read(cx), cx), cx);
        let inputs = vec![
            cx.subscribe(&url, |this, input, event, cx| match event {
                TextInputEvent::Changed => {
                    this.show_error = false;
                    let text = input.read(cx).text().to_string();
                    if let (Some(parsed), false) = (parse_url(&text), this.name_edited) {
                        let default = default_name(&parsed);
                        this.name.update(cx, |name, cx| name.set_text(default, cx));
                    }
                    cx.notify();
                }
                TextInputEvent::Submitted => this.submit(cx),
                _ => {}
            }),
            cx.subscribe(&name, |this, _, event, cx| match event {
                TextInputEvent::Changed => {
                    this.name_edited = true;
                    cx.notify();
                }
                TextInputEvent::Submitted => this.submit(cx),
                _ => {}
            }),
            cx.subscribe(&scopes, |this, _, event, cx| match event {
                TextInputEvent::Changed => cx.notify(),
                TextInputEvent::Submitted => this.submit(cx),
                _ => {}
            }),
        ];
        Self {
            url,
            name,
            scopes,
            name_edited: false,
            show_error: false,
            more_open: false,
            logo: None,
            logo_error: false,
            _inputs: inputs,
        }
    }

    fn scopes_invalid(&self, cx: &gpui::App) -> bool {
        let scopes = self.scopes.read(cx).text();
        !scopes.trim().is_empty() && !valid_scopes(scopes)
    }

    pub(in super::super) fn submit(&mut self, cx: &mut Context<Self>) {
        let Some(url) = parse_url(self.url.read(cx).text()) else {
            self.show_error = true;
            cx.notify();
            return;
        };
        if self.scopes_invalid(cx) {
            self.more_open = true;
            cx.notify();
            return;
        }
        let preset = preset_for_url(&url);
        let typed = self.name.read(cx).text().trim().to_string();
        let scopes = self
            .scopes
            .read(cx)
            .text()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let target = Target {
            name: preset.map_or_else(
                || {
                    if typed.is_empty() {
                        default_name(&url)
                    } else {
                        typed
                    }
                },
                |preset| preset.name.to_string(),
            ),
            url: preset.map_or(url, |preset| preset.url.to_string()),
            preset,
            scopes: (!scopes.is_empty()).then_some(scopes),
            logo: if preset.is_some() {
                None
            } else {
                self.logo.clone()
            },
        };
        cx.emit(CustomEvent::Submitted(target));
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
                .spawn(async move { read_logo(&path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.logo_error = logo.is_none();
                if logo.is_some() {
                    this.logo = logo;
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn label(text: gpui::SharedString, cx: &gpui::App) -> AnyElement {
    div()
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().colors.foreground)
        .child(text)
        .into_any_element()
}

fn field(id: &'static str, input: &Entity<TextInput>, mono: bool, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .debug_selector(move || id.to_string())
        .flex()
        .items_center()
        .w_full()
        .h(px(32.0))
        .px(px(12.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(colors.input)
        .text_size(px(12.0))
        .when(mono, |field| {
            field.font_family(cx.theme().fonts.mono.clone())
        })
        .child(input.clone())
        .into_any_element()
}

fn note(text: gpui::SharedString, error: bool, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .text_size(px(11.0))
        .line_height(px(17.9))
        .text_color(if error {
            colors.destructive
        } else {
            colors.muted_foreground
        })
        .child(text)
        .into_any_element()
}

impl Render for ConnectorCustom {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let url_text = self.url.read(cx).text().to_string();
        let parsed = parse_url(&url_text);
        let shown_name = {
            let typed = self.name.read(cx).text().to_string();
            if typed.is_empty() {
                parsed.as_deref().map_or("?".into(), default_name)
            } else {
                typed
            }
        };
        let scopes_invalid = self.scopes_invalid(cx);
        let more = self.more_open.then(|| {
            div()
                .mt(px(12.0))
                .flex()
                .flex_col()
                .gap(px(12.0))
                .rounded(px(14.0))
                .border_1()
                .border_color(colors.border.opacity(0.6))
                .p(px(12.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(label(cx.t("settings.connectors.custom.scopesLabel"), cx))
                        .child(field("custom-connector-scopes", &self.scopes, true, cx))
                        .child(note(
                            cx.t(if scopes_invalid {
                                "settings.connectors.custom.invalidScopes"
                            } else {
                                "settings.connectors.custom.scopesHint"
                            }),
                            scopes_invalid,
                            cx,
                        )),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(label(cx.t("settings.connectors.custom.logoLabel"), cx))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(glyph(None, self.logo.as_deref(), &shown_name, 36.0, cx))
                                .child(
                                    Button::new("custom-connector-logo")
                                        .variant(ButtonVariant::Outline)
                                        .size(ButtonSize::Xs)
                                        .text_size(12.0)
                                        .on_click(cx.listener(|this, _, _, cx| this.pick_logo(cx)))
                                        .child(cx.t("settings.connectors.custom.chooseLogo")),
                                )
                                .when(self.logo.is_some(), |row| {
                                    row.child(
                                        Button::new("custom-connector-logo-remove")
                                            .variant(ButtonVariant::Ghost)
                                            .size(ButtonSize::Xs)
                                            .text_size(12.0)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.logo = None;
                                                cx.notify();
                                            }))
                                            .child(cx.t("settings.connectors.custom.removeLogo")),
                                    )
                                }),
                        )
                        .child(note(
                            cx.t(if self.logo_error {
                                "settings.connectors.custom.invalidLogo"
                            } else {
                                "settings.connectors.custom.logoHint"
                            }),
                            self.logo_error,
                            cx,
                        )),
                )
        });
        let close = cx.entity().downgrade();
        let open = self.more_open;
        Dialog::new("connector-custom", move |_, cx| {
            let _ = close.update(cx, |_, cx| cx.emit(CustomEvent::Closed));
        })
        .max_width(440.0)
        .child(
            div()
                .debug_selector(|| "connector-custom".into())
                .w_full()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(dialog_header(
                    cx.t("settings.connectors.custom.title"),
                    Some(cx.t("settings.connectors.custom.hint")),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(label(cx.t("settings.connectors.custom.urlLabel"), cx))
                        .child(field("custom-connector-url", &self.url, true, cx))
                        .when(self.show_error, |column| {
                            column.child(note(
                                cx.t("settings.connectors.custom.invalidUrl"),
                                true,
                                cx,
                            ))
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(label(cx.t("settings.connectors.custom.nameLabel"), cx))
                        .child(field("custom-connector-name", &self.name, false, cx)),
                )
                .child(
                    div()
                        .child(
                            div()
                                .id("custom-connector-more")
                                .debug_selector(|| "custom-connector-more".into())
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .cursor_pointer()
                                .text_color(colors.muted_foreground)
                                .hover(|row| row.text_color(colors.foreground))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.more_open = !this.more_open;
                                    cx.notify();
                                }))
                                .child(
                                    Icon::new(if open {
                                        "chevron-down"
                                    } else {
                                        "chevron-right"
                                    })
                                    .size(14.0),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .child(cx.t("settings.connectors.custom.moreOptions")),
                                ),
                        )
                        .children(more),
                )
                .child(
                    div().flex().justify_end().child(
                        Button::new("custom-connector-continue")
                            .size(ButtonSize::Sm)
                            .text_size(12.0)
                            .disabled(url_text.trim().is_empty())
                            .on_click(cx.listener(|this, _, _, cx| this.submit(cx)))
                            .child(cx.t("settings.connectors.custom.continue")),
                    ),
                ),
        )
    }
}
