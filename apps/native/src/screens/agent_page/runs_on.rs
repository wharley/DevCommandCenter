//! "Roda em" in the agent's editor, in the composer's picker's look
//! (`composer::picker`): the provider's tabs with their marks, a search,
//! the models, and the effort as a row of steps under them. What only the
//! composer needs — favorites, ⌘1–9, "Resposta direta" — stays there. Each
//! agent runs on its own model; an agent saved before with none follows
//! the composer until one is picked.

use dcc_core::domain::provider::{ProviderAccount, ProviderDescriptor};
use gpui::{
    AnyElement, AppContext, Context, Entity, FontWeight, SharedString, Window, div, prelude::*, px,
};

use super::editor::AgentEditor;
use crate::data::composer_settings::clamp_effort;
use crate::data::model_picker;
use crate::i18n::Translate;
use crate::project::ProviderIcon;
use crate::theme::ActiveTheme;
use crate::ui::popover::{Popover, PopoverAlign, PopoverSide};
use crate::ui::{Icon, TextInput, TextInputEvent};

/// The panel's moving parts.
pub(crate) struct RunsOn {
    pub open: bool,
    pub tab: String,
    /// The account picked on the tab shown, for the model picked there.
    pub tab_account: Option<String>,
    pub search: Entity<TextInput>,
    _search: gpui::Subscription,
}

impl RunsOn {
    pub fn new(window: &mut Window, cx: &mut Context<AgentEditor>) -> Self {
        let search =
            cx.new(|cx| TextInput::new(window, cx).with_placeholder(cx.t("composer.model.search")));
        let watch = cx.subscribe(&search, |_, _, event, cx| {
            if matches!(event, TextInputEvent::Changed) {
                cx.notify();
            }
        });
        Self {
            open: false,
            tab: String::new(),
            tab_account: None,
            search,
            _search: watch,
        }
    }
}

impl AgentEditor {
    fn providers(&self) -> Vec<&ProviderDescriptor> {
        model_picker::picker_providers(&self.catalog, self.provider_id.as_deref())
    }

    pub(super) fn set_execution(
        &mut self,
        provider_id: Option<String>,
        model: Option<String>,
        effort: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.provider_id = provider_id;
        self.model = model;
        self.effort = effort;
        cx.notify();
    }

    fn open_runs_on(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.runs_on.open = open;
        if open {
            let tab = self
                .provider_id
                .clone()
                .or_else(|| {
                    self.providers()
                        .first()
                        .map(|provider| provider.id.0.clone())
                })
                .unwrap_or_default();
            self.show_tab(tab);
            self.read_accounts(cx);
            self.runs_on
                .search
                .update(cx, |input, cx| input.set_text("", cx));
            let handle = gpui::Focusable::focus_handle(self.runs_on.search.read(cx), cx);
            window.focus(&handle, cx);
        }
        cx.notify();
    }

    /// A model picked: its provider and model, the effort kept within its
    /// steps (none for a model that sets its own).
    fn pick_model(&mut self, provider: String, model: String, cx: &mut Context<Self>) {
        let levels = self
            .catalog
            .iter()
            .find(|candidate| candidate.id.0 == provider)
            .and_then(|candidate| candidate.models.iter().find(|found| found.id == model))
            .map(|found| found.effort_levels.clone())
            .unwrap_or_default();
        let effort = (!levels.is_empty()).then(|| {
            clamp_effort(
                self.effort
                    .as_deref()
                    .unwrap_or(crate::data::composer_settings::DEFAULT_EFFORT),
                &levels,
            )
        });
        // The account picked on the model's tab (an account belongs to its
        // provider).
        self.account_id = if self.runs_on.tab == provider {
            self.runs_on.tab_account.clone()
        } else {
            None
        };
        self.set_execution(Some(provider), Some(model), effort, cx);
    }

