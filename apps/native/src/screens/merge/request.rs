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
    /// How "Resolver arquivo com agente" starts its conversation: the task
    /// and the composer's pick (none: the button explains why).
    pub agent: Option<AgentPick>,
}

/// The task's thread start and the provider and model the composer picks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPick {
    pub start: crate::data::conversation::ThreadStart,
    pub provider_id: String,
    pub model: Option<String>,
}
