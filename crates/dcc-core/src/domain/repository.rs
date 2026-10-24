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
    /// Optional project logo the person uploaded: a small PNG data URI. Shown
    /// when `icon` is `"logo"`.
    #[serde(default)]
    pub logo: Option<String>,
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

/// `repositoryDisplayName`: the name the person gave, else the repository's,
/// else the folder's.
pub fn repository_display_name(repository: &Repository) -> String {
    repository
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| Some(repository.name.trim().to_string()).filter(|name| !name.is_empty()))
        .or_else(|| path_leaf(&repository.root_path))
        .unwrap_or_else(|| "Project".to_string())
}

/// The last segment of a path, either separator.
pub fn path_leaf(path: &str) -> Option<String> {
    path.trim()
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .map(str::to_string)
}
