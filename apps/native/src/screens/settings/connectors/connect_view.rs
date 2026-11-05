//! The connect dialog (440px): the connector's mark over its brand's glow,
//! then what it gets (control, risk, data), the token or Slack steps, the
//! error, and "Continuar no …"; while signing in, "Aguardando …"; when the
//! server registers no clients, the person's own app or a token.

use gpui::{
    AnyElement, Context, Entity, FontWeight, Render, SharedString, Window, div, linear_color_stop,
    linear_gradient, prelude::*, px,
};

use super::connect::{ConnectorConnect, Phase};
use super::glyph::glyph;
use crate::data::connector_marks::brand_color;
use crate::data::connectors::{Auth, USER_APP_REDIRECT_URI, slack_create_app_url, slack_manifest};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::color::hex;
use crate::ui::text_input::TextInput;
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, Icon, spinner};

fn field(id: &'static str, input: &Entity<TextInput>, cx: &gpui::App) -> AnyElement {
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
        .font_family(cx.theme().fonts.mono.clone())
        .text_size(px(12.0))
        .child(input.clone())
        .into_any_element()
}

fn point(icon: &'static str, title: SharedString, body: AnyElement, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .gap(px(12.0))
        .child(
            div()
                .mt(px(2.0))
                .flex_none()
                .child(Icon::new(icon).size(16.0).color(colors.muted_foreground)),
        )
        .child(
            // `min-w-0 flex-1`: the text wraps inside the dialog.
            div()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .child(title),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(12.0))
                        .line_height(px(19.5))
                        .text_color(colors.muted_foreground)
                        .child(body),
                ),
        )
        .into_any_element()
}

fn link(
    id: &'static str,
    text: SharedString,
    url: String,
    size: f32,
    cx: &mut Context<ConnectorConnect>,
) -> AnyElement {
    div()
        .id(id)
        .text_size(px(size))
        .text_color(cx.theme().colors.foreground)
        .underline()
        .cursor_pointer()
        .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&url)))
        .child(text)
        .into_any_element()
}

fn step(number: u8, title: SharedString, body: Vec<AnyElement>, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .gap(px(10.0))
        .child(
            div()
                .mt(px(2.0))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(16.0))
                .rounded_full()
                .bg(colors.muted)
                .text_size(px(10.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.muted_foreground)
                .child(number.to_string()),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .child(title),
                )
                .children(body),
        )
        .into_any_element()
}

fn small(text: SharedString, cx: &gpui::App) -> AnyElement {
    div()
        .text_size(px(11.0))
        .line_height(px(17.9))
        .text_color(cx.theme().colors.muted_foreground)
        .child(text)
        .into_any_element()
}

