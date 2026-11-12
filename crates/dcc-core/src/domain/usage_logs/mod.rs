//! Usage read from the local logs of Claude Code and Codex — every call
//! and turn on this machine, inside DCC or not — and the dashboard's
//! numbers over them (`aggregate`). The logs are parsed and kept by
//! `dcc_infra::usage_logs`; this module only knows the rows.

mod aggregate;
#[cfg(test)]
mod tests;

pub use aggregate::{
    aggregate, Band, DayBar, PeakDay, PeakHour, UsageFilter, UsageSummary, OTHERS_GROUP,
};

/// Which harness wrote the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Runtime {
    Claude,
    Codex,
}

impl Runtime {
    pub const ALL: [Runtime; 2] = [Runtime::Claude, Runtime::Codex];

    pub fn as_str(self) -> &'static str {
        match self {
            Runtime::Claude => "claude",
            Runtime::Codex => "codex",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "claude" => Some(Runtime::Claude),
            "codex" => Some(Runtime::Codex),
            _ => None,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// How the harness was started.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Via {
    Cli,
    Desktop,
    Ide,
    Sdk,
    Exec,
    Tui,
    Dcc,
    Other,
}

impl Via {
    pub const ALL: [Via; 8] = [
        Via::Cli,
        Via::Desktop,
        Via::Ide,
        Via::Sdk,
        Via::Exec,
        Via::Tui,
        Via::Dcc,
        Via::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Via::Cli => "cli",
            Via::Desktop => "desktop",
            Via::Ide => "ide",
            Via::Sdk => "sdk",
            Via::Exec => "exec",
            Via::Tui => "tui",
            Via::Dcc => "dcc",
            Via::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Self {
        Via::ALL
            .into_iter()
            .find(|via| via.as_str() == value)
            .unwrap_or(Via::Other)
    }
}

/// A call (one model response) or a turn (one prompt from a person).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Call,
    Turn,
}

/// One call's tokens, the same buckets for both harnesses: `input` is the
/// fresh input only (Codex's input minus its cached and written part),
/// `reasoning` (thinking) is already inside `output`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TokenBuckets {
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
    pub reasoning: u64,
}

impl TokenBuckets {
    /// Everything the model read and wrote.
    pub fn total(&self) -> u64 {
        self.input + self.cache_read + self.cache_write + self.output
    }

    /// Input + cache written + output: what was not a cache read.
    pub fn new_tokens(&self) -> u64 {
        self.input + self.cache_write + self.output
    }

    pub fn add(&mut self, other: &TokenBuckets) {
        self.input += other.input;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
        self.output += other.output;
        self.reasoning += other.reasoning;
    }
}

/// One parsed log event as the indexer stores it.
#[derive(Clone, Debug, PartialEq)]
pub struct UsageEvent {
    /// Unique across files: the same response repeated (Claude writes it
    /// once per content block, a resumed session copies its history) is
    /// one row.
    pub key: String,
    pub kind: EventKind,
    /// Unix milliseconds, UTC.
    pub at_ms: i64,
    pub runtime: Runtime,
    pub model: String,
    pub via: Via,
    pub project: String,
    /// The subagent's name; empty for the main agent.
    pub agent: String,
    pub session: String,
    pub subagent: bool,
    pub tokens: TokenBuckets,
}

/// A stored row for aggregation, its strings interned in `UsageRows`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UsageRow {
    pub at_ms: i64,
    pub kind: EventKind,
    pub runtime: Runtime,
    pub via: Via,
    pub subagent: bool,
    pub model: u32,
    pub project: u32,
    pub agent: u32,
    pub session: u32,
    pub tokens: TokenBuckets,
}

/// The rows of a period and the strings they point at.
#[derive(Clone, Debug, Default)]
pub struct UsageRows {
    pub rows: Vec<UsageRow>,
    pub strings: Vec<String>,
    index: std::collections::HashMap<String, u32>,
}

impl UsageRows {
    pub fn intern(&mut self, value: &str) -> u32 {
        if let Some(id) = self.index.get(value) {
            return *id;
        }
        let id = self.strings.len() as u32;
        self.strings.push(value.to_string());
        self.index.insert(value.to_string(), id);
        id
    }

    pub fn string(&self, id: u32) -> &str {
        self.strings.get(id as usize).map_or("", String::as_str)
    }

    pub fn push(&mut self, event: &UsageEvent) {
        let row = UsageRow {
            at_ms: event.at_ms,
            kind: event.kind,
            runtime: event.runtime,
            via: event.via,
            subagent: event.subagent,
            model: self.intern(&event.model),
            project: self.intern(&event.project),
            agent: self.intern(&event.agent),
            session: self.intern(&event.session),
            tokens: event.tokens,
        };
        self.rows.push(row);
    }
}

/// What the charts count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Metric {
    #[default]
    TotalTokens,
    NewTokens,
    Output,
    Cost,
    Calls,
    Turns,
}

impl Metric {
    pub const ALL: [Metric; 6] = [
        Metric::TotalTokens,
        Metric::NewTokens,
        Metric::Output,
        Metric::Cost,
        Metric::Calls,
        Metric::Turns,
    ];
}

/// How the days' bars are split.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GroupBy {
    #[default]
    Runtime,
    Model,
    Via,
    Project,
    Agent,
}

impl GroupBy {
    pub const ALL: [GroupBy; 5] = [
        GroupBy::Runtime,
        GroupBy::Model,
        GroupBy::Via,
        GroupBy::Project,
        GroupBy::Agent,
    ];
}

/// Main agents, subagents or both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    All,
    Main,
    Subagents,
}
