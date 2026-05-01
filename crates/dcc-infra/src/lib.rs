pub mod db;
pub mod events;
pub mod fs;
pub mod git;
pub mod process;

pub const PHASE_0A_MODULES: [&str; 5] = ["db", "events", "fs", "git", "process"];
