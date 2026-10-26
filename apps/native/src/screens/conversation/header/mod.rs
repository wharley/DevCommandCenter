//! The conversation's header (`DccWorkbenchChatHeader`): the title, then
//! the actions in the desktop's order — resume, the editor, the terminal
//! with its count, the Browser, the delivery, find, session search, a new
//! conversation, the history; the agent's mascot beside the title and the
//! open conversation's delegations, its ai-memory and the buttons that
//! call a resident agent in the task.

mod actions;
mod ai_memory;
pub(crate) mod call_agents;
mod calling;
pub(crate) mod delivery;
mod editor;
mod history;
mod lineage;
mod view;

pub(crate) use view::header;

/// Programs running in the task's terminals and in all of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalCounts {
    pub current: usize,
    pub total: usize,
}

/// "Fechar conversa?" while a turn runs in it (`PendingSessionClose`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingClose {
    pub session_id: String,
    pub title: String,
    /// It never had a turn: it goes with its history.
    pub delete_history: bool,
    pub requires_abort: bool,
}

#[derive(Default)]
pub struct HeaderState {
    pub terminals: TerminalCounts,
    pub browser_open: bool,
    pub history_open: bool,
    /// "Nova sessão" is starting one.
    pub starting: bool,
    /// The conversation being closed or restored.
    pub busy_session: Option<String>,
    pub pending_close: Option<PendingClose>,
    /// The preferred editor's id (`dcc-preferred-editor`).
    pub editor: &'static str,
    /// The agent being called (one at a time).
    pub calling: Option<String>,
    /// The lineage menu is open.
    pub lineage_open: bool,
    /// The delegation being cancelled from it.
    pub cancelling: Option<String>,
    /// The open conversation's ai-memory: export, sources, curation.
    pub ai_memory: ai_memory::AiMemory,
    /// The agent each session runs as, from the sidebar's agents.
    pub agents:
        std::sync::Arc<std::collections::HashMap<String, crate::data::agents::SessionAgent>>,
}
