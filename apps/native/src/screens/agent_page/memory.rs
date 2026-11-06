//! The agent page's "Memória": the rules and preferences that go with every
//! turn the agent runs. The agent saves one when the person corrects it or
//! states a rule (`dcc_agent_remember`); the person adds, edits and deletes
//! notes here, filters learned from their own, searches, sees how full the
//! memory is, and brings an earlier version back from "Histórico".

use dcc_core::domain::agent_memory::{MAX_MEMORY_CHARS, MAX_NOTE_CHARS};
use gpui::{
    AnyElement, Context, Entity, Render, SharedString, Subscription, Window, div, prelude::*, px,
};

use super::AgentPage;
use crate::data::agent_memory::{AgentMemory, MemoryNote, NoteFilter, visible};
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Icon, TextInput, TextInputEvent, Toaster, ToggleGroup,
    Tooltip,
};

/// The search shows once there are more notes than this.
const SEARCH_FROM: usize = 5;

pub struct MemoryCard {
    pub(crate) agent_id: String,
    pub(crate) api: Option<DesktopApi>,
    /// `None` until the first read answers.
    pub(crate) memory: Option<AgentMemory>,
    pub(crate) filter: NoteFilter,
    pub(crate) search: Entity<TextInput>,
    pub(crate) new_note: Entity<TextInput>,
    /// The note being edited, and its field.
    pub(crate) editing: Option<(String, Entity<TextInput>, Subscription)>,
    pub(crate) history_open: bool,
    pub(crate) busy: bool,
    _inputs: [Subscription; 2],
}

