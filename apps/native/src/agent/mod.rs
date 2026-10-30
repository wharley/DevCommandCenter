//! A resident agent's face: the DCC mascot in the agent's color, arms and
//! eyes, showing what it is doing.

mod avatar;
mod robot;

pub use avatar::AgentAvatarView;
pub use robot::{AgentMascot, RobotState, robot_key};
