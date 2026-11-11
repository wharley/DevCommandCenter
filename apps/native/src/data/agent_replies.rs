//! The replies agents proposed as the app knows them
//! (`dcc_core::domain::agent_reply`): read from DCC's database when an
//! agent's turn ends and on the sources' minute, into a global the room
//! reads. Sending and discarding go through the desktop, which owns the
//! table and the connectors.

use dcc_core::domain::agent_reply::AgentReply;
use gpui::Global;
use serde_json::json;

use crate::data::desktop_api::{ApiError, DesktopApi};

#[derive(Default)]
pub struct Replies {
    pub list: Vec<AgentReply>,
}

impl Global for Replies {}

impl Replies {
    /// The replies waiting in `room` (none: Geral).
    pub fn in_room<'a>(&'a self, room: &'a Option<String>) -> impl Iterator<Item = &'a AgentReply> {
        self.list.iter().filter(move |reply| &reply.room == room)
    }
}

/// The replies waiting for the person, from DCC's database.
pub fn read() -> Vec<AgentReply> {
    let db = crate::data::loader::database_path();
    dcc_infra::db::SqliteSessionRepo::open_read_only(&db)
        .and_then(|repo| repo.list_pending_agent_replies())
        .unwrap_or_default()
}

impl DesktopApi {
    /// "Enviar": the connector answers in its own time.
    pub fn send_reply(&self, id: &str, text: &str) -> Result<(), ApiError> {
        self.request_waiting(
            "POST",
            "/api/v1/mobile/room/replies",
            Some(&json!({ "id": id, "action": "send", "text": text })),
            std::time::Duration::from_secs(60),
        )
        .map(|_: serde_json::Value| ())
    }

    pub fn discard_reply(&self, id: &str) -> Result<(), ApiError> {
        self.post(
            "/api/v1/mobile/room/replies",
            &json!({ "id": id, "action": "discard" }),
        )
        .map(|_| ())
    }
}
