//! "Editar …" (`AgentEditorDialog`): what the person can change on a DCC
//! agent — its name, its mascot (color, arms, eyes), what it runs on
//! (provider → model → effort in one menu), extra instructions, whether the
//! Reviewer offers itself, and the Reporter's cleanup alert. The role is
//! DCC's.

use std::path::PathBuf;

use dcc_core::domain::agent::{
    AGENT_AVATAR_COLORS, AGENT_AVATAR_EYES, AGENT_AVATAR_MAX_ARMS, AGENT_AVATAR_MIN_ARMS,
    AgentAvatar, CHRONICLER_PRESET, REVIEWER_PRESET,
};
use dcc_core::domain::provider::ProviderDescriptor;
use gpui::{
    AnyElement, AppContext, Context, Entity, EventEmitter, FontWeight, IntoElement, Render,
    SharedString, Subscription, Window, div, prelude::*, px,
};
use serde_json::json;

use crate::agent::AgentAvatarView;
use crate::data::agents::AgentRecord;
use crate::data::cleanup_alert::{CleanupAlert, clamp_threshold};
use crate::data::desktop_api::{ApiError, DesktopApi, encode};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Dialog, Switch, TextArea, TextAreaEvent, Toaster,
    dialog_footer, dialog_header, spinner,
};

pub enum EditorEvent {
    Closed,
    /// Saved: the agents are read again.
    Saved,
}

impl DesktopApi {
    /// `agents_save`
    pub fn save_agent(&self, id: &str, draft: &serde_json::Value) -> Result<(), ApiError> {
        self.post(&format!("/api/v1/agents/{}", encode(id)), draft)
            .map(|_| ())
    }
}

pub struct AgentEditor {
    pub(crate) agent: AgentRecord,
    pub(crate) name: Entity<TextInput>,
    pub(crate) extra: Entity<TextArea>,
    pub(crate) avatar: AgentAvatar,
    pub(crate) provider_id: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) offer: bool,
    pub(crate) cleanup_enabled: bool,
    pub(crate) threshold: Entity<TextInput>,
    cleanup_path: PathBuf,
    pub(super) catalog: Vec<ProviderDescriptor>,
    pub(crate) saving: bool,
    pub(crate) api: Option<DesktopApi>,
    _inputs: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for AgentEditor {}

fn label(text: SharedString, cx: &gpui::App) -> AnyElement {
    div()
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().colors.foreground)
        .child(text)
        .into_any_element()
}

fn hint(text: SharedString, cx: &gpui::App) -> AnyElement {
    div()
        .text_size(px(11.0))
        .line_height(px(17.9))
        .text_color(cx.theme().colors.muted_foreground)
        .child(text)
        .into_any_element()
}

/// `Field`: the label, the control, the hint.
fn field(
    label_text: SharedString,
    hint_text: Option<SharedString>,
    control: AnyElement,
    cx: &gpui::App,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(label(label_text, cx))
        .child(control)
        .children(hint_text.map(|text| hint(text, cx)))
        .into_any_element()
}

fn boxed(input: AnyElement, cx: &gpui::App) -> gpui::Div {
    let colors = &cx.theme().colors;
    div()
        .w_full()
        .rounded(px(8.0))
        .border_1()
        .border_color(colors.input)
        .px(px(10.0))
        .text_size(px(14.0))
        .child(input)
}