impl MemoryCard {
    pub fn new(
        agent_id: String,
        api: Option<DesktopApi>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_max_len(120)
                .with_placeholder(cx.t("agents.memory.search"))
        });
        let new_note = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_max_len(MAX_NOTE_CHARS)
                .with_placeholder(cx.t("agents.memory.add"))
        });
        let inputs = [
            cx.subscribe(&search, |_, _, _: &TextInputEvent, cx| cx.notify()),
            cx.subscribe_in(&new_note, window, |card, _, event, _, cx| match event {
                TextInputEvent::Submitted => card.add(cx),
                _ => cx.notify(),
            }),
        ];
        let mut card = Self {
            agent_id,
            api,
            memory: None,
            filter: NoteFilter::All,
            search,
            new_note,
            editing: None,
            history_open: false,
            busy: false,
            _inputs: inputs,
        };
        card.load(cx);
        card
    }

    /// The page shows this agent (again): its memory is read anew, since
    /// the agent may have learned something in a conversation meanwhile.
    pub fn show(&mut self, agent_id: &str, cx: &mut Context<Self>) {
        if self.agent_id != agent_id {
            self.agent_id = agent_id.to_string();
            self.memory = None;
            self.filter = NoteFilter::All;
            self.editing = None;
            self.history_open = false;
            self.search.update(cx, |input, cx| input.set_text("", cx));
        }
        self.load(cx);
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.run(|api, agent| api.agent_memory(agent), cx);
    }

    /// Runs a read or a change off the main thread and shows what the
    /// server answers: the whole memory, after the change.
    pub(crate) fn run(
        &mut self,
        call: impl FnOnce(&DesktopApi, &str) -> Result<AgentMemory, ApiError> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(api) = self.api.clone() else {
            return;
        };
        let agent = self.agent_id.clone();
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let reading = agent.clone();
            let answer = cx
                .background_executor()
                .spawn(async move { call(&api, &reading) })
                .await;
            let _ = this.update(cx, |card, cx| {
                card.busy = false;
                // Another agent's page by now.
                if card.agent_id == agent {
                    match answer {
                        Ok(memory) => card.memory = Some(memory),
                        Err(error) => Toaster::error(error.to_string(), cx),
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn used_chars(&self, without: Option<&str>) -> usize {
        self.memory.as_ref().map_or(0, |memory| {
            memory
                .notes
                .iter()
                .filter(|note| Some(note.id.as_str()) != without)
                .map(|note| note.body.chars().count())
                .sum()
        })
    }

    /// Whether `body` fits, told to the person when it does not.
    fn fits(&self, body: &str, without: Option<&str>, cx: &mut Context<Self>) -> bool {
        let fits = self.used_chars(without) + body.chars().count() <= MAX_MEMORY_CHARS;
        if !fits {
            Toaster::error(cx.t("agents.memory.full"), cx);
        }
        fits
    }

    pub(crate) fn add(&mut self, cx: &mut Context<Self>) {
        let body = self.new_note.read(cx).text().trim().to_string();
        if body.is_empty() || self.busy || !self.fits(&body, None, cx) {
            return;
        }
        self.new_note.update(cx, |input, cx| input.set_text("", cx));
        self.run(move |api, agent| api.add_memory_note(agent, &body), cx);
    }

    pub(crate) fn start_edit(
        &mut self,
        note: &MemoryNote,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            let mut input = TextInput::new(window, cx).with_max_len(MAX_NOTE_CHARS);
            input.set_text(note.body.clone(), cx);
            input
        });
        let events = cx.subscribe(&input, |card, _, event: &TextInputEvent, cx| match event {
            TextInputEvent::Submitted => card.save_edit(cx),
            TextInputEvent::Cancelled => {
                card.editing = None;
                cx.notify();
            }
            _ => {}
        });
        input.update(cx, |input, cx| input.focus_and_select_all(window, cx));
        self.editing = Some((note.id.clone(), input, events));
        cx.notify();
    }

    pub(crate) fn save_edit(&mut self, cx: &mut Context<Self>) {
        let Some((note_id, input, _)) = self.editing.as_ref() else {
            return;
        };
        let (note_id, body) = (note_id.clone(), input.read(cx).text().trim().to_string());
        if body.is_empty() || !self.fits(&body, Some(&note_id), cx) {
            return;
        }
        self.editing = None;
        self.run(
            move |api, agent| api.edit_memory_note(agent, &note_id, &body),
            cx,
        );
    }

    pub(crate) fn remove(&mut self, note_id: String, cx: &mut Context<Self>) {
        self.run(
            move |api, agent| api.remove_memory_note(agent, &note_id),
            cx,
        );
    }

    pub(crate) fn restore(&mut self, version_id: i64, cx: &mut Context<Self>) {
        self.history_open = false;
        self.run(
            move |api, agent| api.restore_memory_version(agent, version_id),
            cx,
        );
    }

    fn note_row(&self, index: usize, note: &MemoryNote, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let group = SharedString::from(format!("memory-note-{index}"));
        if let Some((_, input, _)) = self.editing.as_ref().filter(|(id, _, _)| *id == note.id) {
            return div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(16.0))
                .py(px(8.0))
                .child(field(input.clone(), "agent-memory-edit", cx))
                .child(
                    Button::new("agent-memory-save")
                        .size(ButtonSize::Sm)
                        .on_click(cx.listener(|card, _, _, cx| card.save_edit(cx)))
                        .child(cx.t("agents.memory.save")),
                )
                .into_any_element();
        }
        let learned = note.learned().then(|| {
            div()
                .flex_none()
                .px(px(6.0))
                .rounded(px(4.0))
                .bg(colors.accent)
                .text_size(px(10.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("agents.memory.learnedBadge"))
        });
        let edit_note = note.clone();
        let remove_id = note.id.clone();
        let actions = div()
            .flex()
            .flex_none()
            .gap(px(2.0))
            .opacity(0.0)
            .group_hover(group.clone(), |style| style.opacity(1.0))
            .child(Tooltip::text(
                SharedString::from(format!("tip-memory-edit-{index}")),
                Button::new(SharedString::from(format!("agent-memory-edit-{index}")))
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconXs)
                    .on_click(cx.listener(move |card, _, window, cx| {
                        card.start_edit(&edit_note, window, cx)
                    }))
                    .child(Icon::new("pencil").size(13.0)),
                cx.t("agents.memory.edit"),
            ))
            .child(Tooltip::text(
                SharedString::from(format!("tip-memory-remove-{index}")),
                Button::new(SharedString::from(format!("agent-memory-remove-{index}")))
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconXs)
                    .on_click(cx.listener(move |card, _, _, cx| card.remove(remove_id.clone(), cx)))
                    .child(Icon::new("trash-2").size(13.0)),
                cx.t("agents.memory.remove"),
            ));
        div()
            .id(group.clone())
            .group(group)
            .debug_selector(move || format!("agent-memory-note-{index}"))
            .flex()
            .items_start()
            .gap(px(8.0))
            .px(px(16.0))
            .py(px(8.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.0))
                    .line_height(px(20.0))
                    .text_color(colors.foreground)
                    .child(note.body.clone()),
            )
            .children(learned)
            .child(actions)
            .into_any_element()
    }
}