impl ConnectorConnect {
    fn copy_button(
        &self,
        id: &'static str,
        what: &'static str,
        label: SharedString,
        text: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let copied = self.copied == Some(what);
        Button::new(id)
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
            .text_size(12.0)
            .on_click(cx.listener(move |this, _, _, cx| this.copy(what, text.clone(), cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(Icon::new(if copied { "check" } else { "copy" }).size(14.0))
                    .child(if copied {
                        cx.t("settings.connectors.connect.copied")
                    } else {
                        label
                    }),
            )
            .into_any_element()
    }

    fn slack_steps(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let manifest = serde_json::to_string_pretty(&slack_manifest()).unwrap_or_default();
        let create_url = slack_create_app_url();
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .rounded(px(14.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .p(px(12.0))
            .child(small(cx.t("settings.connectors.connect.slack.intro"), cx))
            .child(step(
                1,
                cx.t("settings.connectors.connect.slack.createTitle"),
                vec![
                    div()
                        .mt(px(6.0))
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            Button::new("connector-slack-create")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Sm)
                                .text_size(12.0)
                                .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&create_url)))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .child(
                                            cx.t("settings.connectors.connect.slack.createButton"),
                                        )
                                        .child(Icon::new("arrow-up-right").size(14.0)),
                                ),
                        )
                        .child(self.copy_button(
                            "connector-slack-manifest",
                            "manifest",
                            cx.t("settings.connectors.connect.slack.copyManifest"),
                            manifest,
                            cx,
                        ))
                        .into_any_element(),
                ],
                cx,
            ))
            .child(step(
                2,
                cx.t("settings.connectors.connect.slack.installTitle"),
                vec![small(
                    cx.t("settings.connectors.connect.slack.installBody"),
                    cx,
                )],
                cx,
            ))
            .child(step(
                3,
                cx.t("settings.connectors.connect.slack.clientIdTitle"),
                vec![
                    small(cx.t("settings.connectors.connect.slack.clientIdBody"), cx),
                    div()
                        .mt(px(6.0))
                        .child(field("connector-client-id", &self.client_id, cx))
                        .into_any_element(),
                ],
                cx,
            ))
            .into_any_element()
    }

    fn intro(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let name = Args::default().with("name", self.target.name.clone());
        let auth = self.target.auth();
        let learn = self.target.preset.map(|preset| {
            link(
                "connector-learn-more",
                cx.t("settings.connectors.connect.learnMore"),
                preset.docs_url.to_string(),
                12.0,
                cx,
            )
        });
        // The text may shrink in the wrapping row, so it wraps inside the
        // dialog instead of running past it.
        let data_body = div()
            .flex()
            .flex_wrap()
            .gap_x(px(4.0))
            .child(
                div()
                    .debug_selector(|| "connector-data-body".into())
                    .min_w_0()
                    .child(cx.t_with("settings.connectors.connect.dataBody", &name)),
            )
            .children(learn)
            .into_any_element();
        let token = (auth == Auth::Token).then(|| {
            let help = self
                .target
                .preset
                .and_then(|preset| preset.token_help_url)
                .map(|url| {
                    link(
                        "connector-create-token",
                        cx.t_with("settings.connectors.connect.createToken", &name),
                        url.to_string(),
                        11.0,
                        cx,
                    )
                });
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .rounded(px(14.0))
                .border_1()
                .border_color(colors.border.opacity(0.6))
                .p(px(12.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t("settings.connectors.connect.tokenLabel")),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_x(px(4.0))
                        .child(
                            div()
                                .debug_selector(|| "connector-token-hint".into())
                                .min_w_0()
                                .child(small(
                                    cx.t_with("settings.connectors.connect.tokenHint", &name),
                                    cx,
                                )),
                        )
                        .children(help),
                )
                .child(field("connector-token", &self.token, cx))
        });
        let failed = match &self.phase {
            Phase::Failed(message) => Some(
                div()
                    .rounded(px(10.0))
                    .bg(colors.destructive.opacity(0.1))
                    .px(px(12.0))
                    .py(px(8.0))
                    .text_size(px(12.0))
                    .text_color(colors.destructive)
                    .child(message.clone()),
            ),
            _ => None,
        };
        let disabled = self.primary_disabled(cx);
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .px(px(24.0))
            .pb(px(24.0))
            .child(point(
                "shield-check",
                cx.t("settings.connectors.connect.controlTitle"),
                cx.t_with("settings.connectors.connect.controlBody", &name)
                    .into_any_element(),
                cx,
            ))
            .child(point(
                "shield-alert",
                cx.t("settings.connectors.connect.riskTitle"),
                cx.t_with("settings.connectors.connect.riskBody", &name)
                    .into_any_element(),
                cx,
            ))
            .child(point(
                "database",
                cx.t("settings.connectors.connect.dataTitle"),
                data_body,
                cx,
            ))
            .children(token)
            .children((auth == Auth::SlackApp).then(|| self.slack_steps(cx)))
            .children(failed)
            .child(
                Button::new("connector-continue")
                    .stretch()
                    .height(36.0)
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, window, cx| this.primary(window, cx)))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(6.0))
                            .text_size(px(13.0))
                            .child(if auth == Auth::Token {
                                cx.t("settings.connectors.connect.connectWithToken")
                            } else {
                                cx.t_with("settings.connectors.connect.continue", &name)
                            })
                            .when(auth != Auth::Token, |row| {
                                row.child(Icon::new("arrow-up-right").size(16.0))
                            }),
                    ),
            )
            .into_any_element()
    }

    fn waiting(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let name = Args::default().with("name", self.target.name.clone());
        div()
            .debug_selector(|| "connector-waiting".into())
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.0))
            .px(px(24.0))
            .pb(px(24.0))
            .text_center()
            .child(spinner("connector-waiting-spin", 20.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .child(cx.t_with("settings.connectors.connect.waiting", &name)),
            )
            .when(self.target.auth() != Auth::Token, |column| {
                column.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("settings.connectors.connect.waitingHint")),
                )
            })
            .child(
                div().mt(px(8.0)).child(
                    Button::new("connector-cancel")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .text_size(12.0)
                        .on_click(cx.listener(|this, _, _, cx| this.close(cx)))
                        .child(cx.t("settings.connectors.connect.cancel")),
                ),
            )
            .into_any_element()
    }

    fn own_app(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let name = Args::default().with("name", self.target.name.clone());
        let card = || {
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .rounded(px(14.0))
                .border_1()
                .border_color(colors.border.opacity(0.6))
                .p(px(12.0))
        };
        let title = |text: SharedString| {
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.foreground)
                .child(text)
        };
        div()
            .debug_selector(|| "connector-own-app".into())
            .flex()
            .flex_col()
            .gap(px(16.0))
            .px(px(24.0))
            .pb(px(24.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(19.5))
                    .text_color(colors.muted_foreground)
                    .child(cx.t_with("settings.connectors.connect.ownApp.intro", &name)),
            )
            .child(
                card()
                    .child(title(cx.t("settings.connectors.connect.ownApp.title")))
                    .child(small(
                        cx.t("settings.connectors.connect.ownApp.redirectLabel"),
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .rounded(px(8.0))
                                    .bg(colors.muted)
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .font_family(cx.theme().fonts.mono.clone())
                                    .text_size(px(11.0))
                                    .child(USER_APP_REDIRECT_URI),
                            )
                            .child(self.copy_button(
                                "connector-copy-redirect",
                                "redirect",
                                cx.t("settings.connectors.connect.copy"),
                                USER_APP_REDIRECT_URI.into(),
                                cx,
                            )),
                    )
                    .child(field("connector-own-client-id", &self.client_id, cx))
                    .child(field("connector-own-secret", &self.client_secret, cx))
                    .child(
                        Button::new("connector-own-continue")
                            .stretch()
                            .disabled(self.text(&self.client_id, cx).is_empty())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.connect_own_app(window, cx)),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap(px(6.0))
                                    .text_size(px(12.0))
                                    .child(cx.t_with("settings.connectors.connect.continue", &name))
                                    .child(Icon::new("arrow-up-right").size(16.0)),
                            ),
                    ),
            )
            .child(
                card()
                    .child(title(cx.t("settings.connectors.connect.ownApp.tokenTitle")))
                    .child(field("connector-own-token", &self.token, cx))
                    .child(
                        Button::new("connector-own-token-connect")
                            .variant(ButtonVariant::Outline)
                            .stretch()
                            .disabled(self.text(&self.token, cx).is_empty())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.connect_token(window, cx)),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .justify_center()
                                    .text_size(px(12.0))
                                    .child(cx.t("settings.connectors.connect.connectWithToken")),
                            ),
                    ),
            )
            .into_any_element()
    }
}

