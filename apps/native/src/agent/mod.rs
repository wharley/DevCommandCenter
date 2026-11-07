//! A resident agent's face: the DCC mascot in the agent's color, arms and
//! eyes, showing what it is doing.

mod avatar;
mod mood;
mod robot;
mod shapes;

pub use avatar::AgentAvatarView;
pub use mood::{MascotBlink, Mood, MoodAvatarView};
pub use robot::{AgentMascot, RobotState, robot_key};
