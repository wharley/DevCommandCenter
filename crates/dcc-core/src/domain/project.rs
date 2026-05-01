use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct ProjectId(pub String);

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Project {
	pub id: ProjectId,
	pub title: String,
	pub workspace_root: String,
	pub created_at: String,
	pub updated_at: String,
}
