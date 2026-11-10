//! What goes on in each room, for the dot next to its name in the team's
//! column. Only the open room has its feeds loaded, so this reads what the
//! sidebar already knows: where each conversation is and which project each
//! task belongs to. The rule is `dcc_core::domain::room_presence`, shared
//! with the phone's list of rooms.

pub use dcc_core::domain::room_presence::{Presence, Where, Work, presence};
