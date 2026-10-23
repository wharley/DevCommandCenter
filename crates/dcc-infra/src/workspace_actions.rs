//! The task writes the sidebar makes (rename, pin, snooze, put on hold,
//! complete, restore), with the rules and messages the desktop shows. Shared
//! by the Tauri commands and the native app.

use chrono::Utc;
use dcc_core::domain::workspace::{Workspace, WorkspaceId, WorkspaceState};
use dcc_core::ports::{WorkspaceBundleRepo, WorkspaceRepo};

use crate::db::SqliteWorkspaceRepo;

pub fn normalize_workspace_name(name: &str) -> Result<String, String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err("workspace name cannot be empty".to_string());
    }
    if name.chars().count() > 120 {
        return Err("workspace name cannot exceed 120 characters".to_string());
    }
    Ok(name)
}

/// A snooze instant in the future (at most 60 days), or `None` to wake.
pub fn normalize_snooze_until(
    until: Option<&str>,
    now: chrono::DateTime<Utc>,
    state: &WorkspaceState,
) -> Result<Option<String>, String> {
    let Some(until) = until.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if matches!(state, WorkspaceState::Archived | WorkspaceState::Completed) {
        return Err("only active tasks can be snoozed".to_string());
    }
    let at = chrono::DateTime::parse_from_rfc3339(until)
        .map_err(|_| "snooze time must be an RFC 3339 timestamp".to_string())?
        .with_timezone(&Utc);
    if at <= now {
        return Err("snooze time must be in the future".to_string());
    }
    if at > now + chrono::Duration::days(60) {
        return Err("a task can be snoozed for at most 60 days".to_string());
    }
    Ok(Some(at.to_rfc3339()))
}

/// Members of a multi-project task change state through the task itself.
pub async fn ensure_workspace_is_not_a_bundle_member(
    repo: &SqliteWorkspaceRepo,
    workspace_id: &WorkspaceId,
    operation: &str,
) -> Result<(), String> {
    let Some(summary) = repo
        .get_workspace_bundle_for_workspace(workspace_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    Err(format!(
        "workspace {} belongs to multi-workspace '{}' ({}); {operation} the multi-workspace instead",
        workspace_id.0, summary.bundle.name, summary.bundle.id.0
    ))
}

async fn load(repo: &SqliteWorkspaceRepo, id: &WorkspaceId) -> Result<Workspace, String> {
    repo.get_workspace(id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("workspace not found: {}", id.0))
}

async fn save(repo: &SqliteWorkspaceRepo, workspace: &Workspace) -> Result<(), String> {
    repo.save_workspace(workspace)
        .await
        .map_err(|error| error.to_string())
}

/// Renames a task; a multi-project task is renamed through its primary.
pub async fn rename_workspace(
    repo: &SqliteWorkspaceRepo,
    workspace_id: String,
    name: &str,
) -> Result<Workspace, String> {
    let name = normalize_workspace_name(name)?;
    let id = WorkspaceId(workspace_id);
    let mut workspace = load(repo, &id).await?;
    let now = Utc::now().to_rfc3339();
    if let Some(mut summary) = repo
        .get_workspace_bundle_for_workspace(&id)
        .await
        .map_err(|error| error.to_string())?
    {
        if summary.bundle.primary_workspace_id != id {
            return Err("only the primary multi-project workspace can rename the task".to_string());
        }
        summary.bundle.name = name.clone();
        summary.bundle.updated_at = now.clone();
        repo.save_workspace_bundle(&summary.bundle, &summary.members)
            .await
            .map_err(|error| error.to_string())?;
    }
    workspace.name = Some(name);
    workspace.updated_at = now;
    save(repo, &workspace).await?;
    Ok(workspace)
}

pub async fn set_workspace_pinned(
    repo: &SqliteWorkspaceRepo,
    workspace_id: String,
    pinned: bool,
) -> Result<Workspace, String> {
    let mut workspace = load(repo, &WorkspaceId(workspace_id)).await?;
    if pinned
        && matches!(
            workspace.state,
            WorkspaceState::Archived | WorkspaceState::Completed
        )
    {
        return Err("only active tasks can be pinned".to_string());
    }
    workspace.pinned_at = pinned.then(|| Utc::now().to_rfc3339());
    save(repo, &workspace).await?;
    Ok(workspace)
}

/// Snoozes an active task until `until`, or wakes it (`None`).
pub async fn set_workspace_snooze(
    repo: &SqliteWorkspaceRepo,
    workspace_id: String,
    until: Option<&str>,
) -> Result<Workspace, String> {
    let mut workspace = load(repo, &WorkspaceId(workspace_id)).await?;
    workspace.snoozed_until = normalize_snooze_until(until, Utc::now(), &workspace.state)?;
    save(repo, &workspace).await?;
    Ok(workspace)
}

/// "Em espera": paused, and unpinned.
pub async fn archive_workspace(
    repo: &SqliteWorkspaceRepo,
    workspace_id: String,
) -> Result<Workspace, String> {
    let id = WorkspaceId(workspace_id);
    ensure_workspace_is_not_a_bundle_member(repo, &id, "archive").await?;
    let mut workspace = load(repo, &id).await?;
    workspace.state = WorkspaceState::Archived;
    workspace.pinned_at = None;
    workspace.updated_at = Utc::now().to_rfc3339();
    save(repo, &workspace).await?;
    Ok(workspace)
}

/// Completed and unpinned. The desktop then checkpoints the task's memory.
pub async fn complete_workspace(
    repo: &SqliteWorkspaceRepo,
    workspace_id: String,
) -> Result<Workspace, String> {
    let id = WorkspaceId(workspace_id);
    ensure_workspace_is_not_a_bundle_member(repo, &id, "complete").await?;
    let mut workspace = load(repo, &id).await?;
    workspace.state = WorkspaceState::Completed;
    workspace.pinned_at = None;
    workspace.updated_at = Utc::now().to_rfc3339();
    save(repo, &workspace).await?;
    Ok(workspace)
}

/// Back to its project as ready.
pub async fn restore_workspace(
    repo: &SqliteWorkspaceRepo,
    workspace_id: String,
) -> Result<Workspace, String> {
    let id = WorkspaceId(workspace_id);
    ensure_workspace_is_not_a_bundle_member(repo, &id, "restore").await?;
    let mut workspace = load(repo, &id).await?;
    workspace.state = WorkspaceState::Ready;
    save(repo, &workspace).await?;
    Ok(workspace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(
            normalize_workspace_name("  Fix   login ").unwrap(),
            "Fix login"
        );
        assert!(normalize_workspace_name("   ").is_err());
        assert!(normalize_workspace_name(&"a".repeat(121)).is_err());
    }

    #[test]
    fn snoozes_must_be_ahead_and_within_sixty_days() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-23T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let ready = WorkspaceState::Ready;
        assert_eq!(normalize_snooze_until(None, now, &ready), Ok(None));
        assert!(normalize_snooze_until(Some("2026-10-23T11:00:00Z"), now, &ready).is_err());
        assert!(normalize_snooze_until(Some("2027-01-01T00:00:00Z"), now, &ready).is_err());
        assert!(
            normalize_snooze_until(Some("2026-10-24T09:00:00Z"), now, &WorkspaceState::Archived)
                .is_err()
        );
        assert!(
            normalize_snooze_until(Some("2026-10-24T09:00:00Z"), now, &ready)
                .unwrap()
                .is_some()
        );
    }
}
