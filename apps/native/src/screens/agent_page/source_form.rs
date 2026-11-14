//! "Nova fonte" on an agent's page: the person makes a source directly,
//! with no proposal to accept. A connector the agent may read, one of its
//! read-only tools, the arguments that tool takes as fields
//! (`data::agent_source_form`), the source's name, how often it is read and
//! the room its news goes to. The sparkle beside the name asks the agent
//! to pick the tool and fill the arguments for what was written ("menções
//! a mim") — one model run, only when clicked; "Testar leitura" reads once
//! with what the form holds and shows what came, before anything is
//! stored.

use dcc_core::domain::agent_source::{DEFAULT_EVERY, EVERY_CHOICES};
use gpui::{
    AnyElement, AppContext, Context, Entity, FontWeight, SharedString, Subscription, Window, div,
    prelude::*, px,
};
use serde_json::{Map, Value, json};

use super::AgentPage;
use crate::data::agent_source_form::{
    BadField, Field, FieldKind, FormTool, SourceTest, Typed, arguments, fields, typed_from,
};
use crate::data::agent_sources::{Sources, read};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Switch, Toaster, ToggleGroup, Tooltip};

/// One argument of the picked tool, as the form holds it.
pub(crate) struct FieldInput {
    pub(crate) field: Field,
    pub(crate) input: Entity<TextInput>,
    /// A switch's state; none: never touched.
    pub(crate) flag: Option<bool>,
    _changed: Subscription,
}

/// The connector's tools, as far as they were read.
pub(crate) enum Tools {
    Loading,
    Ready(Vec<FormTool>),
    Failed(String),
}

/// What the form waits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Busy {
    Filling,
    Testing,
    Creating,
}

/// "Nova fonte", while open.
pub(crate) struct SourceForm {
    /// The connectors the agent may read; none: still asked.
    pub(crate) connectors: Option<Vec<(String, String)>>,
    pub(crate) connector: usize,
    pub(crate) tools: Tools,
    pub(crate) tool: usize,
    pub(crate) fields: Vec<FieldInput>,
    /// Arguments the agent picked that the tool's schema does not list.
    extra: Map<String, Value>,
    pub(crate) name: Entity<TextInput>,
    pub(crate) every: u32,
    pub(crate) room: Option<String>,
    /// The optional arguments show.
    more: bool,
    pub(crate) busy: Option<Busy>,
    pub(crate) test: Option<Result<SourceTest, String>>,
    /// What the agent said of its pick.
    note: String,
    /// Bumped when the connector changes: an answer for the one before is
    /// dropped.
    turn: u64,
    /// The source being changed ("Editar"); none: a new one. Its connector,
    /// tool and arguments fill the form as each is read.
    pub(crate) editing: Option<dcc_core::domain::agent_source::AgentSource>,
    _name_changed: Subscription,
}

impl SourceForm {
    fn connector_id(&self) -> Option<&str> {
        self.connectors
            .as_ref()?
            .get(self.connector)
            .map(|(id, _)| id.as_str())
    }

    fn picked_tool(&self) -> Option<&FormTool> {
        match &self.tools {
            Tools::Ready(tools) => tools.get(self.tool),
            _ => None,
        }
    }

    fn typed(&self, cx: &gpui::App) -> Vec<Typed> {
        self.fields
            .iter()
            .map(|field| Typed {
                text: field.input.read(cx).text().to_string(),
                flag: field.flag,
            })
            .collect()
    }

    /// The tool's arguments as typed, or the field that is wrong.
    fn arguments(&self, cx: &gpui::App) -> Result<Value, BadField> {
        let known: Vec<Field> = self
            .fields
            .iter()
            .map(|field| field.field.clone())
            .collect();
        arguments(&known, &self.typed(cx), &self.extra)
    }
}

/// The tool a connector's form opens on: one that searches or lists, else
/// the first.
fn first_tool(tools: &[FormTool]) -> usize {
    tools
        .iter()
        .position(|tool| tool.name.contains("search") || tool.name.contains("list"))
        .unwrap_or(0)
}

