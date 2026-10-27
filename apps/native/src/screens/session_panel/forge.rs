//! "Contexto de forge" in Detalhes: the checkout's forge read with its
//! accounts, who it acts as, "Trocar conta" / "Conectar no app" (the
//! settings' sign-in dialog; a new login binds the task's repository
//! again) and picking another account.

use gpui::{
    AnyElement, AppContext, Context, FontWeight, SharedString, Window, div, prelude::*, px,
};

use super::SessionPanel;
use super::details::{row, text_row};
use crate::data::forge::ForgeContext;
use crate::data::forge_cli::{CliAccounts, Forge};
use crate::i18n::Translate;
use crate::screens::settings::account::connect::{ConnectEvent, ForgeConnect, connected_message};
use crate::theme::ActiveTheme;
use crate::theme::color::hex;
use crate::ui::{Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Toaster, Tracked};

/// What the forge group read, and the sign-in over it.
#[derive(Default)]
pub(crate) struct ForgeDetails {
    /// The root it was read for; `None` inside while reading.
    pub(crate) root: Option<String>,
    pub(crate) context: Option<ForgeContext>,
    pub(crate) accounts: Option<CliAccounts>,
    pub(crate) connect: Option<(gpui::Entity<ForgeConnect>, gpui::Subscription)>,
}

/// `resolveForgeContext`: GitHub unless it says GitLab, its default host.
fn forge_of(context: Option<&ForgeContext>) -> (Forge, String) {
    let forge = if context.and_then(|context| context.provider.as_deref()) == Some("gitlab") {
        Forge::Gitlab
    } else {
        Forge::Github
    };
    let host = context
        .and_then(|context| context.host.clone())
        .filter(|host| !host.trim().is_empty())
        .unwrap_or_else(|| forge.host(""));
    (forge, host)
}

/// The avatar (`ForgeAccountAvatar`): the picture, else the initials.
fn avatar(url: Option<&str>, label: &str, size: f32, cx: &mut Context<SessionPanel>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let picture = crate::data::avatar::avatar(url, cx);
    div()
        .flex()
        .flex_none()
        .size(px(size))
        .items_center()
        .justify_center()
        .overflow_hidden()
        .rounded_full()
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.background)
        .text_size(px(if size > 20.0 { 13.0 } else { 8.0 }))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.foreground)
        .map(|frame| match picture {
            Some(image) => frame.child(gpui::img(image).size_full()),
            None => frame.child(crate::data::avatar::initials(label)),
        })
        .into_any_element()
}

impl SessionPanel {
    /// The forge group's reads, once per checkout (and after a change).
    pub(super) fn ensure_forge(&mut self, force: bool, cx: &mut Context<Self>) {
        let Some(root) = self.data(cx).and_then(|data| data.workspace_path.clone()) else {
            return;
        };
        if !force && self.forge.root.as_deref() == Some(root.as_str()) {
            return;
        }
        let Some(api) = self.api.clone() else {
            return;
        };
        self.forge.root = Some(root.clone());
        if force {
            self.forge.accounts = None;
        } else {
            self.forge.context = None;
            self.forge.accounts = None;
        }
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    let context = api.forge_context(&root).ok();
                    let accounts = context
                        .as_ref()
                        .filter(|context| context.provider.is_some() && context.host.is_some());
                    let accounts = accounts.and_then(|context| {
                        let (forge, host) = forge_of(Some(context));
                        api.forge_cli_accounts(forge, &host, false).ok()
                    });
                    (context, accounts)
                })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.forge.context = Some(read.0.unwrap_or_default());
                panel.forge.accounts = read.1;
                cx.notify();
            });
        })
        .detach();
    }

    /// `handleSelectForgeLogin`
    pub(super) fn select_login(&mut self, login: String, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            return;
        };
        let (forge, host) = forge_of(self.forge.context.as_ref());
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { api.select_forge_login(forge, &host, &login) })
                .await;
            let _ = this.update(cx, |panel, cx| {
                if let Err(error) = done {
                    Toaster::error(error.to_string(), cx);
                }
                panel.ensure_forge(true, cx);
            });
        })
        .detach();
    }

    /// "Trocar conta" / "Conectar no app": `gh|glab auth login`; a new login
    /// binds the task's repository again.
    fn open_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (forge, host) = forge_of(self.forge.context.as_ref());
        let api = self.api.clone();
        let repository = self.data(cx).and_then(|data| data.repository_id.clone());
        let connect = cx.new(|cx| ForgeConnect::new(forge, host, api, window, cx));
        let events = cx.subscribe(&connect, move |panel, _, event, cx| match event {
            ConnectEvent::Hidden => cx.notify(),
            ConnectEvent::Finished(login) => {
                panel.forge.connect = None;
                if let Some(login) = login {
                    Toaster::success(connected_message(forge, login), cx);
                    if let (Some(api), Some(repository)) = (panel.api.clone(), repository.clone()) {
                        cx.background_executor()
                            .spawn(async move {
                                let _ = api.retry_forge_binding(&repository);
                            })
                            .detach();
                    }
                }
                panel.ensure_forge(true, cx);
                cx.notify();
            }
        });
        self.forge.connect = Some((connect, events));
        cx.notify();
    }

    pub(super) fn connect_dialog(&self, cx: &gpui::App) -> Option<AnyElement> {
        let (connect, _) = self.forge.connect.as_ref()?;
        (!connect.read(cx).closing).then(|| connect.clone().into_any_element())
    }

    pub(super) fn forge_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let context = self.forge.context.clone();
        let connected = context.as_ref().is_some_and(ForgeContext::connected);
        let meta = Badge::new(
            cx.t(if connected {
                "settings.account.readyBadge"
            } else {
                "settings.account.notReadyBadge"
            }),
            if connected {
                BadgeVariant::Success
            } else {
                BadgeVariant::Outline
            },
        )
        .height(20.0)
        .text_size(10.0)
        .weight(FontWeight::NORMAL)
        .into_any_element();
        let accounts = self.forge.accounts.clone();
        self.disclosure(
            "forge",
            cx.t("inspector.groups.forge"),
            Some(meta),
            move |cx| forge_body(context, accounts, cx),
            cx,
        )
    }
}

