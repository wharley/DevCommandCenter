//! A task's origin as the app reads it (`dcc_core::domain::task_origin`):
//! from DCC's database, read only, once when the task (or its pull request)
//! shows, with the names of the rooms its steps happened in. A task the
//! person made has none.

use std::collections::HashMap;

use dcc_core::domain::task_origin::{Started, TaskOrigin};
use dcc_infra::db::SqliteSessionRepo;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OriginRead {
    pub origin: TaskOrigin,
    /// The projects whose rooms the steps are in, by id.
    pub rooms: HashMap<String, String>,
}

/// The origin of task `workspace_id`, when an agent opened it.
pub fn read(workspace_id: &str) -> Option<OriginRead> {
    let db = crate::data::loader::database_path();
    let repo = SqliteSessionRepo::open_read_only(&db).ok()?;
    let origin = repo.task_origin(workspace_id).ok()??;
    let rooms = origin
        .steps
        .iter()
        .filter_map(|step| step.room.clone())
        .filter_map(|room| {
            let (name, _) = repo.project_name_and_path(&room)?;
            Some((room, name))
        })
        .collect();
    Some(OriginRead { origin, rooms })
}

/// One row of the chain, as it is drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainRow {
    pub icon: &'static str,
    /// The text key and what it takes.
    pub title: (&'static str, Vec<(&'static str, String)>),
    /// The words under it (an excerpt), if any.
    pub detail: String,
    pub at: String,
    /// The room it happened in (`Some(None)`: Geral) and the agent whose
    /// conversation it is, when a click can go there.
    pub place: Option<(String, Option<String>)>,
    pub link: Option<String>,
}

/// The chain as rows, oldest first, ending at the agent that opened the
/// task.
pub fn rows(origin: &TaskOrigin) -> Vec<ChainRow> {
    let mut rows = Vec::new();
    for step in &origin.steps {
        let place = Some((step.agent_id.clone(), step.room.clone()));
        let agent = ("agent", step.agent.clone());
        match &step.started {
            Started::Source {
                connector,
                watch,
                excerpt,
                link,
            } => {
                rows.push(ChainRow {
                    icon: "message-square",
                    title: ("origin.row.message", vec![("connector", connector.clone())]),
                    detail: excerpt.clone(),
                    at: String::new(),
                    place: None,
                    link: link.clone(),
                });
                rows.push(ChainRow {
                    icon: "radar",
                    title: ("origin.row.source", vec![("watch", watch.clone()), agent]),
                    detail: String::new(),
                    at: step.at.clone(),
                    place,
                    link: None,
                });
            }
            Started::Person { excerpt } => rows.push(ChainRow {
                icon: "message-square",
                title: ("origin.row.person", vec![agent]),
                detail: excerpt.clone(),
                at: step.at.clone(),
                place,
                link: None,
            }),
            Started::Routine { request } => rows.push(ChainRow {
                icon: "history",
                title: ("origin.row.routine", vec![agent]),
                detail: request.clone(),
                at: step.at.clone(),
                place,
                link: None,
            }),
            Started::Event { line } => rows.push(ChainRow {
                icon: "zap",
                title: ("origin.row.event", vec![agent]),
                detail: line.clone(),
                at: step.at.clone(),
                place,
                link: None,
            }),
            Started::Passed { from, to, request } => rows.push(ChainRow {
                icon: "corner-up-right",
                title: (
                    "origin.row.passed",
                    vec![("from", from.clone()), ("to", to.clone())],
                ),
                detail: request.clone(),
                at: step.at.clone(),
                place,
                link: None,
            }),
        }
    }
    if let Some(last) = origin.steps.last() {
        rows.push(ChainRow {
            icon: "git-fork",
            title: ("origin.row.opened", vec![("agent", last.agent.clone())]),
            detail: String::new(),
            at: origin.opened_at.clone(),
            place: None,
            link: None,
        });
    }
    rows
}

/// The closed line's end: where it all started ("fonte Slack · menções a
/// mim"), as a text key and what it takes.
pub fn start_label(origin: &TaskOrigin) -> Option<(&'static str, Vec<(&'static str, String)>)> {
    Some(match &origin.steps.first()?.started {
        Started::Source {
            connector, watch, ..
        } => (
            "origin.from.source",
            vec![("connector", connector.clone()), ("watch", watch.clone())],
        ),
        Started::Person { .. } => ("origin.from.person", Vec::new()),
        Started::Routine { .. } => ("origin.from.routine", Vec::new()),
        Started::Event { .. } => ("origin.from.event", Vec::new()),
        // Followed back as far as it goes: the teammate that passed it.
        Started::Passed { from, .. } => ("origin.from.passed", vec![("from", from.clone())]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::task_origin::OriginStep;

    fn step(agent: &str, room: Option<&str>, started: Started) -> OriginStep {
        OriginStep {
            started,
            agent_id: agent.to_lowercase(),
            agent: agent.into(),
            session_id: format!("s-{agent}"),
            room: room.map(str::to_string),
            at: "2026-11-14T09:15:00Z".into(),
        }
    }

    #[test]
    fn the_chain_reads_from_the_message_to_the_task() {
        let origin = TaskOrigin {
            steps: vec![
                step(
                    "Suporte",
                    None,
                    Started::Source {
                        connector: "Slack".into(),
                        watch: "menções a mim".into(),
                        excerpt: "o CEP não calcula frete".into(),
                        link: Some("https://acme.slack.com/p1".into()),
                    },
                ),
                step(
                    "Dev",
                    Some("checkout"),
                    Started::Passed {
                        from: "Suporte".into(),
                        to: "Dev".into(),
                        request: "corrige o CEP".into(),
                    },
                ),
            ],
            opened_at: "2026-11-14T09:17:00Z".into(),
        };
        let rows = rows(&origin);
        let keys: Vec<&str> = rows.iter().map(|row| row.title.0).collect();
        assert_eq!(
            keys,
            [
                "origin.row.message",
                "origin.row.source",
                "origin.row.passed",
                "origin.row.opened"
            ]
        );
        assert_eq!(rows[0].link.as_deref(), Some("https://acme.slack.com/p1"));
        assert_eq!(rows[1].place, Some(("suporte".into(), None)));
        assert_eq!(rows[2].place, Some(("dev".into(), Some("checkout".into()))));
        assert_eq!(rows[3].at, "2026-11-14T09:17:00Z");
        assert_eq!(start_label(&origin).unwrap().0, "origin.from.source");
        // What the person asked is one row and the task.
        let asked = TaskOrigin {
            steps: vec![step(
                "Dev",
                None,
                Started::Person {
                    excerpt: "corrige".into(),
                },
            )],
            opened_at: String::new(),
        };
        assert_eq!(super::rows(&asked).len(), 2);
        assert_eq!(start_label(&asked).unwrap().0, "origin.from.person");
        assert_eq!(start_label(&TaskOrigin::default()), None);
    }
}
