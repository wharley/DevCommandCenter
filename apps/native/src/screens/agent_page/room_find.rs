//! "Buscar na sala": ⌘F (or the magnifier in the room's head) opens the
//! find bar a task's conversation has (`ui::find_strip`), over the room's
//! rows — what the person wrote, what the agents answered, the turns
//! passed, the routines' and sources' lines, and the cards by their task or
//! pull request — in the room or on one agent's "Conversa". The current hit's line scrolls into view
//! with the ring around it; Enter and Shift+Enter step, Esc closes and gives
//! the field back. It searches what the room has read, nothing is asked of
//! the database, and it closes with another room or agent.

use gpui::{AnyElement, AppContext, Context, Focusable, KeyDownEvent, Subscription, Window};
use gpui::{div, prelude::*, px};

use super::room::TeamRoom;
use super::room_rows::{RoomModel, Row};
use super::room_said::Said;
use crate::data::team_room::{RoomLine, Speaker};
use crate::i18n::Translate;
use crate::screens::conversation::find::{FindMatch, MIN_QUERY_CHARS, find_in_texts, step};
use crate::theme::ActiveTheme;
use crate::ui::find_strip::{FindIds, FindStrip, find_strip};
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip};
use dcc_core::domain::agent_routine::routine_request;
use dcc_core::domain::agent_source::source_request;
use dcc_core::domain::team_room::handoff_heading;

const IDS: FindIds = FindIds {
    bar: "room-find-bar",
    field: "room-find-field",
    previous: "room-find-previous",
    next: "room-find-next",
    close: "room-find-close",
    close_tip: "room-find-close-tip",
};

pub(crate) struct RoomFind {
    input: gpui::Entity<TextInput>,
    active: usize,
    matches: Vec<FindMatch>,
    /// The room and the agent it was opened on.
    place: (Option<String>, Option<String>),
    _events: Subscription,
}

/// What a row is named by in the search: a line's key, a card's, a
/// check-in's place.
pub(crate) fn row_has(row: &Row, id: &str) -> bool {
    match row {
        Row::Line { line, .. } => line.key == id,
        Row::Sources { lines, .. } => lines.iter().any(|(_, line)| line.key == id),
        Row::Card { card, .. } => card.key == id,
        Row::CheckIn { index, .. } => check_in_id(*index) == id,
        Row::Subject { rows, .. } => rows.iter().any(|row| row_has(row, id)),
        Row::Recap | Row::Bulletin | Row::Day { .. } => false,
    }
}

fn check_in_id(index: usize) -> String {
    format!("check-in-{index}")
}

/// A line's words as the room shows them: the person's message as written,
/// an agent's answer, and for the lines DCC wrote (a turn passed, a
/// routine's, a source's) what their row says.
fn line_text(line: &RoomLine, said: Option<&Said>) -> Option<String> {
    if let Some(said) = said {
        return Some(said.words.clone());
    }
    if line.speaker == Speaker::Agent {
        return Some(line.text.clone());
    }
    if let Some(heading) = handoff_heading(&line.text) {
        return Some(heading.to_string());
    }
    if let Some(request) = routine_request(&line.text) {
        return Some(request.to_string());
    }
    if let Some((title, _)) = dcc_core::domain::task_shipped::shipped_line(&line.text) {
        return Some(title.to_string());
    }
    source_request(&line.text).map(|(watch, connector)| format!("{watch} ({connector})"))
}

/// Everything in the room that can be found, in order, each with what
/// names its row: the lines, the cards by their task or pull request, the
/// check-ins by their title. The room's own cards (the recap, the bulletin)
/// are not searched.
pub(crate) fn texts(model: &RoomModel) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for row in model.line_rows() {
        match row {
            Row::Line { line, said, .. } if model.agent(&line.agent_id).is_some() => {
                if let Some(text) = line_text(line, said.as_deref()) {
                    found.push((line.key.clone(), text));
                }
            }
            Row::Sources { lines, .. } => {
                for (_, line) in lines {
                    if let Some(text) = line_text(line, None) {
                        found.push((line.key.clone(), text));
                    }
                }
            }
            Row::Card { card, .. } => found.push((card.key.clone(), card.title.clone())),
            Row::CheckIn { index, check } => found.push((check_in_id(*index), check.title.clone())),
            _ => {}
        }
    }
    found
}