    /// Shows `tab`, its account strip on the agent's account when the tab is
    /// the agent's provider.
    pub(super) fn show_tab(&mut self, tab: String) {
        self.runs_on.tab_account = if self.provider_id.as_deref() == Some(tab.as_str()) {
            self.account_id.clone()
        } else {
            None
        };
        self.runs_on.tab = tab;
    }

    /// Reads the accounts again (they may have changed in Settings, or the
    /// composer may not have read them yet).
    fn read_accounts(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.provider_accounts() })
                .await;
            if let Ok(accounts) = read {
                let _ = this.update(cx, |editor, cx| {
                    editor.accounts = accounts;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// The accounts of the tab shown, when its provider has more than the
    /// system login.
    fn provider_accounts(&self) -> Vec<&ProviderAccount> {
        let Some(provider) = self
            .catalog
            .iter()
            .find(|provider| provider.id.0 == self.runs_on.tab)
            .filter(|provider| provider.capabilities.supports_accounts)
        else {
            return Vec::new();
        };
        crate::data::provider_accounts::of_provider(&self.accounts, &provider.id.0)
    }

    /// "Padrão" and each account of the agent's provider, as the composer's
    /// strip; none without accounts.
    fn runs_on_accounts(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let accounts = self.provider_accounts();
        if accounts.is_empty() {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let choices: Vec<AnyElement> =
            std::iter::once((None, cx.t("composer.picker.accountDefault")))
                .chain(accounts.iter().map(|account| {
                    (
                        Some(account.id.clone()),
                        SharedString::from(account.label.clone()),
                    )
                }))
                .enumerate()
                .map(|(index, (id, label))| {
                    let checked = id == self.runs_on.tab_account;
                    div()
                        .id(("agent-runs-account", index))
                        .debug_selector(move || format!("agent-runs-account-{index}"))
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .h(px(24.0))
                        .px(px(8.0))
                        .rounded(px(6.0))
                        .text_size(px(11.0))
                        .cursor_pointer()
                        .when(checked, |choice| {
                            choice
                                .bg(colors.accent)
                                .text_color(colors.foreground)
                                .font_weight(FontWeight::MEDIUM)
                        })
                        .when(!checked, |choice| {
                            choice
                                .text_color(colors.muted_foreground)
                                .hover(|choice| choice.bg(colors.accent.opacity(0.6)))
                        })
                        .on_click(cx.listener(move |editor, _, _, cx| {
                            editor.runs_on.tab_account = id.clone();
                            // On the agent's provider it applies to its model now.
                            if editor.provider_id.as_deref() == Some(editor.runs_on.tab.as_str()) {
                                editor.account_id = id.clone();
                            }
                            cx.notify();
                        }))
                        .child(label)
                        .into_any_element()
                })
                .collect();
        Some(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(4.0))
                .px(px(8.0))
                .py(px(6.0))
                .border_b_1()
                .border_color(colors.border.opacity(0.6))
                .child(
                    div()
                        .flex_none()
                        .mr(px(4.0))
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("agents.editor.account")),
                )
                .children(choices)
                .into_any_element(),
        )
    }

    /// The trigger's text: provider · model · effort.
    pub(super) fn summary(&self, cx: &gpui::App) -> SharedString {
        let Some(provider) = self
            .provider_id
            .as_deref()
            .and_then(|id| self.catalog.iter().find(|provider| provider.id.0 == id))
        else {
            return cx.t("agents.editor.providerPick");
        };
        let model = self
            .model
            .as_deref()
            .and_then(|id| provider.models.iter().find(|model| model.id == id));
        let effort = match (self.effort.as_deref(), model) {
            (Some(effort), Some(model)) if !model.effort_levels.is_empty() => {
                Some(clamp_effort(effort, &model.effort_levels))
            }
            _ => None,
        };
        let account = self.account_id.as_deref().and_then(|id| {
            self.accounts
                .iter()
                .find(|account| account.id == id)
                .map(|account| account.label.clone())
        });
        [
            Some(provider.label.clone()),
            model.map(|model| model.label.clone()),
            effort.map(|effort| cx.t(&format!("composer.effort.{effort}")).to_string()),
            account,
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
        .into()
    }

    fn runs_on_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let tabs: Vec<AnyElement> = self
            .providers()
            .into_iter()
            .map(|provider| {
                let id = provider.id.0.clone();
                let selected = id == self.runs_on.tab;
                let label = provider.label.clone();
                let tab = id.clone();
                div()
                    .id(SharedString::from(format!("agent-runs-tab-{id}")))
                    .debug_selector({
                        let id = id.clone();
                        move || format!("agent-runs-tab-{id}")
                    })
                    .relative()
                    .flex()
                    .flex_shrink_0()
                    .h(px(40.0))
                    .min_w(px(36.0))
                    .items_center()
                    .justify_center()
                    .px(px(8.0))
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .text_color(if selected {
                        colors.foreground
                    } else {
                        colors.muted_foreground
                    })
                    .hover(|tab| tab.bg(colors.accent.opacity(0.7)))
                    .on_click(cx.listener(move |editor, _, _, cx| {
                        editor.show_tab(tab.clone());
                        cx.notify();
                    }))
                    .child(ProviderIcon::new(&id).size(17.0))
                    .when(selected, |tab| {
                        tab.child(
                            div()
                                .ml(px(6.0))
                                .max_w(px(144.0))
                                .truncate()
                                .text_size(px(12.0))
                                .font_weight(FontWeight::MEDIUM)
                                .child(label),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(px(8.0))
                                .right(px(8.0))
                                .bottom_0()
                                .h(px(2.0))
                                .rounded_full()
                                .bg(colors.foreground),
                        )
                    })
                    .into_any_element()
            })
            .collect();
        div()
            .id("agent-runs-tabs")
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(2.0))
            .px(px(6.0))
            .border_b_1()
            .border_color(colors.border.opacity(0.6))
            .overflow_x_scroll()
            .children(tabs)
            .into_any_element()
    }

    fn runs_on_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let query = self.runs_on.search.read(cx).text().to_string();
        let rows = model_picker::rows(
            &self.catalog,
            &[],
            &self.runs_on.tab,
            &query,
            self.provider_id.as_deref(),
        );
        let items: Vec<AnyElement> = rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                let on = self.provider_id.as_deref() == Some(row.provider_id.as_str())
                    && self.model.as_deref() == Some(row.model_id.as_str());
                let (provider, model) = (row.provider_id.clone(), row.model_id.clone());
                div()
                    .id(SharedString::from(format!("agent-runs-model-{index}")))
                    .debug_selector(move || format!("agent-runs-model-{index}"))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .when(on, |row| row.bg(colors.accent))
                    .hover(|row| row.bg(colors.accent.opacity(0.7)))
                    .on_click(cx.listener(move |editor, _, _, cx| {
                        editor.pick_model(provider.clone(), model.clone(), cx)
                    }))
                    .when(row.from_other_provider, |line| {
                        line.child(ProviderIcon::new(&row.provider_id).size(13.0))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.0))
                                    .text_color(colors.foreground)
                                    .child(row.label.clone()),
                            )
                            .when(!row.description.is_empty(), |text| {
                                text.child(
                                    div()
                                        .truncate()
                                        .text_size(px(11.0))
                                        .text_color(colors.muted_foreground)
                                        .child(row.description.clone()),
                                )
                            }),
                    )
                    .when(on, |row| row.child(Icon::new("check").size(14.0)))
                    .into_any_element()
            })
            .collect();
        div()
            .id("agent-runs-list")
            .flex()
            .flex_col()
            .max_h(px(280.0))
            .overflow_y_scroll()
            .p(px(4.0))
            .children(items)
            .into_any_element()
    }

    /// The chosen model's effort, as a row of steps.
    fn runs_on_effort(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let colors = cx.theme().colors.clone();
        let model = self
            .provider_id
            .as_deref()
            .and_then(|id| self.catalog.iter().find(|provider| provider.id.0 == id))
            .and_then(|provider| {
                self.model
                    .as_deref()
                    .and_then(|id| provider.models.iter().find(|model| model.id == id))
            })?;
        if model.effort_levels.is_empty() {
            return None;
        }
        let current = self
            .effort
            .as_deref()
            .map(|effort| clamp_effort(effort, &model.effort_levels));
        let steps: Vec<AnyElement> = model
            .effort_levels
            .iter()
            .map(|level| {
                let on = current.as_deref() == Some(level.as_str());
                let chosen = level.clone();
                div()
                    .id(SharedString::from(format!("agent-runs-effort-{level}")))
                    .debug_selector({
                        let level = level.clone();
                        move || format!("agent-runs-effort-{level}")
                    })
                    .flex_1()
                    .flex()
                    .justify_center()
                    .py(px(5.0))
                    .rounded(px(6.0))
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .when(on, |step| {
                        step.bg(colors.background)
                            .text_color(colors.foreground)
                            .font_weight(FontWeight::MEDIUM)
                    })
                    .when(!on, |step| step.text_color(colors.muted_foreground))
                    .on_click(cx.listener(move |editor, _, _, cx| {
                        editor.effort = Some(chosen.clone());
                        cx.notify();
                    }))
                    .child(cx.t(&format!("composer.effort.{level}")))
                    .into_any_element()
            })
            .collect();
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .px(px(12.0))
                .py(px(10.0))
                .border_t_1()
                .border_color(colors.border.opacity(0.6))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("composer.execution.effort")),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(2.0))
                        .p(px(2.0))
                        .rounded(px(8.0))
                        .bg(colors.muted)
                        .children(steps),
                )
                .into_any_element(),
        )
    }

    fn runs_on_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        div()
            .debug_selector(|| "agent-runs-panel".into())
            .flex()
            .flex_col()
            .min_h_0()
            .child(self.runs_on_tabs(cx))
            .children(self.runs_on_accounts(cx))
            .child(
                div()
                    .flex()
                    .h(px(40.0))
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(12.0))
                    .border_b_1()
                    .border_color(colors.border.opacity(0.6))
                    .text_size(px(13.0))
                    .child(
                        Icon::new("search")
                            .size(14.0)
                            .color(colors.muted_foreground.opacity(0.6)),
                    )
                    .child(div().flex_1().min_w_0().child(self.runs_on.search.clone())),
            )
            .child(self.runs_on_list(cx))
            .children(self.runs_on_effort(cx))
            .into_any_element()
    }

    /// The field's trigger and its panel.
    pub(super) fn runs_on_field(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let summary = self.summary(cx);
        let trigger = div()
            .id("agent-editor-runs-on")
            .debug_selector(|| "agent-editor-runs-on".into())
            .flex()
            .h(px(36.0))
            .w_full()
            .items_center()
            .gap(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(colors.border.opacity(0.8))
            .bg(colors.background)
            .px(px(10.0))
            .text_size(px(13.0))
            .text_color(colors.foreground)
            .cursor_pointer()
            .hover(|trigger| trigger.bg(colors.accent.opacity(0.5)))
            .children(
                self.provider_id
                    .as_deref()
                    .map(|provider| ProviderIcon::new(provider).size(15.0)),
            )
            .child(div().flex_1().min_w_0().truncate().child(summary))
            .child(
                Icon::new("chevron-down")
                    .size(14.0)
                    .color(colors.muted_foreground),
            );
        let open = self.runs_on.open;
        let content = open.then(|| self.runs_on_panel(cx));
        let editor = cx.entity().downgrade();
        Popover::new(
            "agent-runs-popover",
            open,
            trigger,
            move |_, _| content.unwrap_or_else(|| div().into_any_element()),
            move |open, window, cx| {
                let _ = editor.update(cx, |editor, cx| editor.open_runs_on(open, window, cx));
            },
        )
        .side(PopoverSide::Bottom)
        .align(PopoverAlign::Start)
        .side_offset(6.0)
        .width(360.0)
        .padding(0.0)
        .into_any_element()
    }
}
