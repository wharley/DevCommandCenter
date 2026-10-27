//! "Decision Provider": two tabs, the configuration (TypeSafe Jev's
//! switch, mode, model routing, key, URL, model and the five decision
//! points) saved as one, and the history of decisions, reread every 10s
//! while it shows.

mod config;
pub(super) use config::field;
mod history;

use std::collections::HashSet;
use std::time::Duration;

use gpui::{
    AnyElement, AppContext, Context, Entity, Subscription, Task, Window, div, prelude::*, px,
};

use super::{SectionId, Settings};
use crate::data::decision_provider::{Draft, HistoryEntry, Point, Settings as Saved};
use crate::i18n::Translate;
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::{Tabs, Toaster};

/// `refetchInterval` of the history.
const HISTORY_POLL: Duration = Duration::from_secs(10);

/// A field of the form typed into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    ApiKey,
    BaseUrl,
    Model,
    Threshold(Point),
}

pub(super) struct Decision {
    /// 0: Configuração, 1: Histórico de decisões.
    pub(super) tab: usize,
    pub(super) saved: Option<Saved>,
    pub(super) draft: Draft,
    pub(super) saving: bool,
    pub(super) api_key: Entity<TextInput>,
    pub(super) base_url: Entity<TextInput>,
    pub(super) model: Entity<TextInput>,
    pub(super) thresholds: Vec<(Point, Entity<TextInput>)>,
    pub(super) history: Option<Vec<HistoryEntry>>,
    pub(super) history_loading: bool,
    /// Entries whose "Ver critérios avaliados" is open.
    pub(super) open_details: HashSet<i64>,
    _poll: Option<Task<()>>,
    _inputs: Vec<Subscription>,
}

fn number(value: Option<f64>) -> String {
    // `String(Number)`: 0.65, 0.8, 1.
    format!("{}", value.unwrap_or(0.65))
}

impl Decision {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Settings>) -> Self {
        let draft = Draft::default();
        let placeholder = Translate::t(&**cx, "settings.decisionProvider.apiKeyPlaceholder");
        let api_key = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder(placeholder)
                .masked()
        });
        let field = |text: String, window: &mut Window, cx: &mut Context<Settings>| {
            let input = cx.new(|cx| TextInput::new(window, cx));
            input.update(cx, |input, cx| input.set_text(text, cx));
            input
        };
        let base_url = field(draft.base_url.clone().unwrap_or_default(), window, cx);
        let model = field(draft.model.clone().unwrap_or_default(), window, cx);
        let thresholds: Vec<(Point, Entity<TextInput>)> = Point::ALL
            .into_iter()
            .map(|point| {
                (
                    point,
                    field(number(Some(draft.threshold(point))), window, cx),
                )
            })
            .collect();
        let mut watched = vec![
            (api_key.clone(), Field::ApiKey),
            (base_url.clone(), Field::BaseUrl),
            (model.clone(), Field::Model),
        ];
        watched.extend(
            thresholds
                .iter()
                .map(|(point, input)| (input.clone(), Field::Threshold(*point))),
        );
        let inputs = watched
            .into_iter()
            .map(|(input, field)| {
                cx.subscribe(&input, move |settings, input, event, cx| {
                    if matches!(event, TextInputEvent::Changed) {
                        let text = input.read(cx).text().to_string();
                        settings.decision_typed(field, text, cx);
                    }
                })
            })
            .collect();
        Self {
            tab: 0,
            saved: None,
            draft,
            saving: false,
            api_key,
            base_url,
            model,
            thresholds,
            history: None,
            history_loading: false,
            open_details: HashSet::new(),
            _poll: None,
            _inputs: inputs,
        }
    }
}