impl TeamRoom {
    pub(crate) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.is_none() {
            let placeholder = cx.t("agents.room.find.placeholder");
            let input = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
            let events =
                cx.subscribe_in(&input, window, |room, _, event, window, cx| match event {
                    TextInputEvent::Changed => room.search(true, cx),
                    TextInputEvent::Submitted => room.step_find(true, cx),
                    TextInputEvent::Cancelled => room.close_find(window, cx),
                    TextInputEvent::Blurred => {}
                });
            self.find = Some(RoomFind {
                input,
                active: 0,
                matches: Vec::new(),
                place: (self.room.clone(), self.focus.clone()),
                _events: events,
            });
        }
        if let Some(find) = &self.find {
            let input = find.input.clone();
            input.update(cx, |input, cx| input.focus_and_select_all(window, cx));
        }
        self.keep_rows(cx);
    }

    /// Opens on `query`, as "Sessões passadas" found it.
    pub(crate) fn open_find_on(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_find(window, cx);
        if let Some(find) = &self.find {
            let input = find.input.clone();
            input.update(cx, |input, cx| input.set_text(query.to_string(), cx));
        }
        self.search(true, cx);
    }

    #[cfg(test)]
    pub(crate) fn find_open(&self) -> bool {
        self.find.is_some()
    }

    /// Closes the bar; the room's field takes the keys back.
    pub(crate) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.take().is_some() {
            let field = self.input.read(cx).focus_handle(cx);
            window.focus(&field, cx);
            self.keep_rows(cx);
        }
    }

    /// Draws again with the rows as they are.
    fn keep_rows(&mut self, cx: &mut Context<Self>) {
        self.live.quiet = true;
        cx.notify();
    }

    /// The hits again, as the query changed; starts from the first.
    fn search(&mut self, restart: bool, cx: &mut Context<Self>) {
        let before = self.find_focus().map(str::to_string);
        self.read_hits(restart, cx);
        let opened = self.find_focus() != before.as_deref() && self.reveal_find();
        self.redraw_find(opened, cx);
    }

    fn read_hits(&mut self, restart: bool, cx: &gpui::App) {
        let model = self.list.model();
        let Some(find) = self.find.as_mut() else {
            return;
        };
        let query = find.input.read(cx).text().to_string();
        let texts = texts(&model);
        find.matches = find_in_texts(
            texts.iter().map(|(id, text)| (id.as_str(), text.as_str())),
            &query,
        );
        find.active = if restart || find.matches.is_empty() {
            0
        } else {
            find.active.min(find.matches.len() - 1)
        };
    }

    /// While the room draws: the bar of another room or agent goes away,
    /// and rows read again are searched again (the view stays put).
    pub(super) fn sync_find(&mut self, rebuilt: bool, cx: &gpui::App) {
        let here = (self.room.clone(), self.focus.clone());
        if self.find.as_ref().is_some_and(|find| find.place != here) {
            self.find = None;
        } else if rebuilt {
            // Opened before the room had read its lines: the first hit
            // shows once they are there.
            let waiting = self.find_focus().is_none();
            self.read_hits(false, cx);
            if waiting {
                self.reveal_find();
            }
        }
    }

    fn step_find(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(find) = self.find.as_mut() else {
            return;
        };
        find.active = step(find.active, find.matches.len(), forward);
        let opened = self.reveal_find();
        self.redraw_find(opened, cx);
    }

    /// The key of the current hit's line, if the bar has one.
    pub(crate) fn find_focus(&self) -> Option<&str> {
        let find = self.find.as_ref()?;
        find.matches
            .get(find.active)
            .map(|found| found.message_id.as_str())
    }

    /// Brings the current hit's row into view; true when its subject's
    /// block had to open for it (the rows are then read again).
    fn reveal_find(&mut self) -> bool {
        let Some(key) = self.find_focus().map(str::to_string) else {
            return false;
        };
        let model = self.list.model();
        let Some(index) = model.rows.iter().position(|row| row_has(row, &key)) else {
            return false;
        };
        // Following the end would pull the view back from the hit.
        self.list.state.pause_following_tail();
        self.list.state.scroll_to_reveal_item(index);
        match &model.rows[index] {
            Row::Subject {
                key, open: false, ..
            } => {
                self.subjects_open.insert(key.clone(), true);
                true
            }
            _ => false,
        }
    }

    /// Draws again after the hit moved: the rows kept, unless a block opened.
    fn redraw_find(&mut self, opened: bool, cx: &mut Context<Self>) {
        if opened {
            cx.notify();
        } else {
            self.keep_rows(cx);
        }
    }

    pub(super) fn find_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let find = self.find.as_ref()?;
        Some(find_strip(
            FindStrip {
                ids: &IDS,
                input: &find.input,
                active: find.active,
                total: find.matches.len(),
                snippet: find
                    .matches
                    .get(find.active)
                    .map(|found| found.snippet.as_str()),
            },
            Self::step_find,
            Self::close_find,
            cx,
        ))
    }

    /// The magnifier in the room's head.
    pub(super) fn find_button(&self, cx: &mut Context<Self>) -> AnyElement {
        Tooltip::text(
            "room-find-open-tip",
            Button::new("room-find-open")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .on_click(cx.listener(|room, _, window, cx| room.open_find(window, cx)))
                .child(Icon::new("search").size(14.0)),
            cx.t("agents.room.find.open"),
        )
        .into_any_element()
    }

    /// ⌘F with the keys anywhere in the room, its field included.
    pub(super) fn find_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        if keystroke.key == "f"
            && modifiers.platform
            && !modifiers.shift
            && !modifiers.alt
            && !modifiers.control
        {
            cx.stop_propagation();
            self.open_find(window, cx);
        }
    }

    /// The ring 4px out around the current hit's row.
    pub(super) fn find_ring(
        &self,
        row: &Row,
        content: Vec<AnyElement>,
        gap: f32,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if content.is_empty() || !self.find_focus().is_some_and(|key| row_has(row, key)) {
            return content;
        }
        let ring = cx.theme().colors.ring.opacity(0.55);
        vec![
            div()
                .debug_selector(|| "room-find-focus".into())
                .relative()
                .flex()
                .flex_col()
                .gap(px(gap))
                .children(content)
                .child(
                    div()
                        .absolute()
                        .top(px(-4.0))
                        .bottom(px(-4.0))
                        .left(px(-4.0))
                        .right(px(-4.0))
                        .rounded(px(8.0))
                        .border_2()
                        .border_color(ring),
                )
                .into_any_element(),
        ]
    }
}

