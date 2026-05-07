use serde::{Deserialize, Serialize};
use specta::Type;

use super::project::ProjectId;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct RepositoryId(pub String);

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub id: RepositoryId,
    pub project_id: ProjectId,
    pub name: String,
    pub root_path: String,
    pub base_branch: String,
    pub created_at: String,
    pub updated_at: String,
}