impl Settings {
    pub(super) fn load_decision(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let settings = cx
                .background_executor()
                .spawn(async move { api.decision_provider_settings() })
                .await;
            let _ = this.update(cx, |settings_view, cx| {
                if let Ok(saved) = settings {
                    settings_view.apply_decision(saved, cx);
                }
            });
        })
        .detach();
    }

    /// The saved settings become the form (`useEffect` on the query).
    fn apply_decision(&mut self, saved: Saved, cx: &mut Context<Self>) {
        let decision = &mut self.decision;
        decision.draft = Draft::from(&saved);
        let placeholder = cx.t(if saved.api_key_configured {
            "settings.decisionProvider.apiKeyConfigured"
        } else {
            "settings.decisionProvider.apiKeyPlaceholder"
        });
        decision.api_key.update(cx, |input, cx| {
            input.set_text("", cx);
            input.set_placeholder(placeholder, cx);
        });
        let base_url = decision.draft.base_url.clone().unwrap_or_default();
        let model = decision.draft.model.clone().unwrap_or_default();
        decision
            .base_url
            .update(cx, |input, cx| input.set_text(base_url, cx));
        decision
            .model
            .update(cx, |input, cx| input.set_text(model, cx));
        for (point, input) in &decision.thresholds {
            let text = number(Some(decision.draft.threshold(*point)));
            input.update(cx, |input, cx| input.set_text(text, cx));
        }
        decision.saved = Some(saved);
        cx.notify();
    }

    fn decision_typed(&mut self, field: Field, text: String, cx: &mut Context<Self>) {
        let draft = &mut self.decision.draft;
        let value = (!text.is_empty()).then(|| text.clone());
        match field {
            Field::ApiKey => {
                draft.api_key = value;
                draft.clear_api_key = false;
            }
            Field::BaseUrl => draft.base_url = value,
            Field::Model => draft.model = value,
            Field::Threshold(point) => draft.set_threshold(point, &text),
        }
        cx.notify();
    }

    pub(super) fn update_decision(
        &mut self,
        change: impl FnOnce(&mut Draft),
        cx: &mut Context<Self>,
    ) {
        change(&mut self.decision.draft);
        cx.notify();
    }

    /// "Remover chave salva": cleared when saved.
    pub(super) fn clear_decision_key(&mut self, cx: &mut Context<Self>) {
        self.decision
            .api_key
            .update(cx, |input, cx| input.set_text("", cx));
        let draft = &mut self.decision.draft;
        draft.api_key = None;
        draft.clear_api_key = true;
        cx.notify();
    }

    pub(super) fn save_decision(&mut self, cx: &mut Context<Self>) {
        let api = self.general.api();
        let draft = self.decision.draft.clone();
        self.decision.saving = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move {
                    api.ok_or(crate::data::desktop_api::ApiError::DesktopClosed)?
                        .save_decision_provider(&draft)
                })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.decision.saving = false;
                match saved {
                    Ok(saved) => {
                        settings.apply_decision(saved, cx);
                        Toaster::success(cx.t("settings.decisionProvider.saved"), cx);
                    }
                    Err(crate::data::desktop_api::ApiError::Failed(message))
                        if !message.is_empty() =>
                    {
                        Toaster::error(message, cx)
                    }
                    Err(_) => Toaster::error(cx.t("settings.decisionProvider.saveError"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn select_decision_tab(&mut self, tab: usize, cx: &mut Context<Self>) {
        self.decision.tab = tab;
        if tab == 1 {
            self.watch_decision_history(cx);
        } else {
            self.decision._poll = None;
        }
        cx.notify();
    }

    /// Reads the history now and every 10s while its tab shows.
    fn watch_decision_history(&mut self, cx: &mut Context<Self>) {
        let api = self.general.api();
        self.decision.history_loading = true;
        self.decision._poll = Some(cx.spawn(async move |this, cx| {
            loop {
                let reader = api.clone();
                let read = cx
                    .background_executor()
                    .spawn(async move { reader.map(|api| api.decision_provider_history(100)) })
                    .await;
                let alive = this
                    .update(cx, |settings, cx| {
                        let decision = &mut settings.decision;
                        decision.history_loading = false;
                        match read {
                            Some(Ok(entries)) => decision.history = Some(entries),
                            // Unread (no desktop): empty, like the desktop.
                            _ if decision.history.is_none() => decision.history = Some(Vec::new()),
                            _ => {}
                        }
                        cx.notify();
                        settings.active == SectionId::DecisionProvider && settings.decision.tab == 1
                    })
                    .unwrap_or(false);
                if !alive || cfg!(test) {
                    break;
                }
                cx.background_executor().timer(HISTORY_POLL).await;
            }
        }));
    }

    pub(super) fn refresh_decision_history(&mut self, cx: &mut Context<Self>) {
        self.watch_decision_history(cx);
        cx.notify();
    }

    pub(super) fn decision_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let tabs = Tabs::new(
            "settings-decision-tab",
            vec![
                cx.t("settings.decisionProvider.tabs.configuration"),
                cx.t("settings.decisionProvider.tabs.history"),
            ],
            self.decision.tab,
        )
        .on_select({
            let this = cx.entity().downgrade();
            move |tab, _, cx| {
                let _ = this.update(cx, |settings, cx| settings.select_decision_tab(tab, cx));
            }
        });
        let content = if self.decision.tab == 0 {
            self.decision_configuration(cx)
        } else {
            self.decision_history(cx)
        };
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(div().w_full().child(tabs))
            .child(content)
            .into_any_element()
    }
}
