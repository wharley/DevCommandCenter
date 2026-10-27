//! The accounts' rows (`AccountRow`, `NamedAccountRow`), the field to add
//! one and the removal's confirmation.

use dcc_core::domain::provider::{ProviderAccount, ProviderDescriptor};
use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::super::Settings;
use super::accounts::{AccountKey, Status};
use crate::data::provider_accounts::of_provider;
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Dialog, Icon, Toaster, dialog_footer, dialog_header, spinner,
};

impl Settings {
    pub(super) fn accounts_list(
        &self,
        provider: &ProviderDescriptor,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let accounts = &self.providers.accounts;
        let mut rows = vec![self.account_row(provider, None, cx)];
        for account in of_provider(&accounts.list, &provider.id.0) {
            rows.push(self.account_row(provider, Some(account), cx));
        }
        let rows = rows.into_iter().enumerate().map(|(index, row)| {
            div()
                .when(index > 0, |row| {
                    row.border_t_1().border_color(colors.border.opacity(0.5))
                })
                .child(row)
        });
        let empty = accounts.new_label.read(cx).text().trim().is_empty();
        let adding = accounts.adding;
        div()
            .debug_selector(|| "settings-provider-accounts".into())
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .overflow_hidden()
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.6))
                    .bg(colors.background)
                    .children(rows),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .debug_selector(|| "settings-account-new".into())
                            .flex()
                            .items_center()
                            .w_full()
                            .max_w(px(320.0))
                            .h(px(32.0))
                            .px(px(12.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(colors.input)
                            .text_size(px(12.0))
                            .child(accounts.new_label.clone()),
                    )
                    .child(
                        Button::new("settings-account-add")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .height(32.0)
                            .text_size(12.0)
                            .disabled(adding || empty)
                            .on_click(cx.listener(|settings, _, _, cx| settings.add_account(cx)))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(if adding {
                                        spinner("settings-account-adding", 14.0).into_any_element()
                                    } else {
                                        Icon::new("plus").size(14.0).into_any_element()
                                    })
                                    .child(cx.t("settings.model.accountsAdd")),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn account_row(
        &self,
        provider: &ProviderDescriptor,
        account: Option<&ProviderAccount>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let amber = match cx.theme().appearance {
            Appearance::Light => tw::AMBER_600.hsla(),
            Appearance::Dark => tw::AMBER_400.hsla(),
        };
        let accounts = &self.providers.accounts;
        let key: AccountKey = (
            provider.id.0.clone(),
            account.map(|account| account.id.clone()),
        );
        let status = accounts
            .statuses
            .get(&key)
            .cloned()
            .unwrap_or(Status::Checking);
        let signing_in = accounts.signing_in.contains(&key);
        let signed_in = matches!(&status, Status::Ready(status) if status.signed_in());
        let warn = matches!(&status, Status::Failed(_))
            || matches!(&status, Status::Ready(status) if !status.signed_in());
        let line: SharedString = match &status {
            Status::Checking => cx.t("settings.model.accountsChecking"),
            Status::Failed(error) => error.clone().into(),
            _ if signing_in => cx.t("settings.model.accountsSigningIn"),
            Status::Ready(ready) if ready.signed_in() => ready
                .who()
                .map(SharedString::from)
                .unwrap_or_else(|| cx.t("settings.model.accountsSignedIn")),
            Status::Ready(_) => cx.t("settings.model.accountsSignedOut"),
        };
        let slug = account.map_or("default".to_string(), |account| account.id.clone());
        let renaming = account.and_then(|account| {
            accounts
                .renaming
                .as_ref()
                .filter(|(id, _, _)| *id == account.id)
                .map(|(_, input, _)| input.clone())
        });
        let busy = account.is_some_and(|account| accounts.busy.contains(&account.id));
        let title: AnyElement = match (&renaming, account) {
            (Some(input), _) => div()
                .flex()
                .items_center()
                .w(px(192.0))
                .h(px(28.0))
                .px(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(colors.ring)
                .text_size(px(13.0))
                .child(input.clone())
                .into_any_element(),
            (None, Some(account)) => div()
                .truncate()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.foreground)
                .child(account.label.clone())
                .into_any_element(),
            (None, None) => div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .truncate()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .child(cx.t("settings.model.accountsDefault")),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("settings.model.accountsDefaultBadge")),
                )
                .into_any_element(),
        };
        let group = SharedString::from(format!("settings-account-{slug}"));
        let actions = account.filter(|_| renaming.is_none()).map(|account| {
            let rename = account.clone();
            let remove = account.clone();
            div()
                .flex()
                .flex_none()
                .items_center()
                .opacity(0.0)
                .group_hover(group.clone(), |style| style.opacity(1.0))
                .child(
                    Button::new(SharedString::from(format!(
                        "settings-account-rename-{}",
                        account.id
                    )))
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconSm)
                    .disabled(busy)
                    .on_click(cx.listener(move |settings, _, window, cx| {
                        settings.start_rename(&rename, window, cx)
                    }))
                    .child(Icon::new("pencil").size(14.0)),
                )
                .child(
                    Button::new(SharedString::from(format!(
                        "settings-account-remove-{}",
                        account.id
                    )))
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconSm)
                    .disabled(busy)
                    .on_click(cx.listener(move |settings, _, _, cx| {
                        settings.providers.accounts.removing = Some(remove.clone());
                        cx.notify();
                    }))
                    .child(Icon::new("trash-2").size(14.0)),
                )
        });
        let sign_in_key = key.clone();
        let cancel_key = key.clone();
        let trailing: Option<AnyElement> = match &status {
            Status::Checking => None,
            _ if signed_in => None,
            _ if signing_in => Some(
                Button::new(SharedString::from(format!(
                    "settings-account-cancel-{slug}"
                )))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .padding_x(8.0)
                .text_size(12.0)
                .on_click(cx.listener(move |settings, _, _, cx| {
                    settings.cancel_sign_in(cancel_key.clone(), cx)
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(spinner(
                            SharedString::from(format!("settings-account-spin-{slug}")),
                            14.0,
                        ))
                        .child(cx.t("settings.model.accountsSignInCancel")),
                )
                .into_any_element(),
            ),
            _ => Some(
                Button::new(SharedString::from(format!(
                    "settings-account-sign-in-{slug}"
                )))
                .size(ButtonSize::Sm)
                .text_size(12.0)
                .on_click(cx.listener(move |settings, _, _, cx| {
                    settings.sign_in_account(sign_in_key.clone(), cx)
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(Icon::new("log-in").size(14.0))
                        .child(cx.t("settings.model.accountsSignIn")),
                )
                .into_any_element(),
            ),
        };
        let command = match &status {
            Status::Ready(ready) if !ready.signed_in() && !signing_in => {
                ready.login_command.clone()
            }
            _ => None,
        };
        div()
            .group(group.clone())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .min_h(px(48.0))
                    .px(px(12.0))
                    .py(px(8.0))
                    .child(
                        div().flex_1().min_w_0().child(title).child(
                            div()
                                .debug_selector(move || {
                                    format!(
                                        "settings-account-line-{}",
                                        key.1.as_deref().unwrap_or("default")
                                    )
                                })
                                .mt(px(2.0))
                                .truncate()
                                .text_size(px(12.0))
                                .text_color(if warn { amber } else { colors.muted_foreground })
                                .child(line),
                        ),
                    )
                    .children(actions)
                    .children(trailing),
            )
            .children(command.map(|command| {
                let copied = command.clone();
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .min_w_0()
                    .px(px(12.0))
                    .pb(px(10.0))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("settings.model.accountsOrTerminal")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .rounded(px(4.0))
                            .bg(colors.muted.opacity(0.6))
                            .px(px(6.0))
                            .py(px(2.0))
                            .font_family(cx.theme().fonts.mono.clone())
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(command),
                    )
                    .child(
                        Button::new(SharedString::from(format!("settings-account-copy-{slug}")))
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::IconXs)
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                    copied.clone(),
                                ));
                                Toaster::success(cx.t("settings.model.accountsCommandCopied"), cx);
                            }))
                            .child(Icon::new("copy").size(12.0)),
                    )
            }))
            .into_any_element()
    }

    /// `window.confirm(accountsRemoveConfirm)`
    pub(in super::super) fn remove_account_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let account = self.providers.accounts.removing.as_ref()?;
        let close = cx.entity().downgrade();
        Some(
            Dialog::new("settings-account-remove-confirm", move |_, cx| {
                let _ = close.update(cx, |settings, cx| {
                    settings.providers.accounts.removing = None;
                    cx.notify();
                });
            })
            .close_button(false)
            .child(dialog_header(
                cx.t_with(
                    "settings.model.accountsRemoveConfirm",
                    &Args::default().with("name", account.label.clone()),
                ),
                None,
                cx,
            ))
            .child(dialog_footer(
                [
                    Button::new("settings-account-remove-cancel")
                        .variant(ButtonVariant::Outline)
                        .on_click(cx.listener(|settings, _, _, cx| {
                            settings.providers.accounts.removing = None;
                            cx.notify();
                        }))
                        .child(cx.t("sidebar.cancel"))
                        .into_any_element(),
                    Button::new("settings-account-remove-ok")
                        .variant(ButtonVariant::Destructive)
                        .on_click(cx.listener(|settings, _, _, cx| settings.remove_account(cx)))
                        .child(cx.t("settings.model.accountsRemove"))
                        .into_any_element(),
                ],
                cx,
            ))
            .into_any_element(),
        )
    }
}
