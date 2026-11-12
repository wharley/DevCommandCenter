//! The usage of Claude Code and Codex on this machine, read from their
//! local logs into a small index of its own (`index.rs`): every call and
//! turn, inside DCC or not. `dcc_core::domain::usage_logs` turns the rows
//! into the dashboard.

pub mod claude;
pub mod codex;
mod index;
mod sources;
#[cfg(test)]
mod tests;

pub use index::{IndexError, RefreshStats, UsageIndex};
pub use sources::{codex_via, discover, DccHints, Resolver, Sources};

/// The index's file, beside DCC's database.
pub fn index_path(dcc_db: &std::path::Path) -> std::path::PathBuf {
    dcc_db.with_file_name("usage-logs.sqlite")
}
