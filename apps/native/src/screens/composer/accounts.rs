//! `ProviderAccountStrip` under the model picker's provider tabs: which
//! account a provider runs as ("Padrão" is the system login). Shown only
//! when the provider has accounts of its own. The open conversation keeps
//! the account it started with, so on its provider the other choices are
//! off; the pick applies to new conversations. The chip says the account
//! too (`useShownAccountLabel`).

use dcc_core::domain::provider::ProviderDescriptor;
use dcc_core::ports::provider::ProviderRuntimeConfig;
use gpui::{AnyElement, Context, FontWeight, SharedString, Task, div, prelude::*, px};

use super::Composer;
use crate::data::account_selection::{self, Selection};
use crate::data::desktop_api::DesktopApi;
use crate::data::provider_accounts;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Tooltip, tooltip::Side};

/// The picks and their read, on the composer.
pub(crate) struct Accounts {
    /// `useProviderAccountSelection()`
    pub selection: Selection,
    _read: Option<Task<()>>,
}

impl Default for Accounts {
    fn default() -> Self {
        Self {
            selection: account_selection::load(),
            _read: None,
        }
    }
}

impl Composer {
    /// The open conversation's provider and account, once it exists.
    fn conversation_account(&self) -> Option<(&str, Option<&str>)> {
        self.session.session_id.as_ref()?;
        let provider = self.session.provider_id.as_deref()?;
        Some((provider, self.session.account_id.as_deref()))
    }

    /// `shownAccountId`: the account shown as chosen, and whether the
    /// conversation holds it.
    pub(crate) fn shown_account(&self, provider_id: &str) -> (Option<String>, bool) {
        let conversation = self.conversation_account();
        let locked = conversation.is_some_and(|(provider, _)| provider == provider_id);
        let known = account_selection::known();
        (
            account_selection::resolve(
                provider_id,
                conversation,
                &self.picker.accounts.selection,
                known.as_deref(),
            ),
            locked,
        )
    }

    /// The runtime a turn on `provider` runs with: Settings' and its account.
    pub(crate) fn runtime_with_account(
        &self,
        provider: &ProviderDescriptor,
    ) -> ProviderRuntimeConfig {
        let mut runtime = crate::data::provider_runtime::load().runtime_for(provider);
        if provider.capabilities.supports_accounts {
            runtime.account_id = self.shown_account(&provider.id.0).0;
        }
        runtime
    }

    /// `useShownAccountLabel`: the selected provider's account, unless it is
    /// the system login.
    pub(crate) fn shown_account_label(&self) -> Option<String> {
        let provider = self
            .provider()
            .filter(|provider| provider.capabilities.supports_accounts)?;
        let id = self.shown_account(&provider.id.0).0?;
        account_selection::known()?
            .into_iter()
            .find(|account| account.id == id)
            .map(|account| account.label)
    }

    /// Reads the accounts again (they may have changed in Settings).
    pub(crate) fn refresh_accounts(&mut self, cx: &mut Context<Self>) {
        let Some(api) = DesktopApi::load() else {
            return;
        };
        self.picker.accounts._read = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.provider_accounts() })
                .await;
            if read.is_ok() {
                let _ = this.update(cx, |_, cx| cx.notify());
            }
        }));
    }

    /// `selectProviderAccount`
    fn pick_account(&mut self, provider_id: &str, account: Option<&str>, cx: &mut Context<Self>) {
        self.picker.accounts.selection.set(provider_id, account);
        account_selection::select(provider_id, account);
        if self.selected_provider_id().as_deref() == Some(provider_id) {
            self.refresh_account_usage(cx);
        }
        // The other account may have other models.
        self.refresh_cli_models(cx);
        cx.notify();
    }
}

/// The strip for the picker's provider tab, or nothing.
pub(super) fn account_strip(composer: &Composer, cx: &mut Context<Composer>) -> Option<AnyElement> {
    let provider = composer
        .catalog
        .as_ref()?
        .iter()
        .find(|provider| provider.id.0 == composer.picker.tab)
        .filter(|provider| provider.capabilities.supports_accounts)?;
    let known = account_selection::known()?;
    let accounts = provider_accounts::of_provider(&known, &provider.id.0);
    if accounts.is_empty() {
        return None;
    }
    let colors = cx.theme().colors.clone();
    let provider_id = provider.id.0.clone();
    let (shown, locked) = composer.shown_account(&provider_id);
    let choices = std::iter::once((None, cx.t("composer.picker.accountDefault")))
        .chain(accounts.iter().map(|account| {
            (
                Some(account.id.clone()),
                SharedString::from(account.label.clone()),
            )
        }))
        .enumerate()
        .map(|(index, (id, label))| {
            let checked = id == shown;
            let disabled = locked && !checked;
            let (hover, foreground) = (colors.accent.opacity(0.6), colors.foreground);
            let provider_id = provider_id.clone();
            div()
                .id(("composer-account", index))
                .debug_selector(move || format!("composer-account-{index}"))
                .flex()
                .flex_shrink_0()
                .items_center()
                .h(px(24.0))
                .px(px(8.0))
                .rounded(px(6.0))
                .text_size(px(11.0))
                .map(|choice| {
                    if checked {
                        choice
                            .bg(colors.accent)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                    } else {
                        choice.text_color(colors.muted_foreground)
                    }
                })
                .when(disabled, |choice| choice.opacity(0.4))
                .when(!checked && !disabled, |choice| {
                    choice
                        .cursor_pointer()
                        .hover(move |style| style.bg(hover).text_color(foreground))
                        .on_click(cx.listener(move |composer, _, _, cx| {
                            cx.stop_propagation();
                            composer.pick_account(&provider_id, id.as_deref(), cx);
                        }))
                })
                .child(label)
        });
    let strip = div()
        .id("composer-account-strip")
        .debug_selector(|| "composer-account-strip".into())
        .flex()
        .flex_shrink_0()
        .h(px(36.0))
        .items_center()
        .gap(px(4.0))
        .overflow_x_scroll()
        .border_b_1()
        .border_color(colors.border.opacity(0.6))
        .px(px(8.0))
        .child(
            div()
                .flex_shrink_0()
                .px(px(4.0))
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("composer.picker.account")),
        )
        .children(choices);
    Some(if locked {
        Tooltip::text(
            "composer-account-locked",
            strip,
            cx.t("composer.picker.accountLocked"),
        )
        .side(Side::Bottom)
        .into_any_element()
    } else {
        strip.into_any_element()
    })
}