/// A one-line field in the language's input look.
pub(crate) fn field(
    input: Entity<TextInput>,
    selector: &'static str,
    cx: &gpui::App,
) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .debug_selector(move || selector.into())
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .h(px(32.0))
        .px(px(10.0))
        .rounded(cx.theme().radius.lg)
        .border_1()
        .border_color(colors.input)
        .bg(crate::ui::input_fill(cx.theme()))
        .text_size(px(13.0))
        .child(input)
        .into_any_element()
}

impl Render for MemoryCard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let memory = self.memory.clone().unwrap_or_default();
        let has_notes = !memory.notes.is_empty();
        let query = self.search.read(cx).text().to_string();
        let shown = visible(&memory.notes, self.filter, &query);
        let usage = cx.t_with(
            "agents.memory.usage",
            &Args::default().with("percent", memory.usage.to_string()),
        );
        let history = (!memory.versions.is_empty()).then(|| {
            Button::new("agent-memory-history")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Xs)
                .on_click(cx.listener(|card, _, _, cx| {
                    card.history_open = true;
                    cx.notify();
                }))
                .child(cx.t("agents.memory.history"))
        });
        let heading = div()
            .flex()
            .items_center()
            .justify_between()
            .child(AgentPage::section_title(cx.t("agents.memory.title"), cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .mb(px(8.0))
                    .child(
                        div()
                            .debug_selector(|| "agent-memory-usage".into())
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(usage),
                    )
                    .children(history),
            );
        let card = cx.entity().downgrade();
        let tools = has_notes.then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(16.0))
                .pt(px(12.0))
                .pb(px(4.0))
                .child(
                    ToggleGroup::new(
                        "agent-memory-filter",
                        vec![
                            cx.t("agents.memory.all"),
                            cx.t("agents.memory.learned"),
                            cx.t("agents.memory.mine"),
                        ],
                        Some(match self.filter {
                            NoteFilter::All => 0,
                            NoteFilter::Learned => 1,
                            NoteFilter::Person => 2,
                        }),
                    )
                    .accent()
                    .on_select(move |index, _, cx| {
                        let _ = card.update(cx, |card, cx| {
                            card.filter = match index {
                                1 => NoteFilter::Learned,
                                2 => NoteFilter::Person,
                                _ => NoteFilter::All,
                            };
                            cx.notify();
                        });
                    }),
                )
                .when(memory.notes.len() > SEARCH_FROM, |row| {
                    row.child(field(self.search.clone(), "agent-memory-search", cx))
                })
        });
        let rows: Vec<AnyElement> = shown
            .iter()
            .enumerate()
            .map(|(index, note)| self.note_row(index, note, cx))
            .collect();
        let hint = if !has_notes {
            Some(cx.t("agents.memory.empty"))
        } else if rows.is_empty() {
            Some(cx.t("agents.memory.noMatch"))
        } else {
            None
        };
        let blank = self.new_note.read(cx).text().trim().is_empty();
        let adder = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(16.0))
            .py(px(12.0))
            .border_t_1()
            .border_color(colors.border.opacity(0.7))
            .child(field(self.new_note.clone(), "agent-memory-new", cx))
            .child(
                Button::new("agent-memory-add")
                    .size(ButtonSize::Sm)
                    .disabled(blank || self.busy)
                    .on_click(cx.listener(|card, _, _, cx| card.add(cx)))
                    .child(cx.t("agents.memory.addButton")),
            );
        div()
            .debug_selector(|| "agent-memory".into())
            .child(heading)
            .child(
                AgentPage::card(cx)
                    .overflow_hidden()
                    .children(tools)
                    .child(div().py(px(4.0)).children(rows))
                    .children(hint.map(|hint| {
                        div()
                            .px(px(16.0))
                            .pb(px(12.0))
                            .when(!has_notes, |hint| hint.pt(px(12.0)))
                            .text_size(px(12.0))
                            .line_height(px(19.5))
                            .text_color(colors.muted_foreground)
                            .child(hint)
                    }))
                    .child(adder),
            )
            .children(self.history_dialog(&memory, cx))
    }
}
