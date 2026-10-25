/// What the resolver opens on (`{ kind: "merge-conflict", … }`): the
/// checkout, its task, the base the merge brings in and who acts on the
/// forge, and how the workspace is called in the labels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeResolverRequest {
    pub root: String,
    pub task_id: Option<String>,
    pub base_branch: Option<String>,
    pub forge_login: Option<String>,
    pub workspace_label: Option<String>,
}
