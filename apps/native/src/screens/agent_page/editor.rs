//! "Editar …": a screen of the agent page, with "Voltar" to it, for what
//! the person can change on a DCC agent — its name, its mascot (color, arms,
//! eyes), what it runs on (provider → model → effort in one menu), the
//! projects it works in, extra instructions and whether the Reviewer offers
//! itself. The Reviewer's role is DCC's.

use dcc_core::domain::agent::{AgentAvatar, REVIEWER_PRESET};
use dcc_core::domain::provider::ProviderDescriptor;
use gpui::{
    AnyElement, AppContext, Context, Entity, EventEmitter, FontWeight, IntoElement, Render,
    SharedString, Subscription, Window, div, prelude::*, px,
};
use serde_json::json;

use crate::agent::AgentAvatarView;
use crate::data::agents::AgentRecord;
use crate::data::desktop_api::{ApiError, DesktopApi, encode};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Icon, Switch, TextArea, TextAreaEvent, Toaster,
    dialog_footer, spinner,
};

pub enum EditorEvent {
    Closed,
    /// Saved: the agents are read again.
    Saved,
    /// "Novo agente" saved: its id, for the page to show it.
    Created(String),
}

impl DesktopApi {
    /// `agents_save`
    pub fn save_agent(&self, id: &str, draft: &serde_json::Value) -> Result<(), ApiError> {
        self.post(&format!("/api/v1/agents/{}", encode(id)), draft)
            .map(|_| ())
    }