impl AgentEditor {
    pub fn new(
        agent: AgentRecord,
        catalog: Vec<ProviderDescriptor>,
        cleanup_path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| TextInput::new(window, cx).with_max_len(60));
        name.update(cx, |input, cx| input.set_text(agent.name.clone(), cx));
        let extra = cx.new(|cx| TextArea::new(window, cx));
        extra.update(cx, |area, cx| area.set_text(&agent.extra_instructions, cx));
        let alert = if cfg!(test) && !cleanup_path.exists() {
            CleanupAlert::default()
        } else {
            CleanupAlert::load(&cleanup_path)
        };
        let threshold = cx.new(|cx| TextInput::new(window, cx).with_max_len(4));
        threshold.update(cx, |input, cx| {
            input.set_text(alert.threshold_gb.to_string(), cx)
        });
        let inputs = vec![
            cx.subscribe(&name, |_, _, event, cx| {
                if matches!(event, TextInputEvent::Changed) {
                    cx.notify();
                }
            }),
            cx.subscribe(&extra, |_, area, event, cx| match event {
                // Instructions run over lines: Enter is a new one.
                TextAreaEvent::Submitted => area.update(cx, |area, cx| area.insert("\n", cx)),
                TextAreaEvent::Changed => cx.notify(),
                _ => {}
            }),
        ];
        Self {
            avatar: agent.avatar.clone(),
            provider_id: agent.provider_id.clone(),
            model: agent.model.clone(),
            effort: agent.effort.clone(),
            offer: !agent.offer_prompt.trim().is_empty(),
            cleanup_enabled: alert.enabled,
            agent,
            name,
            extra,
            threshold,
            cleanup_path,
            catalog,
            saving: false,
            api: if cfg!(test) { None } else { DesktopApi::load() },
            _inputs: inputs,
        }
    }

    /// `ResidentAgentDraft`: the role and the call stay as stored.
    pub(crate) fn draft(&self, cx: &gpui::App) -> serde_json::Value {
        let offer = if self.offer {
            if self.agent.offer_prompt.trim().is_empty() {
                cx.t("agents.presets.reviewerOffer").to_string()
            } else {
                self.agent.offer_prompt.clone()
            }
        } else {
            String::new()
        };
        let extra: String = self.extra.read(cx).text().chars().take(2000).collect();
        json!({
            "name": self.name.read(cx).text().trim(),
            "role": self.agent.role,
            "kickoffPrompt": self.agent.kickoff_prompt,
            "offerPrompt": offer,
            "extraInstructions": extra,
            "providerId": self.provider_id,
            "model": self.model,
            "effort": self.effort,
            "avatar": self.avatar,
        })
    }

    pub(crate) fn save(&mut self, cx: &mut Context<Self>) {
        if self.saving || self.name.read(cx).text().trim().is_empty() {
            return;
        }
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        let (id, draft) = (self.agent.id.clone(), self.draft(cx));
        let chronicler = self.agent.preset.as_deref() == Some(CHRONICLER_PRESET);
        let cleanup = (
            self.cleanup_enabled,
            clamp_threshold(self.threshold.read(cx).text()),
        );
        let path = self.cleanup_path.clone();
        self.saving = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move { api.save_agent(&id, &draft) })
                .await;
            let _ = this.update(cx, |editor, cx| {
                editor.saving = false;
                match saved {
                    Ok(()) => {
                        if chronicler {
                            let _ = CleanupAlert::save_settings(&path, cleanup.0, cleanup.1);
                        }
                        cx.emit(EditorEvent::Saved);
                        cx.emit(EditorEvent::Closed);
                    }
                    Err(error) => Toaster::error(error.to_string(), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn set_execution(
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

    /// `Choice`: a 44px tile, its ring when chosen.
    fn choice(
        &self,
        id: String,
        selected: bool,
        avatar: AgentAvatar,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let chosen = avatar.clone();
        div()
            .id(SharedString::from(id.clone()))
            .debug_selector(move || id.clone())
            .flex()
            .size(px(44.0))
            .items_center()
            .justify_center()
            .rounded(px(8.0))
            .border_1()
            .cursor_pointer()
            .map(|tile| {
                if selected {
                    tile.border_color(colors.ring).bg(colors.accent)
                } else {
                    tile.border_color(colors.border.opacity(0.7))
                        .hover(|tile| tile.bg(colors.accent.opacity(0.5)))
                }
            })
            .on_click(cx.listener(move |editor, _, _, cx| {
                editor.avatar = chosen.clone();
                cx.notify();
            }))
            .child(AgentAvatarView::new(
                SharedString::from(format!("{}-{}-{}", avatar.color, avatar.arms, avatar.eyes)),
                &avatar,
                None,
                28.0,
            ))
            .into_any_element()
    }

    fn switch_row(
        &self,
        id: &'static str,
        title: SharedString,
        text: SharedString,
        on: bool,
        set: fn(&mut AgentEditor, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let this = cx.entity().downgrade();
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(label(title, cx))
                    .child(hint(text, cx)),
            )
            .child(Switch::new(id, on).on_toggle(move |on, _, cx| {
                let _ = this.update(cx, |editor, cx| {
                    set(editor, on);
                    cx.notify();
                });
            }))
            .into_any_element()
    }
}

impl Render for AgentEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let colors_row: Vec<AnyElement> = AGENT_AVATAR_COLORS
            .iter()
            .map(|color| {
                let avatar = AgentAvatar {
                    color: (*color).into(),
                    ..self.avatar.clone()
                };
                self.choice(
                    format!("agent-editor-color-{color}"),
                    self.avatar.color == *color,
                    avatar,
                    cx,
                )
            })
            .collect();
        let mut shapes: Vec<AnyElement> = (AGENT_AVATAR_MIN_ARMS..=AGENT_AVATAR_MAX_ARMS)
            .map(|arms| {
                let avatar = AgentAvatar {
                    arms,
                    ..self.avatar.clone()
                };
                self.choice(
                    format!("agent-editor-arms-{arms}"),
                    self.avatar.arms == arms,
                    avatar,
                    cx,
                )
            })
            .collect();
        for eyes in AGENT_AVATAR_EYES {
            let avatar = AgentAvatar {
                eyes: (*eyes).into(),
                ..self.avatar.clone()
            };
            let selected = self.avatar.eyes == *eyes;
            shapes.push(self.choice(format!("agent-editor-eyes-{eyes}"), selected, avatar, cx));
        }
        let preset = self.agent.preset.clone();
        let name_empty = self.name.read(cx).text().trim().is_empty();
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.0))
                    .child(AgentAvatarView::new(
                        "agent-editor-avatar",
                        &self.avatar,
                        None,
                        72.0,
                    ))
                    .child(
                        div().flex_1().min_w_0().child(field(
                            cx.t("agents.editor.name"),
                            None,
                            boxed(self.name.clone().into_any_element(), cx)
                                .debug_selector(|| "agent-editor-name".into())
                                .h(px(32.0))
                                .flex()
                                .items_center()
                                .into_any_element(),
                            cx,
                        )),
                    ),
            )
            .child(field(
                cx.t("agents.editor.avatar"),
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(div().flex().flex_wrap().gap(px(6.0)).children(colors_row))
                    .child(div().flex().flex_wrap().gap(px(6.0)).children(shapes))
                    .into_any_element(),
                cx,
            ))
            .child(field(
                cx.t("agents.editor.runsOn"),
                Some(cx.t("agents.editor.runsOnHint")),
                self.picker(cx),
                cx,
            ))
            .child(field(
                cx.t("agents.editor.extra"),
                Some(cx.t("agents.editor.extraHint")),
                boxed(self.extra.clone().into_any_element(), cx)
                    .debug_selector(|| "agent-editor-extra".into())
                    .min_h(px(96.0))
                    .py(px(8.0))
                    .into_any_element(),
                cx,
            ));
        // Only the Reviewer offers itself; the Researcher is always called.
        if preset.as_deref() == Some(REVIEWER_PRESET) {
            body = body.child(self.switch_row(
                "agent-editor-offer",
                cx.t("agents.editor.offerSwitch"),
                cx.t("agents.editor.offerSwitchHint"),
                self.offer,
                |editor, on| editor.offer = on,
                cx,
            ));
        }
        if preset.as_deref() == Some(CHRONICLER_PRESET) {
            body = body
                .child(self.switch_row(
                    "agent-editor-cleanup",
                    cx.t("agents.editor.cleanupSwitch"),
                    cx.t("agents.editor.cleanupSwitchHint"),
                    self.cleanup_enabled,
                    |editor, on| editor.cleanup_enabled = on,
                    cx,
                ))
                .when(self.cleanup_enabled, |body| {
                    body.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(12.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("agents.editor.cleanupThreshold"))
                            .child(
                                boxed(self.threshold.clone().into_any_element(), cx)
                                    .debug_selector(|| "agent-editor-threshold".into())
                                    .w(px(80.0))
                                    .h(px(32.0))
                                    .flex()
                                    .items_center()
                                    .font_features(crate::ui::tabular_nums()),
                            )
                            .child("GB"),
                    )
                });
        }
        let close = cx.entity().downgrade();
        let cancel = close.clone();
        let saving = self.saving;
        Dialog::new("agent-editor", move |_, cx| {
            let _ = close.update(cx, |_, cx| cx.emit(EditorEvent::Closed));
        })
        .max_width(576.0)
        .max_height(720.0)
        .child(
            div()
                .debug_selector(|| "agent-editor".into())
                .flex()
                .flex_col()
                .gap(px(16.0))
                .child(dialog_header(
                    cx.t_with(
                        "agents.editor.title",
                        &Args::default().with("agent", self.agent.name.clone()),
                    ),
                    Some(cx.t("agents.editor.description")),
                    cx,
                ))
                .child(body)
                .child(dialog_footer(
                    [
                        Button::new("agent-editor-cancel")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Default)
                            .text_size(14.0)
                            .on_click(move |_, _, cx| {
                                let _ = cancel.update(cx, |_, cx| cx.emit(EditorEvent::Closed));
                            })
                            .child(cx.t("agents.editor.cancel"))
                            .into_any_element(),
                        Button::new("agent-editor-save")
                            .size(ButtonSize::Default)
                            .text_size(14.0)
                            .disabled(name_empty || saving)
                            .on_click(cx.listener(|editor, _, _, cx| editor.save(cx)))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .when(saving, |label| {
                                        label.child(spinner("agent-editor-saving", 16.0))
                                    })
                                    .child(cx.t("agents.editor.save")),
                            )
                            .into_any_element(),
                    ],
                    cx,
                )),
        )
    }
}
