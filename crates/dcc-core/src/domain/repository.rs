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
    /// Optional user-facing project name. The technical repository name stays
    /// stable so re-discovery never overwrites the user's chosen identity.
    pub display_name: Option<String>,
    /// Optional visual identity selected from DCC's controlled icon catalog.
    pub icon: Option<String>,
    /// Optional visual identity selected from DCC's controlled color palette.
    pub color: Option<String>,
    /// When present, this project is promoted ahead of unpinned projects in the sidebar.
    pub pinned_at: Option<String>,
    pub root_path: String,
    pub base_branch: String,
    pub remote: Option<String>,
    pub remote_url: Option<String>,
    pub forge_provider: Option<String>,
    pub forge_login: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
