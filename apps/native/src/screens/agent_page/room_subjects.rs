//! Subjects in the room: the lines of one piece of work — what the person
//! asked, the answer, the pass to a teammate, its result coming back, the
//! task's delivery, the closing word — read as one block that opens and
//! closes, with where it stands on its head. Nothing is stored and no
//! conversation is split: the agents keep one conversation each, with all
//! its context; this is only how the room draws what DCC already links.
//!
//! A line joins a subject by what it names: an answer follows the message
//! that started its turn; a pass and a return name the answer they came
//! from; a reply quotes the answer it replies to; a delivery names the turn
//! the work started with; a message right after an answer in the same
//! conversation goes on with it, up to a size. Anything else starts a
//! subject. Only a subject that moved — passed on, returned, delivered or
//! replied to — becomes a block: a question and its answer, and a long talk
//! with one agent, stay as they always were (and the list keeps drawing
//! only the rows on screen).

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use dcc_core::domain::agent_chain::{passed_answer, passed_by};
use dcc_core::domain::agent_routine::{ROUTINE_MARK, routine_request};
use dcc_core::domain::agent_source::{SOURCE_MARK, source_request};
use dcc_core::domain::task_shipped::{shipped_line, shipped_origin};
use dcc_core::domain::team_room::handoff_heading;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px};

use super::room::TeamRoom;
use super::room_reply::quote_of;
use super::room_rows::{RoomModel, Row};
use crate::data::team_room::{RoomLine, Speaker};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::Icon;

/// A message this long after the conversation's last line still goes on
/// with its subject (the room's own "segue o assunto").
const CONTINUES_MINUTES: i64 = 30;
/// Lines a subject takes by going on alone; the next message starts
/// another, so a long talk never becomes one row of the list.
const MAX_CONTINUED_LINES: usize = 24;
const TITLE_CHARS: usize = 90;

/// Where a subject stands, by its lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SubjectState {
    /// The last word is not an agent's: one is at work, or is to answer.
    Working,
    Answered,
    Merged,
    Live,
    DeployFailed,
}

impl SubjectState {
    /// Over: nothing more is expected of it.
    fn delivered(self) -> bool {
        matches!(self, Self::Merged | Self::Live)
    }

    fn key(self) -> &'static str {
        match self {
            Self::Working => "agents.room.subject.working",
            Self::Answered => "agents.room.subject.answered",
            Self::Merged => "agents.room.subject.merged",
            Self::Live => "agents.room.subject.live",
            Self::DeployFailed => "agents.room.subject.deployFailed",
        }
    }
}