impl AgentPage {
    /// "Editar" on a source: the form on it, saved over it.
    pub(super) fn edit_source(
        &mut self,
        source: &dcc_core::domain::agent_source::AgentSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_source_form(window, cx);
        if let Some(form) = &mut self.source_form {
            let watch = source.watch.clone();
            form.name.update(cx, |input, cx| input.set_text(watch, cx));
            form.every = source.every_minutes;
            form.room = source.room.clone();
            form.editing = Some(source.clone());
        }
    }

    pub(super) fn open_source_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let placeholder = cx.t("agents.sources.form.namePlaceholder");
        let name = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder(placeholder)
                .with_max_len(120)
        });
        let changed = cx.subscribe(&name, |_, _, _: &TextInputEvent, cx| cx.notify());
        self.source_form = Some(SourceForm {
            connectors: None,
            connector: 0,
            tools: Tools::Loading,
            tool: 0,
            fields: Vec::new(),
            extra: Map::new(),
            name,
            every: DEFAULT_EVERY,
            room: None,
            more: false,
            busy: None,
            test: None,
            note: String::new(),
            turn: 0,
            editing: None,
            _name_changed: changed,
        });
        cx.notify();
        let Some(api) = self.api.clone() else {
            return;
        };
        let agent = self.agent_id.clone();
        cx.spawn_in(window, async move |page, cx| {
            let found = cx
                .background_executor()
                .spawn(async move { api.source_connectors(&agent) })
                .await;
            let _ = page.update_in(cx, |page, window, cx| match found {
                Ok(connectors) => page.take_source_connectors(connectors, window, cx),
                Err(error) => {
                    page.source_form = None;
                    Toaster::error(error.to_string(), cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The connectors the agent may read; the first one's tools are asked.
    pub(crate) fn take_source_connectors(
        &mut self,
        connectors: Vec<(String, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = &mut self.source_form else {
            return;
        };
        let any = !connectors.is_empty();
        // A source being edited opens on its own connector.
        let first = form
            .editing
            .as_ref()
            .and_then(|source| {
                connectors
                    .iter()
                    .position(|(id, _)| *id == source.connector_id)
            })
            .unwrap_or(0);
        form.connectors = Some(connectors);
        cx.notify();
        if any {
            self.pick_source_connector(first, window, cx);
        }
    }

    fn pick_source_connector(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = &mut self.source_form else {
            return;
        };
        form.connector = index;
        form.tools = Tools::Loading;
        form.tool = 0;
        form.fields.clear();
        form.extra.clear();
        form.test = None;
        form.note.clear();
        form.turn += 1;
        let turn = form.turn;
        cx.notify();
        let (Some(api), Some(connector)) =
            (self.api.clone(), form.connector_id().map(str::to_string))
        else {
            return;
        };
        let agent = self.agent_id.clone();
        cx.spawn_in(window, async move |page, cx| {
            let found = cx
                .background_executor()
                .spawn(async move { api.source_tools(&agent, &connector) })
                .await;
            let _ = page.update_in(cx, |page, window, cx| {
                if page
                    .source_form
                    .as_ref()
                    .is_some_and(|form| form.turn == turn)
                {
                    page.take_source_tools(found.map_err(|error| error.to_string()), window, cx);
                }
            });
        })
        .detach();
    }

    /// The connector's read-only tools, or why they could not be read.
    pub(crate) fn take_source_tools(
        &mut self,
        found: Result<Vec<FormTool>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = &mut self.source_form else {
            return;
        };
        match found {
            Ok(tools) => {
                // A source being edited, on its connector: its tool, with
                // the arguments it has.
                let edited = form
                    .editing
                    .as_ref()
                    .filter(|source| form.connector_id() == Some(source.connector_id.as_str()))
                    .and_then(|source| {
                        let index = tools.iter().position(|tool| tool.name == source.tool)?;
                        let arguments: Value = serde_json::from_str(&source.arguments).ok()?;
                        Some((index, arguments))
                    });
                let first = first_tool(&tools);
                form.tools = Tools::Ready(tools);
                match edited {
                    Some((index, arguments)) => {
                        self.pick_source_tool(index, Some(&arguments), window, cx)
                    }
                    None => self.pick_source_tool(first, None, window, cx),
                }
            }
            Err(error) => {
                form.tools = Tools::Failed(error);
                cx.notify();
            }
        }
    }

    /// Shows `index`'s arguments as fields, filled with `picked` when the
    /// agent chose them.
    fn pick_source_tool(
        &mut self,
        index: usize,
        picked: Option<&Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = &mut self.source_form else {
            return;
        };
        form.tool = index;
        form.test = None;
        let known = form
            .picked_tool()
            .map(|tool| fields(&tool.input_schema))
            .unwrap_or_default();
        let (typed, extra) = match picked {
            Some(picked) => typed_from(&known, picked),
            None => (vec![Typed::default(); known.len()], Map::new()),
        };
        let inputs: Vec<FieldInput> = known
            .into_iter()
            .zip(typed)
            .map(|(field, typed)| {
                let input = cx.new(|cx| TextInput::new(window, cx));
                if !typed.text.is_empty() {
                    input.update(cx, |input, cx| input.set_text(typed.text, cx));
                }
                // What was read no longer says anything of what is typed now.
                let changed = cx.subscribe(&input, |page, _, event: &TextInputEvent, cx| {
                    if matches!(event, TextInputEvent::Changed)
                        && let Some(form) = &mut page.source_form
                    {
                        form.test = None;
                        cx.notify();
                    }
                });
                FieldInput {
                    field,
                    input,
                    flag: typed.flag,
                    _changed: changed,
                }
            })
            .collect();
        if let Some(form) = &mut self.source_form {
            form.fields = inputs;
            form.extra = extra;
            cx.notify();
        }
    }

    /// The sparkle: the agent picks the tool and its arguments for the name.
    fn fill_source_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = &mut self.source_form else {
            return;
        };
        let watch = form.name.read(cx).text().trim().to_string();
        if watch.is_empty() {
            Toaster::error(cx.t("agents.sources.form.needName"), cx);
            return;
        }
        let (Some(api), Some(connector)) =
            (self.api.clone(), form.connector_id().map(str::to_string))
        else {
            return;
        };
        form.busy = Some(Busy::Filling);
        form.note.clear();
        let turn = form.turn;
        cx.notify();
        let agent = self.agent_id.clone();
        cx.spawn_in(window, async move |page, cx| {
            let picked = cx
                .background_executor()
                .spawn(async move { api.source_suggestion(&agent, &connector, &watch) })
                .await;
            let _ = page.update_in(cx, |page, window, cx| {
                let Some(form) = page.source_form.as_mut().filter(|form| form.turn == turn) else {
                    return;
                };
                form.busy = None;
                match picked {
                    Ok(pick) => {
                        let index = pick
                            .tools
                            .iter()
                            .position(|tool| tool.name == pick.tool)
                            .unwrap_or(0);
                        form.tools = Tools::Ready(pick.tools);
                        form.note = pick.note;
                        page.pick_source_tool(index, Some(&pick.arguments), window, cx);
                    }
                    Err(error) => {
                        Toaster::error(error.to_string(), cx);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// What the form holds, as the desktop takes it; an error is said.
    fn source_form_input(&self, named: bool, cx: &mut Context<Self>) -> Option<Value> {
        let form = self.source_form.as_ref()?;
        let connector = form.connector_id()?.to_string();
        let tool = form.picked_tool()?.name.clone();
        let watch = form.name.read(cx).text().trim().to_string();
        if named && watch.is_empty() {
            Toaster::error(cx.t("agents.sources.form.needName"), cx);
            return None;
        }
        let arguments = match form.arguments(cx) {
            Ok(arguments) => arguments,
            Err(bad) => {
                let (key, field) = match bad {
                    BadField::Missing(field) => ("agents.sources.form.missing", field),
                    BadField::NotANumber(field) => ("agents.sources.form.notNumber", field),
                    BadField::NotJson(field) => ("agents.sources.form.notJson", field),
                };
                Toaster::error(cx.t_with(key, &Args::default().with("field", field)), cx);
                return None;
            }
        };
        Some(json!({
            "connectorId": connector,
            "tool": tool,
            "arguments": arguments,
            "watch": watch,
            "everyMinutes": form.every,
            "room": form.room,
        }))
    }

    /// "Testar leitura": one read with what the form holds.
    fn test_source_form(&mut self, cx: &mut Context<Self>) {
        let (Some(input), Some(api)) = (self.source_form_input(false, cx), self.api.clone()) else {
            return;
        };
        let Some(form) = &mut self.source_form else {
            return;
        };
        form.busy = Some(Busy::Testing);
        form.test = None;
        let turn = form.turn;
        cx.notify();
        let agent = self.agent_id.clone();
        cx.spawn(async move |page, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    api.source_test(
                        &agent,
                        input["connectorId"].as_str().unwrap_or_default(),
                        input["tool"].as_str().unwrap_or_default(),
                        &input["arguments"],
                    )
                })
                .await;
            let _ = page.update(cx, |page, cx| {
                if let Some(form) = page.source_form.as_mut().filter(|form| form.turn == turn) {
                    form.busy = None;
                    form.test = Some(read.map_err(|error| error.to_string()));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// "Criar fonte": stored and on; the list is read again.
    fn create_source_form(&mut self, cx: &mut Context<Self>) {
        let (Some(input), Some(api)) = (self.source_form_input(true, cx), self.api.clone()) else {
            return;
        };
        let Some(form) = &mut self.source_form else {
            return;
        };
        form.busy = Some(Busy::Creating);
        let editing = form.editing.as_ref().map(|source| source.id.clone());
        cx.notify();
        let agent = self.agent_id.clone();
        cx.spawn(async move |page, cx| {
            let made = cx
                .background_executor()
                .spawn(async move {
                    match editing {
                        Some(id) => api.update_source(&agent, &id, &input),
                        None => api.create_source(&agent, &input).map(|_| ()),
                    }
                    .map(|()| read())
                })
                .await;
            let _ = page.update(cx, |page, cx| {
                match made {
                    Ok(list) => {
                        cx.set_global(Sources { list });
                        page.source_form = None;
                    }
                    Err(error) => {
                        if let Some(form) = &mut page.source_form {
                            form.busy = None;
                        }
                        Toaster::error(error.to_string(), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn label(text: impl Into<SharedString>, required: bool, cx: &gpui::App) -> gpui::Div {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .gap(px(3.0))
        .text_size(px(11.0))
        .text_color(colors.muted_foreground)
        .child(text.into())
        .when(required, |label| {
            label.child(div().text_color(colors.destructive).child("*"))
        })
}

fn boxed(input: Entity<TextInput>, cx: &gpui::App) -> gpui::Div {
    let colors = &cx.theme().colors;
    div()
        .h(px(30.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .rounded(px(8.0))
        .border_1()
        .border_color(colors.input)
        .bg(crate::ui::input_fill(cx.theme()))
        .text_size(px(13.0))
        .child(input)
}

fn hint(text: impl Into<SharedString>, cx: &gpui::App) -> gpui::Div {
    div()
        .text_size(px(11.0))
        .line_height(px(16.0))
        .text_color(cx.theme().colors.muted_foreground)
        .child(text.into())
}

impl AgentPage {
    /// A row of choices, one picked.
    fn source_chips(
        &self,
        prefix: &'static str,
        labels: Vec<SharedString>,
        picked: usize,
        pick: fn(&mut Self, usize, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        div()
            .flex()
            .flex_wrap()
            .gap(px(6.0))
            .children(labels.into_iter().enumerate().map(|(index, label)| {
                let on = index == picked;
                div()
                    .id(SharedString::from(format!("{prefix}-{index}")))
                    .debug_selector(move || format!("{prefix}-{index}"))
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded_full()
                    .border_1()
                    .border_color(colors.border)
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .when(on, |chip| {
                        chip.bg(colors.accent).text_color(colors.foreground)
                    })
                    .when(!on, |chip| chip.text_color(colors.muted_foreground))
                    .on_click(cx.listener(move |page, _, window, cx| pick(page, index, window, cx)))
                    .child(label)
            }))
            .into_any_element()
    }

    fn source_field(&self, index: usize, field: &FieldInput, cx: &mut Context<Self>) -> AnyElement {
        let name = field.field.name.clone();
        let described = (!field.field.hint.is_empty()).then(|| hint(field.field.hint.clone(), cx));
        let control = match field.field.kind {
            FieldKind::Flag => Switch::new(
                SharedString::from(format!("source-field-switch-{index}")),
                field.flag.unwrap_or(false),
            )
            .on_toggle({
                let page = cx.entity().downgrade();
                move |on, _, cx| {
                    let _ = page.update(cx, |page, cx| {
                        if let Some(form) = &mut page.source_form
                            && let Some(field) = form.fields.get_mut(index)
                        {
                            field.flag = Some(on);
                            form.test = None;
                            cx.notify();
                        }
                    });
                }
            })
            .into_any_element(),
            _ => boxed(field.input.clone(), cx).into_any_element(),
        };
        div()
            .debug_selector(move || format!("source-field-{name}"))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(label(field.field.name.clone(), field.field.required, cx))
            .child(control)
            .children(described)
            .into_any_element()
    }

    /// The outcome of "Testar leitura".
    fn source_test_box(
        &self,
        test: &Result<SourceTest, String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let (tone, title, lines): (gpui::Hsla, SharedString, Vec<String>) = match test {
            Ok(read) if read.count == 0 => (
                colors.muted_foreground,
                cx.t("agents.sources.form.testEmpty"),
                Vec::new(),
            ),
            Ok(read) => (
                colors.success,
                cx.t_with(
                    "agents.sources.form.testOk",
                    &Args::count(read.count as i64),
                ),
                read.items.clone(),
            ),
            Err(error) => (
                colors.destructive,
                cx.t("agents.sources.form.testFailed"),
                vec![error.clone()],
            ),
        };
        div()
            .debug_selector(|| "source-form-test-result".into())
            .flex()
            .flex_col()
            .gap(px(4.0))
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(tone.opacity(0.3))
            .bg(tone.opacity(0.08))
            .child(
                div()
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(tone)
                    .child(title),
            )
            .children(lines.into_iter().map(|line| {
                div()
                    .truncate()
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(line.replace('\n', " "))
            }))
            .into_any_element()
    }

    /// The form, under the agent's sources.
    pub(super) fn source_form_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let form = self.source_form.as_ref()?;
        let colors = cx.theme().colors.clone();
        let shell = |body: Vec<AnyElement>| {
            div()
                .debug_selector(|| "agent-source-form".into())
                .flex()
                .flex_col()
                .gap(px(12.0))
                .p(px(12.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(colors.border.opacity(0.7))
                .children(body)
                .into_any_element()
        };
        let cancel = Button::new("agent-source-cancel")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
            .on_click(cx.listener(|page, _, _, cx| {
                page.source_form = None;
                cx.notify();
            }))
            .child(cx.t("agents.sources.form.cancel"));
        let Some(connectors) = &form.connectors else {
            return Some(shell(vec![
                hint(cx.t("agents.sources.form.loading"), cx).into_any_element(),
            ]));
        };
        if connectors.is_empty() {
            return Some(shell(vec![
                hint(cx.t("agents.sources.form.noConnectors"), cx).into_any_element(),
                div().flex().justify_end().child(cancel).into_any_element(),
            ]));
        }
        let mut body: Vec<AnyElement> = Vec::new();
        // The name first: it is what the sparkle reads.
        let filling = form.busy == Some(Busy::Filling);
        body.push(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(label(cx.t("agents.sources.form.name"), true, cx))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            boxed(form.name.clone(), cx)
                                .debug_selector(|| "source-form-name".into())
                                .flex_1(),
                        )
                        .child(Tooltip::text(
                            "source-form-fill-tip",
                            Button::new("source-form-fill")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Sm)
                                .disabled(form.busy.is_some())
                                .on_click(cx.listener(|page, _, window, cx| {
                                    page.fill_source_form(window, cx)
                                }))
                                .child(Icon::new("sparkles").size(14.0))
                                .child(cx.t(if filling {
                                    "agents.sources.form.filling"
                                } else {
                                    "agents.sources.form.fill"
                                })),
                            cx.t("agents.sources.form.fillTip"),
                        )),
                )
                .into_any_element(),
        );
        body.push(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(label(cx.t("agents.sources.form.connector"), false, cx))
                .child(
                    self.source_chips(
                        "source-connector",
                        connectors
                            .iter()
                            .map(|(_, name)| SharedString::from(name.clone()))
                            .collect(),
                        form.connector,
                        Self::pick_source_connector,
                        cx,
                    ),
                )
                .into_any_element(),
        );
        match &form.tools {
            Tools::Loading => {
                body.push(hint(cx.t("agents.sources.form.loadingTools"), cx).into_any_element())
            }
            Tools::Failed(error) => body.push(
                div()
                    .debug_selector(|| "source-form-tools-failed".into())
                    .text_size(px(12.0))
                    .text_color(colors.destructive)
                    .child(error.clone())
                    .into_any_element(),
            ),
            Tools::Ready(tools) if tools.is_empty() => {
                body.push(hint(cx.t("agents.sources.form.noTools"), cx).into_any_element())
            }
            Tools::Ready(tools) => {
                let described = tools
                    .get(form.tool)
                    .map(|tool| tool.description.trim().replace('\n', " "))
                    .filter(|text| !text.is_empty())
                    .map(|text| {
                        let short: String = text.chars().take(240).collect();
                        hint(short, cx)
                    });
                body.push(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(label(cx.t("agents.sources.form.tool"), false, cx))
                        .child(
                            self.source_chips(
                                "source-tool",
                                tools
                                    .iter()
                                    .map(|tool| SharedString::from(tool.name.clone()))
                                    .collect(),
                                form.tool,
                                |page, index, window, cx| {
                                    page.pick_source_tool(index, None, window, cx)
                                },
                                cx,
                            ),
                        )
                        .children(described)
                        .into_any_element(),
                );
                if !form.note.is_empty() {
                    body.push(
                        div()
                            .debug_selector(|| "source-form-note".into())
                            .flex()
                            .items_start()
                            .gap(px(6.0))
                            .text_size(px(12.0))
                            .line_height(px(18.0))
                            .text_color(colors.foreground)
                            .child(
                                Icon::new("sparkles")
                                    .size(13.0)
                                    .color(colors.muted_foreground),
                            )
                            .child(form.note.clone())
                            .into_any_element(),
                    );
                }
                // The required arguments, and the optional ones that hold
                // something; the rest under "Mais opções".
                let filled = |field: &FieldInput| {
                    field.flag.is_some() || !field.input.read(cx).text().trim().is_empty()
                };
                let hidden = form
                    .fields
                    .iter()
                    .filter(|field| !field.field.required && !filled(field))
                    .count();
                let shown: Vec<usize> = form
                    .fields
                    .iter()
                    .enumerate()
                    .filter(|(_, field)| form.more || field.field.required || filled(field))
                    .map(|(index, _)| index)
                    .collect();
                for index in shown {
                    body.push(self.source_field(index, &form.fields[index], cx));
                }
                if hidden > 0 || form.more {
                    let text = if form.more {
                        cx.t("agents.sources.form.less")
                    } else {
                        cx.t_with("agents.sources.form.more", &Args::count(hidden as i64))
                    };
                    body.push(
                        div()
                            .id("source-form-more")
                            .debug_selector(|| "source-form-more".into())
                            .text_size(px(12.0))
                            .text_color(colors.primary)
                            .cursor_pointer()
                            .on_click(cx.listener(|page, _, _, cx| {
                                if let Some(form) = &mut page.source_form {
                                    form.more = !form.more;
                                    cx.notify();
                                }
                            }))
                            .child(text)
                            .into_any_element(),
                    );
                }
            }
        }
        // How often, and the room its news goes to.
        let every = EVERY_CHOICES
            .iter()
            .position(|choice| *choice == form.every)
            .unwrap_or(0);
        let page = cx.entity().downgrade();
        let rooms: Vec<(Option<String>, SharedString)> = {
            let sidebar = self.sidebar.read(cx);
            let projects = super::editor_projects::project_options(sidebar.projects());
            std::iter::once((None, cx.t("agents.sources.form.roomGeneral")))
                .chain(self.rooms.iter().map(|room| {
                    let name = projects
                        .iter()
                        .find(|(id, _)| id == room)
                        .map(|(_, name)| name.clone())
                        .unwrap_or_else(|| room.clone());
                    (Some(room.clone()), SharedString::from(name))
                }))
                .collect()
        };
        let room = rooms
            .iter()
            .position(|(id, _)| *id == form.room)
            .unwrap_or(0);
        let room_ids: Vec<Option<String>> = rooms.iter().map(|(id, _)| id.clone()).collect();
        body.push(
            div()
                .flex()
                .flex_wrap()
                .gap(px(16.0))
                .pt(px(12.0))
                .border_t_1()
                .border_color(colors.border.opacity(0.6))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(label(cx.t("agents.sources.form.every"), false, cx))
                        .child(
                            ToggleGroup::new(
                                "source-form-every",
                                EVERY_CHOICES
                                    .iter()
                                    .map(|minutes| SharedString::from(format!("{minutes} min")))
                                    .collect(),
                                Some(every),
                            )
                            .on_select(move |index, _, cx| {
                                let _ = page.update(cx, |page, cx| {
                                    if let Some(form) = &mut page.source_form {
                                        form.every = EVERY_CHOICES[index.min(3)];
                                        cx.notify();
                                    }
                                });
                            }),
                        ),
                )
                .when(rooms.len() > 1, |row| {
                    row.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(label(cx.t("agents.sources.form.room"), false, cx))
                            .child(div().flex().flex_wrap().gap(px(6.0)).children(
                                rooms.iter().enumerate().map(|(index, (_, name))| {
                                    let on = index == room;
                                    let id = room_ids[index].clone();
                                    div()
                                        .id(SharedString::from(format!("source-room-{index}")))
                                        .px(px(10.0))
                                        .py(px(4.0))
                                        .rounded_full()
                                        .border_1()
                                        .border_color(colors.border)
                                        .text_size(px(12.0))
                                        .cursor_pointer()
                                        .when(on, |chip| {
                                            chip.bg(colors.accent).text_color(colors.foreground)
                                        })
                                        .when(!on, |chip| chip.text_color(colors.muted_foreground))
                                        .on_click(cx.listener(move |page, _, _, cx| {
                                            if let Some(form) = &mut page.source_form {
                                                form.room = id.clone();
                                                cx.notify();
                                            }
                                        }))
                                        .child(name.clone())
                                }),
                            )),
                    )
                })
                .into_any_element(),
        );
        if let Some(test) = &form.test {
            body.push(self.source_test_box(test, cx));
        }
        let ready = form.picked_tool().is_some() && form.busy.is_none();
        body.push(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(cancel)
                .child(
                    Button::new("agent-source-test")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .disabled(!ready)
                        .on_click(cx.listener(|page, _, _, cx| page.test_source_form(cx)))
                        .child(cx.t(if form.busy == Some(Busy::Testing) {
                            "agents.sources.form.testing"
                        } else {
                            "agents.sources.form.test"
                        })),
                )
                .child(
                    Button::new("agent-source-create")
                        .size(ButtonSize::Sm)
                        .disabled(!ready)
                        .on_click(cx.listener(|page, _, _, cx| page.create_source_form(cx)))
                        .child(
                            cx.t(match (form.busy == Some(Busy::Creating), &form.editing) {
                                (true, _) => "agents.sources.form.creating",
                                (false, Some(_)) => "agents.sources.form.save",
                                (false, None) => "agents.sources.form.create",
                            }),
                        ),
                )
                .into_any_element(),
        );
        Some(shell(body))
    }
}