impl super::AgentPage {
    /// ⌘F with the keys outside the room: finds in the room on screen (the
    /// team's, or the agent's "Conversa"), if one is.
    pub(crate) fn open_room_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let on_screen = self.editor.is_none()
            && (self.room_open || self.tab == super::page_tabs::AgentTab::Conversation);
        if let Some((room, _)) = self.room.as_ref().filter(|_| on_screen) {
            room.update(cx, |room, cx| room.open_find(window, cx));
        }
    }
}

impl super::AgentPage {
    /// The agent's "Conversa" in `room`, with `query` in its find bar (a
    /// result of "Sessões passadas"). The page is on the agent already.
    pub(crate) fn find_in_room(
        &mut self,
        room: Option<String>,
        query: &str,
        cx: &mut Context<Self>,
    ) {
        self.agent_room = Some(room);
        self.room_find = (query.chars().count() >= MIN_QUERY_CHARS).then(|| query.to_string());
        cx.notify();
    }

    /// The magnifier in the head of the agent's "Conversa".
    pub(super) fn conversation_find_button(&self, cx: &mut Context<Self>) -> AnyElement {
        Tooltip::text(
            "agent-find-open-tip",
            Button::new("agent-find-open")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .on_click(cx.listener(|page, _, window, cx| page.open_room_find(window, cx)))
                .child(Icon::new("search").size(14.0)),
            cx.t("agents.room.find.open"),
        )
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Modifiers, TestAppContext};

    use super::super::room_list_tests::room_with_lines;
    use crate::data::agents::AgentState;
    use crate::data::team_room::{RoomLine, Speaker};

    fn line(index: usize, speaker: Speaker, text: &str) -> RoomLine {
        RoomLine {
            key: format!("l-{index}"),
            agent_id: "dev".into(),
            session_id: "s-geral".into(),
            speaker,
            text: text.into(),
            at: format!("2026-11-10T10:0{index}:00Z"),
            receipt: None,
            tasks: Vec::new(),
            steps: Default::default(),
        }
    }

    fn lines() -> Vec<RoomLine> {
        vec![
            line(0, Speaker::Person, "vigia minhas menções no Slack"),
            line(1, Speaker::Agent, "Proponho uma fonte para o Slack."),
            line(2, Speaker::Person, "corrige o login"),
            line(3, Speaker::Agent, "Feito."),
        ]
    }