fn parse(at: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

/// The quote a reply opens with (`room_reply::quoted_message`).
fn reply_quote(text: &str) -> Option<&str> {
    let first = text.lines().next()?;
    first.strip_prefix("> ").map(str::trim)
}

/// What `assign` found.
pub(crate) struct Assigned {
    /// The subject of each line, as an index into the subjects.
    pub(crate) of_line: Vec<usize>,
    /// Whether each subject moved: a pass, a return, a delivery or a reply
    /// joined it.
    pub(crate) moved: Vec<bool>,
}

/// The subject of each of `lines`, in the order given (oldest first);
/// `names` gives an agent's id by its name, for a pass the person clicked.
pub(crate) fn assign(lines: &[&RoomLine], names: &HashMap<String, String>) -> Assigned {
    let mut moved: Vec<bool> = Vec::new();
    let mut sizes: Vec<usize> = Vec::new();
    let mut of_line: Vec<usize> = Vec::with_capacity(lines.len());
    // The subject of a line, by its key (an answer's is its turn's).
    let mut by_key: HashMap<&str, usize> = HashMap::new();
    // The subject each conversation is on, and when it last spoke of it.
    let mut by_session: HashMap<&str, (usize, Option<DateTime<Utc>>)> = HashMap::new();
    // The subject each agent last spoke of.
    let mut by_agent: HashMap<&str, usize> = HashMap::new();
    // The answers so far, for a reply's quote.
    let mut quotes: Vec<(String, usize)> = Vec::new();
    for line in lines {
        let at = parse(&line.at);
        let text = line.text.as_str();
        // Whether this line is a link: it names what it belongs to.
        let mut link = false;
        let joined: Option<usize> = if line.speaker == Speaker::Agent {
            // The answer of the turn its conversation is on.
            by_session
                .get(line.session_id.as_str())
                .map(|(subject, _)| *subject)
        } else if text.starts_with(SOURCE_MARK) || text.starts_with(ROUTINE_MARK) {
            None
        } else if let Some((session, origin_at)) = shipped_origin(text) {
            // The delivery of the work a turn of that conversation started.
            link = true;
            let origin_at = parse(origin_at);
            lines
                .iter()
                .zip(&of_line)
                .filter(|(known, _)| {
                    known.session_id == session
                        && match (parse(&known.at), origin_at) {
                            (Some(known), Some(origin)) => known <= origin + Duration::seconds(5),
                            _ => false,
                        }
                })
                .map(|(_, subject)| *subject)
                .last()
        } else if handoff_heading(text).is_some() {
            // A pass or a return names the answer it came from; one the
            // person clicked, who passed it.
            link = true;
            passed_answer(text)
                .and_then(|key| by_key.get(key).copied())
                .or_else(|| {
                    let from = names.get(&passed_by(text)?.to_lowercase())?;
                    by_agent.get(from.as_str()).copied()
                })
        } else if let Some(quote) = reply_quote(text) {
            link = true;
            quotes
                .iter()
                .rev()
                .find(|(known, _)| known == quote)
                .map(|(_, subject)| *subject)
        } else {
            // The person, right after this conversation's last line.
            by_session
                .get(line.session_id.as_str())
                .filter(|(subject, last)| {
                    sizes[*subject] < MAX_CONTINUED_LINES
                        && match (last, at) {
                            (Some(last), Some(at)) => {
                                at - *last <= Duration::minutes(CONTINUES_MINUTES)
                            }
                            _ => false,
                        }
                })
                .map(|(subject, _)| *subject)
        };
        let subject = joined.unwrap_or_else(|| {
            moved.push(false);
            sizes.push(0);
            moved.len() - 1
        });
        // Only a link that found its subject makes it one that moved.
        if link && joined.is_some() {
            moved[subject] = true;
        }
        sizes[subject] += 1;
        of_line.push(subject);
        by_key.insert(line.key.as_str(), subject);
        by_session.insert(line.session_id.as_str(), (subject, at));
        if line.speaker == Speaker::Agent {
            by_agent.insert(line.agent_id.as_str(), subject);
            quotes.push((quote_of(text), subject));
        }
    }
    Assigned { of_line, moved }
}

/// Where a subject stands, by its lines (oldest first).
pub(crate) fn state_of(lines: &[&RoomLine]) -> SubjectState {
    let shipped = lines
        .iter()
        .rev()
        .find_map(|line| shipped_line(&line.text).map(|(_, state)| state));
    match shipped {
        Some("live") => SubjectState::Live,
        Some("deploy failed") => SubjectState::DeployFailed,
        Some(_) => SubjectState::Merged,
        None if lines
            .last()
            .is_some_and(|line| line.speaker == Speaker::Agent) =>
        {
            SubjectState::Answered
        }
        None => SubjectState::Working,
    }
}

/// What a subject is called: its first line, as the room shows it.
pub(crate) fn title_of(first: &RoomLine) -> String {
    let text = first.text.as_str();
    let words = if let Some((watch, connector)) = source_request(text) {
        format!("{connector} — {watch}")
    } else if let Some(request) = routine_request(text) {
        request.to_string()
    } else if let Some((task, _)) = shipped_line(text) {
        task.to_string()
    } else if let Some(heading) = handoff_heading(text) {
        heading.to_string()
    } else {
        // A reply's quote is not its title.
        text.lines()
            .find(|line| !line.trim().is_empty() && !line.starts_with("> "))
            .unwrap_or(text)
            .to_string()
    };
    let flat = words.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= TITLE_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(TITLE_CHARS).collect();
    format!("{}…", cut.trim_end())
}

/// `rows` with each subject that moved gathered into one `Row::Subject`,
/// where its last line was; the rest stay where they are.
/// `open` holds what the person opened or closed, by the subject's key.
pub(crate) fn group(
    rows: Vec<Row>,
    names: &HashMap<String, String>,
    open: &HashMap<String, bool>,
) -> Vec<Row> {
    let lines: Vec<&RoomLine> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Line { line, .. } => Some(line),
            _ => None,
        })
        .collect();
    if lines.len() < 3 {
        return rows;
    }
    let Assigned { of_line, moved } = assign(&lines, names);
    if !moved.iter().any(|moved| *moved) {
        return rows;
    }
    let subjects = moved.len();
    let mut last = vec![0usize; subjects];
    for (index, subject) in of_line.iter().enumerate() {
        last[*subject] = index;
    }
    // Each block's rows, taken out of the flow.
    let mut held: Vec<Vec<Row>> = (0..subjects).map(|_| Vec::new()).collect();
    let mut out: Vec<Row> = Vec::with_capacity(rows.len());
    let mut seen = 0usize;
    for row in rows {
        let Row::Line { .. } = &row else {
            out.push(row);
            continue;
        };
        let (subject, index) = (of_line[seen], seen);
        seen += 1;
        if !moved[subject] {
            out.push(row);
            continue;
        }
        held[subject].push(row);
        if index == last[subject] {
            let inner = super::room_flow::arrange(std::mem::take(&mut held[subject]));
            let inner_lines: Vec<&RoomLine> = inner
                .iter()
                .filter_map(|row| match row {
                    Row::Line { line, .. } => Some(line),
                    _ => None,
                })
                .collect();
            let Some(first) = inner_lines.first() else {
                continue;
            };
            let state = state_of(&inner_lines);
            let key = first.key.clone();
            out.push(Row::Subject {
                title: title_of(first),
                at: inner_lines
                    .last()
                    .map(|line| line.at.clone())
                    .unwrap_or_default(),
                count: inner_lines.len(),
                // Delivered work folds away; the rest shows until closed.
                open: open.get(&key).copied().unwrap_or(!state.delivered()),
                state,
                key,
                rows: inner,
            });
        }
    }
    out
}