fn forge_body(
    context: Option<ForgeContext>,
    accounts: Option<CliAccounts>,
    cx: &mut Context<SessionPanel>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let (forge, _) = forge_of(context.as_ref());
    let connected = context.as_ref().is_some_and(ForgeContext::connected);
    let selected = context
        .as_ref()
        .and_then(|context| context.effective_login.clone());
    let bound = context
        .as_ref()
        .and_then(|context| context.bound_login.clone())
        .filter(|login| !login.trim().is_empty());
    let list = accounts
        .as_ref()
        .map(|accounts| accounts.accounts.clone())
        .unwrap_or_default();
    let account_of = |login: &Option<String>| {
        login
            .as_ref()
            .and_then(|login| list.iter().find(|account| &account.login == login).cloned())
    };
    let identity = account_of(&selected).or_else(|| account_of(&bound));
    let login = selected.clone().or(bound.clone());
    let label = identity
        .as_ref()
        .and_then(|account| account.name.clone())
        .filter(|name| !name.trim().is_empty())
        .or(login.clone())
        .unwrap_or_else(|| forge.label().to_string());
    let message = context.as_ref().map_or_else(
        || Some(format!("Checking {} CLI...", forge.label())),
        |context| context.message.clone(),
    );
    let has_target = context
        .as_ref()
        .is_some_and(|context| context.provider.is_some() && context.host.is_some());
    let unavailable = context
        .as_ref()
        .and_then(|context| context.remote_state.as_deref())
        .is_some_and(|state| state == "unauthenticated" || state == "unavailable");
    let not_ready = cx.t("settings.account.notReadyBadge").to_string();
    let identity_row = div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .child(avatar(
            identity
                .as_ref()
                .and_then(|account| account.avatar_url.as_deref()),
            &label,
            40.0,
            cx,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .truncate()
                                .text_size(px(13.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(colors.foreground)
                                .child(if connected {
                                    label.clone()
                                } else {
                                    format!("{} {}", forge.label(), not_ready.to_lowercase())
                                }),
                        )
                        .when(
                            context
                                .as_ref()
                                .is_some_and(|context| context.provider.is_some()),
                            |line| {
                                line.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .rounded_full()
                                        .border_1()
                                        .border_color(colors.border.opacity(0.6))
                                        .bg(colors.background.opacity(0.8))
                                        .px(px(8.0))
                                        .py(px(2.0))
                                        .text_size(px(10.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(colors.muted_foreground)
                                        .child(div().size(px(6.0)).rounded_full().bg(
                                            if forge == Forge::Gitlab {
                                                hex(0xFC6D26)
                                            } else {
                                                colors.foreground
                                            },
                                        ))
                                        .child(Tracked::new(
                                            forge.label().to_uppercase(),
                                            10.0,
                                            0.08,
                                        )),
                                )
                            },
                        ),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(12.0))
                        .text_color(colors.muted_foreground)
                        .child(if connected {
                            login
                                .as_ref()
                                .map_or_else(String::new, |login| format!("@{login}"))
                        } else {
                            message.clone().unwrap_or_else(|| "—".into())
                        }),
                ),
        )
        .when(has_target, |identity| {
            identity.child(
                Button::new("session-forge-connect")
                    .variant(if connected {
                        ButtonVariant::Outline
                    } else {
                        ButtonVariant::Default
                    })
                    .size(ButtonSize::Sm)
                    .text_size(12.0)
                    .when(!connected, |button| button.padding_x(16.0))
                    .on_click(cx.listener(|panel, _, window, cx| panel.open_connect(window, cx)))
                    .child(cx.t(if connected {
                        "settings.account.switchAccount"
                    } else {
                        "settings.account.connect"
                    })),
            )
        });
    let field = |key: &str, cx: &gpui::App| cx.t(&format!("inspector.fields.{key}"));
    let remote = context.as_ref().map(|context| {
        format!(
            "{}/{}/{}",
            context.remote_name.clone().unwrap_or_default(),
            context.namespace.clone().unwrap_or_default(),
            context.repo.clone().unwrap_or_default()
        )
    });
    let hosts = context
        .as_ref()
        .filter(|context| !context.known_hosts.is_empty())
        .map(|context| context.known_hosts.join(", "));
    let account_value: AnyElement = if !has_target {
        div().child("—").into_any_element()
    } else if accounts.is_none() {
        div()
            .text_color(colors.muted_foreground)
            .child(cx.t("settings.account.checking"))
            .into_any_element()
    } else if list.is_empty() {
        div()
            .text_color(colors.muted_foreground)
            .child("—")
            .into_any_element()
    } else {
        let chosen_by_cli = accounts
            .as_ref()
            .and_then(|accounts| accounts.selected_login.clone());
        let buttons: Vec<AnyElement> = list
            .iter()
            .enumerate()
            .map(|(index, account)| {
                let active = selected.as_deref() == Some(account.login.as_str())
                    || (selected.is_none()
                        && chosen_by_cli.as_deref() == Some(account.login.as_str()));
                let label = account.label();
                let login = account.login.clone();
                Button::new(SharedString::from(format!("session-forge-account-{index}")))
                    .variant(if active {
                        ButtonVariant::Default
                    } else {
                        ButtonVariant::Outline
                    })
                    .size(ButtonSize::Sm)
                    .text_size(12.0)
                    .on_click(
                        cx.listener(move |panel, _, _, cx| panel.select_login(login.clone(), cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(avatar(account.avatar_url.as_deref(), &label, 16.0, cx))
                            .child(label),
                    )
                    .into_any_element()
            })
            .collect();
        div()
            .flex()
            .flex_wrap()
            .gap(px(8.0))
            .children(buttons)
            .into_any_element()
    };
    let ready = !unavailable;
    let status = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .child(
            Badge::new(
                cx.t(if ready {
                    "settings.account.readyBadge"
                } else {
                    "settings.account.notReadyBadge"
                }),
                if ready {
                    BadgeVariant::Success
                } else {
                    BadgeVariant::Outline
                },
            )
            .text_size(10.0)
            .weight(FontWeight::NORMAL),
        )
        .child(
            div()
                .text_color(colors.muted_foreground)
                .child(if connected {
                    cx.t("settings.account.accountLabel").to_string()
                } else {
                    message.unwrap_or_else(|| "—".into())
                }),
        )
        .into_any_element();
    div()
        .p(px(4.0))
        .child(identity_row)
        .child(
            div()
                .mt(px(12.0))
                .border_t_1()
                .border_color(colors.border.opacity(0.45)),
        )
        .child(text_row(
            field("host", cx),
            context.as_ref().and_then(|context| context.host.clone()),
            cx,
        ))
        .child(text_row(field("remote", cx), remote, cx))
        .child(text_row(field("knownHosts", cx), hosts, cx))
        .child(row(field("account", cx), account_value, cx))
        .child(row(field("forgeStatus", cx), status, cx))
        .into_any_element()
}
