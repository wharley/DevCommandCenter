//! The section's header and the forge card.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, img, prelude::*, px};

use super::super::Settings;
use super::super::decision::field;
use crate::data::avatar::{avatar, initials};
use crate::data::forge_cli::{CliAccount, Forge};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::color::mix_oklab;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tabs, spinner};

/// `Badge` at `h-8 px-3 text-[12px] font-normal`.
fn pill(text: SharedString, background: Option<gpui::Hsla>, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(px(32.0))
        .px(px(12.0))
        .rounded(px(26.0))
        .border_1()
        .border_color(if background.is_some() {
            gpui::transparent_black()
        } else {
            colors.border
        })
        .when_some(background, |pill, background| pill.bg(background))
        .text_size(px(12.0))
        .text_color(colors.foreground)
        .child(text)
        .into_any_element()
}

impl Settings {
    pub(in super::super) fn account_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(24.0))
                    .border_b_1()
                    .border_color(colors.border.opacity(0.4))
                    .pb(px(16.0))
                    .child(
                        div()
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(cx.t("settings.account.title")),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(12.0))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("settings.account.hint")),
                            ),
                    )
                    .child(div().text_color(colors.muted_foreground).child(pill(
                        cx.t("settings.account.sectionBadge"),
                        None,
                        cx,
                    ))),
            )
            .child(self.forge_card(cx))
            .into_any_element()
    }

    fn forge_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let account = &self.account;
        let forge = account.forge;
        let host = account.host();
        let loading =
            account.loading.contains(&(forge, host.clone())) && account.current().is_none();
        let accounts = account.current().cloned();
        let ready = accounts.as_ref().is_some_and(|accounts| accounts.ready());
        let effective = accounts
            .as_ref()
            .and_then(|accounts| accounts.effective_login());
        let tabs = Tabs::new(
            "settings-account-forge",
            Forge::ALL
                .map(|forge| SharedString::from(forge.label()))
                .to_vec(),
            usize::from(forge == Forge::Gitlab),
        )
        .on_select({
            let this = cx.entity().downgrade();
            move |index, _, cx| {
                let _ = this.update(cx, |settings, cx| {
                    settings.select_forge(Forge::ALL[index], cx)
                });
            }
        });
        let known = account.known_hosts.get(&forge).cloned().unwrap_or_default();
        let host_buttons = known.iter().map(|candidate| {
            let active = *candidate == host;
            let chosen = candidate.clone();
            Button::new(SharedString::from(format!(
                "settings-account-host-{candidate}"
            )))
            .variant(if active {
                ButtonVariant::Default
            } else {
                ButtonVariant::Outline
            })
            .size(ButtonSize::Sm)
            .on_click(
                cx.listener(move |settings, _, _, cx| settings.set_forge_host(chosen.clone(), cx)),
            )
            .child(candidate.clone())
            .into_any_element()
        });
        let host_buttons: Vec<AnyElement> = host_buttons.collect();
        let host_hint: SharedString = if known.is_empty() {
            cx.t(match forge {
                Forge::Github => "settings.account.githubHint",
                Forge::Gitlab => "settings.account.gitlabHint",
            })
        } else {
            cx.t_with(
                "settings.account.knownHosts",
                &Args::default().with("hosts", known.join(", ")),
            )
        };
        let terminal = |cx: &gpui::App| {
            Icon::new("square-terminal")
                .size(14.0)
                .color(cx.theme().colors.foreground)
        };
        let connect_label = if ready {
            "settings.account.switchAccount"
        } else {
            "settings.account.connect"
        };
        let connect = Button::new("settings-account-connect")
            .variant(ButtonVariant::Outline)
            .size(ButtonSize::Sm)
            .on_click(
                cx.listener(|settings, _, window, cx| settings.open_forge_connect(window, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(terminal(cx))
                    .child(cx.t(connect_label)),
            );
        let refresh = Button::new("settings-account-refresh")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
            .on_click(cx.listener(|settings, _, _, cx| settings.load_account(true, cx)))
            .child(cx.t("settings.account.refresh"));
        let status_row: Vec<AnyElement> = if loading {
            vec![
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .h(px(32.0))
                    .px(px(12.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.6))
                    .text_size(px(12.0))
                    .text_color(colors.muted_foreground)
                    .child(spinner("settings-account-checking", 14.0))
                    .child(cx.t("settings.account.checking"))
                    .into_any_element(),
            ]
        } else if let Some(accounts) = accounts.as_ref().filter(|accounts| accounts.ready()) {
            let who = if accounts.accounts.len() > 1 {
                cx.t_with(
                    "settings.account.accountsConnected",
                    &Args::count(accounts.accounts.len() as i64)
                        .with("logins", accounts.logins().join(", ")),
                )
                .to_string()
            } else {
                accounts
                    .login
                    .clone()
                    .unwrap_or_else(|| accounts.message.clone())
            };
            vec![
                div()
                    .debug_selector(|| "settings-account-who".into())
                    .flex()
                    .min_w_0()
                    .items_center()
                    .gap(px(6.0))
                    .h(px(32.0))
                    .px(px(12.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.6))
                    .text_size(px(12.0))
                    .text_color(colors.foreground)
                    .child(terminal(cx))
                    .child(div().truncate().child(who))
                    .into_any_element(),
                refresh.into_any_element(),
                connect.into_any_element(),
            ]
        } else {
            vec![connect.into_any_element(), refresh.into_any_element()]
        };
        let choices = accounts
            .as_ref()
            .filter(|accounts| accounts.ready() && !accounts.accounts.is_empty())
            .map(|accounts| {
                let buttons: Vec<AnyElement> = accounts
                    .accounts
                    .iter()
                    .map(|account| self.forge_account_button(account, effective.as_deref(), cx))
                    .collect();
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(cx.t("settings.account.accountLabel")),
                    )
                    .child(div().flex().flex_wrap().gap(px(8.0)).children(buttons))
            });
        let (message, command) = match &accounts {
            Some(accounts) => (accounts.message.clone(), accounts.login_command.clone()),
            None => (String::new(), forge.login_command(&host)),
        };
        div()
            .debug_selector(|| "settings-account-forge-card".into())
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
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(cx.t("settings.account.cardTitle")),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(12.0))
                                    .line_height(px(19.5))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("settings.account.cardHint")),
                            ),
                    )
                    .child(pill(
                        cx.t(if ready {
                            "settings.account.readyBadge"
                        } else {
                            "settings.account.notReadyBadge"
                        }),
                        ready.then(|| mix_oklab(colors.success, 0.2, colors.secondary)),
                        cx,
                    )),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(div().w_full().child(tabs))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(cx.t("settings.account.hostLabel")),
                            )
                            .child(field("settings-account-host", &account.host_input, cx))
                            .when(!known.is_empty(), |column| {
                                column.child(
                                    div().flex().flex_wrap().gap(px(8.0)).children(host_buttons),
                                )
                            })
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .line_height(px(17.9))
                                    .text_color(colors.muted_foreground)
                                    .child(host_hint),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.0))
                            .children(status_row),
                    )
                    .children(choices)
                    .child(
                        div()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .line_height(px(19.5))
                                    .text_color(colors.muted_foreground)
                                    .child(message),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .text_size(px(11.0))
                                    .line_height(px(15.1))
                                    .text_color(colors.muted_foreground.opacity(0.8))
                                    .child(cx.t_with(
                                        "settings.account.command",
                                        &Args::default().with("command", command),
                                    )),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn forge_account_button(
        &self,
        account: &CliAccount,
        effective: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let active = effective == Some(account.login.as_str());
        let label = account.label();
        let picture = avatar(account.avatar_url.as_deref(), cx);
        let face = div()
            .flex()
            .size(px(16.0))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .rounded_full()
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.background)
            .text_size(px(8.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(colors.foreground)
            .map(|face| match picture {
                Some(picture) => face.child(img(picture).size_full().rounded_full()),
                None => face.child(initials(&label)),
            });
        let login = account.login.clone();
        let button = Button::new(SharedString::from(format!(
            "settings-account-login-{}",
            account.login
        )))
        .variant(if active {
            ButtonVariant::Default
        } else {
            ButtonVariant::Outline
        })
        .size(ButtonSize::Sm)
        .on_click(
            cx.listener(move |settings, _, _, cx| settings.choose_forge_login(login.clone(), cx)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(face)
                .child(label),
        );
        crate::ui::Tooltip::text(
            SharedString::from(format!("settings-account-login-tip-{}", account.login)),
            button,
            account.title(),
        )
        .into_any_element()
    }
}