    #[gpui::test]
    fn the_room_is_searched_and_the_hits_are_stepped(cx: &mut TestAppContext) {
        let (page, room, cx) = room_with_lines(cx, lines(), AgentState::Idle);
        assert!(cx.debug_bounds("room-find-bar").is_none());

        // The magnifier in the room's head opens the bar.
        let open = cx.debug_bounds("room-find-open").unwrap();
        cx.simulate_click(open.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("room-find-bar").is_some());

        // Both the person's line and the agent's answer, in order.
        cx.simulate_input("slack");
        cx.run_until_parked();
        let focus = |cx: &mut gpui::VisualTestContext| {
            room.read_with(cx, |room, _| room.find_focus().map(str::to_string))
        };
        assert_eq!(focus(cx).as_deref(), Some("l-0"));
        assert!(cx.debug_bounds("room-find-focus").is_some());
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(focus(cx).as_deref(), Some("l-1"));
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(focus(cx).as_deref(), Some("l-0"), "wraps");

        // Esc closes it and the room's field has the keys again.
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(!room.read_with(cx, |room, _| room.find_open()));
        assert!(cx.debug_bounds("room-find-bar").is_none());
        cx.simulate_input("oi");
        cx.run_until_parked();
        assert_eq!(
            room.read_with(cx, |room, cx| room.input.read(cx).text().to_string()),
            "oi"
        );

        // ⌘F from the field opens it again; so does the page, asked from
        // outside the room.
        cx.simulate_keystrokes("cmd-f");
        cx.run_until_parked();
        assert!(room.read_with(cx, |room, _| room.find_open()));
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        page.update_in(cx, |page, window, cx| page.open_room_find(window, cx));
        cx.run_until_parked();
        assert!(room.read_with(cx, |room, _| room.find_open()));

        // On one agent it is another conversation: the bar goes away, and
        // the magnifier in that page's head opens its own.
        page.update(cx, |page, cx| page.show_conversation("dev".into(), cx));
        cx.run_until_parked();
        assert!(!room.read_with(cx, |room, _| room.find_open()));
        let open = cx.debug_bounds("agent-find-open").unwrap();
        cx.simulate_click(open.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(room.read_with(cx, |room, _| room.find_open()));
    }

    #[gpui::test]
    fn a_rooms_result_of_the_sessions_search_opens_in_the_room(cx: &mut TestAppContext) {
        use crate::data::session_search_rooms::room_hit;
        use dcc_core::domain::agent_conversation::home_project_id;
        use dcc_core::domain::session::SessionSearchResult;
        let result = |session: &str, project: String| SessionSearchResult {
            session_id: dcc_core::domain::session::SessionId(session.into()),
            workspace_id: dcc_core::domain::workspace::WorkspaceId("w".into()),
            project_id: dcc_core::domain::project::ProjectId(project),
            thread_title: "Conversa".into(),
            workspace_name: None,
            workspace_branch: None,
            workspace_root_path: None,
            provider_id: "codex".into(),
            model: None,
            archived_at: None,
            created_at: String::new(),
            updated_at: String::new(),
            snippet: String::new(),
        };
        let agents = [super::super::room_list_tests::dev(AgentState::Idle)];
        let hit = room_hit(&result("s-geral", home_project_id("dev")), &agents).unwrap();
        assert_eq!((hit.agent_name.as_str(), &hit.room), ("Dev", &None));
        assert!(hit.shown);
        let checkout = room_hit(&result("s-checkout", home_project_id("dev")), &agents).unwrap();
        assert_eq!(checkout.room.as_deref(), Some("checkout"));
        // A task's session, and a room session the team does not have.
        assert_eq!(room_hit(&result("s-geral", "p-app".into()), &agents), None);
        assert_eq!(
            room_hit(&result("gone", home_project_id("dev")), &agents),
            None
        );

        // Picked: the agent's "Conversa" with the query in its find bar.
        let (page, room, cx) = room_with_lines(cx, lines(), AgentState::Idle);
        page.update(cx, |page, cx| {
            page.show_conversation("dev".into(), cx);
            page.find_in_room(None, "slack", cx);
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("room-find-bar").is_some());
        assert_eq!(
            room.read_with(cx, |room, _| room.find_focus().map(str::to_string))
                .as_deref(),
            Some("l-0")
        );
        // One letter is no search: the room opens with no bar.
        page.update(cx, |page, cx| page.find_in_room(None, "s", cx));
        assert!(page.read_with(cx, |page, _| page.room_find.is_none()));
    }

    #[gpui::test]
    fn the_lines_dcc_wrote_are_found_by_what_their_row_says(cx: &mut TestAppContext) {
        use dcc_core::domain::agent_routine::ROUTINE_MARK;
        let routine = format!("{ROUTINE_MARK} (diária): resumo dos PRs abertos");
        assert!(dcc_core::domain::agent_routine::routine_request(&routine).is_some());
        let mut all = lines();
        all.push(line(4, Speaker::Person, &routine));
        let (_page, room, cx) = room_with_lines(cx, all, AgentState::Idle);
        let model = room.read_with(cx, |room, _| room.list.model());
        let texts = super::texts(&model);
        let of = |key: &str| {
            texts
                .iter()
                .find(|(id, _)| id == key)
                .map(|(_, text)| text.as_str())
        };
        assert_eq!(of("l-0"), Some("vigia minhas menções no Slack"));
        assert_eq!(of("l-3"), Some("Feito."));
        // The routine's line by its request, not by the mark DCC wrote.
        assert_eq!(of("l-4"), Some("resumo dos PRs abertos"));
    }
}
