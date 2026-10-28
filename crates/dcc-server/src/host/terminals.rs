//! The shells for the native app's terminal dock (`http_api::terminals`).

use serde_json::Value;

use crate::command_api::ApiError;
use crate::http_api::DesktopTerminals;
use crate::terminals::{self, TerminalEvents, TerminalState};

pub struct HostTerminals {
    pub state: TerminalState,
    pub events: TerminalEvents,
}

fn message(error: ApiError) -> String {
    error.message
}

fn ok(value: Value) -> bool {
    value.get("ok").and_then(Value::as_bool).unwrap_or(false)
}

impl DesktopTerminals for HostTerminals {
    fn get_or_create(&self, owner_key: &str, options: Value) -> Result<Value, String> {
        terminals::terminal_get_or_create(
            &self.state,
            self.events.clone(),
            owner_key.to_string(),
            options,
        )
        .map_err(message)
    }

    fn write(&self, pty_id: &str, data: String) -> Result<bool, String> {
        terminals::terminal_write(&self.state, pty_id.to_string(), data)
            .map(ok)
            .map_err(message)
    }

    fn resize(&self, pty_id: &str, cols: u16, rows: u16) -> Result<bool, String> {
        terminals::terminal_resize(&self.state, pty_id.to_string(), cols, rows)
            .map(ok)
            .map_err(message)
    }

    fn kill(&self, pty_id: &str) -> Result<bool, String> {
        terminals::terminal_kill(&self.state, self.events.clone(), pty_id.to_string())
            .map(ok)
            .map_err(message)
    }

    fn activity(&self) -> Result<Vec<Value>, String> {
        terminals::terminal_list_activity(&self.state).map_err(message)
    }
}
