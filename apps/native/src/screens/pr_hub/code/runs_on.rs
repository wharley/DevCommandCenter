//! What the reviewer's panel runs a review on, as one menu: the provider
//! and model, the effort and the account. The pick stays for the next
//! reviews and never touches the reviewer's own settings.

use gpui::{AnyElement, Context, SharedString, WeakEntity, div, prelude::*, px};

use crate::data::account_selection;
use crate::data::pr_review_pick::ReviewPick;
use crate::data::provider_accounts;
use crate::i18n::Translate;
use crate::screens::pr_hub::PrHub;
use crate::theme::ActiveTheme;
use crate::ui::{Icon, Menu, MenuEntry, MenuItem};

/// A menu choice: its label, whether it is the current one, the pick it makes.
type Choice = (SharedString, bool, ReviewPick);

/// A heading and its choices.
type Section = (SharedString, Vec<Choice>);

/// The menu: each provider opening into its models, then the model's
/// efforts and the provider's accounts when there are any.
#[derive(Default)]
struct RunsOn {
    providers: Vec<Section>,
    efforts: Option<Section>,
    accounts: Option<Section>,
}

fn item(this: &WeakEntity<PrHub>, (label, current, pick): Choice) -> MenuEntry {
    let this = this.clone();
    let item = MenuItem::new(label, move |_, cx| {
        let pick = pick.clone();
        let _ = this.update(cx, |hub, cx| hub.set_review_pick(pick, cx));
    });
    MenuEntry::Item(if current { item.trailing("✓") } else { item })
}

impl PrHub {
    /// The pick pinned to what runs now, so a change keeps the rest.
    fn pinned_pick(&self) -> Option<ReviewPick> {
        let target = self.review_target()?;
        let mut pick = self.review_pick.clone();
        if pick.on(&target.provider.id.0).is_none() || pick.model.is_none() {
            let model = target.model.map(|model| model.id).unwrap_or_default();
            pick.pick_model(&target.provider.id.0, &model);
        }
        Some(pick)
    }

    fn runs_on_choices(&self, cx: &gpui::App) -> RunsOn {
        let (Some(target), Some(pinned)) = (self.review_target(), self.pinned_pick()) else {
            return RunsOn::default();
        };
        let mut menu = RunsOn::default();
        for provider in self.review_providers() {
            let models = provider
                .models
                .iter()
                .map(|model| {
                    let mut pick = pinned.clone();
                    pick.pick_model(&provider.id.0, &model.id);
                    let current = provider.id == target.provider.id
                        && target.model.as_ref().map(|m| &m.id) == Some(&model.id);
                    (model.label.clone().into(), current, pick)
                })
                .collect();
            menu.providers.push((provider.label.clone().into(), models));
        }
        let levels = target
            .model
            .as_ref()
            .map(|model| model.effort_levels.clone())
            .unwrap_or_default();
        if !levels.is_empty() {
            let efforts = levels
                .iter()
                .map(|level| {
                    let pick = ReviewPick {
                        effort: Some(level.clone()),
                        ..pinned.clone()
                    };
                    let label = cx.t(&format!("composer.effort.{level}"));
                    (label, target.effort.as_ref() == Some(level), pick)
                })
                .collect();
            menu.efforts = Some((cx.t("composer.execution.effort"), efforts));
        }
        let known = account_selection::known().unwrap_or_default();
        let accounts = provider_accounts::of_provider(&known, &target.provider.id.0);
        if target.provider.capabilities.supports_accounts && !accounts.is_empty() {
            let choices = std::iter::once((String::new(), cx.t("composer.picker.accountDefault")))
                .chain(
                    accounts
                        .iter()
                        .map(|account| (account.id.clone(), account.label.clone().into())),
                )
                .map(|(id, label)| {
                    let current = target.account.as_deref().unwrap_or("") == id;
                    let pick = ReviewPick {
                        account: Some(id),
                        ..pinned.clone()
                    };
                    (label, current, pick)
                })
                .collect();
            menu.accounts = Some((cx.t("composer.picker.account"), choices));
        }
        menu
    }

    /// "Conta · Provedor · Modelo · Esforço"; the account only when it is
    /// not the system login.
    fn runs_on_label(&self, cx: &gpui::App) -> String {
        let Some(target) = self.review_target() else {
            return String::new();
        };
        let account = target.account.as_ref().and_then(|id| {
            account_selection::known()?
                .into_iter()
                .find(|account| &account.id == id)
                .map(|account| account.label)
        });
        let effort = target
            .effort
            .as_ref()
            .map(|level| cx.t(&format!("composer.effort.{level}")).to_string());
        [
            account,
            Some(target.provider.label.clone()),
            target.model.as_ref().map(|model| model.label.clone()),
            effort,
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
    }

    pub(super) fn review_runs_on(&self, busy: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let label = self.runs_on_label(cx);
        let menu = self.runs_on_choices(cx);
        let this = cx.entity().downgrade();
        Menu::dropdown(
            "pr-review-provider",
            move |_| {
                div()
                    .debug_selector(|| "pr-review-provider".into())
                    .flex()
                    .h(px(24.0))
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(6.0))
                    .px(px(8.0))
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .cursor_pointer()
                    .hover(|trigger| trigger.bg(colors.accent))
                    .when(busy, |trigger| trigger.opacity(0.5))
                    .child(label)
                    .child(Icon::new("chevron-down").size(12.0))
                    .into_any_element()
            },
            move |_, _| {
                let choices =
                    |choices: &[Choice]| choices.iter().map(|c| item(&this, c.clone())).collect();
                let mut entries: Vec<MenuEntry> = menu
                    .providers
                    .iter()
                    .map(|(label, models)| MenuEntry::Submenu {
                        label: label.clone(),
                        icon: None,
                        entries: choices(models),
                    })
                    .collect();
                for (label, section) in [&menu.efforts, &menu.accounts].into_iter().flatten() {
                    entries.push(MenuEntry::Separator);
                    entries.push(MenuEntry::Label(label.clone()));
                    entries.extend(choices(section));
                }
                entries
            },
        )
        .into_any_element()
    }
}