    /// "Novo agente": the new agent's id.
    pub fn create_agent(&self, draft: &serde_json::Value) -> Result<String, ApiError> {
        let answer = self.post("/api/v1/agents", draft)?;
        answer["agent"]["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| ApiError::Failed("the new agent has no id".to_string()))
    }
}

pub struct AgentEditor {
    pub(crate) agent: AgentRecord,
    pub(crate) name: Entity<TextInput>,
    pub(crate) extra: Entity<TextArea>,
    /// The role and what a call asks first, for an agent of the person's.
    pub(crate) role: Entity<TextArea>,
    pub(crate) kickoff: Entity<TextArea>,
    pub(crate) avatar: AgentAvatar,
    pub(crate) provider_id: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    /// The projects it works in; empty: every project.
    pub(crate) projects: Vec<String>,
    /// Every project to pick from: id and name.
    pub(crate) project_options: Vec<(String, String)>,
    pub(crate) offer: bool,
    /// The template "Novo agente" started from, if any.
    pub(crate) template: Option<&'static str>,
    pub(super) catalog: Vec<ProviderDescriptor>,
    pub(crate) saving: bool,
    pub(crate) api: Option<DesktopApi>,
    _inputs: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for AgentEditor {}

pub(super) fn label(text: SharedString, cx: &gpui::App) -> AnyElement {
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
pub(super) fn field(
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

pub(super) fn boxed(input: AnyElement, cx: &gpui::App) -> gpui::Div {
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| TextInput::new(window, cx).with_max_len(60));
        name.update(cx, |input, cx| input.set_text(agent.name.clone(), cx));
        let extra = cx.new(|cx| TextArea::new(window, cx));
        extra.update(cx, |area, cx| area.set_text(&agent.extra_instructions, cx));
        let role = cx.new(|cx| TextArea::new(window, cx));
        role.update(cx, |area, cx| area.set_text(&agent.role, cx));
        let kickoff = cx.new(|cx| TextArea::new(window, cx));
        kickoff.update(cx, |area, cx| area.set_text(&agent.kickoff_prompt, cx));
        let multiline = |_: &mut Self,
                         area: Entity<TextArea>,
                         event: &TextAreaEvent,
                         cx: &mut Context<Self>| {
            match event {
                // Instructions run over lines: Enter is a new one.
                TextAreaEvent::Submitted => area.update(cx, |area, cx| area.insert("\n", cx)),
                TextAreaEvent::Changed => cx.notify(),
                _ => {}
            }
        };
        let inputs = vec![
            cx.subscribe(&role, multiline),
            cx.subscribe(&kickoff, multiline),
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
            projects: agent.projects.clone(),
            project_options: Vec::new(),
            offer: !agent.offer_prompt.trim().is_empty(),
            template: None,
            agent,
            name,
            extra,
            role,
            kickoff,
            catalog,
            saving: false,
            api: if cfg!(test) { None } else { DesktopApi::load() },
            _inputs: inputs,
        }
    }

    /// The projects the person can pick; a project the agent keeps that is
    /// no longer in DCC still shows, so it can be dropped.
    pub(crate) fn with_projects(mut self, mut options: Vec<(String, String)>) -> Self {
        for project in &self.projects {
            if !options.iter().any(|(id, _)| id == project) {
                options.push((project.clone(), project.clone()));
            }
        }
        self.project_options = options;
        self
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
        // A built-in agent's role is DCC's; the person writes their own's.
        let (role, kickoff) = if self.agent.preset.is_none() {
            (
                self.role.read(cx).text().trim().to_string(),
                self.kickoff.read(cx).text().trim().to_string(),
            )
        } else {
            (self.agent.role.clone(), self.agent.kickoff_prompt.clone())
        };
        json!({
            "name": self.name.read(cx).text().trim(),
            "role": role,
            "kickoffPrompt": kickoff,
            "offerPrompt": offer,
            "extraInstructions": extra,
            "providerId": self.provider_id,
            "model": self.model,
            "effort": self.effort,
            "avatar": self.avatar,
            "projects": self.projects,
        })
    }

    pub(crate) fn save(&mut self, cx: &mut Context<Self>) {
        if self.saving || !self.can_save(cx) {
            return;
        }
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        let (id, draft) = (self.agent.id.clone(), self.draft(cx));
        self.saving = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move {
                    if id.is_empty() {
                        api.create_agent(&draft).map(Some)
                    } else {
                        api.save_agent(&id, &draft).map(|_| None)
                    }
                })
                .await;
            let _ = this.update(cx, |editor, cx| {
                editor.saving = false;
                match saved {
                    Ok(created) => {
                        cx.emit(match created {
                            Some(id) => EditorEvent::Created(id),
                            None => EditorEvent::Saved,
                        });
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
        let preset = self.agent.preset.clone();
        let cannot_save = !self.can_save(cx);
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .children(self.template_row(cx))
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
            .children(self.role_fields(cx))
            .child(field(
                cx.t("agents.editor.avatar"),
                None,
                self.look_rows(cx),
                cx,
            ))
            .child(field(
                cx.t("agents.editor.runsOn"),
                Some(cx.t("agents.editor.runsOnHint")),
                self.picker(cx),
                cx,
            ))
            .child(field(
                cx.t("agents.editor.projects"),
                Some(cx.t("agents.editor.projectsHint")),
                self.project_chips(cx),
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
        // Only the Reviewer offers itself.
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
        let back = cx.entity().downgrade();
        let cancel = back.clone();
        let saving = self.saving;
        let title = if self.is_new() {
            cx.t("agents.editor.newTitle")
        } else {
            cx.t_with(
                "agents.editor.title",
                &Args::default().with("agent", self.agent.name.clone()),
            )
        };
        let back_label = if self.is_new() {
            cx.t("agents.editor.backToTeam")
        } else {
            cx.t_with(
                "agents.editor.back",
                &Args::default().with("agent", self.agent.name.clone()),
            )
        };
        div()
            .debug_selector(|| "agent-editor".into())
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .id("agent-editor-back")
                    .debug_selector(|| "agent-editor-back".into())
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .py(px(4.0))
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .cursor_pointer()
                    .hover(|row| row.text_color(colors.foreground))
                    .on_click(move |_, _, cx| {
                        let _ = back.update(cx, |_, cx| cx.emit(EditorEvent::Closed));
                    })
                    .child(Icon::new("arrow-left").size(14.0))
                    .child(back_label),
            )
            .child(
                div()
                    .debug_selector(|| "agent-editor-heading".into())
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(20.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.foreground)
                            .child(title),
                    )
                    .child(hint(cx.t("agents.editor.description"), cx)),
            )
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
                        .disabled(cannot_save || saving)
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
            ))
    }
}