impl RoomModel {
    /// Every line row, the ones inside a subject's block included.
    pub(crate) fn line_rows(&self) -> impl Iterator<Item = &Row> {
        self.rows.iter().flat_map(|row| match row {
            Row::Subject { rows, .. } => rows.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
    }
}

impl TeamRoom {
    /// Whether a conversation of the subject's lines waits on the person.
    fn subject_waits(&self, rows: &[Row], cx: &gpui::App) -> bool {
        let agents = self.sidebar.read(cx).agents();
        rows.iter().any(|row| {
            let Row::Line { line, .. } = row else {
                return false;
            };
            agents
                .iter()
                .filter(|agent| agent.id == line.agent_id)
                .flat_map(|agent| agent.sessions.iter())
                .any(|session| {
                    session.session_id == line.session_id
                        && session.state == crate::data::agents::AgentState::NeedsYou
                })
        })
    }

    /// Opens or closes the subject `key`; the rows are read again.
    pub(crate) fn toggle_subject(&mut self, key: String, open: bool, cx: &mut Context<Self>) {
        self.subjects_open.insert(key, open);
        cx.notify();
    }

    /// A subject's block: its head, and its lines while open.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn subject_row(
        &mut self,
        key: &str,
        title: &str,
        state: SubjectState,
        count: usize,
        open: bool,
        rows: &[Row],
        model: &RoomModel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.theme().colors.clone();
        // At work, and one of its conversations waits on the person (an
        // approval, a question): that is what it stands on.
        let waiting = state == SubjectState::Working && self.subject_waits(rows, cx);
        let tone = match state {
            _ if waiting => crate::theme::tailwind::AMBER_500.hsla(),
            SubjectState::Live => colors.success,
            SubjectState::DeployFailed => colors.destructive,
            SubjectState::Working => colors.primary,
            SubjectState::Answered | SubjectState::Merged => colors.muted_foreground,
        };
        let stands = if waiting {
            cx.t("agents.room.subject.waiting")
        } else {
            cx.t(state.key())
        };
        let id = SharedString::from(format!("room-subject-{key}"));
        let selector = id.to_string();
        let toggled = key.to_string();
        let head = div()
            .id(id)
            .debug_selector(move || selector.clone())
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .on_click(
                cx.listener(move |room, _, _, cx| room.toggle_subject(toggled.clone(), !open, cx)),
            )
            .child(
                Icon::new(if open {
                    "chevron-down"
                } else {
                    "chevron-right"
                })
                .size(13.0)
                .color(colors.muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .child(title.to_string()),
            )
            .when(!open, |head| {
                head.child(
                    div()
                        .flex_none()
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t_with("agents.room.subject.lines", &Args::count(count as i64))),
                )
            })
            .child(
                div()
                    .flex_none()
                    .px(px(8.0))
                    .py(px(2.0))
                    .rounded_full()
                    .bg(tone.opacity(0.1))
                    .text_size(px(11.0))
                    .text_color(tone)
                    .child(stands),
            );
        let mut body: Vec<AnyElement> = Vec::new();
        if open {
            for row in rows {
                body.extend(self.inner_row(row, model, window, cx));
            }
        }
        vec![
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .px(px(12.0))
                .py(px(10.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(colors.border.opacity(0.8))
                .child(head)
                .children(body)
                .into_any_element(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::agent_chain::{auto_handoff_prompt, return_prompt};

    fn line(
        key: &str,
        session: &str,
        agent: &str,
        speaker: Speaker,
        minute: u32,
        text: &str,
    ) -> RoomLine {
        RoomLine {
            key: key.into(),
            agent_id: agent.into(),
            session_id: session.into(),
            speaker,
            text: text.into(),
            at: format!("2026-11-14T10:{minute:02}:00Z"),
            receipt: None,
            tasks: Vec::new(),
            steps: Default::default(),
        }
    }

    #[test]
    fn a_pass_its_return_and_a_reply_stay_with_what_started_them() {
        let pass = auto_handoff_prompt(
            "Suporte",
            "Dev",
            "corrige o CEP",
            "vou passar",
            "s-sup:t1",
            1,
        );
        let back = return_prompt("Dev", "Suporte", "Abri a tarefa.", "s-dev:t2");
        let lines = vec![
            line(
                "s-sup:m1",
                "s-sup",
                "sup",
                Speaker::Person,
                0,
                "o CEP não calcula frete",
            ),
            line(
                "s-sup:t1",
                "s-sup",
                "sup",
                Speaker::Agent,
                1,
                "Vou passar para o Dev.",
            ),
            // Another subject, in between, in another conversation.
            line(
                "s-qa:m1",
                "s-qa",
                "qa",
                Speaker::Person,
                2,
                "e o deploy de hoje?",
            ),
            line("s-dev:m2", "s-dev", "dev", Speaker::Person, 3, &pass),
            line(
                "s-dev:t2",
                "s-dev",
                "dev",
                Speaker::Agent,
                4,
                "Abri a tarefa.",
            ),
            line("s-qa:t1", "s-qa", "qa", Speaker::Agent, 5, "Sai às 18h."),
            line("s-sup:m3", "s-sup", "sup", Speaker::Person, 6, &back),
            line(
                "s-sup:t3",
                "s-sup",
                "sup",
                Speaker::Agent,
                7,
                "O Dev abriu a tarefa.",
            ),
            // Hours later, the same conversation: another subject.
            line(
                "s-sup:m4",
                "s-sup",
                "sup",
                Speaker::Person,
                59,
                "> Sai às 18h.\n\nconfirmado?",
            ),
        ];
        let refs: Vec<&RoomLine> = lines.iter().collect();
        let Assigned { of_line, moved } = assign(&refs, &HashMap::new());
        assert!(moved[of_line[0]] && moved[of_line[2]], "both moved");
        // The CEP: the request, the answer, the pass, Dev's answer, the
        // return and the closing word.
        assert_eq!(of_line[0], of_line[1]);
        assert_eq!(of_line[0], of_line[3]);
        assert_eq!(of_line[0], of_line[4]);
        assert_eq!(of_line[0], of_line[6]);
        assert_eq!(of_line[0], of_line[7]);
        // The deploy: its own, and the reply that quotes its answer joins it
        // though it was written in another conversation.
        assert_eq!(of_line[2], of_line[5]);
        assert_ne!(of_line[2], of_line[0]);
        assert_eq!(of_line[8], of_line[2]);
    }

    #[test]
    fn a_message_goes_on_with_its_conversation_only_for_a_while() {
        let lines = vec![
            line("s:m1", "s", "dev", Speaker::Person, 0, "corrige o login"),
            line("s:t1", "s", "dev", Speaker::Agent, 1, "Feito."),
            line("s:m2", "s", "dev", Speaker::Person, 10, "e o teste?"),
            line("s:t2", "s", "dev", Speaker::Agent, 11, "Passou."),
            line(
                "s:m3",
                "s",
                "dev",
                Speaker::Person,
                55,
                "outra coisa: o cache",
            ),
        ];
        let refs: Vec<&RoomLine> = lines.iter().collect();
        let found = assign(&refs, &HashMap::new());
        assert_eq!(found.of_line, [0, 0, 0, 0, 1]);
        // Talk alone moves nothing: no block.
        assert_eq!(found.moved, [false, false]);
        // A long talk is cut into subjects, never one row of the list.
        let long: Vec<RoomLine> = (0..120)
            .map(|index| {
                let speaker = if index % 2 == 0 {
                    Speaker::Person
                } else {
                    Speaker::Agent
                };
                let mut said = line(&format!("s:{index}"), "s", "dev", speaker, 0, "oi");
                said.at = format!("2026-11-14T{:02}:{:02}:00Z", 10 + index / 60, index % 60);
                said
            })
            .collect();
        let refs: Vec<&RoomLine> = long.iter().collect();
        let found = assign(&refs, &HashMap::new());
        let largest = (0..found.moved.len())
            .map(|subject| found.of_line.iter().filter(|of| **of == subject).count())
            .max()
            .unwrap();
        assert!(largest <= MAX_CONTINUED_LINES + 1, "{largest}");
    }

    #[test]
    fn where_a_subject_stands_and_what_it_is_called() {
        let asked = line(
            "k",
            "s",
            "dev",
            Speaker::Person,
            0,
            "> citação\n\ncorrige   o login",
        );
        assert_eq!(title_of(&asked), "corrige o login");
        let answer = line("k2", "s", "dev", Speaker::Agent, 1, "Feito.");
        assert_eq!(state_of(&[&asked]), SubjectState::Working);
        assert_eq!(state_of(&[&asked, &answer]), SubjectState::Answered);
        let shipped = line(
            "k3",
            "s",
            "dev",
            Speaker::Person,
            2,
            "[DCC] Delivered \"Corrigir login\" (#7): live.\n\nmore",
        );
        assert_eq!(state_of(&[&asked, &answer, &shipped]), SubjectState::Live);
        assert_eq!(title_of(&shipped), "Corrigir login");
    }

    #[gpui::test]
    fn a_subject_that_moved_is_a_block_that_opens_and_closes(cx: &mut gpui::TestAppContext) {
        use super::super::room_list_tests::room_with_lines;
        use crate::data::agents::AgentState;
        use gpui::Modifiers;
        let room_line = |index: usize, speaker: Speaker, text: String| RoomLine {
            key: format!("l-{index}"),
            agent_id: "dev".into(),
            session_id: "s-geral".into(),
            speaker,
            text,
            at: format!("2026-11-10T10:0{index}:00Z"),
            receipt: None,
            tasks: Vec::new(),
            steps: Default::default(),
        };
        let back = return_prompt("QA", "Dev", "Testei e passou.", "l-1");
        let (_page, room, cx) = room_with_lines(
            cx,
            vec![
                room_line(0, Speaker::Person, "corrige o login".into()),
                room_line(1, Speaker::Agent, "Vou pedir o teste ao QA.".into()),
                room_line(2, Speaker::Person, back),
                room_line(3, Speaker::Agent, "O QA testou e passou.".into()),
            ],
            AgentState::Idle,
        );
        // One block, named after what started it, open: its lines show.
        assert!(cx.debug_bounds("room-subject-l-0").is_some());
        assert!(cx.debug_bounds("room-said-0").is_some());
        assert!(cx.debug_bounds("room-returned-2").is_some());
        assert!(cx.debug_bounds("room-answer-3").is_some());
        room.read_with(cx, |room, _| {
            let model = room.list.model();
            // Its four lines are one row of the list, none left loose.
            assert!(!model.rows.iter().any(|row| matches!(row, Row::Line { .. })));
            let subject = model
                .rows
                .iter()
                .find(|row| matches!(row, Row::Subject { .. }))
                .expect("a subject");
            let Row::Subject {
                title,
                state,
                count,
                ..
            } = subject
            else {
                unreachable!()
            };
            assert_eq!(title, "corrige o login");
            assert_eq!((*state, *count), (SubjectState::Answered, 4));
            // The search still reads every line.
            assert_eq!(super::super::room_find::texts(&model).len(), 4);
        });

        // Closed: the head alone, with how many lines it holds.
        let head = cx.debug_bounds("room-subject-l-0").unwrap();
        cx.simulate_click(
            gpui::point(head.left() + gpui::px(40.0), head.center().y),
            Modifiers::none(),
        );
        cx.run_until_parked();
        assert!(cx.debug_bounds("room-subject-l-0").is_some());
        assert!(cx.debug_bounds("room-said-0").is_none());
        assert!(cx.debug_bounds("room-answer-3").is_none());

        // And open again.
        let head = cx.debug_bounds("room-subject-l-0").unwrap();
        cx.simulate_click(
            gpui::point(head.left() + gpui::px(40.0), head.center().y),
            Modifiers::none(),
        );
        cx.run_until_parked();
        assert!(cx.debug_bounds("room-answer-3").is_some());
    }
}
