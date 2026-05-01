use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct SessionId(pub String);

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Turn {
	pub id: String,
	pub session_id: SessionId,
	pub role: String,
	pub content: String,
	pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Checkpoint {
	pub id: String,
	pub session_id: SessionId,
	pub label: String,
	pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Session {
	pub id: SessionId,
	pub project_id: String,
	pub provider_id: String,
	pub state: String,
	pub created_at: String,
	pub updated_at: String,
}