impl Render for ConnectorConnect {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let logo = self.target.preset.and_then(|preset| preset.logo);
        let glow = logo.and_then(brand_color).map(hex);
        let body = match self.phase {
            Phase::Waiting => self.waiting(cx),
            Phase::NeedsApp => self.own_app(cx),
            _ => self.intro(cx),
        };
        let logo_uri = self.logo_uri();
        let close = cx.entity().downgrade();
        Dialog::new("connector-connect", move |_, cx| {
            let _ = close.update(cx, |this, cx| this.close(cx));
        })
        .max_width(440.0)
        .padding(0.0)
        .gap(0.0)
        .child(
            div()
                .id("connector-connect-body")
                .debug_selector(|| "connector-connect".into())
                .w_full()
                .overflow_y_scroll()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .px(px(24.0))
                        .pt(px(32.0))
                        .pb(px(20.0))
                        .rounded_t(crate::ui::inner_radius(cx.theme().radius.dialog, false))
                        .when_some(glow, |header, glow| {
                            // `radial-gradient(ellipse at top, glow 22%, transparent)`
                            header.bg(linear_gradient(
                                180.0,
                                linear_color_stop(glow.opacity(0.22), 0.0),
                                linear_color_stop(colors.background.opacity(0.0), 0.7),
                            ))
                        })
                        .child(glyph(
                            logo,
                            logo_uri.as_deref(),
                            &self.target.name,
                            48.0,
                            cx,
                        ))
                        .child(
                            div()
                                .mt(px(12.0))
                                .text_size(px(16.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(colors.foreground)
                                .child(cx.t_with(
                                    "settings.connectors.connect.title",
                                    &Args::default().with("name", self.target.name.clone()),
                                )),
                        ),
                )
                .child(body),
        )
    }
}
