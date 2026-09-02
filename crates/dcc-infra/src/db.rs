use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::{fmt, path::Path};

use async_trait::async_trait;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Row};
use serde::de::DeserializeOwned;
use serde_json::{from_str, to_string};

#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
use crate::guarded_undo::macos_store::{MacArtifactStore, OrphanRecoveryReport};

use dcc_core::{
    domain::{
        delegation::{
            Delegation, DelegationBudget, DelegationContextPolicy, DelegationId, DelegationMode,
            DelegationStatus,
        },
        delegation_apply::{
            DelegationApplyTransaction, DelegationApplyTransactionId,
            DelegationApplyTransactionState,
        },
        delegation_worktree::{
            DelegationWorktreeOperation, DelegationWorktreeOperationId,
            DelegationWorktreeOperationState,
        },
        guarded_undo::{
            validate_restore_set_manifest, ArtifactKey, GitIdentityV1, GuardedUndoReasonCode,
            OpaqueRepoPath, PhysicalRootId, PreparedIdentityV1, RecoveryDetailsV1,
            RegularFileMetadataV1, RestoreSetId, RestoreSetState, Sha256Digest, TurnRestoreFile,
            TurnRestoreSet, UndoOperation, UndoOperationFile, UndoOperationFileState,
            UndoOperationId, UndoOperationState, VerificationOutcome, MAX_RESTORE_FILES,
            RESTORE_CAPTURE_VERSION, UNDO_JOURNAL_SCHEMA_VERSION,
        },
        project::ProjectId,
        repository::{Repository, RepositoryId},
        session::{
            AssistantMessagePhase, Session, SessionEventKind, SessionEventRecord, SessionId,
            SessionProjection, SessionSearchResult, SessionState, TurnChangeSet, TurnId,
            WorkspaceSessionSummary,
        },
        thread::{Thread, ThreadId},
        usage::{
            DailyUsageSummary, ModelTokenUsage, ModelUsageSummary, ProviderUsageSummary,
            UsageDashboard, UsageDashboardInput, UsageTotals,
        },
        workspace::{Workspace, WorkspaceId, WorkspaceSource, WorkspaceState},
        workspace_bundle::{
            WorkspaceBundle, WorkspaceBundleId, WorkspaceBundleMember, WorkspaceBundleState,
            WorkspaceBundleSummary,
        },
    },
    ports::{
        AppendEventOutcome, DelegationApplyTransactionRepo, DelegationRepo,
        DelegationWorktreeOperationRepo, RepositoryRepo, SessionEventRepo, SessionRepo, ThreadRepo,
        UsageRepo, WorkspaceBundleRepo, WorkspaceRepo,
    },
    Result,
};

const WORKSPACE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_workspaces (
	id TEXT PRIMARY KEY NOT NULL,
	project_id TEXT NOT NULL,
	name TEXT NULL,
	root_path TEXT NOT NULL,
	base_branch TEXT NOT NULL,
	worktree_path TEXT NULL,
	source_json TEXT NULL,
	state TEXT NOT NULL,
	setup_report_json TEXT NULL,
	pinned_at TEXT NULL,
	created_at TEXT NOT NULL,
	updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_dcc_workspaces_project_id
ON dcc_workspaces(project_id);
"#;

const REPOSITORY_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_repositories (
	id TEXT PRIMARY KEY NOT NULL,
	project_id TEXT NOT NULL,
	name TEXT NOT NULL,
	display_name TEXT NULL,
	icon TEXT NULL,
	color TEXT NULL,
	pinned_at TEXT NULL,
	root_path TEXT NOT NULL UNIQUE,
	base_branch TEXT NOT NULL,
	remote TEXT NULL,
	remote_url TEXT NULL,
	forge_provider TEXT NULL,
	forge_login TEXT NULL,
	created_at TEXT NOT NULL,
	updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_dcc_repositories_project_id
ON dcc_repositories(project_id);

CREATE INDEX IF NOT EXISTS idx_dcc_repositories_updated_at
ON dcc_repositories(updated_at DESC);
"#;

const WORKSPACE_BUNDLE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_workspace_bundles (
	id TEXT PRIMARY KEY NOT NULL,
	name TEXT NOT NULL,
	primary_workspace_id TEXT NOT NULL,
	state TEXT NOT NULL,
	created_at TEXT NOT NULL,
	updated_at TEXT NOT NULL,
	FOREIGN KEY (primary_workspace_id) REFERENCES dcc_workspaces(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dcc_workspace_bundles_updated_at
ON dcc_workspace_bundles(updated_at DESC);

CREATE TABLE IF NOT EXISTS dcc_workspace_bundle_members (
	bundle_id TEXT NOT NULL,
	workspace_id TEXT NOT NULL UNIQUE,
	created_for_bundle INTEGER NOT NULL DEFAULT 1,
	position INTEGER NOT NULL DEFAULT 0,
	-- Compatibility columns retained for databases created by the previous prototype.
	role TEXT NOT NULL DEFAULT 'contributor',
	allow_analysis INTEGER NOT NULL DEFAULT 1,
	allow_implementation INTEGER NOT NULL DEFAULT 1,
	workspace_state_before_archive TEXT NULL,
	PRIMARY KEY (bundle_id, workspace_id),
	FOREIGN KEY (bundle_id) REFERENCES dcc_workspace_bundles(id) ON DELETE CASCADE,
	FOREIGN KEY (workspace_id) REFERENCES dcc_workspaces(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dcc_workspace_bundle_members_bundle_position
ON dcc_workspace_bundle_members(bundle_id, position);
"#;

const FORGE_LOGIN_PREFERENCE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_forge_login_preferences (
	provider TEXT NOT NULL,
	host TEXT NOT NULL,
	login TEXT NOT NULL,
	PRIMARY KEY (provider, host)
);
"#;

const SESSION_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_sessions (
	id TEXT PRIMARY KEY NOT NULL,
	project_id TEXT NOT NULL,
	workspace_id TEXT NOT NULL,
	provider_id TEXT NOT NULL,
	model TEXT NULL,
	provider_runtime_json TEXT NULL,
	working_directory_override TEXT NULL,
	additional_workspace_ids_json TEXT NOT NULL DEFAULT '[]',
	state TEXT NOT NULL,
	created_at TEXT NOT NULL,
	updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_dcc_sessions_workspace_id
	ON dcc_sessions(workspace_id);

CREATE TABLE IF NOT EXISTS dcc_threads (
	id TEXT PRIMARY KEY NOT NULL,
	project_id TEXT NOT NULL,
	session_id TEXT NULL UNIQUE,
	title TEXT NOT NULL,
	archived_at TEXT NULL,
	FOREIGN KEY (session_id) REFERENCES dcc_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dcc_threads_session_id
	ON dcc_threads(session_id);

CREATE INDEX IF NOT EXISTS idx_dcc_threads_archived_at
	ON dcc_threads(archived_at);

CREATE TABLE IF NOT EXISTS dcc_session_events (
	event_id TEXT PRIMARY KEY NOT NULL,
	session_id TEXT NOT NULL,
	sequence INTEGER NOT NULL,
	occurred_at TEXT NOT NULL,
	kind_json TEXT NOT NULL,
	terminal_turn_id TEXT NULL,
	terminal_kind TEXT NULL,
	UNIQUE(session_id, sequence),
	CHECK((terminal_turn_id IS NULL AND terminal_kind IS NULL)
	      OR (terminal_turn_id IS NOT NULL AND terminal_kind IN ('completed', 'aborted'))),
	FOREIGN KEY (session_id) REFERENCES dcc_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dcc_session_events_session_sequence
	ON dcc_session_events(session_id, sequence);

CREATE VIRTUAL TABLE IF NOT EXISTS dcc_session_search USING fts5(
	session_id UNINDEXED,
	workspace_id UNINDEXED,
	project_id UNINDEXED,
	thread_title,
	search_text,
	provider_id UNINDEXED,
	model UNINDEXED,
	archived_at UNINDEXED,
	created_at UNINDEXED,
	updated_at UNINDEXED,
	tokenize = 'unicode61'
);
"#;

/// Durable browser location metadata is deliberately kept separate from the
/// session projection. It contains only a caller-sanitized URL and expires
/// opportunistically; no page data, browser state, or capability is stored.
const BROWSER_LOCATION_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_browser_locations (
	workspace_id TEXT NOT NULL CHECK(length(workspace_id) BETWEEN 1 AND 128),
	session_id TEXT NOT NULL DEFAULT '' CHECK(length(session_id) <= 128),
	safe_url TEXT NOT NULL CHECK(length(safe_url) BETWEEN 1 AND 2048),
	saved_at_ms INTEGER NOT NULL CHECK(saved_at_ms >= 0),
	expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms > saved_at_ms),
	PRIMARY KEY (workspace_id, session_id)
);

CREATE INDEX IF NOT EXISTS idx_dcc_browser_locations_expires_at
	ON dcc_browser_locations(expires_at_ms);
"#;

/// Provider enablement is independent from the legacy application-level
/// `providers.is_active` table. This row only controls DCC's registered
/// runtime authority for one physical session database.
const PROVIDER_AVAILABILITY_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_provider_availability (
	provider_id TEXT PRIMARY KEY NOT NULL CHECK(length(provider_id) BETWEEN 1 AND 128),
	enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
	generation INTEGER NOT NULL CHECK(generation >= 0),
	updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0)
);
"#;

const PROVIDER_AVAILABILITY_MAX_PROVIDER_ID_CHARS: usize = 128;

/// Typed durable provider availability. The absence of a record represents
/// the backwards-compatible enabled state at generation zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderAvailabilityRecord {
    pub provider_id: String,
    pub enabled: bool,
    pub generation: u64,
    pub updated_at_ms: u64,
}

pub const DEFAULT_BROWSER_LOCATION_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;
const BROWSER_LOCATION_MAX_SCOPE_CHARS: usize = 128;
const BROWSER_LOCATION_MAX_URL_CHARS: usize = 2048;

const USAGE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_turn_model_usage (
	session_id TEXT NOT NULL,
	turn_id TEXT NOT NULL,
	model TEXT NOT NULL,
	input_tokens INTEGER NOT NULL DEFAULT 0,
	output_tokens INTEGER NOT NULL DEFAULT 0,
	cached_input_tokens INTEGER NOT NULL DEFAULT 0,
	cache_write_input_tokens INTEGER NOT NULL DEFAULT 0,
	reasoning_output_tokens INTEGER NOT NULL DEFAULT 0,
	total_tokens INTEGER NOT NULL DEFAULT 0,
	cost_usd REAL NULL,
	recorded_at TEXT NOT NULL,
	PRIMARY KEY (session_id, turn_id, model),
	FOREIGN KEY (session_id) REFERENCES dcc_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dcc_turn_model_usage_recorded_at
	ON dcc_turn_model_usage(recorded_at);

CREATE INDEX IF NOT EXISTS idx_dcc_turn_model_usage_session
	ON dcc_turn_model_usage(session_id);

CREATE INDEX IF NOT EXISTS idx_dcc_sessions_created_at
	ON dcc_sessions(created_at);

CREATE INDEX IF NOT EXISTS idx_dcc_session_events_completed_occurred_at
	ON dcc_session_events(occurred_at, session_id)
	WHERE json_extract(kind_json, '$.type') = 'turn_completed';
"#;

const TURN_CHANGE_SET_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_turn_change_sets (
    snapshot_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    capture_version INTEGER NOT NULL,
    state TEXT NOT NULL,
    base_tree TEXT NULL,
    result_tree TEXT NULL,
    baseline_untracked_json TEXT NOT NULL DEFAULT '[]',
    result_untracked_json TEXT NOT NULL DEFAULT '[]',
    files_json TEXT NOT NULL DEFAULT '[]',
    file_diffs_json TEXT NOT NULL DEFAULT '{}',
    observed_validations_json TEXT NOT NULL DEFAULT '[]',
    diff_truncated INTEGER NOT NULL DEFAULT 0,
    turn_outcome TEXT NULL,
    outcome_reason TEXT NULL,
    error TEXT NULL,
    created_at TEXT NOT NULL,
    completed_at TEXT NULL,
    UNIQUE(session_id, turn_id, workspace_id),
    FOREIGN KEY (session_id) REFERENCES dcc_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (workspace_id) REFERENCES dcc_workspaces(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dcc_turn_change_sets_session_completed
ON dcc_turn_change_sets(session_id, completed_at DESC);
"#;

const GUARDED_UNDO_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_turn_restore_sets (
    restore_set_id TEXT PRIMARY KEY NOT NULL,
    snapshot_id TEXT NOT NULL UNIQUE,
    session_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    root_id BLOB NULL CHECK(root_id IS NULL OR (typeof(root_id) = 'blob' AND length(root_id) BETWEEN 3 AND 1024)),
    capture_version INTEGER NOT NULL CHECK(capture_version > 0),
    state TEXT NOT NULL CHECK(length(state) BETWEEN 1 AND 64),
    reason_code TEXT NULL CHECK(reason_code IS NULL OR length(reason_code) BETWEEN 1 AND 64),
    git_identity_json TEXT NULL CHECK(git_identity_json IS NULL OR length(git_identity_json) BETWEEN 1 AND 65536),
    artifact_bytes INTEGER NOT NULL DEFAULT 0 CHECK(artifact_bytes >= 0),
    file_count INTEGER NOT NULL DEFAULT 0 CHECK(file_count >= 0),
    manifest_digest BLOB NULL CHECK(manifest_digest IS NULL OR (typeof(manifest_digest) = 'blob' AND length(manifest_digest) = 32)),
    created_at TEXT NOT NULL,
    completed_at TEXT NULL,
    expires_at TEXT NULL,
    FOREIGN KEY (snapshot_id) REFERENCES dcc_turn_change_sets(snapshot_id) ON DELETE RESTRICT,
    FOREIGN KEY (session_id) REFERENCES dcc_sessions(id) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id) REFERENCES dcc_workspaces(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_dcc_turn_restore_sets_workspace_state
ON dcc_turn_restore_sets(workspace_id, state, completed_at DESC);

CREATE INDEX IF NOT EXISTS idx_dcc_turn_restore_sets_session_turn
ON dcc_turn_restore_sets(session_id, turn_id);

CREATE TABLE IF NOT EXISTS dcc_turn_restore_files (
    restore_set_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    path_bytes BLOB NOT NULL CHECK(typeof(path_bytes) = 'blob' AND length(path_bytes) BETWEEN 3 AND 4096),
    status TEXT NOT NULL CHECK(length(status) BETWEEN 1 AND 64),
    pre_size INTEGER NOT NULL CHECK(pre_size >= 0),
    pre_sha256 BLOB NOT NULL CHECK(typeof(pre_sha256) = 'blob' AND length(pre_sha256) = 32),
    pre_artifact_key BLOB NOT NULL CHECK(typeof(pre_artifact_key) = 'blob' AND length(pre_artifact_key) = 16),
    result_size INTEGER NOT NULL CHECK(result_size >= 0),
    result_sha256 BLOB NOT NULL CHECK(typeof(result_sha256) = 'blob' AND length(result_sha256) = 32),
    metadata_fingerprint_json TEXT NOT NULL CHECK(length(metadata_fingerprint_json) BETWEEN 1 AND 4096),
    PRIMARY KEY (restore_set_id, ordinal),
    UNIQUE (restore_set_id, path_bytes),
    UNIQUE (restore_set_id, pre_artifact_key),
    FOREIGN KEY (restore_set_id) REFERENCES dcc_turn_restore_sets(restore_set_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS dcc_undo_operations (
    operation_id TEXT PRIMARY KEY NOT NULL,
    restore_set_id TEXT NOT NULL,
    journal_version INTEGER NOT NULL CHECK(journal_version > 0),
    state TEXT NOT NULL CHECK(length(state) BETWEEN 1 AND 64),
    active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0, 1)),
    cleanup_pending INTEGER NOT NULL DEFAULT 1 CHECK(cleanup_pending IN (0, 1)),
    preview_token_digest BLOB NULL CHECK(preview_token_digest IS NULL OR (typeof(preview_token_digest) = 'blob' AND length(preview_token_digest) = 32)),
    prepared_identity_json TEXT NOT NULL CHECK(length(prepared_identity_json) BETWEEN 1 AND 131072),
    reason_code TEXT NULL CHECK(reason_code IS NULL OR length(reason_code) BETWEEN 1 AND 64),
    recovery_details_json TEXT NULL CHECK(recovery_details_json IS NULL OR length(recovery_details_json) BETWEEN 1 AND 1024),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    completed_at TEXT NULL,
    UNIQUE (operation_id, restore_set_id),
    FOREIGN KEY (restore_set_id) REFERENCES dcc_turn_restore_sets(restore_set_id) ON DELETE RESTRICT,
    CHECK(
        (state IN ('completed', 'rolled_back', 'blocked') AND active = 0)
        OR
        (state NOT IN ('completed', 'rolled_back', 'blocked') AND active = 1)
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_dcc_undo_operations_one_active_set
ON dcc_undo_operations(restore_set_id) WHERE active = 1;

CREATE INDEX IF NOT EXISTS idx_dcc_undo_operations_state_updated
ON dcc_undo_operations(state, updated_at);

CREATE TABLE IF NOT EXISTS dcc_undo_operation_files (
    operation_id TEXT NOT NULL,
    restore_set_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    path_bytes BLOB NOT NULL CHECK(typeof(path_bytes) = 'blob' AND length(path_bytes) BETWEEN 3 AND 4096),
    exchange_artifact_key BLOB NOT NULL CHECK(typeof(exchange_artifact_key) = 'blob' AND length(exchange_artifact_key) = 16),
    expected_result_size INTEGER NOT NULL CHECK(expected_result_size >= 0),
    expected_result_sha256 BLOB NOT NULL CHECK(typeof(expected_result_sha256) = 'blob' AND length(expected_result_sha256) = 32),
    expected_metadata_json TEXT NOT NULL CHECK(length(expected_metadata_json) BETWEEN 1 AND 4096),
    pre_size INTEGER NOT NULL CHECK(pre_size >= 0),
    pre_sha256 BLOB NOT NULL CHECK(typeof(pre_sha256) = 'blob' AND length(pre_sha256) = 32),
    staged_metadata_json TEXT NULL CHECK(staged_metadata_json IS NULL OR length(staged_metadata_json) BETWEEN 1 AND 4096),
    displaced_size INTEGER NULL CHECK(displaced_size IS NULL OR displaced_size >= 0),
    displaced_sha256 BLOB NULL CHECK(displaced_sha256 IS NULL OR (typeof(displaced_sha256) = 'blob' AND length(displaced_sha256) = 32)),
    displaced_metadata_json TEXT NULL CHECK(displaced_metadata_json IS NULL OR length(displaced_metadata_json) BETWEEN 1 AND 4096),
    state TEXT NOT NULL CHECK(length(state) BETWEEN 1 AND 64),
    verification_outcome TEXT NOT NULL CHECK(length(verification_outcome) BETWEEN 1 AND 64),
    recovery_details_json TEXT NULL CHECK(recovery_details_json IS NULL OR length(recovery_details_json) BETWEEN 1 AND 1024),
    updated_at TEXT NOT NULL,
    CHECK(
        (displaced_size IS NULL AND displaced_sha256 IS NULL AND displaced_metadata_json IS NULL)
        OR
        (displaced_size IS NOT NULL AND displaced_sha256 IS NOT NULL AND displaced_metadata_json IS NOT NULL)
    ),
    PRIMARY KEY (operation_id, ordinal),
    UNIQUE (operation_id, path_bytes),
    FOREIGN KEY (operation_id, restore_set_id)
        REFERENCES dcc_undo_operations(operation_id, restore_set_id) ON DELETE CASCADE,
    FOREIGN KEY (restore_set_id, ordinal)
        REFERENCES dcc_turn_restore_files(restore_set_id, ordinal) ON DELETE RESTRICT
);

CREATE TRIGGER IF NOT EXISTS trg_dcc_undo_operations_preserve_active
BEFORE DELETE ON dcc_undo_operations
WHEN OLD.active = 1
BEGIN
    SELECT RAISE(ABORT, 'active undo journal cannot be deleted');
END;

CREATE TRIGGER IF NOT EXISTS trg_dcc_undo_operation_files_preserve_active
BEFORE DELETE ON dcc_undo_operation_files
WHEN EXISTS (
    SELECT 1 FROM dcc_undo_operations operation
     WHERE operation.operation_id = OLD.operation_id AND operation.active = 1
)
BEGIN
    SELECT RAISE(ABORT, 'active undo journal file cannot be deleted');
END;

CREATE TRIGGER IF NOT EXISTS trg_dcc_turn_restore_files_preserve_active
BEFORE DELETE ON dcc_turn_restore_files
WHEN EXISTS (
    SELECT 1 FROM dcc_undo_operations operation
     WHERE operation.restore_set_id = OLD.restore_set_id AND operation.active = 1
)
BEGIN
    SELECT RAISE(ABORT, 'restoration file referenced by active journal cannot be deleted');
END;
"#;

const DELEGATION_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_delegations (
	id TEXT PRIMARY KEY NOT NULL,
	parent_session_id TEXT NOT NULL,
	parent_turn_id TEXT NULL,
	child_session_id TEXT NULL,
	workspace_id TEXT NOT NULL,
	target_provider_id TEXT NOT NULL,
	target_model_id TEXT NULL,
	mode TEXT NOT NULL,
	status TEXT NOT NULL,
	prompt TEXT NOT NULL,
	context_policy_json TEXT NOT NULL,
	budget_json TEXT NOT NULL,
	result_summary TEXT NULL,
	touched_files_json TEXT NOT NULL DEFAULT '[]',
	diff_summary TEXT NULL,
	validation_summary TEXT NULL,
	created_at TEXT NOT NULL,
	updated_at TEXT NOT NULL,
	FOREIGN KEY (parent_session_id) REFERENCES dcc_sessions(id) ON DELETE CASCADE,
	FOREIGN KEY (child_session_id) REFERENCES dcc_sessions(id) ON DELETE SET NULL,
	FOREIGN KEY (workspace_id) REFERENCES dcc_workspaces(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dcc_delegations_workspace_id
	ON dcc_delegations(workspace_id);

CREATE INDEX IF NOT EXISTS idx_dcc_delegations_parent_session_id
	ON dcc_delegations(parent_session_id);

CREATE INDEX IF NOT EXISTS idx_dcc_delegations_child_session_id
	ON dcc_delegations(child_session_id);

CREATE INDEX IF NOT EXISTS idx_dcc_delegations_status
	ON dcc_delegations(status);
"#;

const DELEGATION_WORKTREE_OPERATION_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_delegation_worktree_operations (
    operation_id TEXT PRIMARY KEY NOT NULL,
    delegation_key TEXT NULL,
    delegation_id TEXT NULL,
    workspace_id TEXT NOT NULL,
    parent_session_id TEXT NULL,
    child_session_id TEXT NULL,
    source_root TEXT NOT NULL,
    worktree_path TEXT NOT NULL,
    branch TEXT NOT NULL,
    base_commit TEXT NOT NULL,
    expected_branch_oid TEXT NULL,
    source_root_id BLOB NULL
        CHECK(source_root_id IS NULL OR (typeof(source_root_id) = 'blob' AND length(source_root_id) BETWEEN 3 AND 1024)),
    worktree_root_id BLOB NULL
        CHECK(worktree_root_id IS NULL OR (typeof(worktree_root_id) = 'blob' AND length(worktree_root_id) BETWEEN 3 AND 1024)),
    common_dir_id BLOB NULL
        CHECK(common_dir_id IS NULL OR (typeof(common_dir_id) = 'blob' AND length(common_dir_id) BETWEEN 3 AND 1024)),
    state TEXT NOT NULL CHECK(state IN (
        'preparing', 'prepared', 'bound', 'review_pending', 'applying',
        'applied', 'removing', 'removed', 'cleanup_required'
    )),
    last_error TEXT NULL,
    recovery_owner TEXT NULL,
    recovery_lease_until TEXT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK(last_error IS NULL OR length(trim(last_error)) > 0),
    CHECK(state <> 'cleanup_required' OR last_error IS NOT NULL),
    CHECK((recovery_owner IS NULL AND recovery_lease_until IS NULL)
       OR (state = 'removing' AND recovery_owner IS NOT NULL AND recovery_lease_until IS NOT NULL))
);

-- These identifiers deliberately have no destructive foreign keys. The
-- journal must survive partially completed workspace/session row deletion.
CREATE UNIQUE INDEX IF NOT EXISTS idx_dcc_delegation_worktree_active_path
    ON dcc_delegation_worktree_operations(worktree_path)
    WHERE state <> 'removed';

CREATE UNIQUE INDEX IF NOT EXISTS idx_dcc_delegation_worktree_active_branch
    ON dcc_delegation_worktree_operations(source_root, branch)
    WHERE state <> 'removed';

CREATE UNIQUE INDEX IF NOT EXISTS idx_dcc_delegation_worktree_active_physical_root
    ON dcc_delegation_worktree_operations(worktree_root_id)
    WHERE state <> 'removed' AND worktree_root_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_dcc_delegation_worktree_active_physical_branch
    ON dcc_delegation_worktree_operations(common_dir_id, branch)
    WHERE state <> 'removed' AND common_dir_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_dcc_delegation_worktree_delegation
    ON dcc_delegation_worktree_operations(delegation_id)
    WHERE delegation_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_dcc_delegation_worktree_recovery
    ON dcc_delegation_worktree_operations(state, updated_at, created_at)
    WHERE state <> 'removed';

CREATE INDEX IF NOT EXISTS idx_dcc_delegation_worktree_workspace
    ON dcc_delegation_worktree_operations(workspace_id, updated_at DESC);
"#;

const DELEGATION_WORKTREE_OPERATION_SELECT: &str = r#"
SELECT operation_id, delegation_key, delegation_id, workspace_id,
       parent_session_id, child_session_id, source_root, worktree_path,
       branch, base_commit, expected_branch_oid, source_root_id,
       worktree_root_id, common_dir_id, state, last_error,
       recovery_owner, recovery_lease_until, created_at, updated_at
  FROM dcc_delegation_worktree_operations
"#;

const DELEGATION_APPLY_TRANSACTION_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_delegation_apply_transactions (
    transaction_id TEXT PRIMARY KEY NOT NULL,
    operation_id TEXT NOT NULL,
    delegation_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    source_head_oid TEXT NULL,
    destination_head_oid TEXT NULL,
    destination_ref TEXT NULL,
    destination_index_tree_oid TEXT NULL,
    manifest_digest TEXT NULL,
    file_count INTEGER NOT NULL DEFAULT 0 CHECK(file_count >= 0),
    artifact_bytes INTEGER NOT NULL DEFAULT 0 CHECK(artifact_bytes >= 0),
    state TEXT NOT NULL CHECK(state IN (
        'preparing', 'prepared', 'applying', 'applied', 'rolled_back',
        'recovery_required'
    )),
    recovery_owner TEXT NULL,
    recovery_lease_until TEXT NULL,
    last_error TEXT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK(source_head_oid IS NULL OR length(source_head_oid) IN (40, 64)),
    CHECK(destination_head_oid IS NULL OR length(destination_head_oid) IN (40, 64)),
    CHECK(destination_index_tree_oid IS NULL OR length(destination_index_tree_oid) IN (40, 64)),
    CHECK(manifest_digest IS NULL OR length(manifest_digest) = 64),
    CHECK(destination_ref IS NULL OR length(destination_ref) BETWEEN 1 AND 1024),
    CHECK(last_error IS NULL OR length(trim(last_error)) > 0),
    CHECK((recovery_owner IS NULL AND recovery_lease_until IS NULL)
       OR (state = 'applying' AND recovery_owner IS NOT NULL AND recovery_lease_until IS NOT NULL))
);

-- Logical links deliberately have no destructive foreign keys. A recovery
-- transaction must survive deletion of workspace/session/delegation rows.
CREATE UNIQUE INDEX IF NOT EXISTS idx_dcc_delegation_apply_one_active_operation
    ON dcc_delegation_apply_transactions(operation_id)
    WHERE state IN ('preparing', 'prepared', 'applying', 'recovery_required');

CREATE INDEX IF NOT EXISTS idx_dcc_delegation_apply_recovery
    ON dcc_delegation_apply_transactions(state, recovery_lease_until, updated_at);

CREATE INDEX IF NOT EXISTS idx_dcc_delegation_apply_operation_history
    ON dcc_delegation_apply_transactions(operation_id, created_at DESC, transaction_id DESC);
"#;

const DELEGATION_APPLY_TRANSACTION_SELECT: &str = r#"
SELECT transaction_id, operation_id, delegation_id, workspace_id,
       source_head_oid, destination_head_oid, destination_ref,
       destination_index_tree_oid, manifest_digest, file_count, artifact_bytes,
       state, recovery_owner, recovery_lease_until, last_error, created_at, updated_at
  FROM dcc_delegation_apply_transactions
"#;

#[derive(Clone)]
pub struct SqliteWorkspaceRepo {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone)]
pub struct ForgeBoundRepositoryRecord {
    pub id: RepositoryId,
    pub login: String,
}

impl SqliteWorkspaceRepo {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path.as_ref())
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let repo = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        repo.ensure_schema()?;
        Ok(repo)
    }

    pub fn from_connection(conn: Arc<Mutex<Connection>>) -> Result<Self> {
        let repo = Self { conn };
        repo.ensure_schema()?;
        Ok(repo)
    }

    fn ensure_schema(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute_batch(&format!(
            "PRAGMA foreign_keys = ON;\n{WORKSPACE_TABLE_SQL}\n{REPOSITORY_TABLE_SQL}\n{WORKSPACE_BUNDLE_TABLE_SQL}\n{FORGE_LOGIN_PREFERENCE_TABLE_SQL}\n{BROWSER_LOCATION_TABLE_SQL}"
        ))
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Self::ensure_column(&conn, "dcc_workspaces", "setup_report_json", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_workspaces", "source_json", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_workspaces", "pinned_at", "TEXT NULL")?;
        Self::ensure_column(
            &conn,
            "dcc_workspace_bundle_members",
            "workspace_state_before_archive",
            "TEXT NULL",
        )?;
        Self::ensure_column(
            &conn,
            "dcc_workspace_bundle_members",
            "role",
            "TEXT NOT NULL DEFAULT 'contributor'",
        )?;
        Self::ensure_column(
            &conn,
            "dcc_workspace_bundle_members",
            "allow_analysis",
            "INTEGER NOT NULL DEFAULT 1",
        )?;
        Self::ensure_column(
            &conn,
            "dcc_workspace_bundle_members",
            "allow_implementation",
            "INTEGER NOT NULL DEFAULT 1",
        )?;
        Self::ensure_column(&conn, "dcc_repositories", "remote", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_repositories", "remote_url", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_repositories", "forge_provider", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_repositories", "forge_login", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_repositories", "display_name", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_repositories", "icon", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_repositories", "color", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_repositories", "pinned_at", "TEXT NULL")?;
        Ok(())
    }

    /// Updates only the per-workspace comparison branch and timestamp.
    ///
    /// Callers which have performed filesystem work from a previously read
    /// workspace must not write that stale aggregate back over concurrent
    /// changes to source, state, worktree, setup, or pin metadata.
    pub fn update_workspace_base_branch(
        &self,
        workspace_id: &WorkspaceId,
        base_branch: &str,
        updated_at: &str,
    ) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                "UPDATE dcc_workspaces SET base_branch = ?1, updated_at = ?2 WHERE id = ?3",
                params![base_branch, updated_at, workspace_id.0],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        if changed != 1 {
            return Err(dcc_core::CoreError::Repository(
                "workspace not found".to_string(),
            ));
        }
        Ok(())
    }

    pub fn get_forge_login_preference(&self, provider: &str, host: &str) -> Result<Option<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            "SELECT login FROM dcc_forge_login_preferences WHERE provider = ?1 AND host = ?2",
            params![provider, host],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    pub fn set_forge_login_preference(
        &self,
        provider: &str,
        host: &str,
        login: Option<&str>,
    ) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let normalized_login = login.map(str::trim).filter(|login| !login.is_empty());
        if let Some(login) = normalized_login {
            conn.execute(
                "INSERT INTO dcc_forge_login_preferences (provider, host, login)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(provider, host) DO UPDATE SET login = excluded.login",
                params![provider, host, login],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        } else {
            conn.execute(
                "DELETE FROM dcc_forge_login_preferences WHERE provider = ?1 AND host = ?2",
                params![provider, host],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        }
        Ok(())
    }

    pub fn update_repository_forge_login(
        &self,
        repository_id: &RepositoryId,
        login: Option<&str>,
    ) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            "UPDATE dcc_repositories SET forge_login = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![login.map(str::trim).filter(|value| !value.is_empty()), repository_id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    pub fn update_repository_forge_login_if_current(
        &self,
        repository_id: &RepositoryId,
        expected_created_at: &str,
        expected_updated_at: &str,
        login: Option<&str>,
    ) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                "UPDATE dcc_repositories
                    SET forge_login = ?1, updated_at = datetime('now')
                  WHERE id = ?2 AND created_at = ?3 AND updated_at = ?4",
                params![
                    login.map(str::trim).filter(|value| !value.is_empty()),
                    repository_id.0.clone(),
                    expected_created_at,
                    expected_updated_at,
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(changed > 0)
    }

    /// Updates Forge metadata without re-inserting a repository that may have
    /// been removed while an optional background refresh was running.
    pub fn update_repository_forge_metadata_if_exists(
        &self,
        repository: &Repository,
    ) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                "UPDATE dcc_repositories
                    SET remote = ?1,
                        remote_url = ?2,
                        forge_provider = ?3,
                        updated_at = ?4
                  WHERE id = ?5 AND created_at = ?6 AND updated_at = ?7",
                params![
                    repository.remote.clone(),
                    repository.remote_url.clone(),
                    repository.forge_provider.clone(),
                    repository.updated_at.clone(),
                    repository.id.0.clone(),
                    repository.created_at.clone(),
                    repository.updated_at.clone(),
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(changed > 0)
    }

    pub fn update_repository_identity(
        &self,
        repository_id: &RepositoryId,
        display_name: Option<&str>,
        icon: Option<&str>,
        color: Option<&str>,
    ) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                "UPDATE dcc_repositories SET display_name = ?1, icon = ?2, color = ?3, updated_at = datetime('now') WHERE id = ?4",
                params![
                    display_name.map(str::trim).filter(|value| !value.is_empty()),
                    icon.map(str::trim).filter(|value| !value.is_empty()),
                    color.map(str::trim).filter(|value| !value.is_empty()),
                    repository_id.0.clone()
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(changed > 0)
    }

    pub fn update_repository_pinned_at(
        &self,
        repository_id: &RepositoryId,
        pinned_at: Option<&str>,
    ) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                "UPDATE dcc_repositories SET pinned_at = ?1 WHERE id = ?2",
                params![pinned_at, repository_id.0.clone()],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(changed > 0)
    }

    pub fn list_repositories_needing_forge_binding(&self) -> Result<Vec<RepositoryId>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id
                  FROM dcc_repositories
                 WHERE forge_login IS NULL
                   AND remote_url IS NOT NULL
                   AND forge_provider IN ('github', 'gitlab')
                 ORDER BY updated_at DESC, created_at DESC
                "#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = stmt
            .query_map([], |row| Ok(RepositoryId(row.get::<_, String>(0)?)))
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let mut repository_ids = Vec::new();
        for row in rows {
            repository_ids
                .push(row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?);
        }
        Ok(repository_ids)
    }

    pub fn list_forge_bound_repositories(&self) -> Result<Vec<ForgeBoundRepositoryRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, forge_login
                  FROM dcc_repositories
                 WHERE forge_login IS NOT NULL
                   AND remote_url IS NOT NULL
                   AND forge_provider IN ('github', 'gitlab')
                 ORDER BY updated_at DESC, created_at DESC
                "#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ForgeBoundRepositoryRecord {
                    id: RepositoryId(row.get::<_, String>(0)?),
                    login: row.get::<_, String>(1)?,
                })
            })
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let mut repositories = Vec::new();
        for row in rows {
            repositories
                .push(row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?);
        }
        Ok(repositories)
    }

    fn ensure_column(conn: &Connection, table: &str, column: &str, sql_type: &str) -> Result<()> {
        let pragma = format!("PRAGMA table_info({table})");
        let mut stmt = conn
            .prepare(&pragma)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let existing_columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        if existing_columns.iter().any(|existing| existing == column) {
            return Ok(());
        }

        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {sql_type}");
        conn.execute(&sql, [])
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    fn workspace_state_as_str(state: &WorkspaceState) -> &'static str {
        match state {
            WorkspaceState::Initializing => "initializing",
            WorkspaceState::SetupPending => "setup_pending",
            WorkspaceState::Ready => "ready",
            WorkspaceState::Archived => "archived",
            WorkspaceState::Completed => "completed",
        }
    }

    fn workspace_bundle_state_as_str(state: &WorkspaceBundleState) -> &'static str {
        match state {
            WorkspaceBundleState::Ready => "ready",
            WorkspaceBundleState::Archived => "archived",
            WorkspaceBundleState::Completed => "completed",
        }
    }

    fn workspace_bundle_state_from_str(
        value: &str,
        column: usize,
    ) -> rusqlite::Result<WorkspaceBundleState> {
        match value {
            "ready" => Ok(WorkspaceBundleState::Ready),
            "archived" => Ok(WorkspaceBundleState::Archived),
            "completed" => Ok(WorkspaceBundleState::Completed),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown workspace bundle state: {other}"),
                )),
            )),
        }
    }

    fn workspace_bundle_from_row(row: &Row<'_>) -> rusqlite::Result<WorkspaceBundle> {
        Ok(WorkspaceBundle {
            id: WorkspaceBundleId(row.get::<_, String>(0)?),
            name: row.get::<_, String>(1)?,
            primary_workspace_id: WorkspaceId(row.get::<_, String>(2)?),
            state: Self::workspace_bundle_state_from_str(&row.get::<_, String>(3)?, 3)?,
            created_at: row.get::<_, String>(4)?,
            updated_at: row.get::<_, String>(5)?,
        })
    }

    fn workspace_bundle_members(
        conn: &Connection,
        bundle_id: &WorkspaceBundleId,
    ) -> Result<Vec<WorkspaceBundleMember>> {
        let mut stmt = conn
            .prepare(
                r#"
				SELECT bundle_id, workspace_id, created_for_bundle, position
				  FROM dcc_workspace_bundle_members
				 WHERE bundle_id = ?1
				 ORDER BY position ASC, workspace_id ASC
				"#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = stmt
            .query_map(params![bundle_id.0.clone()], |row| {
                Ok(WorkspaceBundleMember {
                    bundle_id: WorkspaceBundleId(row.get::<_, String>(0)?),
                    workspace_id: WorkspaceId(row.get::<_, String>(1)?),
                    created_for_bundle: row.get::<_, bool>(2)?,
                    position: row.get::<_, u32>(3)?,
                })
            })
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    fn workspace_from_row(row: &Row<'_>) -> rusqlite::Result<Workspace> {
        let source_json = row.get::<_, Option<String>>(6)?;
        let source = source_json
            .as_deref()
            .map(|json| {
                from_str::<WorkspaceSource>(json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        6,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            })
            .transpose()?;
        let state = row.get::<_, String>(7)?;
        let state = match state.as_str() {
            "initializing" => WorkspaceState::Initializing,
            "setup_pending" => WorkspaceState::SetupPending,
            "ready" => WorkspaceState::Ready,
            "archived" => WorkspaceState::Archived,
            "completed" => WorkspaceState::Completed,
            other => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    7,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("unknown workspace state: {other}"),
                    )),
                ))
            }
        };

        let setup_report_json = row.get::<_, Option<String>>(8)?;
        let setup_report = setup_report_json
            .as_deref()
            .map(|json| {
                from_str(json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        8,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            })
            .transpose()?;

        Ok(Workspace {
            id: WorkspaceId(row.get::<_, String>(0)?),
            project_id: ProjectId(row.get::<_, String>(1)?),
            name: row.get::<_, Option<String>>(2)?,
            root_path: row.get::<_, String>(3)?,
            base_branch: row.get::<_, String>(4)?,
            worktree_path: row.get::<_, Option<String>>(5)?,
            source,
            state,
            setup_report,
            pinned_at: row.get::<_, Option<String>>(9)?,
            created_at: row.get::<_, String>(10)?,
            updated_at: row.get::<_, String>(11)?,
        })
    }

    fn repository_from_row(row: &Row<'_>) -> rusqlite::Result<Repository> {
        Ok(Repository {
            id: RepositoryId(row.get::<_, String>(0)?),
            project_id: ProjectId(row.get::<_, String>(1)?),
            name: row.get::<_, String>(2)?,
            display_name: row.get::<_, Option<String>>(3)?,
            icon: row.get::<_, Option<String>>(4)?,
            color: row.get::<_, Option<String>>(5)?,
            pinned_at: row.get::<_, Option<String>>(6)?,
            root_path: row.get::<_, String>(7)?,
            base_branch: row.get::<_, String>(8)?,
            remote: row.get::<_, Option<String>>(9)?,
            remote_url: row.get::<_, Option<String>>(10)?,
            forge_provider: row.get::<_, Option<String>>(11)?,
            forge_login: row.get::<_, Option<String>>(12)?,
            created_at: row.get::<_, String>(13)?,
            updated_at: row.get::<_, String>(14)?,
        })
    }
}

#[derive(Clone)]
pub struct SqliteSessionRepo {
    conn: Arc<Mutex<Connection>>,
}

/// Content-free projection of a capture-v2 restoration record for review UI.
///
/// It deliberately excludes artifact locators, digests, physical identities,
/// paths, and captured content. Callers must treat a failed lookup as
/// unavailable rather than inferring restoration eligibility.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardedUndoCaptureSummary {
    pub state: String,
    pub reason_code: Option<String>,
    pub file_count: u32,
    pub artifact_bytes: u64,
    pub completed_at: Option<String>,
    pub expires_at: Option<String>,
}

/// Content-free active-operation projection for persistent recovery UI.
/// It excludes token digests, paths, Git identities and file fingerprints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardedUndoActiveOperationSummary {
    pub operation_id: String,
    pub state: String,
    pub reason_code: Option<String>,
    pub updated_at: String,
}

/// Capability proving that the caller owns the process-wide maintenance
/// lease.  The lease acquisition bridge is deliberately outside this Phase
/// 1B DB surface; keeping the constructor private makes startup/retention
/// impossible to invoke without that bridge being added explicitly.
#[allow(dead_code)]
pub(crate) struct MaintenanceAuthority {
    _private: (),
}

#[allow(dead_code)]
impl MaintenanceAuthority {
    fn new() -> Self {
        Self { _private: () }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RestoreRetentionCandidate {
    pub(crate) restore_set_id: RestoreSetId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) artifact_bytes: u64,
    pub(crate) file_count: u32,
    pub(crate) completed_at: String,
    pub(crate) created_at: String,
}

impl fmt::Debug for SqliteSessionRepo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SqliteSessionRepo").finish_non_exhaustive()
    }
}

impl SqliteSessionRepo {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path.as_ref())
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let repo = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        repo.ensure_schema()?;
        Ok(repo)
    }

    /// Opens an existing session database for a strictly read-only projection.
    /// This deliberately skips schema creation and migration work.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open_with_flags(path.as_ref(), OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn from_connection(conn: Arc<Mutex<Connection>>) -> Result<Self> {
        let repo = Self { conn };
        repo.ensure_schema()?;
        Ok(repo)
    }

    fn ensure_schema(&self) -> Result<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute_batch(&format!(
            "PRAGMA foreign_keys = ON;\n{WORKSPACE_TABLE_SQL}\n{SESSION_TABLE_SQL}\n{BROWSER_LOCATION_TABLE_SQL}\n{PROVIDER_AVAILABILITY_TABLE_SQL}\n{USAGE_TABLE_SQL}\n{TURN_CHANGE_SET_TABLE_SQL}\n{GUARDED_UNDO_TABLE_SQL}\n{DELEGATION_TABLE_SQL}\n{DELEGATION_WORKTREE_OPERATION_TABLE_SQL}\n{DELEGATION_APPLY_TRANSACTION_TABLE_SQL}"
        ))
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_delegations",
            "target_model_id",
            "TEXT NULL",
        )?;
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_delegation_worktree_operations",
            "recovery_owner",
            "TEXT NULL",
        )?;
        // Pre-release Guarded Undo journals did not persist the physical
        // identity of the staged preimage. Such rows deliberately fail schema
        // validation instead of being guessed as recoverable authority.
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_undo_operation_files",
            "staged_metadata_json",
            "TEXT NULL",
        )?;
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_undo_operations",
            "cleanup_pending",
            "INTEGER NOT NULL DEFAULT 1 CHECK(cleanup_pending IN (0, 1))",
        )?;
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_delegation_worktree_operations",
            "recovery_lease_until",
            "TEXT NULL",
        )?;
        // A pre-lease prototype could have crashed after recording `removing`.
        // Convert that unowned state into an explicitly recoverable failure so
        // the new claim protocol can take ownership without guessing.
        conn.execute(
            r#"
            UPDATE dcc_delegation_worktree_operations
               SET state = 'cleanup_required',
                   last_error = COALESCE(
                       NULLIF(TRIM(last_error), ''),
                       'legacy removing operation requires recovery ownership'
                   )
             WHERE state = 'removing'
               AND (recovery_owner IS NULL OR recovery_lease_until IS NULL)
            "#,
            [],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute_batch(
            r#"
            CREATE INDEX IF NOT EXISTS idx_dcc_delegation_worktree_removal_claim
                ON dcc_delegation_worktree_operations(
                    operation_id, state, recovery_lease_until
                );
            "#,
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_delegations",
            "result_summary",
            "TEXT NULL",
        )?;
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_delegations",
            "touched_files_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        SqliteWorkspaceRepo::ensure_column(&conn, "dcc_delegations", "diff_summary", "TEXT NULL")?;
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_delegations",
            "validation_summary",
            "TEXT NULL",
        )?;
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_sessions",
            "working_directory_override",
            "TEXT NULL",
        )?;
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_sessions",
            "additional_workspace_ids_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_turn_change_sets",
            "turn_outcome",
            "TEXT NULL",
        )?;
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_turn_change_sets",
            "outcome_reason",
            "TEXT NULL",
        )?;
        // Keep the terminal-key migration atomic: ALTER/backfill/duplicate
        // validation/index creation must all roll back together on failure.
        let migration = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        SqliteWorkspaceRepo::ensure_column(
            &migration,
            "dcc_session_events",
            "terminal_turn_id",
            "TEXT NULL",
        )?;
        SqliteWorkspaceRepo::ensure_column(
            &migration,
            "dcc_session_events",
            "terminal_kind",
            "TEXT NULL",
        )?;
        Self::backfill_terminal_event_keys(&migration)?;
        migration
            .execute_batch(
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_dcc_session_events_terminal_turn\n\
                 ON dcc_session_events(session_id, terminal_turn_id)\n\
                 WHERE terminal_turn_id IS NOT NULL;",
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        migration
            .commit()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        // Compatibility cleanup for bundles removed before their sessions were
        // deleted explicitly. Keep orphaned single-workspace history unchanged;
        // only multi-root sessions can be identified safely here.
        conn.execute(
            r#"
            DELETE FROM dcc_sessions
             WHERE TRIM(additional_workspace_ids_json) NOT IN ('', '[]', 'null')
               AND NOT EXISTS (
                   SELECT 1
                     FROM dcc_workspaces w
                    WHERE w.id = dcc_sessions.workspace_id
               )
            "#,
            [],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Self::rebuild_search_index_sync(&conn)?;
        Ok(())
    }

    fn backfill_terminal_event_keys(conn: &Connection) -> Result<()> {
        let mut statement = conn
            .prepare(
                "SELECT event_id, kind_json, terminal_turn_id, terminal_kind FROM dcc_session_events",
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        drop(statement);
        for (event_id, kind_json, existing_turn_id, existing_kind) in rows {
            let value = serde_json::from_str::<serde_json::Value>(&kind_json)
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let kind_name = value.get("type").and_then(serde_json::Value::as_str);
            let parsed = if matches!(kind_name, Some("turn_completed" | "turn_aborted")) {
                Some(from_str::<SessionEventKind>(&kind_json).map_err(|_| {
                    dcc_core::CoreError::Repository(
                        "known terminal event kind is invalid".to_string(),
                    )
                })?)
            } else {
                None
            };
            let terminal = match parsed {
                Some(SessionEventKind::TurnCompleted { turn_id }) => {
                    if turn_id.0.trim().is_empty() {
                        return Err(dcc_core::CoreError::Repository(
                            "known terminal event has invalid turnId".to_string(),
                        ));
                    }
                    Some((turn_id.0, "completed"))
                }
                Some(SessionEventKind::TurnAborted { turn_id, .. }) => {
                    if turn_id.0.trim().is_empty() {
                        return Err(dcc_core::CoreError::Repository(
                            "known terminal event has invalid turnId".to_string(),
                        ));
                    }
                    Some((turn_id.0, "aborted"))
                }
                _ => None,
            };
            match terminal {
                Some((turn_id, terminal_kind)) => {
                    if existing_turn_id
                        .as_deref()
                        .is_some_and(|existing| existing != turn_id)
                        || existing_kind
                            .as_deref()
                            .is_some_and(|existing| existing != terminal_kind)
                        || (existing_turn_id.is_some() != existing_kind.is_some())
                    {
                        return Err(dcc_core::CoreError::Repository(
                            "terminal event metadata disagrees with kind_json".to_string(),
                        ));
                    }
                    conn.execute(
                        "UPDATE dcc_session_events SET terminal_turn_id = ?1, terminal_kind = ?2 WHERE event_id = ?3",
                        params![turn_id, terminal_kind, event_id],
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                }
                None => {
                    // Unknown/future and non-terminal events remain opaque,
                    // but can never carry terminal classification metadata.
                    if existing_turn_id.is_some() || existing_kind.is_some() {
                        return Err(dcc_core::CoreError::Repository(
                            "non-terminal event has terminal metadata".to_string(),
                        ));
                    }
                }
            }
        }
        let duplicate = conn
            .query_row(
                r#"
                SELECT EXISTS(
                    SELECT 1
                      FROM dcc_session_events
                     WHERE terminal_turn_id IS NOT NULL
                     GROUP BY session_id, terminal_turn_id
                    HAVING COUNT(*) > 1
                )
                "#,
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        if duplicate != 0 {
            return Err(dcc_core::CoreError::Repository(
                "duplicate terminal event for session turn".to_string(),
            ));
        }
        Ok(())
    }

    pub fn save_turn_change_set(&self, change_set: &TurnChangeSet) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            r#"
            INSERT INTO dcc_turn_change_sets (
                snapshot_id, session_id, turn_id, workspace_id, capture_version,
                state, base_tree, result_tree, baseline_untracked_json,
                result_untracked_json, files_json, file_diffs_json,
                observed_validations_json, diff_truncated, turn_outcome,
                outcome_reason, error, created_at, completed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
            ON CONFLICT(session_id, turn_id, workspace_id) DO UPDATE SET
                state = excluded.state,
                base_tree = excluded.base_tree,
                result_tree = excluded.result_tree,
                baseline_untracked_json = excluded.baseline_untracked_json,
                result_untracked_json = excluded.result_untracked_json,
                files_json = excluded.files_json,
                file_diffs_json = excluded.file_diffs_json,
                observed_validations_json = excluded.observed_validations_json,
                diff_truncated = excluded.diff_truncated,
                turn_outcome = COALESCE(dcc_turn_change_sets.turn_outcome, excluded.turn_outcome),
                outcome_reason = COALESCE(dcc_turn_change_sets.outcome_reason, excluded.outcome_reason),
                error = excluded.error,
                completed_at = excluded.completed_at
            WHERE dcc_turn_change_sets.turn_outcome IS NULL
            "#,
            params![
                change_set.snapshot_id,
                change_set.session_id.0,
                change_set.turn_id.0,
                change_set.workspace_id.0,
                change_set.capture_version,
                change_set.state,
                change_set.base_tree,
                change_set.result_tree,
                to_string(&change_set.baseline_untracked)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?,
                to_string(&change_set.result_untracked)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?,
                to_string(&change_set.files)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?,
                to_string(&change_set.file_diffs)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?,
                to_string(&change_set.observed_validations)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?,
                change_set.diff_truncated,
                change_set.turn_outcome,
                change_set.outcome_reason,
                change_set.error,
                change_set.created_at,
                change_set.completed_at,
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    fn turn_change_set_json<T: DeserializeOwned>(
        column: usize,
        raw: String,
    ) -> rusqlite::Result<T> {
        serde_json::from_str(&raw).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
    }

    fn turn_change_set_from_row(row: &Row<'_>) -> rusqlite::Result<TurnChangeSet> {
        Ok(TurnChangeSet {
            snapshot_id: row.get(0)?,
            session_id: SessionId(row.get(1)?),
            turn_id: TurnId(row.get(2)?),
            workspace_id: WorkspaceId(row.get(3)?),
            capture_version: row.get(4)?,
            state: row.get(5)?,
            base_tree: row.get(6)?,
            result_tree: row.get(7)?,
            baseline_untracked: Self::turn_change_set_json(8, row.get(8)?)?,
            result_untracked: Self::turn_change_set_json(9, row.get(9)?)?,
            files: Self::turn_change_set_json(10, row.get(10)?)?,
            file_diffs: Self::turn_change_set_json(11, row.get(11)?)?,
            observed_validations: Self::turn_change_set_json(12, row.get(12)?)?,
            diff_truncated: row.get(13)?,
            turn_outcome: row.get(14)?,
            outcome_reason: row.get(15)?,
            error: row.get(16)?,
            created_at: row.get(17)?,
            completed_at: row.get(18)?,
        })
    }

    pub fn get_turn_change_set(&self, snapshot_id: &str) -> Result<Option<TurnChangeSet>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            r#"SELECT snapshot_id, session_id, turn_id, workspace_id, capture_version,
                      state, base_tree, result_tree, baseline_untracked_json,
                      result_untracked_json, files_json, file_diffs_json,
                      observed_validations_json, diff_truncated, turn_outcome,
                      outcome_reason, error, created_at, completed_at
                 FROM dcc_turn_change_sets WHERE snapshot_id = ?1"#,
            params![snapshot_id],
            Self::turn_change_set_from_row,
        )
        .optional()
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    pub fn list_turn_change_sets_by_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<TurnChangeSet>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"SELECT snapshot_id, session_id, turn_id, workspace_id, capture_version,
                          state, base_tree, result_tree, baseline_untracked_json,
                          result_untracked_json, files_json, '{}' AS file_diffs_json,
                          observed_validations_json, diff_truncated, turn_outcome,
                          outcome_reason, error, created_at, completed_at
                     FROM dcc_turn_change_sets
                    WHERE session_id = ?1
                    ORDER BY COALESCE(completed_at, created_at) DESC, snapshot_id DESC"#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = stmt
            .query_map(params![session_id.0], Self::turn_change_set_from_row)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    pub fn recover_interrupted_turn_change_sets(&self, completed_at: &str) -> Result<Vec<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut statement = conn
            .prepare(
                "SELECT snapshot_id FROM dcc_turn_change_sets WHERE state = 'collecting' AND turn_outcome IS NULL",
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let snapshot_ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        drop(statement);
        conn.execute(
            r#"UPDATE dcc_turn_change_sets
                  SET state = 'interrupted',
                      turn_outcome = 'aborted',
                      outcome_reason = 'Application restarted before review capture completed',
                      error = 'turn ended before review evidence could be finalized',
                      completed_at = ?1
                WHERE state = 'collecting' AND turn_outcome IS NULL"#,
            params![completed_at],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(snapshot_ids)
    }

    /// Creates only the empty `collecting` Phase 0 row. Raw artifacts and
    /// capture v2 are intentionally outside this implementation.
    pub fn create_turn_restore_set(&self, restore_set: &TurnRestoreSet) -> Result<()> {
        restore_set.validate().map_err(guarded_undo_error)?;
        if restore_set.state != RestoreSetState::Collecting
            || restore_set.file_count != 0
            || restore_set.artifact_bytes != 0
            || restore_set.manifest_digest.is_some()
        {
            return Err(guarded_undo_error(
                "new restoration set must be an empty collecting record",
            ));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let attribution: Option<(String, String, String)> = conn
            .query_row(
                "SELECT session_id, turn_id, workspace_id FROM dcc_turn_change_sets WHERE snapshot_id = ?1",
                params![restore_set.snapshot_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        if attribution
            != Some((
                restore_set.session_id.0.clone(),
                restore_set.turn_id.0.clone(),
                restore_set.workspace_id.0.clone(),
            ))
        {
            return Err(guarded_undo_error(
                "restoration attribution does not match its M3 snapshot",
            ));
        }
        conn.execute(
            r#"INSERT INTO dcc_turn_restore_sets (
                restore_set_id, snapshot_id, session_id, turn_id, workspace_id,
                root_id, capture_version, state, reason_code, git_identity_json,
                artifact_bytes, file_count, manifest_digest, created_at,
                completed_at, expires_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, 0, 0, NULL, ?10, NULL, NULL)"#,
            params![
                restore_set.restore_set_id.0,
                restore_set.snapshot_id,
                restore_set.session_id.0,
                restore_set.turn_id.0,
                restore_set.workspace_id.0,
                restore_set
                    .root_id
                    .as_ref()
                    .map(|root_id| root_id.0.as_slice()),
                restore_set.capture_version,
                restore_set.state.as_str(),
                optional_json(&restore_set.git_identity)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?,
                restore_set.created_at,
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    /// Records a zero-file preflight decision when capture cannot safely
    /// create a collecting row. Attribution and insertion share one SQLite
    /// transaction, so a snapshot is never attached to caller-supplied IDs
    /// without proving the durable M3 relationship.
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    pub(crate) fn record_capture_v2_preflight_terminal(
        &self,
        restore_set: &TurnRestoreSet,
    ) -> Result<()> {
        restore_set.validate().map_err(guarded_undo_error)?;
        if !matches!(
            restore_set.state,
            RestoreSetState::Failed | RestoreSetState::Ineligible
        ) || restore_set.file_count != 0
            || restore_set.artifact_bytes != 0
            || restore_set.manifest_digest.is_some()
        {
            return Err(guarded_undo_error(
                "preflight terminal restoration record must contain zero files",
            ));
        }
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn.transaction().map_err(guarded_undo_error)?;
        let attribution: Option<(String, String, String)> = transaction
            .query_row(
                "SELECT session_id, turn_id, workspace_id FROM dcc_turn_change_sets WHERE snapshot_id = ?1",
                params![restore_set.snapshot_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(guarded_undo_error)?;
        if attribution
            != Some((
                restore_set.session_id.0.clone(),
                restore_set.turn_id.0.clone(),
                restore_set.workspace_id.0.clone(),
            ))
        {
            return Err(guarded_undo_error(
                "preflight restoration attribution mismatch",
            ));
        }
        transaction
            .execute(
                r#"INSERT INTO dcc_turn_restore_sets (
                    restore_set_id, snapshot_id, session_id, turn_id, workspace_id,
                    root_id, capture_version, state, reason_code, git_identity_json,
                    artifact_bytes, file_count, manifest_digest, created_at,
                    completed_at, expires_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, 0, NULL, ?11, ?12, NULL)"#,
                params![
                    restore_set.restore_set_id.0,
                    restore_set.snapshot_id,
                    restore_set.session_id.0,
                    restore_set.turn_id.0,
                    restore_set.workspace_id.0,
                    restore_set.root_id.as_ref().map(|root| root.0.as_slice()),
                    restore_set.capture_version,
                    restore_set.state.as_str(),
                    restore_set
                        .reason_code
                        .as_ref()
                        .map(|reason| reason.as_str()),
                    optional_json(&restore_set.git_identity)
                        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?,
                    restore_set.created_at,
                    restore_set.completed_at,
                ],
            )
            .map_err(guarded_undo_error)?;
        transaction.commit().map_err(guarded_undo_error)?;
        Ok(())
    }

    /// Finalizes a collecting set with one compare-and-swap transaction. An
    /// eligible state cannot become visible before all file rows, accounting,
    /// and the canonical manifest digest are committed together.
    pub fn finalize_turn_restore_set(
        &self,
        restore_set: &TurnRestoreSet,
        files: &[TurnRestoreFile],
    ) -> Result<bool> {
        restore_set.validate().map_err(guarded_undo_error)?;
        if restore_set.state == RestoreSetState::Collecting
            || !RestoreSetState::Collecting.can_transition_to(&restore_set.state)
        {
            return Err(guarded_undo_error("invalid restoration final state"));
        }
        if restore_set.state == RestoreSetState::Eligible {
            validate_restore_set_manifest(restore_set, files).map_err(guarded_undo_error)?;
        } else if !files.is_empty()
            || restore_set.file_count != 0
            || restore_set.artifact_bytes != 0
            || restore_set.manifest_digest.is_some()
        {
            return Err(guarded_undo_error(
                "ineligible or failed restoration set cannot retain usable file rows",
            ));
        }

        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn
            .transaction()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let (persisted_set, persisted_files) =
            load_turn_restore_set(&transaction, &restore_set.restore_set_id)?
                .ok_or_else(|| guarded_undo_error("restoration set does not exist"))?;
        let current_state = persisted_set.state.clone();
        if current_state == restore_set.state {
            let mut replay_files = files.to_vec();
            replay_files.sort_by_key(|file| file.ordinal);
            if persisted_set == *restore_set && persisted_files == replay_files {
                return Ok(false);
            }
            return Err(guarded_undo_error(
                "conflicting idempotent restoration finalization",
            ));
        }
        if current_state != RestoreSetState::Collecting
            || !current_state.can_transition_to(&restore_set.state)
            || persisted_set.capture_version != RESTORE_CAPTURE_VERSION
            || persisted_set.snapshot_id != restore_set.snapshot_id
            || persisted_set.session_id != restore_set.session_id
            || persisted_set.turn_id != restore_set.turn_id
            || persisted_set.workspace_id != restore_set.workspace_id
            || persisted_set.created_at != restore_set.created_at
            || persisted_set
                .root_id
                .as_ref()
                .is_some_and(|root_id| Some(root_id) != restore_set.root_id.as_ref())
            || persisted_set
                .git_identity
                .as_ref()
                .is_some_and(|git| Some(git) != restore_set.git_identity.as_ref())
        {
            return Err(guarded_undo_error(
                "restoration lifecycle compare-and-swap rejected",
            ));
        }

        for file in files {
            insert_turn_restore_file(&transaction, file)?;
        }
        let changed = transaction
            .execute(
                r#"UPDATE dcc_turn_restore_sets
                      SET state = ?1, reason_code = ?2, root_id = ?3,
                          git_identity_json = ?4, artifact_bytes = ?5,
                          file_count = ?6, manifest_digest = ?7,
                          completed_at = ?8, expires_at = ?9
                    WHERE restore_set_id = ?10 AND state = 'collecting'"#,
                params![
                    restore_set.state.as_str(),
                    restore_set
                        .reason_code
                        .as_ref()
                        .map(|reason| reason.as_str()),
                    restore_set
                        .root_id
                        .as_ref()
                        .map(|root_id| root_id.0.as_slice()),
                    optional_json(&restore_set.git_identity)
                        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?,
                    i64::try_from(restore_set.artifact_bytes)
                        .map_err(|_| guarded_undo_error("artifact accounting exceeds SQLite"))?,
                    i64::from(restore_set.file_count),
                    restore_set.manifest_digest.map(|digest| digest.0.to_vec()),
                    restore_set.completed_at,
                    restore_set.expires_at,
                    restore_set.restore_set_id.0,
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        if changed != 1 {
            return Err(guarded_undo_error(
                "restoration lifecycle compare-and-swap lost",
            ));
        }
        transaction
            .commit()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(true)
    }

    pub fn get_turn_restore_set(
        &self,
        restore_set_id: &RestoreSetId,
    ) -> Result<Option<(TurnRestoreSet, Vec<TurnRestoreFile>)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        load_turn_restore_set(&conn, restore_set_id)
    }

    /// Loads the opaque restoration manifest owned by one immutable M3
    /// snapshot. Mutation callers must still revalidate its workspace and
    /// physical authority; the snapshot id is attribution, not authority.
    pub fn get_turn_restore_set_by_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<(TurnRestoreSet, Vec<TurnRestoreFile>)>> {
        if snapshot_id.trim().is_empty() {
            return Err(guarded_undo_error("snapshot id is empty"));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let restore_set_id = conn
            .query_row(
                "SELECT restore_set_id FROM dcc_turn_restore_sets WHERE snapshot_id = ?1",
                params![snapshot_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(guarded_undo_error)?;
        restore_set_id
            .map(|id| load_turn_restore_set(&conn, &RestoreSetId(id)))
            .transpose()
            .map(Option::flatten)
    }

    pub fn get_active_guarded_undo_summary(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<GuardedUndoActiveOperationSummary>> {
        if snapshot_id.trim().is_empty() {
            return Err(guarded_undo_error("snapshot id is empty"));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            r#"SELECT operation.operation_id, operation.state, operation.reason_code, operation.updated_at
                 FROM dcc_undo_operations operation
                 JOIN dcc_turn_restore_sets restore
                   ON restore.restore_set_id = operation.restore_set_id
                WHERE restore.snapshot_id = ?1 AND operation.active = 1
                LIMIT 1"#,
            params![snapshot_id],
            |row| {
                Ok(GuardedUndoActiveOperationSummary {
                    operation_id: row.get(0)?,
                    state: row.get(1)?,
                    reason_code: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(guarded_undo_error)
    }

    pub fn has_guarded_undo_cleanup_pending(&self, snapshot_id: &str) -> Result<bool> {
        if snapshot_id.trim().is_empty() {
            return Err(guarded_undo_error("snapshot id is empty"));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            r#"SELECT EXISTS(
                    SELECT 1 FROM dcc_undo_operations operation
                    JOIN dcc_turn_restore_sets restore
                      ON restore.restore_set_id = operation.restore_set_id
                    WHERE restore.snapshot_id = ?1 AND operation.cleanup_pending = 1
                )"#,
            params![snapshot_id],
            |row| row.get(0),
        )
        .map_err(guarded_undo_error)
    }

    /// Loads the one restoration record attributed to an M3 snapshot without
    /// exposing raw restoration metadata. The unique snapshot relationship is
    /// verified explicitly so a damaged database fails closed.
    pub fn get_guarded_undo_capture_summary(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<GuardedUndoCaptureSummary>> {
        if snapshot_id.trim().is_empty() || snapshot_id.len() > 512 {
            return Err(guarded_undo_error("invalid restoration snapshot lookup"));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let ids = conn
            .prepare(
                "SELECT restore_set_id FROM dcc_turn_restore_sets WHERE snapshot_id = ?1 LIMIT 2",
            )
            .map_err(guarded_undo_error)?
            .query_map(params![snapshot_id], |row| row.get::<_, String>(0))
            .map_err(guarded_undo_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(guarded_undo_error)?;
        let Some(restore_set_id) = ids.first() else {
            return Ok(None);
        };
        if ids.len() != 1 {
            return Err(guarded_undo_error(
                "restoration snapshot has conflicting records",
            ));
        }
        let Some((restore_set, _files)) =
            load_turn_restore_set(&conn, &RestoreSetId(restore_set_id.clone()))?
        else {
            return Err(guarded_undo_error(
                "restoration record disappeared during lookup",
            ));
        };
        if restore_set.snapshot_id != snapshot_id {
            return Err(guarded_undo_error("restoration attribution mismatch"));
        }
        Ok(Some(GuardedUndoCaptureSummary {
            state: restore_set.state.as_str().to_owned(),
            reason_code: restore_set
                .reason_code
                .map(|reason| reason.as_str().to_owned()),
            file_count: restore_set.file_count,
            artifact_bytes: restore_set.artifact_bytes,
            completed_at: restore_set.completed_at,
            expires_at: restore_set.expires_at,
        }))
    }

    /// Marks v2 captures left in `collecting` after a restart as failed.  The
    /// update is a transaction-level CAS and deliberately does not touch M3
    /// change-set rows or any artifact data.
    #[allow(dead_code)]
    pub(crate) fn recover_interrupted_restore_sets(
        &self,
        authority: &MaintenanceAuthority,
        completed_at: &str,
    ) -> Result<Vec<String>> {
        let _ = authority;
        require_maintenance_timestamp(completed_at)?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn
            .transaction()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut statement = transaction
            .prepare(
                "SELECT restore_set_id FROM dcc_turn_restore_sets WHERE state = 'collecting' ORDER BY restore_set_id",
            )
            .map_err(guarded_undo_error)?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(guarded_undo_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(guarded_undo_error)?;
        drop(statement);
        for id in &ids {
            let restore_set_id = RestoreSetId(id.clone());
            let (set, files) = load_turn_restore_set(&transaction, &restore_set_id)?
                .ok_or_else(|| guarded_undo_error("collecting restoration set disappeared"))?;
            if set.state != RestoreSetState::Collecting
                || set.capture_version != RESTORE_CAPTURE_VERSION
                || !files.is_empty()
                || set.file_count != 0
                || set.artifact_bytes != 0
                || set.manifest_digest.is_some()
            {
                return Err(guarded_undo_error(
                    "invalid collecting restoration set during recovery",
                ));
            }
            let changed = transaction
                .execute(
                    r#"UPDATE dcc_turn_restore_sets
                          SET state = 'failed', reason_code = 'capture_interrupted',
                              completed_at = ?1
                        WHERE restore_set_id = ?2 AND state = 'collecting'"#,
                    params![completed_at, id],
                )
                .map_err(guarded_undo_error)?;
            if changed != 1 {
                return Err(guarded_undo_error(
                    "restoration recovery compare-and-swap lost",
                ));
            }
        }
        transaction.commit().map_err(guarded_undo_error)?;
        Ok(ids)
    }

    /// Startup-only capture-v2 recovery. Possession of the store proves the
    /// caller holds its app-data lifetime lock; no maintenance authority is
    /// exposed outside this database module.
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    pub(crate) fn recover_capture_v2_startup(
        &self,
        store: &MacArtifactStore,
        completed_at: &str,
    ) -> Result<OrphanRecoveryReport> {
        let authority = MaintenanceAuthority::new();
        self.recover_interrupted_restore_sets(&authority, completed_at)?;
        let referenced = {
            let conn = self
                .conn
                .lock()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let mut statement = conn
                .prepare(
                    r#"SELECT files.pre_artifact_key
                         FROM dcc_turn_restore_files AS files
                         JOIN dcc_turn_restore_sets AS sets
                           ON sets.restore_set_id = files.restore_set_id
                        WHERE sets.state = 'eligible'
                        ORDER BY files.restore_set_id, files.ordinal"#,
                )
                .map_err(guarded_undo_error)?;
            let raw = statement
                .query_map([], |row| row.get::<_, Vec<u8>>(0))
                .map_err(guarded_undo_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(guarded_undo_error)?;
            let mut keys = std::collections::HashSet::with_capacity(raw.len());
            for bytes in raw {
                let key = ArtifactKey::from_slice(&bytes).map_err(guarded_undo_error)?;
                // The store is content-addressed, so multiple eligible files
                // and sets may legitimately reference the same artifact.
                keys.insert(key);
            }
            keys
        };
        store
            .recover_orphans(&referenced)
            .map_err(|_| guarded_undo_error("capture artifact recovery failed"))
    }

    /// Expires an eligible set while retaining its complete manifest and file
    /// rows for audit/accounting.  An active journal is an explicit barrier.
    #[allow(dead_code)]
    pub(crate) fn expire_eligible_restore_set(
        &self,
        authority: &MaintenanceAuthority,
        restore_set_id: &RestoreSetId,
        completed_at: &str,
    ) -> Result<bool> {
        let _ = authority;
        require_maintenance_timestamp(completed_at)?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn
            .transaction()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let Some((set, _files)) = load_turn_restore_set(&transaction, restore_set_id)? else {
            transaction.commit().map_err(guarded_undo_error)?;
            return Ok(false);
        };
        if set.state == RestoreSetState::Expired {
            return Ok(false);
        }
        if set.state != RestoreSetState::Eligible {
            return Ok(false);
        }
        let active: Option<i64> = transaction
            .query_row(
                "SELECT 1 FROM dcc_undo_operations WHERE restore_set_id = ?1 AND active = 1 LIMIT 1",
                params![restore_set_id.0],
                |row| row.get(0),
            )
            .optional()
            .map_err(guarded_undo_error)?;
        if active.is_some() {
            return Err(guarded_undo_error(
                "active undo journal blocks retention expiration",
            ));
        }
        let changed = transaction
            .execute(
                r#"UPDATE dcc_turn_restore_sets
                      SET state = 'expired', reason_code = 'retention_expired',
                          completed_at = ?1
                    WHERE restore_set_id = ?2 AND state = 'eligible'
                      AND NOT EXISTS (
                          SELECT 1 FROM dcc_undo_operations
                           WHERE restore_set_id = ?2 AND active = 1
                      )"#,
                params![completed_at, restore_set_id.0],
            )
            .map_err(guarded_undo_error)?;
        if changed != 1 {
            return Err(guarded_undo_error(
                "retention expiration compare-and-swap lost",
            ));
        }
        transaction.commit().map_err(guarded_undo_error)?;
        Ok(true)
    }

    /// Records an artifact integrity failure without deleting the manifest or
    /// its rows.  Only the two artifact-specific reasons are accepted.
    #[allow(dead_code)]
    pub(crate) fn fail_eligible_restore_set_integrity(
        &self,
        authority: &MaintenanceAuthority,
        restore_set_id: &RestoreSetId,
        reason: &GuardedUndoReasonCode,
        completed_at: &str,
    ) -> Result<bool> {
        let _ = authority;
        if !matches!(
            reason,
            GuardedUndoReasonCode::ArtifactMissing | GuardedUndoReasonCode::ArtifactCorrupt
        ) {
            return Err(guarded_undo_error(
                "integrity failure requires artifact_missing or artifact_corrupt",
            ));
        }
        require_maintenance_timestamp(completed_at)?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn
            .transaction()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let Some((set, _files)) = load_turn_restore_set(&transaction, restore_set_id)? else {
            transaction.commit().map_err(guarded_undo_error)?;
            return Ok(false);
        };
        if set.state == RestoreSetState::Failed {
            if set.reason_code.as_ref() == Some(reason) {
                return Ok(false);
            }
            return Ok(false);
        }
        if set.state != RestoreSetState::Eligible {
            return Ok(false);
        }
        let active: Option<i64> = transaction
            .query_row(
                "SELECT 1 FROM dcc_undo_operations WHERE restore_set_id = ?1 AND active = 1 LIMIT 1",
                params![restore_set_id.0],
                |row| row.get(0),
            )
            .optional()
            .map_err(guarded_undo_error)?;
        if active.is_some() {
            return Err(guarded_undo_error(
                "active undo journal blocks integrity failure transition",
            ));
        }
        let changed = transaction
            .execute(
                r#"UPDATE dcc_turn_restore_sets
                      SET state = 'failed', reason_code = ?1, completed_at = ?2
                    WHERE restore_set_id = ?3 AND state = 'eligible'
                      AND NOT EXISTS (
                          SELECT 1 FROM dcc_undo_operations
                           WHERE restore_set_id = ?3 AND active = 1
                      )"#,
                params![reason.as_str(), completed_at, restore_set_id.0],
            )
            .map_err(guarded_undo_error)?;
        if changed != 1 {
            return Err(guarded_undo_error(
                "integrity failure compare-and-swap lost",
            ));
        }
        transaction.commit().map_err(guarded_undo_error)?;
        Ok(true)
    }

    /// Returns oldest-first eligible sets, bounded by both count and retained
    /// artifact bytes.  A candidate that exceeds the remaining byte budget
    /// stops the oldest-first prefix; newer sets are not preferred over it.
    #[allow(dead_code)]
    pub(crate) fn list_retention_candidates(
        &self,
        authority: &MaintenanceAuthority,
        workspace_id: &WorkspaceId,
        max_count: u32,
        max_bytes: u64,
    ) -> Result<Vec<RestoreRetentionCandidate>> {
        let _ = authority;
        if max_count == 0 || max_bytes == 0 {
            return Ok(Vec::new());
        }
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn.transaction().map_err(guarded_undo_error)?;
        let mut statement = transaction
            .prepare(
                r#"SELECT restore_set_id, workspace_id, artifact_bytes,
                          file_count, completed_at, created_at
                     FROM dcc_turn_restore_sets AS sets
                    WHERE workspace_id = ?1 AND state = 'eligible'
                      AND NOT EXISTS (
                          SELECT 1 FROM dcc_undo_operations operation
                           WHERE operation.restore_set_id = sets.restore_set_id
                             AND operation.active = 1
                      )
                    ORDER BY completed_at ASC, created_at ASC, restore_set_id ASC
                    LIMIT ?2"#,
            )
            .map_err(guarded_undo_error)?;
        let rows = statement
            .query_map(params![workspace_id.0, i64::from(max_count)], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(guarded_undo_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(guarded_undo_error)?;
        drop(statement);
        let mut out = Vec::new();
        let mut bytes = 0_u64;
        for (id, persisted_workspace, artifact_bytes, file_count, completed_at, created_at) in rows
        {
            let candidate_bytes = checked_u64(artifact_bytes, "artifact_bytes")?;
            let candidate_count = checked_u32(file_count, "file_count")?;
            let restore_set_id = RestoreSetId(id);
            let (set, _files) = load_turn_restore_set(&transaction, &restore_set_id)?
                .ok_or_else(|| guarded_undo_error("retention candidate disappeared"))?;
            if set.state != RestoreSetState::Eligible
                || set.workspace_id.0 != persisted_workspace
                || set.artifact_bytes != candidate_bytes
                || set.file_count != candidate_count
                || set.completed_at.as_deref() != Some(completed_at.as_str())
                || set.created_at != created_at
            {
                return Err(guarded_undo_error(
                    "retention candidate changed during enumeration",
                ));
            }
            if candidate_bytes > max_bytes.saturating_sub(bytes) {
                break;
            }
            bytes = bytes
                .checked_add(candidate_bytes)
                .ok_or_else(|| guarded_undo_error("retention accounting overflow"))?;
            out.push(RestoreRetentionCandidate {
                restore_set_id,
                workspace_id: WorkspaceId(persisted_workspace),
                artifact_bytes: candidate_bytes,
                file_count: candidate_count,
                completed_at,
                created_at,
            });
        }
        transaction.commit().map_err(guarded_undo_error)?;
        Ok(out)
    }

    /// Lists opaque artifact references while retaining set/ordinal order.
    /// Missing or malformed rows fail closed; no artifact file is opened.
    #[allow(dead_code)]
    pub(crate) fn list_referenced_artifact_keys(
        &self,
        authority: &MaintenanceAuthority,
        restore_set_ids: &[RestoreSetId],
    ) -> Result<Vec<ArtifactKey>> {
        let _ = authority;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn.transaction().map_err(guarded_undo_error)?;
        let mut out = Vec::new();
        for restore_set_id in restore_set_ids {
            let Some((_set, files)) = load_turn_restore_set(&transaction, restore_set_id)? else {
                return Err(guarded_undo_error("referenced restoration set is missing"));
            };
            out.extend(files.into_iter().map(|file| file.pre_artifact_key));
        }
        transaction.commit().map_err(guarded_undo_error)?;
        Ok(out)
    }

    /// Phase 0 fixture journal insertion. It records no filesystem mutation and
    /// requires the referenced restoration set to already be eligible.
    pub fn create_undo_operation(
        &self,
        operation: &UndoOperation,
        files: &[UndoOperationFile],
    ) -> Result<()> {
        operation.validate().map_err(guarded_undo_error)?;
        if operation.state != UndoOperationState::Preparing {
            return Err(guarded_undo_error(
                "new undo journal must begin in preparing",
            ));
        }
        if !operation.active {
            return Err(guarded_undo_error("new undo journal must be active"));
        }
        if operation.preview_token_digest.is_none() {
            return Err(guarded_undo_error(
                "new undo journal requires a persisted preview token digest",
            ));
        }
        for (ordinal, file) in files.iter().enumerate() {
            file.validate().map_err(guarded_undo_error)?;
            if file.operation_id != operation.operation_id || file.ordinal as usize != ordinal {
                return Err(guarded_undo_error(
                    "operation file ownership or ordinal is invalid",
                ));
            }
            if file.state != UndoOperationFileState::Planned
                || file.verification_outcome != VerificationOutcome::Pending
                || file.displaced_size.is_some()
                || file.recovery_details.is_some()
            {
                return Err(guarded_undo_error(
                    "new undo journal files must be planned and unmodified",
                ));
            }
        }
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn
            .transaction()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let (restore_set, restore_files) =
            load_turn_restore_set(&transaction, &operation.restore_set_id)?
                .ok_or_else(|| guarded_undo_error("restoration set does not exist"))?;
        if restore_set.state != RestoreSetState::Eligible {
            return Err(guarded_undo_error(
                "only an eligible known restoration set may start an operation",
            ));
        }
        if files.is_empty() || files.len() != restore_files.len() {
            return Err(guarded_undo_error(
                "journal files do not cover the complete restoration manifest",
            ));
        }
        if operation.prepared_identity.root_id
            != restore_set.root_id.clone().expect("eligible root")
            || operation.prepared_identity.git
                != restore_set
                    .git_identity
                    .clone()
                    .expect("eligible git identity")
            || Some(operation.prepared_identity.manifest_digest) != restore_set.manifest_digest
        {
            return Err(guarded_undo_error(
                "prepared identity is not bound to the restoration manifest",
            ));
        }
        validate_operation_manifest_binding(operation, files, &restore_set, &restore_files)?;
        transaction
            .execute(
                r#"INSERT INTO dcc_undo_operations (
                    operation_id, restore_set_id, journal_version, state, active,
                    preview_token_digest, prepared_identity_json, reason_code,
                    recovery_details_json, created_at, updated_at, completed_at
                ) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"#,
                params![
                    operation.operation_id.0,
                    operation.restore_set_id.0,
                    operation.journal_version,
                    operation.state.as_str(),
                    operation
                        .preview_token_digest
                        .map(|digest| digest.0.to_vec()),
                    to_string(&operation.prepared_identity)
                        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?,
                    operation.reason_code.as_ref().map(|reason| reason.as_str()),
                    optional_json(&operation.recovery_details)?,
                    operation.created_at,
                    operation.updated_at,
                    operation.completed_at,
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        for file in files {
            insert_undo_operation_file(&transaction, file)?;
        }
        transaction
            .commit()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    pub fn get_undo_operation(
        &self,
        operation_id: &UndoOperationId,
    ) -> Result<Option<(UndoOperation, Vec<UndoOperationFile>)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let Some(operation) = load_undo_operation(&conn, operation_id)? else {
            return Ok(None);
        };
        if operation.0.journal_version == UNDO_JOURNAL_SCHEMA_VERSION
            && operation.0.state.is_known()
        {
            let restore = load_turn_restore_set(&conn, &operation.0.restore_set_id)?
                .ok_or_else(|| guarded_undo_error("restoration set disappeared"))?;
            validate_operation_manifest_binding(
                &operation.0,
                &operation.1,
                &restore.0,
                &restore.1,
            )?;
        }
        Ok(Some(operation))
    }

    /// Returns every journal that can still own same-directory recovery
    /// files. Callers must hold the app-data lifetime lock before using this
    /// startup-recovery inventory.
    pub fn list_active_undo_operations(
        &self,
    ) -> Result<Vec<(UndoOperation, Vec<UndoOperationFile>)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let ids = {
            let mut statement = conn
                .prepare(
                    "SELECT operation_id FROM dcc_undo_operations WHERE active = 1 ORDER BY created_at, operation_id",
                )
                .map_err(guarded_undo_error)?;
            let ids = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(guarded_undo_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(guarded_undo_error)?;
            ids
        };
        ids.into_iter()
            .map(|id| {
                load_undo_operation(&conn, &UndoOperationId(id))?
                    .ok_or_else(|| guarded_undo_error("active undo operation disappeared"))
            })
            .collect()
    }

    pub fn list_undo_operations_pending_cleanup(
        &self,
    ) -> Result<Vec<(UndoOperation, Vec<UndoOperationFile>)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let ids = {
            let mut statement = conn
                .prepare(
                    r#"SELECT operation_id FROM dcc_undo_operations
                        WHERE active = 0 AND cleanup_pending = 1
                        ORDER BY completed_at, operation_id"#,
                )
                .map_err(guarded_undo_error)?;
            let ids = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(guarded_undo_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(guarded_undo_error)?;
            ids
        };
        ids.into_iter()
            .map(|id| {
                load_undo_operation(&conn, &UndoOperationId(id))?
                    .ok_or_else(|| guarded_undo_error("cleanup undo operation disappeared"))
            })
            .collect()
    }

    pub fn finish_undo_operation_cleanup(&self, operation_id: &UndoOperationId) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                r#"UPDATE dcc_undo_operations SET cleanup_pending = 0
                    WHERE operation_id = ?1 AND active = 0 AND cleanup_pending = 1"#,
                params![operation_id.0],
            )
            .map_err(guarded_undo_error)?;
        Ok(changed == 1)
    }

    pub fn transition_undo_operation_file(
        &self,
        expected: &UndoOperationFileState,
        next_file: &UndoOperationFile,
    ) -> Result<bool> {
        next_file.validate().map_err(guarded_undo_error)?;
        if !expected.is_known()
            || !next_file.state.is_known()
            || !expected.can_transition_to(&next_file.state)
        {
            return Err(guarded_undo_error("invalid undo file transition"));
        }
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn
            .transaction()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let (operation, files) = load_undo_operation(&transaction, &next_file.operation_id)?
            .ok_or_else(|| guarded_undo_error("undo operation does not exist"))?;
        if !operation.active {
            return Err(guarded_undo_error(
                "terminal undo operation files are immutable",
            ));
        }
        let (restore_set, restore_files) =
            load_turn_restore_set(&transaction, &operation.restore_set_id)?
                .ok_or_else(|| guarded_undo_error("restoration set disappeared"))?;
        validate_operation_manifest_binding(&operation, &files, &restore_set, &restore_files)?;
        let current = files
            .into_iter()
            .find(|file| file.ordinal == next_file.ordinal)
            .ok_or_else(|| guarded_undo_error("undo operation file does not exist"))?;
        if current.state == next_file.state {
            if current == *next_file {
                return Ok(false);
            }
            return Err(guarded_undo_error(
                "conflicting idempotent undo file transition",
            ));
        }
        if &current.state != expected || !current.state.can_transition_to(&next_file.state) {
            return Err(guarded_undo_error(
                "undo file lifecycle compare-and-swap rejected",
            ));
        }
        if current.operation_id != next_file.operation_id
            || current.restore_set_id != next_file.restore_set_id
            || current.ordinal != next_file.ordinal
            || current.path_bytes != next_file.path_bytes
            || current.exchange_artifact_key != next_file.exchange_artifact_key
            || current.expected_result_size != next_file.expected_result_size
            || current.expected_result_sha256 != next_file.expected_result_sha256
            || current.expected_metadata != next_file.expected_metadata
            || current.pre_size != next_file.pre_size
            || current.pre_sha256 != next_file.pre_sha256
            || (current.staged_metadata != next_file.staged_metadata
                && !(current.state == UndoOperationFileState::Planned
                    && next_file.state == UndoOperationFileState::Staged
                    && current.staged_metadata.is_none()
                    && next_file.staged_metadata.is_some()))
        {
            return Err(guarded_undo_error("immutable undo file identity changed"));
        }
        let changed = transaction
            .execute(
                r#"UPDATE dcc_undo_operation_files
                      SET staged_metadata_json = ?1, displaced_size = ?2,
                          displaced_sha256 = ?3, displaced_metadata_json = ?4,
                          state = ?5, verification_outcome = ?6,
                          recovery_details_json = ?7, updated_at = ?8
                    WHERE operation_id = ?9 AND ordinal = ?10 AND state = ?11"#,
                params![
                    optional_json(&next_file.staged_metadata)?,
                    next_file
                        .displaced_size
                        .map(i64::try_from)
                        .transpose()
                        .map_err(|_| guarded_undo_error("displaced_size exceeds SQLite"))?,
                    next_file.displaced_sha256.map(|digest| digest.0.to_vec()),
                    optional_json(&next_file.displaced_metadata)?,
                    next_file.state.as_str(),
                    next_file.verification_outcome.as_str(),
                    optional_json(&next_file.recovery_details)?,
                    next_file.updated_at,
                    next_file.operation_id.0,
                    i64::from(next_file.ordinal),
                    expected.as_str(),
                ],
            )
            .map_err(guarded_undo_error)?;
        if changed != 1 {
            return Err(guarded_undo_error("undo file compare-and-swap lost"));
        }
        transaction.commit().map_err(guarded_undo_error)?;
        Ok(true)
    }

    /// Journal compare-and-swap. `completed` consumes the restoration set in
    /// the same transaction. Blocked/rolled-back operations become inactive so
    /// a later prepare can create a distinct operation; recovery-required rows
    /// remain active and therefore keep blocking cleanup and retries.
    pub fn transition_undo_operation(
        &self,
        operation_id: &UndoOperationId,
        expected: &UndoOperationState,
        next: &UndoOperationState,
        reason: Option<&GuardedUndoReasonCode>,
        recovery_details: Option<&RecoveryDetailsV1>,
        updated_at: &str,
    ) -> Result<bool> {
        if !expected.is_known() || !next.is_known() || !expected.can_transition_to(next) {
            return Err(guarded_undo_error("invalid undo operation transition"));
        }
        if let Some(reason) = reason {
            if !reason.is_known() {
                return Err(guarded_undo_error("unknown undo reason code"));
            }
        }
        if let Some(details) = recovery_details {
            details.validate().map_err(guarded_undo_error)?;
        }
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn
            .transaction()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let (current, restore_set_id, journal_version): (String, String, i64) = transaction
            .query_row(
                "SELECT state, restore_set_id, journal_version FROM dcc_undo_operations WHERE operation_id = ?1",
                params![operation_id.0],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
            .ok_or_else(|| guarded_undo_error("undo operation does not exist"))?;
        let current: UndoOperationState = current.parse().map_err(guarded_undo_error)?;
        if checked_u32(journal_version, "journal_version")? != UNDO_JOURNAL_SCHEMA_VERSION {
            return Err(guarded_undo_error(
                "unknown undo journal version cannot transition",
            ));
        }
        let persisted = load_undo_operation(&transaction, operation_id)?
            .ok_or_else(|| guarded_undo_error("undo operation disappeared"))?;
        let restore = load_turn_restore_set(&transaction, &persisted.0.restore_set_id)?
            .ok_or_else(|| guarded_undo_error("restoration set disappeared"))?;
        validate_operation_manifest_binding(&persisted.0, &persisted.1, &restore.0, &restore.1)?;
        if &current == next {
            if persisted.0.reason_code.as_ref() == reason
                && persisted.0.recovery_details.as_ref() == recovery_details
            {
                return Ok(false);
            }
            return Err(guarded_undo_error("conflicting idempotent undo transition"));
        }
        if &current != expected || !current.can_transition_to(next) {
            return Err(guarded_undo_error(
                "undo operation lifecycle compare-and-swap rejected",
            ));
        }
        let inactive = matches!(
            next,
            UndoOperationState::Completed
                | UndoOperationState::RolledBack
                | UndoOperationState::Blocked
        );
        let mut desired = persisted.0;
        desired.state = next.clone();
        desired.active = !inactive;
        desired.reason_code = reason.cloned();
        desired.recovery_details = recovery_details.cloned();
        desired.updated_at = updated_at.to_owned();
        desired.completed_at = inactive.then(|| updated_at.to_owned());
        desired.validate().map_err(guarded_undo_error)?;
        if next == &UndoOperationState::Prepared
            && persisted
                .1
                .iter()
                .any(|file| file.state != UndoOperationFileState::Staged)
        {
            return Err(guarded_undo_error(
                "prepared journal requires every exchange file staged",
            ));
        }
        if next == &UndoOperationState::Completed
            && persisted.1.iter().any(|file| {
                file.state != UndoOperationFileState::Verified
                    || file.verification_outcome != VerificationOutcome::Verified
            })
        {
            return Err(guarded_undo_error(
                "completed journal requires every file verified",
            ));
        }
        if next == &UndoOperationState::RolledBack
            && persisted.1.iter().any(|file| {
                file.state != UndoOperationFileState::RolledBack
                    || file.verification_outcome != VerificationOutcome::Verified
            })
        {
            return Err(guarded_undo_error(
                "rolled-back journal requires every file rollback verified",
            ));
        }
        let changed = transaction
            .execute(
                r#"UPDATE dcc_undo_operations
                      SET state = ?1, active = ?2, reason_code = ?3,
                          recovery_details_json = ?4, updated_at = ?5,
                          completed_at = CASE WHEN ?2 = 0 THEN ?5 ELSE completed_at END
                    WHERE operation_id = ?6 AND state = ?7"#,
                params![
                    next.as_str(),
                    if inactive { 0 } else { 1 },
                    reason.map(|reason| reason.as_str()),
                    optional_json(&recovery_details.cloned())?,
                    updated_at,
                    operation_id.0,
                    expected.as_str(),
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        if changed != 1 {
            return Err(guarded_undo_error("undo lifecycle compare-and-swap lost"));
        }
        if next == &UndoOperationState::Completed {
            let consumed = transaction
                .execute(
                    "UPDATE dcc_turn_restore_sets SET state = 'consumed', reason_code = NULL, completed_at = ?1 WHERE restore_set_id = ?2 AND state = 'eligible'",
                    params![updated_at, restore_set_id],
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            if consumed != 1 {
                return Err(guarded_undo_error(
                    "completed operation could not atomically consume restoration set",
                ));
            }
        }
        transaction
            .commit()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(true)
    }

    pub fn list_session_ids_for_workspace_scope(
        &self,
        workspace_ids: &[WorkspaceId],
    ) -> Result<Vec<SessionId>> {
        if workspace_ids.is_empty() {
            return Ok(Vec::new());
        }
        let workspace_ids = workspace_ids
            .iter()
            .map(|workspace_id| workspace_id.0.as_str())
            .collect::<BTreeSet<_>>();
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut statement = conn
            .prepare("SELECT id, workspace_id, additional_workspace_ids_json FROM dcc_sessions")
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut session_ids = Vec::new();
        for row in rows {
            let (session_id, workspace_id, additional_json) =
                row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let additional =
                serde_json::from_str::<Vec<String>>(&additional_json).unwrap_or_default();
            if workspace_ids.contains(workspace_id.as_str())
                || additional
                    .iter()
                    .any(|workspace_id| workspace_ids.contains(workspace_id.as_str()))
            {
                session_ids.push(SessionId(session_id));
            }
        }
        Ok(session_ids)
    }

    fn workspace_history_scope(
        conn: &Connection,
        workspace_ids: &BTreeSet<String>,
    ) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
        let mut session_ids = BTreeSet::new();
        let mut statement = conn
            .prepare("SELECT id, workspace_id, additional_workspace_ids_json FROM dcc_sessions")
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        for row in rows {
            let (session_id, workspace_id, additional_json) =
                row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let additional =
                serde_json::from_str::<Vec<String>>(&additional_json).unwrap_or_default();
            if workspace_ids.contains(&workspace_id)
                || additional
                    .iter()
                    .any(|workspace_id| workspace_ids.contains(workspace_id))
            {
                session_ids.insert(session_id);
            }
        }
        drop(statement);

        let mut delegation_ids = BTreeSet::new();
        loop {
            let mut changed = false;
            let mut statement = conn
                .prepare(
                    "SELECT id, parent_session_id, child_session_id, workspace_id FROM dcc_delegations",
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            for row in rows {
                let (delegation_id, parent_session_id, child_session_id, workspace_id) =
                    row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                let belongs_to_scope = workspace_ids.contains(&workspace_id)
                    || session_ids.contains(&parent_session_id)
                    || child_session_id
                        .as_ref()
                        .is_some_and(|session_id| session_ids.contains(session_id));
                if !belongs_to_scope {
                    continue;
                }
                delegation_ids.insert(delegation_id);
                if let Some(child_session_id) = child_session_id {
                    changed |= session_ids.insert(child_session_id);
                }
            }
            drop(statement);
            if !changed {
                break;
            }
        }

        Ok((session_ids, delegation_ids))
    }

    fn workspace_restore_set_ids(
        conn: &Connection,
        workspace_ids: &BTreeSet<String>,
        session_ids: &BTreeSet<String>,
    ) -> Result<Vec<String>> {
        let mut statement = conn
            .prepare("SELECT restore_set_id, workspace_id, session_id FROM dcc_turn_restore_sets")
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut restore_set_ids = Vec::new();
        for row in rows {
            let (restore_set_id, workspace_id, session_id) =
                row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            if workspace_ids.contains(&workspace_id) || session_ids.contains(&session_id) {
                restore_set_ids.push(restore_set_id);
            }
        }
        Ok(restore_set_ids)
    }

    fn ensure_restore_sets_are_discardable(
        conn: &Connection,
        restore_set_ids: &[String],
    ) -> Result<()> {
        for restore_set_id in restore_set_ids {
            let active = conn
                .query_row(
                    "SELECT state FROM dcc_undo_operations WHERE restore_set_id = ?1 AND active = 1 LIMIT 1",
                    params![restore_set_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(guarded_undo_error)?;
            if active.is_some() {
                return Err(guarded_undo_error(
                    "An Undo recovery is still active for this task. Open Last turn, refresh the recovery status, and try deleting again.",
                ));
            }
        }
        Ok(())
    }

    /// Verifies that retiring a workspace cannot destroy an active Undo
    /// recovery. Eligible, ineligible and terminal captures are ordinary
    /// workspace history and are intentionally discardable on deletion.
    pub fn ensure_workspace_history_deletable(&self, workspace_ids: &[WorkspaceId]) -> Result<()> {
        if workspace_ids.is_empty() {
            return Ok(());
        }
        let workspace_ids = workspace_ids
            .iter()
            .map(|workspace_id| workspace_id.0.clone())
            .collect::<BTreeSet<_>>();
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let (session_ids, _) = Self::workspace_history_scope(&conn, &workspace_ids)?;
        let restore_set_ids = Self::workspace_restore_set_ids(&conn, &workspace_ids, &session_ids)?;
        Self::ensure_restore_sets_are_discardable(&conn, &restore_set_ids)
    }

    /// Atomically removes session, review and guarded-Undo history owned by a
    /// retiring workspace scope. Active recovery journals fail before any row
    /// is changed; terminal journals and captures are deleted in dependency
    /// order so their RESTRICT constraints cannot strand the workspace.
    pub fn delete_workspace_history(&self, workspace_ids: &[WorkspaceId]) -> Result<()> {
        if workspace_ids.is_empty() {
            return Ok(());
        }
        let workspace_ids = workspace_ids
            .iter()
            .map(|workspace_id| workspace_id.0.clone())
            .collect::<BTreeSet<_>>();
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let (session_ids, delegation_ids) =
            Self::workspace_history_scope(&transaction, &workspace_ids)?;
        let restore_set_ids =
            Self::workspace_restore_set_ids(&transaction, &workspace_ids, &session_ids)?;
        Self::ensure_restore_sets_are_discardable(&transaction, &restore_set_ids)?;

        for restore_set_id in &restore_set_ids {
            transaction
                .execute(
                    "DELETE FROM dcc_undo_operations WHERE restore_set_id = ?1 AND active = 0",
                    params![restore_set_id],
                )
                .map_err(guarded_undo_error)?;
            transaction
                .execute(
                    "DELETE FROM dcc_turn_restore_sets WHERE restore_set_id = ?1",
                    params![restore_set_id],
                )
                .map_err(guarded_undo_error)?;
        }
        for delegation_id in delegation_ids {
            transaction
                .execute(
                    "DELETE FROM dcc_delegations WHERE id = ?1",
                    params![delegation_id],
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        }
        for session_id in &session_ids {
            transaction
                .execute(
                    "DELETE FROM dcc_sessions WHERE id = ?1",
                    params![session_id],
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            transaction
                .execute(
                    "DELETE FROM dcc_session_search WHERE session_id = ?1",
                    params![session_id],
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        }
        for workspace_id in workspace_ids {
            transaction
                .execute(
                    "DELETE FROM dcc_browser_locations WHERE workspace_id = ?1",
                    params![workspace_id],
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            transaction
                .execute(
                    "DELETE FROM dcc_session_search WHERE workspace_id = ?1",
                    params![workspace_id],
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        }
        transaction
            .commit()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    pub fn delete_delegation_record(&self, id: &DelegationId) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                "DELETE FROM dcc_delegations WHERE id = ?1",
                params![id.0.clone()],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(changed > 0)
    }

    pub fn delete_search_rows_for_workspaces(&self, workspace_ids: &[WorkspaceId]) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        for workspace_id in workspace_ids {
            conn.execute(
                "DELETE FROM dcc_session_search WHERE workspace_id = ?1",
                params![workspace_id.0.clone()],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        }
        Ok(())
    }

    fn session_state_as_str(state: &SessionState) -> &'static str {
        match state {
            SessionState::Draft => "draft",
            SessionState::Active => "active",
            SessionState::Completed => "completed",
            SessionState::Aborted => "aborted",
        }
    }

    fn session_state_from_str(state: &str, column: usize) -> rusqlite::Result<SessionState> {
        match state {
            "draft" => Ok(SessionState::Draft),
            "active" => Ok(SessionState::Active),
            "completed" => Ok(SessionState::Completed),
            "aborted" => Ok(SessionState::Aborted),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown session state: {other}"),
                )),
            )),
        }
    }

    fn delegation_mode_as_str(mode: &DelegationMode) -> &'static str {
        match mode {
            DelegationMode::Review => "review",
            DelegationMode::Implement => "implement",
            DelegationMode::Explain => "explain",
            DelegationMode::Test => "test",
            DelegationMode::Research => "research",
        }
    }

    fn delegation_mode_from_str(mode: &str, column: usize) -> rusqlite::Result<DelegationMode> {
        match mode {
            "review" => Ok(DelegationMode::Review),
            "implement" => Ok(DelegationMode::Implement),
            "explain" => Ok(DelegationMode::Explain),
            "test" => Ok(DelegationMode::Test),
            "research" => Ok(DelegationMode::Research),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown delegation mode: {other}"),
                )),
            )),
        }
    }

    fn delegation_status_as_str(status: &DelegationStatus) -> &'static str {
        match status {
            DelegationStatus::Draft => "draft",
            DelegationStatus::Queued => "queued",
            DelegationStatus::Running => "running",
            DelegationStatus::ReviewPending => "review_pending",
            DelegationStatus::Completed => "completed",
            DelegationStatus::Failed => "failed",
            DelegationStatus::Cancelled => "cancelled",
        }
    }

    fn delegation_status_from_str(
        status: &str,
        column: usize,
    ) -> rusqlite::Result<DelegationStatus> {
        match status {
            "draft" => Ok(DelegationStatus::Draft),
            "queued" => Ok(DelegationStatus::Queued),
            "running" => Ok(DelegationStatus::Running),
            "review_pending" => Ok(DelegationStatus::ReviewPending),
            "completed" => Ok(DelegationStatus::Completed),
            "failed" => Ok(DelegationStatus::Failed),
            "cancelled" => Ok(DelegationStatus::Cancelled),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown delegation status: {other}"),
                )),
            )),
        }
    }

    fn delegation_from_row(row: &Row<'_>) -> rusqlite::Result<Delegation> {
        let context_policy_json = row.get::<_, String>(10)?;
        let context_policy =
            from_str::<DelegationContextPolicy>(&context_policy_json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    10,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
        let budget_json = row.get::<_, String>(11)?;
        let budget = from_str::<DelegationBudget>(&budget_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                11,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
        let touched_files_json = row.get::<_, String>(13)?;
        let touched_files = from_str::<Vec<String>>(&touched_files_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                13,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

        Ok(Delegation {
            id: DelegationId(row.get::<_, String>(0)?),
            parent_session_id: SessionId(row.get::<_, String>(1)?),
            parent_turn_id: row.get::<_, Option<String>>(2)?.map(TurnId),
            child_session_id: row.get::<_, Option<String>>(3)?.map(SessionId),
            workspace_id: WorkspaceId(row.get::<_, String>(4)?),
            target_provider_id: dcc_core::domain::provider::ProviderId(row.get::<_, String>(5)?),
            target_model_id: row.get::<_, Option<String>>(6)?,
            mode: Self::delegation_mode_from_str(&row.get::<_, String>(7)?, 7)?,
            status: Self::delegation_status_from_str(&row.get::<_, String>(8)?, 8)?,
            prompt: row.get::<_, String>(9)?,
            context_policy,
            budget,
            result_summary: row.get::<_, Option<String>>(12)?,
            touched_files,
            diff_summary: row.get::<_, Option<String>>(14)?,
            validation_summary: row.get::<_, Option<String>>(15)?,
            created_at: row.get::<_, String>(16)?,
            updated_at: row.get::<_, String>(17)?,
        })
    }

    fn delegation_worktree_state_as_str(state: &DelegationWorktreeOperationState) -> &'static str {
        match state {
            DelegationWorktreeOperationState::Preparing => "preparing",
            DelegationWorktreeOperationState::Prepared => "prepared",
            DelegationWorktreeOperationState::Bound => "bound",
            DelegationWorktreeOperationState::ReviewPending => "review_pending",
            DelegationWorktreeOperationState::Applying => "applying",
            DelegationWorktreeOperationState::Applied => "applied",
            DelegationWorktreeOperationState::Removing => "removing",
            DelegationWorktreeOperationState::Removed => "removed",
            DelegationWorktreeOperationState::CleanupRequired => "cleanup_required",
        }
    }

    fn delegation_worktree_state_from_str(
        state: &str,
        column: usize,
    ) -> rusqlite::Result<DelegationWorktreeOperationState> {
        match state {
            "preparing" => Ok(DelegationWorktreeOperationState::Preparing),
            "prepared" => Ok(DelegationWorktreeOperationState::Prepared),
            "bound" => Ok(DelegationWorktreeOperationState::Bound),
            "review_pending" => Ok(DelegationWorktreeOperationState::ReviewPending),
            "applying" => Ok(DelegationWorktreeOperationState::Applying),
            "applied" => Ok(DelegationWorktreeOperationState::Applied),
            "removing" => Ok(DelegationWorktreeOperationState::Removing),
            "removed" => Ok(DelegationWorktreeOperationState::Removed),
            "cleanup_required" => Ok(DelegationWorktreeOperationState::CleanupRequired),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown delegation worktree operation state: {other}"),
                )),
            )),
        }
    }

    fn normalize_delegation_recovery_timestamp(value: &str, field: &str) -> Result<String> {
        let parsed = DateTime::parse_from_rfc3339(value).map_err(|_| {
            dcc_core::CoreError::InvalidInput(format!(
                "delegation worktree {field} must be RFC3339"
            ))
        })?;
        Ok(parsed
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Nanos, true))
    }

    fn delegation_worktree_operation_from_row(
        row: &Row<'_>,
    ) -> rusqlite::Result<DelegationWorktreeOperation> {
        let source_root_id = row.get::<_, Option<Vec<u8>>>(11)?.map(PhysicalRootId);
        let worktree_root_id = row.get::<_, Option<Vec<u8>>>(12)?.map(PhysicalRootId);
        let common_dir_id = row.get::<_, Option<Vec<u8>>>(13)?.map(PhysicalRootId);
        for root_id in [
            source_root_id.as_ref(),
            worktree_root_id.as_ref(),
            common_dir_id.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            root_id.validate().map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    11,
                    rusqlite::types::Type::Blob,
                    Box::new(error),
                )
            })?;
        }

        let operation = DelegationWorktreeOperation {
            operation_id: DelegationWorktreeOperationId(row.get::<_, String>(0)?),
            delegation_key: row.get::<_, Option<String>>(1)?,
            delegation_id: row.get::<_, Option<String>>(2)?.map(DelegationId),
            workspace_id: WorkspaceId(row.get::<_, String>(3)?),
            parent_session_id: row.get::<_, Option<String>>(4)?.map(SessionId),
            child_session_id: row.get::<_, Option<String>>(5)?.map(SessionId),
            source_root: row.get::<_, String>(6)?,
            worktree_path: row.get::<_, String>(7)?,
            branch: row.get::<_, String>(8)?,
            base_commit: row.get::<_, String>(9)?,
            expected_branch_oid: row.get::<_, Option<String>>(10)?,
            source_root_id,
            worktree_root_id,
            common_dir_id,
            state: Self::delegation_worktree_state_from_str(&row.get::<_, String>(14)?, 14)?,
            last_error: row.get::<_, Option<String>>(15)?,
            recovery_owner: row.get::<_, Option<String>>(16)?,
            recovery_lease_until: row.get::<_, Option<String>>(17)?,
            created_at: row.get::<_, String>(18)?,
            updated_at: row.get::<_, String>(19)?,
        };
        operation.validate().map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
            )
        })?;
        Ok(operation)
    }

    fn delegation_apply_state_as_str(state: &DelegationApplyTransactionState) -> &'static str {
        match state {
            DelegationApplyTransactionState::Preparing => "preparing",
            DelegationApplyTransactionState::Prepared => "prepared",
            DelegationApplyTransactionState::Applying => "applying",
            DelegationApplyTransactionState::Applied => "applied",
            DelegationApplyTransactionState::RolledBack => "rolled_back",
            DelegationApplyTransactionState::RecoveryRequired => "recovery_required",
        }
    }

    fn delegation_apply_state_from_str(
        state: &str,
        column: usize,
    ) -> rusqlite::Result<DelegationApplyTransactionState> {
        match state {
            "preparing" => Ok(DelegationApplyTransactionState::Preparing),
            "prepared" => Ok(DelegationApplyTransactionState::Prepared),
            "applying" => Ok(DelegationApplyTransactionState::Applying),
            "applied" => Ok(DelegationApplyTransactionState::Applied),
            "rolled_back" => Ok(DelegationApplyTransactionState::RolledBack),
            "recovery_required" => Ok(DelegationApplyTransactionState::RecoveryRequired),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown delegation apply transaction state: {other}"),
                )),
            )),
        }
    }

    fn delegation_apply_transaction_from_row(
        row: &Row<'_>,
    ) -> rusqlite::Result<DelegationApplyTransaction> {
        let file_count = row.get::<_, i64>(9)?;
        let artifact_bytes = row.get::<_, i64>(10)?;
        let transaction = DelegationApplyTransaction {
            transaction_id: DelegationApplyTransactionId(row.get(0)?),
            operation_id: DelegationWorktreeOperationId(row.get(1)?),
            delegation_id: DelegationId(row.get(2)?),
            workspace_id: WorkspaceId(row.get(3)?),
            source_head_oid: row.get(4)?,
            destination_head_oid: row.get(5)?,
            destination_ref: row.get(6)?,
            destination_index_tree_oid: row.get(7)?,
            manifest_digest: row.get(8)?,
            file_count: u32::try_from(file_count)
                .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(9, file_count))?,
            artifact_bytes: u64::try_from(artifact_bytes)
                .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(10, artifact_bytes))?,
            state: Self::delegation_apply_state_from_str(&row.get::<_, String>(11)?, 11)?,
            recovery_owner: row.get(12)?,
            recovery_lease_until: row.get(13)?,
            last_error: row.get(14)?,
            created_at: row.get(15)?,
            updated_at: row.get(16)?,
        };
        transaction.validate().map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
            )
        })?;
        Ok(transaction)
    }

    fn session_from_row(row: &Row<'_>) -> rusqlite::Result<Session> {
        let provider_runtime_json = row.get::<_, Option<String>>(5)?;
        let provider_runtime = provider_runtime_json
            .as_deref()
            .map(|json| {
                from_str(json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            })
            .transpose()?;
        let additional_workspace_ids = from_str(&row.get::<_, String>(7)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

        Ok(Session {
            id: SessionId(row.get::<_, String>(0)?),
            project_id: ProjectId(row.get::<_, String>(1)?),
            workspace_id: WorkspaceId(row.get::<_, String>(2)?),
            provider_id: row.get::<_, String>(3)?,
            model: row.get::<_, Option<String>>(4)?,
            provider_runtime,
            working_directory_override: row.get::<_, Option<String>>(6)?,
            additional_workspace_ids,
            state: Self::session_state_from_str(&row.get::<_, String>(8)?, 8)?,
            created_at: row.get::<_, String>(9)?,
            updated_at: row.get::<_, String>(10)?,
        })
    }

    fn thread_from_row(row: &Row<'_>) -> rusqlite::Result<Thread> {
        Ok(Thread {
            id: ThreadId(row.get::<_, String>(0)?),
            project_id: ProjectId(row.get::<_, String>(1)?),
            session_id: row.get::<_, Option<String>>(2)?.map(SessionId),
            title: row.get::<_, String>(3)?,
            archived_at: row.get::<_, Option<String>>(4)?,
        })
    }

    fn delete_search_row_sync(conn: &Connection, session_id: &SessionId) -> Result<()> {
        conn.execute(
            "DELETE FROM dcc_session_search WHERE session_id = ?1",
            params![session_id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    fn normalize_search_query(query: &str) -> Option<String> {
        let tokens = query
            .split(|character: char| !character.is_alphanumeric())
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .take(8)
            .map(|token| format!("{token}*"))
            .collect::<Vec<_>>();
        if tokens.is_empty() {
            return None;
        }
        Some(tokens.join(" "))
    }

    fn build_search_text(events: &[SessionEventRecord]) -> String {
        #[derive(Default)]
        struct ToolCallBuffer {
            label: String,
            content: String,
            failure_reason: Option<String>,
        }

        struct AssistantMessageBuffer {
            id: String,
            phase: AssistantMessagePhase,
            content: String,
        }

        let mut fragments = Vec::new();
        let mut assistant_by_turn = HashMap::<String, Vec<AssistantMessageBuffer>>::new();
        let mut reasoning_by_turn = HashMap::<String, BTreeMap<String, String>>::new();
        let mut tool_calls_by_turn = HashMap::<String, BTreeMap<String, ToolCallBuffer>>::new();

        fn push_fragment(fragments: &mut Vec<String>, value: String) {
            let normalized = value.trim();
            if !normalized.is_empty() {
                fragments.push(normalized.to_string());
            }
        }

        fn flush_turn(
            fragments: &mut Vec<String>,
            turn_id: &str,
            assistant_by_turn: &mut HashMap<String, Vec<AssistantMessageBuffer>>,
            reasoning_by_turn: &mut HashMap<String, BTreeMap<String, String>>,
            tool_calls_by_turn: &mut HashMap<String, BTreeMap<String, ToolCallBuffer>>,
        ) {
            if let Some(messages) = assistant_by_turn.remove(turn_id) {
                let terminal_index = messages
                    .iter()
                    .rposition(|message| message.phase == AssistantMessagePhase::FinalAnswer)
                    .or_else(|| {
                        messages
                            .iter()
                            .rposition(|message| !message.content.trim().is_empty())
                    });
                for (index, message) in messages.into_iter().enumerate() {
                    let label = if Some(index) == terminal_index {
                        "Assistant"
                    } else {
                        "Assistant commentary"
                    };
                    push_fragment(fragments, format!("{label}: {}", message.content));
                }
            }

            if let Some(reasoning) = reasoning_by_turn.remove(turn_id) {
                for content in reasoning.into_values() {
                    push_fragment(fragments, format!("Reasoning: {content}"));
                }
            }

            if let Some(tool_calls) = tool_calls_by_turn.remove(turn_id) {
                for tool_call in tool_calls.into_values() {
                    let mut text = tool_call.label;
                    if !tool_call.content.trim().is_empty() {
                        text.push(' ');
                        text.push_str(tool_call.content.trim());
                    }
                    if let Some(reason) = tool_call.failure_reason {
                        if !reason.trim().is_empty() {
                            text.push(' ');
                            text.push_str(reason.trim());
                        }
                    }
                    push_fragment(fragments, text);
                }
            }
        }

        for event in events {
            match &event.kind {
                SessionEventKind::TurnStarted { prompt, .. } => {
                    push_fragment(&mut fragments, format!("User: {prompt}"));
                }
                SessionEventKind::TurnSteered { prompt, .. } => {
                    push_fragment(&mut fragments, format!("User guidance: {prompt}"));
                }
                SessionEventKind::TurnQueued { queued_turn } => {
                    push_fragment(
                        &mut fragments,
                        format!("Queued follow-up: {}", queued_turn.prompt),
                    );
                }
                SessionEventKind::TurnDelta { turn_id, content } => {
                    let messages = assistant_by_turn.entry(turn_id.0.clone()).or_default();
                    let index = messages
                        .iter()
                        .position(|message| message.id == "legacy")
                        .unwrap_or_else(|| {
                            messages.push(AssistantMessageBuffer {
                                id: "legacy".to_string(),
                                phase: AssistantMessagePhase::Unknown,
                                content: String::new(),
                            });
                            messages.len() - 1
                        });
                    messages[index].content.push_str(content);
                }
                SessionEventKind::TurnAssistantMessageStarted {
                    turn_id,
                    message_id,
                    phase,
                } => {
                    let messages = assistant_by_turn.entry(turn_id.0.clone()).or_default();
                    if let Some(message) = messages
                        .iter_mut()
                        .find(|message| message.id == *message_id)
                    {
                        message.phase = phase.clone();
                    } else {
                        messages.push(AssistantMessageBuffer {
                            id: message_id.clone(),
                            phase: phase.clone(),
                            content: String::new(),
                        });
                    }
                }
                SessionEventKind::TurnAssistantMessageDelta {
                    turn_id,
                    message_id,
                    content,
                } => {
                    let messages = assistant_by_turn.entry(turn_id.0.clone()).or_default();
                    let index = messages
                        .iter()
                        .position(|message| message.id == *message_id)
                        .unwrap_or_else(|| {
                            messages.push(AssistantMessageBuffer {
                                id: message_id.clone(),
                                phase: AssistantMessagePhase::Unknown,
                                content: String::new(),
                            });
                            messages.len() - 1
                        });
                    messages[index].content.push_str(content);
                }
                SessionEventKind::TurnAssistantMessageCompleted {
                    turn_id,
                    message_id,
                    phase,
                    content,
                } => {
                    let messages = assistant_by_turn.entry(turn_id.0.clone()).or_default();
                    let index = messages
                        .iter()
                        .position(|message| message.id == *message_id)
                        .unwrap_or_else(|| {
                            messages.push(AssistantMessageBuffer {
                                id: message_id.clone(),
                                phase: AssistantMessagePhase::Unknown,
                                content: String::new(),
                            });
                            messages.len() - 1
                        });
                    messages[index].phase = phase.clone();
                    if let Some(content) = content {
                        messages[index].content = content.clone();
                    }
                }
                SessionEventKind::TurnReasoningDelta {
                    turn_id,
                    reasoning_id,
                    content,
                } => {
                    reasoning_by_turn
                        .entry(turn_id.0.clone())
                        .or_default()
                        .entry(reasoning_id.clone())
                        .or_default()
                        .push_str(content);
                }
                SessionEventKind::TurnToolCallStarted {
                    turn_id,
                    tool_call_id,
                    action,
                    command,
                    file,
                } => {
                    let mut label = format!("Tool: {action}");
                    if let Some(command) =
                        command.as_deref().filter(|value| !value.trim().is_empty())
                    {
                        label.push(' ');
                        label.push_str(command.trim());
                    }
                    if let Some(file) = file.as_deref().filter(|value| !value.trim().is_empty()) {
                        label.push(' ');
                        label.push_str(file.trim());
                    }
                    tool_calls_by_turn
                        .entry(turn_id.0.clone())
                        .or_default()
                        .entry(tool_call_id.clone())
                        .or_insert_with(|| ToolCallBuffer {
                            label,
                            content: String::new(),
                            failure_reason: None,
                        });
                }
                SessionEventKind::TurnToolCallDelta {
                    turn_id,
                    tool_call_id,
                    content,
                } => {
                    tool_calls_by_turn
                        .entry(turn_id.0.clone())
                        .or_default()
                        .entry(tool_call_id.clone())
                        .or_insert_with(|| ToolCallBuffer {
                            label: "Tool: output".to_string(),
                            content: String::new(),
                            failure_reason: None,
                        })
                        .content
                        .push_str(content);
                }
                SessionEventKind::TurnToolCallFailed {
                    turn_id,
                    tool_call_id,
                    reason,
                } => {
                    if let Some(tool_call) = tool_calls_by_turn
                        .entry(turn_id.0.clone())
                        .or_default()
                        .get_mut(tool_call_id)
                    {
                        tool_call.failure_reason = reason.clone();
                    }
                }
                SessionEventKind::TurnUserInputRequested { questions, .. } => {
                    for question in questions {
                        push_fragment(
                            &mut fragments,
                            format!("User input requested: {}", question.question),
                        );
                    }
                }
                SessionEventKind::TurnUserInputResolved { answers, .. } => {
                    for answer in answers {
                        push_fragment(
                            &mut fragments,
                            format!("User input: {} {}", answer.question, answer.answer),
                        );
                    }
                }
                SessionEventKind::TurnPermissionRequested {
                    tool_name,
                    title,
                    description,
                    command,
                    file,
                    ..
                } => {
                    let mut text = format!("Permission request: {tool_name}");
                    for value in [
                        title.as_deref(),
                        description.as_deref(),
                        command.as_deref(),
                        file.as_deref(),
                    ] {
                        if let Some(value) = value.filter(|candidate| !candidate.trim().is_empty())
                        {
                            text.push(' ');
                            text.push_str(value.trim());
                        }
                    }
                    push_fragment(&mut fragments, text);
                }
                SessionEventKind::TurnPermissionResolved { behavior, .. } => {
                    push_fragment(&mut fragments, format!("Permission resolved: {behavior}"));
                }
                SessionEventKind::TurnNativeSubagentActivity {
                    name,
                    role,
                    model,
                    status,
                    ..
                } => {
                    let identity = name
                        .as_deref()
                        .or(role.as_deref())
                        .unwrap_or("Native subagent");
                    let model = model.as_deref().unwrap_or("model not reported");
                    push_fragment(
                        &mut fragments,
                        format!("Native subagent: {identity} · {model} · {status:?}"),
                    );
                }
                SessionEventKind::TurnNativeSubagentModelConfirmed { .. }
                | SessionEventKind::TurnNativeSubagentModelRequested { .. } => {}
                SessionEventKind::TurnModelEffective { .. } => {}
                SessionEventKind::TurnCompleted { turn_id }
                | SessionEventKind::TurnAborted { turn_id, .. } => {
                    flush_turn(
                        &mut fragments,
                        &turn_id.0,
                        &mut assistant_by_turn,
                        &mut reasoning_by_turn,
                        &mut tool_calls_by_turn,
                    );
                }
                SessionEventKind::SessionAborted { reason } => {
                    if let Some(reason) = reason.as_deref().filter(|value| !value.trim().is_empty())
                    {
                        push_fragment(&mut fragments, format!("Session aborted: {reason}"));
                    }
                }
                SessionEventKind::CheckpointCreated { label, .. } => {
                    push_fragment(&mut fragments, format!("Checkpoint: {label}"));
                }
                SessionEventKind::SessionStarted { .. }
                | SessionEventKind::QueuedTurnRemoved { .. }
                | SessionEventKind::TurnQueueReordered { .. }
                | SessionEventKind::QueuedTurnDispatched { .. }
                | SessionEventKind::TurnReasoningStarted { .. }
                | SessionEventKind::TurnReasoningCompleted { .. }
                | SessionEventKind::TurnToolCallCompleted { .. }
                | SessionEventKind::SessionCompleted
                | SessionEventKind::DelegationRequested { .. }
                | SessionEventKind::DelegationStarted { .. }
                | SessionEventKind::DelegationDelta { .. }
                | SessionEventKind::DelegationCompleted { .. }
                | SessionEventKind::DelegationFailed { .. }
                | SessionEventKind::DelegationCancelled { .. }
                | SessionEventKind::PlanApproved { .. }
                | SessionEventKind::PlanHandedOff { .. }
                | SessionEventKind::SessionResumed => {}
            }
        }

        let pending_turns = assistant_by_turn
            .keys()
            .chain(reasoning_by_turn.keys())
            .chain(tool_calls_by_turn.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        for turn_id in pending_turns {
            flush_turn(
                &mut fragments,
                &turn_id,
                &mut assistant_by_turn,
                &mut reasoning_by_turn,
                &mut tool_calls_by_turn,
            );
        }

        fragments.join("\n\n")
    }

    fn rebuild_search_index_sync(conn: &Connection) -> Result<()> {
        let mut stmt = conn
            .prepare("SELECT id FROM dcc_sessions ORDER BY created_at ASC")
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let session_ids = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        drop(stmt);

        conn.execute("DELETE FROM dcc_session_search", [])
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        for session_id in session_ids {
            Self::reindex_session_sync(conn, &SessionId(session_id))?;
        }
        Ok(())
    }

    fn reindex_session_sync(conn: &Connection, session_id: &SessionId) -> Result<()> {
        let session_and_thread = conn
            .query_row(
                r#"
                SELECT
                    s.id,
                    s.project_id,
                    s.workspace_id,
                    s.provider_id,
                    s.model,
                    s.created_at,
                    s.updated_at,
                    t.title,
                    t.archived_at
                  FROM dcc_sessions s
                  JOIN dcc_threads t ON t.session_id = s.id
                 WHERE s.id = ?1
                "#,
                params![session_id.0.clone()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let Some((
            session_id_value,
            project_id,
            workspace_id,
            provider_id,
            model,
            created_at,
            updated_at,
            thread_title,
            archived_at,
        )) = session_and_thread
        else {
            return Self::delete_search_row_sync(conn, session_id);
        };

        let mut stmt = conn
            .prepare(
                r#"
                SELECT event_id, session_id, sequence, kind_json, occurred_at,
                       terminal_turn_id, terminal_kind
                  FROM dcc_session_events
                 WHERE session_id = ?1
                 ORDER BY sequence ASC
                "#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = stmt
            .query_map(params![session_id_value.clone()], Self::event_from_row)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let events = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        drop(stmt);

        let search_text = Self::build_search_text(&events);
        Self::delete_search_row_sync(conn, session_id)?;
        conn.execute(
            r#"
            INSERT INTO dcc_session_search (
                session_id,
                workspace_id,
                project_id,
                thread_title,
                search_text,
                provider_id,
                model,
                archived_at,
                created_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                session_id_value,
                workspace_id,
                project_id,
                thread_title,
                search_text,
                provider_id,
                model,
                archived_at,
                created_at,
                updated_at,
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    fn event_from_row(row: &Row<'_>) -> rusqlite::Result<SessionEventRecord> {
        let kind_json = row.get::<_, String>(3)?;
        let kind = from_str::<SessionEventKind>(&kind_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

        Ok(SessionEventRecord {
            event_id: row.get::<_, String>(0)?,
            session_id: SessionId(row.get::<_, String>(1)?),
            sequence: row.get::<_, u64>(2)?,
            occurred_at: row.get::<_, String>(4)?,
            kind,
        })
    }

    fn event_with_metadata_from_row(
        row: &Row<'_>,
    ) -> rusqlite::Result<(SessionEventRecord, Option<String>, Option<String>)> {
        Ok((
            Self::event_from_row(row)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    }

    fn validate_event_metadata(
        event: &SessionEventRecord,
        terminal_turn_id: Option<&str>,
        terminal_kind: Option<&str>,
    ) -> Result<()> {
        let expected = match &event.kind {
            SessionEventKind::TurnCompleted { turn_id } => Some((turn_id.0.as_str(), "completed")),
            SessionEventKind::TurnAborted { turn_id, .. } => Some((turn_id.0.as_str(), "aborted")),
            _ => None,
        };
        match expected {
            Some((turn_id, kind))
                if terminal_turn_id == Some(turn_id) && terminal_kind == Some(kind) =>
            {
                Ok(())
            }
            None if terminal_turn_id.is_none() && terminal_kind.is_none() => Ok(()),
            _ => Err(dcc_core::CoreError::Repository(
                "durable event terminal metadata is inconsistent".to_string(),
            )),
        }
    }

    pub fn list_events_by_session_sync(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<SessionEventRecord>> {
        self.list_events_by_session_limited_sync(session_id, i64::MAX as usize)
    }

    /// Reads a bounded prefix in SQLite rather than materializing an unbounded
    /// session history in the process. Callers request `max + 1` when they
    /// need to reject overflow without returning a partial history.
    pub fn list_events_by_session_limited_sync(
        &self,
        session_id: &SessionId,
        limit: usize,
    ) -> Result<Vec<SessionEventRecord>> {
        let limit = i64::try_from(limit).map_err(|_| {
            dcc_core::CoreError::Repository("session event limit overflow".to_string())
        })?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"
				SELECT event_id, session_id, sequence, kind_json, occurred_at
				  FROM dcc_session_events
				 WHERE session_id = ?1
				 ORDER BY sequence ASC
				 LIMIT ?2
				"#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = stmt
            .query_map(params![session_id.0.clone(), limit], Self::event_from_row)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?);
        }

        Ok(events)
    }

    fn build_summary(&self, session: Session, thread: Thread) -> Result<WorkspaceSessionSummary> {
        let events = self.list_events_by_session_sync(&session.id)?;
        let mut last_turn_prompt = None;
        let mut last_turn_state = None;
        let mut last_turn_id = None;
        let mut last_turn_started_at = None;
        let mut last_turn_completed_at = None;
        for event in &events {
            match &event.kind {
                SessionEventKind::TurnStarted {
                    turn_id, prompt, ..
                } => {
                    last_turn_id = Some(turn_id.clone());
                    last_turn_prompt = Some(prompt.clone());
                    last_turn_state = Some("running".to_string());
                    last_turn_started_at = Some(event.occurred_at.clone());
                    last_turn_completed_at = None;
                }
                SessionEventKind::TurnCompleted { turn_id }
                    if last_turn_id.as_ref() == Some(turn_id) =>
                {
                    last_turn_state = Some("completed".to_string());
                    last_turn_completed_at = Some(event.occurred_at.clone());
                }
                SessionEventKind::TurnAborted { turn_id, .. }
                    if last_turn_id.as_ref() == Some(turn_id) =>
                {
                    last_turn_state = Some("aborted".to_string());
                    last_turn_completed_at = Some(event.occurred_at.clone());
                }
                SessionEventKind::SessionCompleted => {
                    if last_turn_state.is_none() {
                        last_turn_state = Some("completed".to_string());
                    }
                }
                SessionEventKind::SessionAborted { .. } => {
                    if last_turn_state.is_none() {
                        last_turn_state = Some("aborted".to_string());
                    }
                }
                _ => {}
            }
        }

        let projection = SessionProjection::fold(&events).unwrap_or_else(|| {
            SessionProjection::new(
                session.id.clone(),
                session.project_id.clone(),
                session.workspace_id.clone(),
                session.provider_id.clone(),
                session.model.clone(),
                session.created_at.clone(),
            )
        });

        Ok(WorkspaceSessionSummary {
            session,
            thread,
            projection,
            last_turn_prompt,
            last_turn_state,
            last_turn_started_at,
            last_turn_completed_at,
        })
    }

    fn session_search_result_from_row(row: &Row<'_>) -> rusqlite::Result<SessionSearchResult> {
        Ok(SessionSearchResult {
            session_id: SessionId(row.get::<_, String>(0)?),
            workspace_id: WorkspaceId(row.get::<_, String>(1)?),
            project_id: ProjectId(row.get::<_, String>(2)?),
            thread_title: row.get::<_, String>(3)?,
            workspace_name: row.get::<_, Option<String>>(4)?,
            workspace_branch: row.get::<_, Option<String>>(5)?,
            workspace_root_path: row.get::<_, Option<String>>(6)?,
            provider_id: row.get::<_, String>(7)?,
            model: row.get::<_, Option<String>>(8)?,
            archived_at: row.get::<_, Option<String>>(9)?,
            created_at: row.get::<_, String>(10)?,
            updated_at: row.get::<_, String>(11)?,
            snippet: row.get::<_, String>(12)?,
        })
    }

    pub fn list_workspace_sessions(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<Vec<WorkspaceSessionSummary>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"
				SELECT
					s.id, s.project_id, s.workspace_id, s.provider_id, s.model,
					s.provider_runtime_json, s.working_directory_override,
					s.additional_workspace_ids_json, s.state, s.created_at, s.updated_at,
					t.id, t.project_id, t.session_id, t.title, t.archived_at
				  FROM dcc_sessions s
				  JOIN dcc_threads t ON t.session_id = s.id
				 WHERE s.workspace_id = ?1
				 ORDER BY
				   CASE WHEN t.archived_at IS NULL THEN 0 ELSE 1 END ASC,
				   s.created_at DESC,
				   t.title DESC
				"#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let rows = stmt
            .query_map(params![workspace_id.0.clone()], |row| {
                let session = Session {
                    id: SessionId(row.get::<_, String>(0)?),
                    project_id: ProjectId(row.get::<_, String>(1)?),
                    workspace_id: WorkspaceId(row.get::<_, String>(2)?),
                    provider_id: row.get::<_, String>(3)?,
                    model: row.get::<_, Option<String>>(4)?,
                    provider_runtime: row
                        .get::<_, Option<String>>(5)?
                        .as_deref()
                        .map(|json| {
                            from_str(json).map_err(|error| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    5,
                                    rusqlite::types::Type::Text,
                                    Box::new(error),
                                )
                            })
                        })
                        .transpose()?,
                    working_directory_override: row.get::<_, Option<String>>(6)?,
                    additional_workspace_ids: from_str(&row.get::<_, String>(7)?).map_err(
                        |error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                7,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        },
                    )?,
                    state: Self::session_state_from_str(&row.get::<_, String>(8)?, 8)?,
                    created_at: row.get::<_, String>(9)?,
                    updated_at: row.get::<_, String>(10)?,
                };
                let thread = Thread {
                    id: ThreadId(row.get::<_, String>(11)?),
                    project_id: ProjectId(row.get::<_, String>(12)?),
                    session_id: row.get::<_, Option<String>>(13)?.map(SessionId),
                    title: row.get::<_, String>(14)?,
                    archived_at: row.get::<_, Option<String>>(15)?,
                };
                Ok((session, thread))
            })
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let pairs = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        drop(stmt);
        drop(conn);

        pairs
            .into_iter()
            .map(|(session, thread)| self.build_summary(session, thread))
            .collect()
    }

    pub fn search_sessions(&self, query: &str, limit: usize) -> Result<Vec<SessionSearchResult>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let safe_limit = limit.clamp(1, 100) as i64;
        let normalized_query = Self::normalize_search_query(query);
        let sql = if normalized_query.is_some() {
            r#"
            SELECT
                dcc_session_search.session_id,
                dcc_session_search.workspace_id,
                dcc_session_search.project_id,
                dcc_session_search.thread_title,
                w.name,
                w.base_branch,
                w.root_path,
                dcc_session_search.provider_id,
                dcc_session_search.model,
                dcc_session_search.archived_at,
                dcc_session_search.created_at,
                dcc_session_search.updated_at,
                snippet(dcc_session_search, 4, '', '', ' … ', 20) AS snippet
              FROM dcc_session_search
              LEFT JOIN dcc_workspaces w ON w.id = dcc_session_search.workspace_id
             WHERE dcc_session_search MATCH ?1
             ORDER BY bm25(dcc_session_search, 12.0, 1.0), dcc_session_search.updated_at DESC
             LIMIT ?2
            "#
        } else {
            r#"
            SELECT
                dcc_session_search.session_id,
                dcc_session_search.workspace_id,
                dcc_session_search.project_id,
                dcc_session_search.thread_title,
                w.name,
                w.base_branch,
                w.root_path,
                dcc_session_search.provider_id,
                dcc_session_search.model,
                dcc_session_search.archived_at,
                dcc_session_search.created_at,
                dcc_session_search.updated_at,
                substr(dcc_session_search.search_text, 1, 240) AS snippet
              FROM dcc_session_search
              LEFT JOIN dcc_workspaces w ON w.id = dcc_session_search.workspace_id
             ORDER BY dcc_session_search.updated_at DESC
             LIMIT ?1
            "#
        };
        let mut stmt = conn
            .prepare(sql)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = if let Some(normalized_query) = normalized_query {
            stmt.query_map(
                params![normalized_query, safe_limit],
                Self::session_search_result_from_row,
            )
        } else {
            stmt.query_map(params![safe_limit], Self::session_search_result_from_row)
        }
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }
}

#[async_trait]
impl WorkspaceRepo for SqliteWorkspaceRepo {
    async fn save_workspace(&self, workspace: &Workspace) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        conn.execute(
            r#"
			INSERT INTO dcc_workspaces (
				id, project_id, name, root_path, base_branch, worktree_path,
				source_json, state, setup_report_json, pinned_at, created_at, updated_at
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
			ON CONFLICT(id) DO UPDATE SET
				project_id = excluded.project_id,
				name = excluded.name,
				root_path = excluded.root_path,
				base_branch = excluded.base_branch,
				worktree_path = excluded.worktree_path,
				source_json = excluded.source_json,
				state = excluded.state,
				setup_report_json = excluded.setup_report_json,
				pinned_at = excluded.pinned_at,
				created_at = excluded.created_at,
				updated_at = excluded.updated_at
			"#,
            params![
                workspace.id.0.clone(),
                workspace.project_id.0.clone(),
                workspace.name.clone(),
                workspace.root_path.clone(),
                workspace.base_branch.clone(),
                workspace.worktree_path.clone(),
                workspace
                    .source
                    .as_ref()
                    .map(to_string)
                    .transpose()
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?,
                Self::workspace_state_as_str(&workspace.state),
                workspace
                    .setup_report
                    .as_ref()
                    .map(to_string)
                    .transpose()
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?,
                workspace.pinned_at.clone(),
                workspace.created_at.clone(),
                workspace.updated_at.clone(),
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        Ok(())
    }

    async fn get_workspace(&self, id: &WorkspaceId) -> Result<Option<Workspace>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let workspace = conn
            .query_row(
                r#"
				SELECT id, project_id, name, root_path, base_branch, worktree_path,
				       source_json, state, setup_report_json, pinned_at, created_at, updated_at
				  FROM dcc_workspaces
				 WHERE id = ?1
				"#,
                params![id.0.clone()],
                Self::workspace_from_row,
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        Ok(workspace)
    }

    async fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"
				SELECT id, project_id, name, root_path, base_branch, worktree_path,
				       source_json, state, setup_report_json, pinned_at, created_at, updated_at
				  FROM dcc_workspaces
				 ORDER BY updated_at DESC, created_at DESC
				"#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = stmt
            .query_map([], Self::workspace_from_row)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let mut workspaces = Vec::new();
        for row in rows {
            workspaces
                .push(row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?);
        }

        Ok(workspaces)
    }

    async fn delete_workspace(&self, id: &WorkspaceId) -> Result<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn
            .transaction()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        transaction
            .execute(
                "DELETE FROM dcc_browser_locations WHERE workspace_id = ?1",
                params![id.0.clone()],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        transaction
            .execute(
                "DELETE FROM dcc_workspaces WHERE id = ?1",
                params![id.0.clone()],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        transaction
            .commit()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl WorkspaceBundleRepo for SqliteWorkspaceRepo {
    async fn save_workspace_bundle(
        &self,
        bundle: &WorkspaceBundle,
        members: &[WorkspaceBundleMember],
    ) -> Result<()> {
        if bundle.name.trim().is_empty() {
            return Err(dcc_core::CoreError::InvalidInput(
                "workspace bundle name cannot be empty".to_string(),
            ));
        }
        if members.len() < 2 {
            return Err(dcc_core::CoreError::InvalidInput(
                "workspace bundle requires at least two members".to_string(),
            ));
        }
        let mut workspace_ids = BTreeSet::new();
        for member in members {
            if member.bundle_id != bundle.id {
                return Err(dcc_core::CoreError::InvalidInput(
                    "workspace bundle member references a different bundle".to_string(),
                ));
            }
            if !workspace_ids.insert(member.workspace_id.0.clone()) {
                return Err(dcc_core::CoreError::InvalidInput(format!(
                    "workspace bundle contains duplicate member: {}",
                    member.workspace_id.0
                )));
            }
        }
        if !workspace_ids.contains(&bundle.primary_workspace_id.0) {
            return Err(dcc_core::CoreError::InvalidInput(
                "workspace bundle primary workspace must be a member".to_string(),
            ));
        }

        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        tx.execute(
            r#"
			INSERT INTO dcc_workspace_bundles (
				id, name, primary_workspace_id, state, created_at, updated_at
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
			ON CONFLICT(id) DO UPDATE SET
				name = excluded.name,
				primary_workspace_id = excluded.primary_workspace_id,
				state = excluded.state,
				created_at = excluded.created_at,
				updated_at = excluded.updated_at
			"#,
            params![
                bundle.id.0.clone(),
                bundle.name.clone(),
                bundle.primary_workspace_id.0.clone(),
                Self::workspace_bundle_state_as_str(&bundle.state),
                bundle.created_at.clone(),
                bundle.updated_at.clone(),
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        tx.execute(
            "DELETE FROM dcc_workspace_bundle_members WHERE bundle_id = ?1",
            params![bundle.id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        for member in members {
            tx.execute(
                r#"
				INSERT INTO dcc_workspace_bundle_members (
					bundle_id, workspace_id, created_for_bundle, position,
					role, allow_analysis, allow_implementation
				) VALUES (?1, ?2, ?3, ?4, ?5, 1, 1)
				"#,
                params![
                    member.bundle_id.0.clone(),
                    member.workspace_id.0.clone(),
                    member.created_for_bundle,
                    member.position,
                    if member.workspace_id == bundle.primary_workspace_id {
                        "primary"
                    } else {
                        "contributor"
                    },
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        }
        tx.commit()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    async fn get_workspace_bundle(
        &self,
        id: &WorkspaceBundleId,
    ) -> Result<Option<WorkspaceBundleSummary>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let bundle = conn
            .query_row(
                r#"
				SELECT id, name, primary_workspace_id, state, created_at, updated_at
				  FROM dcc_workspace_bundles
				 WHERE id = ?1
				"#,
                params![id.0.clone()],
                Self::workspace_bundle_from_row,
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let Some(bundle) = bundle else {
            return Ok(None);
        };
        let members = Self::workspace_bundle_members(&conn, &bundle.id)?;
        Ok(Some(WorkspaceBundleSummary { bundle, members }))
    }

    async fn get_workspace_bundle_for_workspace(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<Option<WorkspaceBundleSummary>> {
        let bundle_id = {
            let conn = self
                .conn
                .lock()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            conn.query_row(
                "SELECT bundle_id FROM dcc_workspace_bundle_members WHERE workspace_id = ?1",
                params![workspace_id.0.clone()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
        };
        let Some(bundle_id) = bundle_id else {
            return Ok(None);
        };
        self.get_workspace_bundle(&WorkspaceBundleId(bundle_id))
            .await
    }

    async fn list_workspace_bundles(&self) -> Result<Vec<WorkspaceBundleSummary>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let bundles = {
            let mut stmt = conn
                .prepare(
                    r#"
					SELECT id, name, primary_workspace_id, state, created_at, updated_at
					  FROM dcc_workspace_bundles
					 ORDER BY updated_at DESC, created_at DESC
					"#,
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let rows = stmt
                .query_map([], Self::workspace_bundle_from_row)
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
        };
        bundles
            .into_iter()
            .map(|bundle| {
                let members = Self::workspace_bundle_members(&conn, &bundle.id)?;
                Ok(WorkspaceBundleSummary { bundle, members })
            })
            .collect()
    }

    async fn set_workspace_bundle_state(
        &self,
        id: &WorkspaceBundleId,
        state: WorkspaceBundleState,
        updated_at: String,
    ) -> Result<Option<WorkspaceBundleSummary>> {
        {
            let mut conn = self
                .conn
                .lock()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let tx = conn
                .transaction()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let updated = tx
                .execute(
                    "UPDATE dcc_workspace_bundles SET state = ?1, updated_at = ?2 WHERE id = ?3",
                    params![
                        Self::workspace_bundle_state_as_str(&state),
                        updated_at.clone(),
                        id.0.clone()
                    ],
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            if updated == 0 {
                return Ok(None);
            }
            match state {
                WorkspaceBundleState::Archived => {
                    tx.execute(
                        r#"
						UPDATE dcc_workspace_bundle_members
						   SET workspace_state_before_archive = (
						       SELECT state
						         FROM dcc_workspaces
						        WHERE id = dcc_workspace_bundle_members.workspace_id
						   )
						 WHERE bundle_id = ?1
						   AND workspace_state_before_archive IS NULL
						"#,
                        params![id.0.clone()],
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                    tx.execute(
                        r#"
						UPDATE dcc_workspaces
						   SET state = 'archived', pinned_at = NULL, updated_at = ?1
						 WHERE id IN (
						       SELECT workspace_id
						         FROM dcc_workspace_bundle_members
						        WHERE bundle_id = ?2
						 )
						"#,
                        params![updated_at.clone(), id.0.clone()],
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                }
                WorkspaceBundleState::Completed => {
                    tx.execute(
                        r#"
						UPDATE dcc_workspaces
						   SET state = 'completed', pinned_at = NULL, updated_at = ?1
						 WHERE id IN (
						       SELECT workspace_id
						         FROM dcc_workspace_bundle_members
						        WHERE bundle_id = ?2
						 )
						"#,
                        params![updated_at.clone(), id.0.clone()],
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                }
                WorkspaceBundleState::Ready => {
                    tx.execute(
                        r#"
						UPDATE dcc_workspaces
						   SET state = COALESCE((
						       SELECT workspace_state_before_archive
						         FROM dcc_workspace_bundle_members
						        WHERE bundle_id = ?2
						          AND workspace_id = dcc_workspaces.id
						   ), 'ready'),
						       updated_at = ?1
						 WHERE id IN (
						       SELECT workspace_id
						         FROM dcc_workspace_bundle_members
						        WHERE bundle_id = ?2
						 )
						"#,
                        params![updated_at.clone(), id.0.clone()],
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                    tx.execute(
                        r#"
						UPDATE dcc_workspace_bundle_members
						   SET workspace_state_before_archive = NULL
						 WHERE bundle_id = ?1
						"#,
                        params![id.0.clone()],
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                }
            }
            tx.commit()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        }
        self.get_workspace_bundle(id).await
    }

    async fn delete_workspace_bundle(&self, id: &WorkspaceBundleId) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            "DELETE FROM dcc_workspace_bundles WHERE id = ?1",
            params![id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl RepositoryRepo for SqliteWorkspaceRepo {
    async fn save_repository(&self, repository: &Repository) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        conn.execute(
            r#"
			INSERT INTO dcc_repositories (
				id, project_id, name, display_name, icon, color, pinned_at, root_path, base_branch, remote, remote_url, forge_provider, forge_login, created_at, updated_at
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
			ON CONFLICT(root_path) DO UPDATE SET
				id = excluded.id,
				project_id = excluded.project_id,
				name = excluded.name,
				display_name = COALESCE(excluded.display_name, dcc_repositories.display_name),
				icon = COALESCE(excluded.icon, dcc_repositories.icon),
				color = COALESCE(excluded.color, dcc_repositories.color),
				pinned_at = COALESCE(excluded.pinned_at, dcc_repositories.pinned_at),
				base_branch = excluded.base_branch,
				remote = excluded.remote,
				remote_url = excluded.remote_url,
				forge_provider = excluded.forge_provider,
				-- A missing login in a repository snapshot means "not provided".
				-- Explicit logout uses update_repository_forge_login and can still
				-- clear this value intentionally.
				forge_login = COALESCE(excluded.forge_login, dcc_repositories.forge_login),
				created_at = excluded.created_at,
				updated_at = excluded.updated_at
			"#,
            params![
                repository.id.0.clone(),
                repository.project_id.0.clone(),
                repository.name.clone(),
                repository.display_name.clone(),
                repository.icon.clone(),
                repository.color.clone(),
                repository.pinned_at.clone(),
                repository.root_path.clone(),
                repository.base_branch.clone(),
                repository.remote.clone(),
                repository.remote_url.clone(),
                repository.forge_provider.clone(),
                repository.forge_login.clone(),
                repository.created_at.clone(),
                repository.updated_at.clone(),
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        Ok(())
    }

    async fn get_repository(&self, id: &RepositoryId) -> Result<Option<Repository>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let repository = conn
            .query_row(
                r#"
				SELECT id, project_id, name, display_name, icon, color, pinned_at, root_path, base_branch, remote, remote_url, forge_provider, forge_login, created_at, updated_at
				  FROM dcc_repositories
				 WHERE id = ?1
				"#,
                params![id.0.clone()],
                Self::repository_from_row,
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        Ok(repository)
    }

    async fn list_repositories(&self) -> Result<Vec<Repository>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"
				SELECT id, project_id, name, display_name, icon, color, pinned_at, root_path, base_branch, remote, remote_url, forge_provider, forge_login, created_at, updated_at
				  FROM dcc_repositories
				 ORDER BY updated_at DESC, created_at DESC, name ASC
				"#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = stmt
            .query_map([], Self::repository_from_row)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let mut repositories = Vec::new();
        for row in rows {
            repositories
                .push(row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?);
        }

        Ok(repositories)
    }

    async fn delete_repository(&self, id: &RepositoryId) -> Result<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        tx.execute(
            "DELETE FROM dcc_browser_locations WHERE workspace_id IN (SELECT id FROM dcc_workspaces WHERE root_path = ?1)",
            params![id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        tx.execute(
            "DELETE FROM dcc_workspaces WHERE root_path = ?1",
            params![id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        tx.execute(
            "DELETE FROM dcc_repositories WHERE id = ?1",
            params![id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        tx.commit()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }
}

fn guarded_undo_error(error: impl ToString) -> dcc_core::CoreError {
    dcc_core::CoreError::Repository(format!("guarded undo: {}", error.to_string()))
}

fn require_maintenance_timestamp(value: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(guarded_undo_error(
            "maintenance timestamp must not be empty",
        ))
    } else {
        Ok(())
    }
}

fn optional_json<T: serde::Serialize>(value: &Option<T>) -> Result<Option<String>> {
    value
        .as_ref()
        .map(|value| to_string(value).map_err(guarded_undo_error))
        .transpose()
}

fn parse_guarded_json<T: DeserializeOwned>(raw: &str, field: &str) -> Result<T> {
    from_str(raw).map_err(|_| guarded_undo_error(format!("invalid {field} schema")))
}

fn checked_u64(value: i64, field: &str) -> Result<u64> {
    u64::try_from(value).map_err(|_| guarded_undo_error(format!("invalid {field}")))
}

fn checked_u32(value: i64, field: &str) -> Result<u32> {
    u32::try_from(value).map_err(|_| guarded_undo_error(format!("invalid {field}")))
}

fn digest_from_blob(bytes: Vec<u8>) -> Result<Sha256Digest> {
    Sha256Digest::from_slice(&bytes).map_err(guarded_undo_error)
}

fn artifact_key_from_blob(bytes: Vec<u8>) -> Result<ArtifactKey> {
    ArtifactKey::from_slice(&bytes).map_err(guarded_undo_error)
}

fn validate_operation_manifest_binding(
    operation: &UndoOperation,
    operation_files: &[UndoOperationFile],
    restore_set: &TurnRestoreSet,
    restore_files: &[TurnRestoreFile],
) -> Result<()> {
    let state_matches = restore_set.state == RestoreSetState::Eligible
        || (restore_set.state == RestoreSetState::Consumed
            && operation.state == UndoOperationState::Completed);
    if restore_set.capture_version != RESTORE_CAPTURE_VERSION
        || !state_matches
        || operation.restore_set_id != restore_set.restore_set_id
        || operation_files.is_empty()
        || operation_files.len() != restore_files.len()
        || operation_files
            .iter()
            .any(|file| !file.state.is_known() || !file.verification_outcome.is_known())
        || operation.prepared_identity.root_id
            != restore_set
                .root_id
                .clone()
                .ok_or_else(|| guarded_undo_error("eligible set has no physical root"))?
        || operation.prepared_identity.git
            != restore_set
                .git_identity
                .clone()
                .ok_or_else(|| guarded_undo_error("eligible set has no Git identity"))?
        || Some(operation.prepared_identity.manifest_digest) != restore_set.manifest_digest
    {
        return Err(guarded_undo_error(
            "undo journal is not bound to an eligible restoration manifest",
        ));
    }
    validate_restore_set_manifest(restore_set, restore_files).map_err(guarded_undo_error)?;
    let mut exchange_keys = BTreeSet::new();
    for (journal, restored) in operation_files.iter().zip(restore_files) {
        if journal.operation_id != operation.operation_id
            || journal.restore_set_id != restore_set.restore_set_id
            || journal.ordinal != restored.ordinal
            || journal.path_bytes != restored.path_bytes
            || journal.expected_result_size != restored.result_size
            || journal.expected_result_sha256 != restored.result_sha256
            || journal.expected_metadata != restored.metadata_fingerprint
            || journal.pre_size != restored.pre_size
            || journal.pre_sha256 != restored.pre_sha256
            || !exchange_keys.insert(journal.exchange_artifact_key.0)
        {
            return Err(guarded_undo_error(
                "undo journal file is not bound to its opaque restoration record",
            ));
        }
    }
    Ok(())
}

fn insert_turn_restore_file(conn: &Connection, file: &TurnRestoreFile) -> Result<()> {
    file.validate().map_err(guarded_undo_error)?;
    conn.execute(
        r#"INSERT INTO dcc_turn_restore_files (
            restore_set_id, ordinal, path_bytes, status, pre_size, pre_sha256,
            pre_artifact_key, result_size, result_sha256, metadata_fingerprint_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"#,
        params![
            file.restore_set_id.0,
            i64::from(file.ordinal),
            file.path_bytes.0,
            file.status.as_str(),
            i64::try_from(file.pre_size)
                .map_err(|_| guarded_undo_error("pre_size exceeds SQLite"))?,
            file.pre_sha256.0.to_vec(),
            file.pre_artifact_key.0.to_vec(),
            i64::try_from(file.result_size)
                .map_err(|_| guarded_undo_error("result_size exceeds SQLite"))?,
            file.result_sha256.0.to_vec(),
            to_string(&file.metadata_fingerprint).map_err(guarded_undo_error)?,
        ],
    )
    .map_err(guarded_undo_error)?;
    Ok(())
}

fn load_turn_restore_set(
    conn: &Connection,
    restore_set_id: &RestoreSetId,
) -> Result<Option<(TurnRestoreSet, Vec<TurnRestoreFile>)>> {
    let bounds = conn
        .query_row(
            r#"SELECT COALESCE(length(git_identity_json), 0),
                      COALESCE(length(root_id), 0),
                      (SELECT COUNT(*) FROM dcc_turn_restore_files files WHERE files.restore_set_id = sets.restore_set_id),
                      (SELECT COALESCE(MAX(length(path_bytes)), 0) FROM dcc_turn_restore_files files WHERE files.restore_set_id = sets.restore_set_id),
                      (SELECT COALESCE(MAX(length(metadata_fingerprint_json)), 0) FROM dcc_turn_restore_files files WHERE files.restore_set_id = sets.restore_set_id)
                 FROM dcc_turn_restore_sets sets WHERE restore_set_id = ?1"#,
            params![restore_set_id.0],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()
        .map_err(guarded_undo_error)?;
    let Some((git_json_bytes, root_bytes, child_count, max_path, max_metadata)) = bounds else {
        return Ok(None);
    };
    if git_json_bytes > 65_536
        || root_bytes > 1_024
        || child_count > i64::from(MAX_RESTORE_FILES)
        || max_path > 4_096
        || max_metadata > 4_096
    {
        return Err(guarded_undo_error(
            "restoration record exceeds persistence bounds",
        ));
    }
    let raw = conn
        .query_row(
            r#"SELECT restore_set_id, snapshot_id, session_id, turn_id,
                      workspace_id, root_id, capture_version, state, reason_code,
                      git_identity_json, artifact_bytes, file_count,
                      manifest_digest, created_at, completed_at, expires_at
                 FROM dcc_turn_restore_sets WHERE restore_set_id = ?1"#,
            params![restore_set_id.0],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, Option<Vec<u8>>>(12)?,
                    row.get::<_, String>(13)?,
                    row.get::<_, Option<String>>(14)?,
                    row.get::<_, Option<String>>(15)?,
                ))
            },
        )
        .optional()
        .map_err(guarded_undo_error)?;
    let Some((
        id,
        snapshot_id,
        session_id,
        turn_id,
        workspace_id,
        root_id,
        capture_version,
        state,
        reason,
        git_json,
        artifact_bytes,
        file_count,
        digest,
        created_at,
        completed_at,
        expires_at,
    )) = raw
    else {
        return Ok(None);
    };
    let root_id = root_id.map(PhysicalRootId);
    if let Some(root_id) = &root_id {
        root_id.validate().map_err(guarded_undo_error)?;
    }
    let git_identity = git_json
        .as_deref()
        .map(|json| parse_guarded_json::<GitIdentityV1>(json, "git identity"))
        .transpose()?;
    if let Some(git_identity) = &git_identity {
        git_identity.validate().map_err(guarded_undo_error)?;
    }
    let state: RestoreSetState = state.parse().map_err(guarded_undo_error)?;
    let reason_code = reason
        .map(|reason| reason.parse().map_err(guarded_undo_error))
        .transpose()?;
    let restore_set = TurnRestoreSet {
        restore_set_id: RestoreSetId(id),
        snapshot_id,
        session_id: SessionId(session_id),
        turn_id: TurnId(turn_id),
        workspace_id: WorkspaceId(workspace_id),
        root_id,
        capture_version: checked_u32(capture_version, "capture_version")?,
        state,
        reason_code,
        git_identity,
        artifact_bytes: checked_u64(artifact_bytes, "artifact_bytes")?,
        file_count: checked_u32(file_count, "file_count")?,
        manifest_digest: digest.map(digest_from_blob).transpose()?,
        created_at,
        completed_at,
        expires_at,
    };
    // Validate the complete parent before interpreting any state-specific
    // child rows.  This rejects unknown/future states and reasons closed.
    restore_set.validate().map_err(guarded_undo_error)?;
    if restore_set.file_count > MAX_RESTORE_FILES {
        return Err(guarded_undo_error(
            "restoration accounting exceeds file limit",
        ));
    }
    let mut statement = conn
        .prepare(
            r#"SELECT ordinal, path_bytes, status, pre_size, pre_sha256,
                      pre_artifact_key, result_size, result_sha256,
                      metadata_fingerprint_json
                 FROM dcc_turn_restore_files WHERE restore_set_id = ?1
                ORDER BY ordinal"#,
        )
        .map_err(guarded_undo_error)?;
    let raw_files = statement
        .query_map(params![restore_set_id.0], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Vec<u8>>(4)?,
                row.get::<_, Vec<u8>>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Vec<u8>>(7)?,
                row.get::<_, String>(8)?,
            ))
        })
        .map_err(guarded_undo_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(guarded_undo_error)?;
    let mut files = Vec::with_capacity(raw_files.len());
    for (
        ordinal,
        path,
        status,
        pre_size,
        pre_digest,
        artifact_key,
        result_size,
        result_digest,
        metadata_json,
    ) in raw_files
    {
        let metadata: RegularFileMetadataV1 = parse_guarded_json(&metadata_json, "file metadata")?;
        metadata.validate().map_err(guarded_undo_error)?;
        let file = TurnRestoreFile {
            restore_set_id: restore_set.restore_set_id.clone(),
            ordinal: checked_u32(ordinal, "ordinal")?,
            path_bytes: OpaqueRepoPath::from_persisted(path),
            status: status.parse().map_err(guarded_undo_error)?,
            pre_size: checked_u64(pre_size, "pre_size")?,
            pre_sha256: digest_from_blob(pre_digest)?,
            pre_artifact_key: artifact_key_from_blob(artifact_key)?,
            result_size: checked_u64(result_size, "result_size")?,
            result_sha256: digest_from_blob(result_digest)?,
            metadata_fingerprint: metadata,
        };
        file.validate().map_err(guarded_undo_error)?;
        files.push(file);
    }
    if !files.is_empty() {
        validate_restore_set_manifest(&restore_set, &files).map_err(guarded_undo_error)?;
    } else if restore_set.manifest_digest.is_some() {
        return Err(guarded_undo_error(
            "restoration manifest is missing its file rows",
        ));
    }
    Ok(Some((restore_set, files)))
}

fn insert_undo_operation_file(conn: &Connection, file: &UndoOperationFile) -> Result<()> {
    file.validate().map_err(guarded_undo_error)?;
    conn.execute(
        r#"INSERT INTO dcc_undo_operation_files (
            operation_id, restore_set_id, ordinal, path_bytes, exchange_artifact_key,
            expected_result_size, expected_result_sha256, expected_metadata_json,
            pre_size, pre_sha256, staged_metadata_json, displaced_size, displaced_sha256,
            displaced_metadata_json, state, verification_outcome,
            recovery_details_json, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)"#,
        params![
            file.operation_id.0,
            file.restore_set_id.0,
            i64::from(file.ordinal),
            file.path_bytes.0,
            file.exchange_artifact_key.0.to_vec(),
            i64::try_from(file.expected_result_size)
                .map_err(|_| guarded_undo_error("expected_result_size exceeds SQLite"))?,
            file.expected_result_sha256.0.to_vec(),
            to_string(&file.expected_metadata).map_err(guarded_undo_error)?,
            i64::try_from(file.pre_size)
                .map_err(|_| guarded_undo_error("pre_size exceeds SQLite"))?,
            file.pre_sha256.0.to_vec(),
            optional_json(&file.staged_metadata)?,
            file.displaced_size
                .map(i64::try_from)
                .transpose()
                .map_err(|_| guarded_undo_error("displaced_size exceeds SQLite"))?,
            file.displaced_sha256.map(|digest| digest.0.to_vec()),
            optional_json(&file.displaced_metadata)?,
            file.state.as_str(),
            file.verification_outcome.as_str(),
            optional_json(&file.recovery_details)?,
            file.updated_at,
        ],
    )
    .map_err(guarded_undo_error)?;
    Ok(())
}

fn load_undo_operation(
    conn: &Connection,
    operation_id: &UndoOperationId,
) -> Result<Option<(UndoOperation, Vec<UndoOperationFile>)>> {
    let bounds = conn
        .query_row(
            r#"SELECT length(prepared_identity_json),
                      COALESCE(length(recovery_details_json), 0),
                      (SELECT COUNT(*) FROM dcc_undo_operation_files files WHERE files.operation_id = operations.operation_id),
                      (SELECT COALESCE(MAX(length(path_bytes)), 0) FROM dcc_undo_operation_files files WHERE files.operation_id = operations.operation_id),
                      (SELECT COALESCE(MAX(length(expected_metadata_json)), 0) FROM dcc_undo_operation_files files WHERE files.operation_id = operations.operation_id),
                      (SELECT COALESCE(MAX(length(staged_metadata_json)), 0) FROM dcc_undo_operation_files files WHERE files.operation_id = operations.operation_id),
                      (SELECT COALESCE(MAX(length(displaced_metadata_json)), 0) FROM dcc_undo_operation_files files WHERE files.operation_id = operations.operation_id),
                      (SELECT COALESCE(MAX(length(recovery_details_json)), 0) FROM dcc_undo_operation_files files WHERE files.operation_id = operations.operation_id)
                 FROM dcc_undo_operations operations WHERE operation_id = ?1"#,
            params![operation_id.0],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            },
        )
        .optional()
        .map_err(guarded_undo_error)?;
    let Some((
        prepared_bytes,
        recovery_bytes,
        child_count,
        max_path,
        max_expected,
        max_staged,
        max_displaced,
        max_recovery,
    )) = bounds
    else {
        return Ok(None);
    };
    if prepared_bytes > 131_072
        || recovery_bytes > 1_024
        || child_count > i64::from(MAX_RESTORE_FILES)
        || max_path > 4_096
        || max_expected > 4_096
        || max_staged > 4_096
        || max_displaced > 4_096
        || max_recovery > 1_024
    {
        return Err(guarded_undo_error(
            "undo journal exceeds persistence bounds",
        ));
    }
    let raw = conn
        .query_row(
            r#"SELECT operation_id, restore_set_id, journal_version, state,
                      active, preview_token_digest, prepared_identity_json,
                      reason_code, recovery_details_json, created_at, updated_at,
                      completed_at
                 FROM dcc_undo_operations WHERE operation_id = ?1"#,
            params![operation_id.0],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, bool>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, Option<String>>(11)?,
                ))
            },
        )
        .optional()
        .map_err(guarded_undo_error)?;
    let Some((
        id,
        set_id,
        version,
        state,
        active,
        token,
        prepared_json,
        reason,
        recovery_json,
        created_at,
        updated_at,
        completed_at,
    )) = raw
    else {
        return Ok(None);
    };
    let prepared_identity: PreparedIdentityV1 =
        parse_guarded_json(&prepared_json, "prepared identity")?;
    prepared_identity.validate().map_err(guarded_undo_error)?;
    let recovery_details = recovery_json
        .as_deref()
        .map(|json| parse_guarded_json::<RecoveryDetailsV1>(json, "recovery details"))
        .transpose()?;
    if let Some(details) = &recovery_details {
        details.validate().map_err(guarded_undo_error)?;
    }
    let operation = UndoOperation {
        operation_id: UndoOperationId(id),
        restore_set_id: RestoreSetId(set_id),
        journal_version: checked_u32(version, "journal_version")?,
        state: state.parse().map_err(guarded_undo_error)?,
        active,
        preview_token_digest: token.map(digest_from_blob).transpose()?,
        prepared_identity,
        reason_code: reason
            .map(|value| value.parse().map_err(guarded_undo_error))
            .transpose()?,
        recovery_details,
        created_at,
        updated_at,
        completed_at,
    };
    let mut statement = conn
        .prepare(
            r#"SELECT ordinal, restore_set_id, path_bytes, exchange_artifact_key, expected_result_size,
                  expected_result_sha256, expected_metadata_json, pre_size,
                  pre_sha256, staged_metadata_json, displaced_size, displaced_sha256,
                  displaced_metadata_json, state, verification_outcome,
                  recovery_details_json, updated_at
             FROM dcc_undo_operation_files WHERE operation_id = ?1 ORDER BY ordinal"#,
        )
        .map_err(guarded_undo_error)?;
    let rows = statement
        .query_map(params![operation_id.0], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Vec<u8>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, Vec<u8>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<i64>>(10)?,
                row.get::<_, Option<Vec<u8>>>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, String>(13)?,
                row.get::<_, String>(14)?,
                row.get::<_, Option<String>>(15)?,
                row.get::<_, String>(16)?,
            ))
        })
        .map_err(guarded_undo_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(guarded_undo_error)?;
    let mut files = Vec::with_capacity(rows.len());
    for (
        ordinal,
        restore_set_id,
        path,
        key,
        expected_size,
        expected_digest,
        expected_meta,
        pre_size,
        pre_digest,
        staged_meta,
        displaced_size,
        displaced_digest,
        displaced_meta,
        state,
        outcome,
        recovery,
        updated_at,
    ) in rows
    {
        let file = UndoOperationFile {
            operation_id: operation.operation_id.clone(),
            restore_set_id: RestoreSetId(restore_set_id),
            ordinal: checked_u32(ordinal, "ordinal")?,
            path_bytes: OpaqueRepoPath::from_persisted(path),
            exchange_artifact_key: artifact_key_from_blob(key)?,
            expected_result_size: checked_u64(expected_size, "expected_result_size")?,
            expected_result_sha256: digest_from_blob(expected_digest)?,
            expected_metadata: parse_guarded_json(&expected_meta, "expected metadata")?,
            pre_size: checked_u64(pre_size, "pre_size")?,
            pre_sha256: digest_from_blob(pre_digest)?,
            staged_metadata: staged_meta
                .as_deref()
                .map(|json| parse_guarded_json(json, "staged metadata"))
                .transpose()?,
            displaced_size: displaced_size
                .map(|value| checked_u64(value, "displaced_size"))
                .transpose()?,
            displaced_sha256: displaced_digest.map(digest_from_blob).transpose()?,
            displaced_metadata: displaced_meta
                .as_deref()
                .map(|json| parse_guarded_json(json, "displaced metadata"))
                .transpose()?,
            state: state.parse().map_err(guarded_undo_error)?,
            verification_outcome: outcome.parse().map_err(guarded_undo_error)?,
            recovery_details: recovery
                .as_deref()
                .map(|json| parse_guarded_json(json, "file recovery details"))
                .transpose()?,
            updated_at,
        };
        if file.state.is_known() && file.verification_outcome.is_known() {
            file.validate().map_err(guarded_undo_error)?;
        }
        files.push(file);
    }
    if operation.state.is_known() {
        operation.validate().map_err(guarded_undo_error)?;
    }
    Ok(Some((operation, files)))
}

fn validate_browser_location_scope(workspace_id: &str, session_id: &str) -> Result<()> {
    if workspace_id.is_empty()
        || workspace_id.chars().count() > BROWSER_LOCATION_MAX_SCOPE_CHARS
        || session_id.chars().count() > BROWSER_LOCATION_MAX_SCOPE_CHARS
        || workspace_id.chars().any(char::is_control)
        || session_id.chars().any(char::is_control)
    {
        return Err(dcc_core::CoreError::InvalidInput(
            "browser location scope is invalid".to_string(),
        ));
    }
    Ok(())
}

fn validate_browser_location_url(safe_url: &str) -> Result<()> {
    if safe_url.is_empty()
        || safe_url.chars().count() > BROWSER_LOCATION_MAX_URL_CHARS
        || safe_url.chars().any(char::is_control)
    {
        return Err(dcc_core::CoreError::InvalidInput(
            "browser location URL is invalid".to_string(),
        ));
    }
    Ok(())
}

fn browser_location_session_id(session_id: Option<&str>) -> &str {
    session_id.unwrap_or("")
}

fn validate_provider_availability_provider_id(provider_id: &str) -> Result<()> {
    if provider_id.trim().is_empty()
        || provider_id.chars().count() > PROVIDER_AVAILABILITY_MAX_PROVIDER_ID_CHARS
        || provider_id.chars().any(char::is_control)
    {
        return Err(dcc_core::CoreError::InvalidInput(
            "provider availability provider id is invalid".to_string(),
        ));
    }
    Ok(())
}

impl SqliteSessionRepo {
    /// Reads DCC runtime availability. A missing row deliberately remains
    /// enabled for compatibility with existing session databases.
    pub fn load_provider_availability(
        &self,
        provider_id: &str,
    ) -> Result<Option<ProviderAvailabilityRecord>> {
        validate_provider_availability_provider_id(provider_id)?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let row = conn
            .query_row(
                "SELECT enabled, generation, updated_at_ms FROM dcc_provider_availability WHERE provider_id = ?1",
                params![provider_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let Some((enabled, generation, updated_at_ms)) = row else {
            return Ok(None);
        };
        if !matches!(enabled, 0 | 1) || generation < 0 || updated_at_ms < 0 {
            return Err(dcc_core::CoreError::Repository(
                "provider availability record is invalid".to_string(),
            ));
        }
        Ok(Some(ProviderAvailabilityRecord {
            provider_id: provider_id.to_string(),
            enabled: enabled == 1,
            generation: generation as u64,
            updated_at_ms: updated_at_ms as u64,
        }))
    }

    /// Persists a complete, bounded availability record. Generation is owned
    /// by the runtime transition layer and may never move backwards.
    pub fn save_provider_availability(&self, record: &ProviderAvailabilityRecord) -> Result<()> {
        validate_provider_availability_provider_id(&record.provider_id)?;
        let generation = i64::try_from(record.generation).map_err(|_| {
            dcc_core::CoreError::InvalidInput(
                "provider availability generation is invalid".to_string(),
            )
        })?;
        let updated_at_ms = i64::try_from(record.updated_at_ms).map_err(|_| {
            dcc_core::CoreError::InvalidInput("provider availability time is invalid".to_string())
        })?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                r#"
            INSERT INTO dcc_provider_availability
                (provider_id, enabled, generation, updated_at_ms)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(provider_id) DO UPDATE SET
                enabled = excluded.enabled,
                generation = excluded.generation,
                updated_at_ms = excluded.updated_at_ms
            WHERE excluded.generation > dcc_provider_availability.generation
               OR (
                    excluded.generation = dcc_provider_availability.generation
                AND excluded.enabled = dcc_provider_availability.enabled
               )
            "#,
                params![
                    &record.provider_id,
                    if record.enabled { 1_i64 } else { 0_i64 },
                    generation,
                    updated_at_ms,
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        if changed != 1 {
            return Err(dcc_core::CoreError::Repository(
                "provider availability generation is stale".to_string(),
            ));
        }
        Ok(())
    }

    /// Stores a caller-sanitized browser URL for one workspace/session scope.
    /// The repository enforces a bounded lifetime but does not parse URLs;
    /// URL policy remains owned by the Browser command layer.
    pub fn save_browser_location(
        &self,
        workspace_id: &str,
        session_id: Option<&str>,
        safe_url: &str,
        saved_at_ms: i64,
        expires_at_ms: i64,
    ) -> Result<()> {
        let session_id = browser_location_session_id(session_id);
        validate_browser_location_scope(workspace_id, session_id)?;
        validate_browser_location_url(safe_url)?;
        if saved_at_ms < 0
            || expires_at_ms <= saved_at_ms
            || expires_at_ms
                .checked_sub(saved_at_ms)
                .is_none_or(|ttl| ttl > DEFAULT_BROWSER_LOCATION_TTL_MS)
        {
            return Err(dcc_core::CoreError::InvalidInput(
                "browser location expiry is invalid".to_string(),
            ));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            r#"
            INSERT INTO dcc_browser_locations
                (workspace_id, session_id, safe_url, saved_at_ms, expires_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(workspace_id, session_id) DO UPDATE SET
                safe_url = excluded.safe_url,
                saved_at_ms = excluded.saved_at_ms,
                expires_at_ms = excluded.expires_at_ms
            WHERE excluded.saved_at_ms >= dcc_browser_locations.saved_at_ms
            "#,
            params![
                workspace_id,
                session_id,
                safe_url,
                saved_at_ms,
                expires_at_ms
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    /// Loads a current URL and removes an expired row in the same critical
    /// section. Invalid legacy rows fail closed and are removed.
    pub fn load_browser_location(
        &self,
        workspace_id: &str,
        session_id: Option<&str>,
        now_ms: i64,
    ) -> Result<Option<String>> {
        let session_id = browser_location_session_id(session_id);
        validate_browser_location_scope(workspace_id, session_id)?;
        if now_ms < 0 {
            return Err(dcc_core::CoreError::InvalidInput(
                "browser location time is invalid".to_string(),
            ));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let row = conn
            .query_row(
                "SELECT safe_url, saved_at_ms, expires_at_ms FROM dcc_browser_locations WHERE workspace_id = ?1 AND session_id = ?2",
                params![workspace_id, session_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let Some((safe_url, saved_at_ms, expires_at_ms)) = row else {
            return Ok(None);
        };
        let valid = validate_browser_location_url(&safe_url).is_ok()
            && saved_at_ms >= 0
            && expires_at_ms > saved_at_ms
            && expires_at_ms
                .checked_sub(saved_at_ms)
                .is_some_and(|ttl| ttl <= DEFAULT_BROWSER_LOCATION_TTL_MS);
        if !valid || expires_at_ms <= now_ms {
            conn.execute(
                "DELETE FROM dcc_browser_locations WHERE workspace_id = ?1 AND session_id = ?2",
                params![workspace_id, session_id],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            return Ok(None);
        }
        Ok(Some(safe_url))
    }

    pub fn delete_browser_location(
        &self,
        workspace_id: &str,
        session_id: Option<&str>,
    ) -> Result<bool> {
        let session_id = browser_location_session_id(session_id);
        validate_browser_location_scope(workspace_id, session_id)?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                "DELETE FROM dcc_browser_locations WHERE workspace_id = ?1 AND session_id = ?2",
                params![workspace_id, session_id],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(changed > 0)
    }

    pub fn delete_browser_locations_for_session(&self, session_id: &str) -> Result<usize> {
        if session_id.is_empty() {
            return Err(dcc_core::CoreError::InvalidInput(
                "browser location session is invalid".to_string(),
            ));
        }
        validate_browser_location_scope("session-cleanup", session_id)?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            "DELETE FROM dcc_browser_locations WHERE session_id = ?1",
            params![session_id],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    pub fn delete_browser_locations_for_workspace(&self, workspace_id: &str) -> Result<usize> {
        validate_browser_location_scope(workspace_id, "")?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            "DELETE FROM dcc_browser_locations WHERE workspace_id = ?1",
            params![workspace_id],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }
}

#[async_trait]
impl SessionRepo for SqliteSessionRepo {
    async fn save_session(&self, session: &Session) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let provider_runtime_json = session
            .provider_runtime
            .as_ref()
            .map(|runtime| {
                to_string(runtime)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
            })
            .transpose()?;
        let additional_workspace_ids_json = to_string(&session.additional_workspace_ids)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        conn.execute(
            r#"
			INSERT INTO dcc_sessions (
				id, project_id, workspace_id, provider_id, model,
				provider_runtime_json, working_directory_override, additional_workspace_ids_json,
				state, created_at, updated_at
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
			ON CONFLICT(id) DO UPDATE SET
				project_id = excluded.project_id,
				workspace_id = excluded.workspace_id,
				provider_id = excluded.provider_id,
				model = excluded.model,
				provider_runtime_json = excluded.provider_runtime_json,
				working_directory_override = excluded.working_directory_override,
				additional_workspace_ids_json = excluded.additional_workspace_ids_json,
				state = excluded.state,
				created_at = excluded.created_at,
				updated_at = excluded.updated_at
			"#,
            params![
                session.id.0.clone(),
                session.project_id.0.clone(),
                session.workspace_id.0.clone(),
                session.provider_id.clone(),
                session.model.clone(),
                provider_runtime_json,
                session.working_directory_override.clone(),
                additional_workspace_ids_json,
                Self::session_state_as_str(&session.state),
                session.created_at.clone(),
                session.updated_at.clone(),
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Self::reindex_session_sync(&conn, &session.id)?;

        Ok(())
    }

    async fn get_session(&self, id: &SessionId) -> Result<Option<Session>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            r#"
			SELECT id, project_id, workspace_id, provider_id, model,
			       provider_runtime_json, working_directory_override,
			       additional_workspace_ids_json, state, created_at, updated_at
			  FROM dcc_sessions
			 WHERE id = ?1
			"#,
            params![id.0.clone()],
            Self::session_from_row,
        )
        .optional()
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    async fn delete_session(&self, id: &SessionId) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            "DELETE FROM dcc_browser_locations WHERE session_id = ?1",
            params![id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            "DELETE FROM dcc_sessions WHERE id = ?1",
            params![id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Self::delete_search_row_sync(&conn, id)?;
        Ok(())
    }
}

#[async_trait]
impl ThreadRepo for SqliteSessionRepo {
    async fn save_thread(&self, thread: &Thread) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            r#"
			INSERT INTO dcc_threads (id, project_id, session_id, title, archived_at)
			VALUES (?1, ?2, ?3, ?4, ?5)
			ON CONFLICT(id) DO UPDATE SET
				project_id = excluded.project_id,
				session_id = excluded.session_id,
				title = excluded.title,
				archived_at = excluded.archived_at
			"#,
            params![
                thread.id.0.clone(),
                thread.project_id.0.clone(),
                thread
                    .session_id
                    .as_ref()
                    .map(|session_id| session_id.0.clone()),
                thread.title.clone(),
                thread.archived_at.clone(),
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        if let Some(session_id) = thread.session_id.as_ref() {
            Self::reindex_session_sync(&conn, session_id)?;
        }
        Ok(())
    }

    async fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            r#"
			SELECT id, project_id, session_id, title, archived_at
			  FROM dcc_threads
			 WHERE id = ?1
			"#,
            params![id.0.clone()],
            Self::thread_from_row,
        )
        .optional()
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    async fn find_thread_by_session_id(&self, session_id: &SessionId) -> Result<Option<Thread>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            r#"
			SELECT id, project_id, session_id, title, archived_at
			  FROM dcc_threads
			 WHERE session_id = ?1
			"#,
            params![session_id.0.clone()],
            Self::thread_from_row,
        )
        .optional()
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    async fn delete_thread(&self, id: &ThreadId) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let session_id = conn
            .query_row(
                "SELECT session_id FROM dcc_threads WHERE id = ?1",
                params![id.0.clone()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
            .flatten();
        conn.execute(
            "DELETE FROM dcc_threads WHERE id = ?1",
            params![id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        if let Some(session_id) = session_id {
            Self::delete_search_row_sync(&conn, &SessionId(session_id))?;
        }
        Ok(())
    }
}

#[async_trait]
impl SessionEventRepo for SqliteSessionRepo {
    async fn append_event(&self, event: &SessionEventRecord) -> Result<AppendEventOutcome> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let kind_json = to_string(&event.kind)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let terminal = match &event.kind {
            SessionEventKind::TurnCompleted { turn_id } => Some((turn_id.0.as_str(), "completed")),
            SessionEventKind::TurnAborted { turn_id, .. } => Some((turn_id.0.as_str(), "aborted")),
            _ => None,
        };
        let transaction = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let existing_by_id = transaction
            .query_row(
                r#"
                SELECT event_id, session_id, sequence, kind_json, occurred_at,
                       terminal_turn_id, terminal_kind
                  FROM dcc_session_events
                 WHERE event_id = ?1
                "#,
                params![event.event_id.clone()],
                Self::event_with_metadata_from_row,
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        if let Some((existing, terminal_turn_id, terminal_kind)) = existing_by_id {
            Self::validate_event_metadata(
                &existing,
                terminal_turn_id.as_deref(),
                terminal_kind.as_deref(),
            )?;
            let existing_kind_json = to_string(&existing.kind)
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            if existing.session_id != event.session_id
                || existing.occurred_at != event.occurred_at
                || existing_kind_json != kind_json
            {
                return Err(dcc_core::CoreError::Repository(
                    "event identity conflicts with existing event".to_string(),
                ));
            }
            transaction
                .commit()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            return Ok(AppendEventOutcome::Existing(existing));
        }

        if let Some((terminal_turn_id, _terminal_kind)) = terminal {
            let existing_terminal = transaction
                .query_row(
                    r#"
                    SELECT event_id, session_id, sequence, kind_json, occurred_at,
                           terminal_turn_id, terminal_kind
                      FROM dcc_session_events
                     WHERE session_id = ?1 AND terminal_turn_id = ?2
                     ORDER BY sequence ASC
                     LIMIT 1
                    "#,
                    params![event.session_id.0.clone(), terminal_turn_id],
                    Self::event_with_metadata_from_row,
                )
                .optional()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            if let Some((existing, existing_turn_id, existing_kind)) = existing_terminal {
                Self::validate_event_metadata(
                    &existing,
                    existing_turn_id.as_deref(),
                    existing_kind.as_deref(),
                )?;
                transaction
                    .commit()
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                return Ok(AppendEventOutcome::Existing(existing));
            }
        }

        let next_sequence = transaction
            .query_row(
                "SELECT COALESCE(MAX(sequence), 0) + 1 FROM dcc_session_events WHERE session_id = ?1",
                params![event.session_id.0.clone()],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let sequence = u64::try_from(next_sequence).map_err(|_| {
            dcc_core::CoreError::Repository("session event sequence overflow".to_string())
        })?;
        let (terminal_turn_id, terminal_kind): (Option<&str>, Option<&str>) = terminal
            .map(|(turn_id, kind)| (Some(turn_id), Some(kind)))
            .unwrap_or((None, None));
        transaction
            .execute(
                r#"
			INSERT INTO dcc_session_events (
				event_id, session_id, sequence, occurred_at, kind_json,
                terminal_turn_id, terminal_kind
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
			"#,
                params![
                    event.event_id.clone(),
                    event.session_id.0.clone(),
                    i64::try_from(sequence).map_err(|_| {
                        dcc_core::CoreError::Repository(
                            "session event sequence overflow".to_string(),
                        )
                    })?,
                    event.occurred_at.clone(),
                    kind_json,
                    terminal_turn_id,
                    terminal_kind,
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Self::reindex_session_sync(&transaction, &event.session_id)?;
        transaction
            .commit()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut canonical = event.clone();
        canonical.sequence = sequence;
        Ok(AppendEventOutcome::Inserted(canonical))
    }

    async fn find_terminal_event(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
    ) -> Result<Option<SessionEventRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut statement = conn
            .prepare(
                r#"
                SELECT event_id, session_id, sequence, kind_json, occurred_at,
                       terminal_turn_id, terminal_kind
                  FROM dcc_session_events
                 WHERE session_id = ?1 AND terminal_turn_id = ?2
                 LIMIT 2
                "#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = statement
            .query_map(
                params![session_id.0.clone(), turn_id.0.clone()],
                Self::event_with_metadata_from_row,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut canonical = None;
        for row in rows {
            let (event, terminal_turn_id, terminal_kind) =
                row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            Self::validate_event_metadata(
                &event,
                terminal_turn_id.as_deref(),
                terminal_kind.as_deref(),
            )?;
            if event.session_id != *session_id {
                return Err(dcc_core::CoreError::Repository(
                    "terminal event session attribution is inconsistent".to_string(),
                ));
            }
            if canonical.replace(event).is_some() {
                return Err(dcc_core::CoreError::Repository(
                    "multiple terminal events exist for one turn".to_string(),
                ));
            }
        }
        Ok(canonical)
    }

    async fn list_events_by_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<SessionEventRecord>> {
        self.list_events_by_session_sync(session_id)
    }

    async fn list_events_by_session_limited(
        &self,
        session_id: &SessionId,
        limit: usize,
    ) -> Result<Vec<SessionEventRecord>> {
        self.list_events_by_session_limited_sync(session_id, limit)
    }

    async fn delete_events_by_session(&self, session_id: &SessionId) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            "DELETE FROM dcc_session_events WHERE session_id = ?1",
            params![session_id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Self::reindex_session_sync(&conn, session_id)?;
        Ok(())
    }
}

fn merge_earliest(slot: &mut Option<String>, candidate: Option<String>) {
    let Some(candidate) = candidate else {
        return;
    };
    if slot.as_ref().is_none_or(|current| candidate < *current) {
        *slot = Some(candidate);
    }
}

fn merge_latest(slot: &mut Option<String>, candidate: Option<String>) {
    let Some(candidate) = candidate else {
        return;
    };
    if slot.as_ref().is_none_or(|current| candidate > *current) {
        *slot = Some(candidate);
    }
}

#[async_trait]
impl UsageRepo for SqliteSessionRepo {
    async fn replace_turn_usage(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
        recorded_at: &str,
        models: &[ModelTokenUsage],
    ) -> Result<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let turn_model = conn
            .query_row(
                r#"
				SELECT json_extract(kind_json, '$.model')
				  FROM dcc_session_events
				 WHERE session_id = ?1
				   AND json_extract(kind_json, '$.turnId') = ?2
				   AND json_extract(kind_json, '$.type') IN ('turn_model_effective', 'turn_started')
				   AND json_extract(kind_json, '$.model') IS NOT NULL
				 ORDER BY sequence DESC
				 LIMIT 1
				"#,
                params![session_id.0.clone(), turn_id.0.clone()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
            .flatten();
        let fallback_model = turn_model.or(conn
            .query_row(
                "SELECT model FROM dcc_sessions WHERE id = ?1",
                params![session_id.0.clone()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
            .flatten());
        let transaction = conn
            .transaction()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        transaction
            .execute(
                "DELETE FROM dcc_turn_model_usage WHERE session_id = ?1 AND turn_id = ?2",
                params![session_id.0.clone(), turn_id.0.clone()],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        for usage in models {
            let model = usage
                .model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or_else(|| fallback_model.clone())
                .unwrap_or_else(|| "unknown".to_string());
            transaction
                .execute(
                    r#"
					INSERT INTO dcc_turn_model_usage (
						session_id, turn_id, model, input_tokens, output_tokens,
						cached_input_tokens, cache_write_input_tokens,
						reasoning_output_tokens, total_tokens, cost_usd, recorded_at
					) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
					"#,
                    params![
                        session_id.0.clone(),
                        turn_id.0.clone(),
                        model,
                        usage.input_tokens,
                        usage.output_tokens,
                        usage.cached_input_tokens,
                        usage.cache_write_input_tokens,
                        usage.reasoning_output_tokens,
                        usage.total_tokens,
                        usage.cost_usd,
                        recorded_at,
                    ],
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        }
        transaction
            .commit()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    async fn usage_dashboard(&self, input: &UsageDashboardInput) -> Result<UsageDashboard> {
        let now = Utc::now();
        let period_started_at = input.period_days.and_then(|days| {
            (days > 0).then(|| (now - Duration::days(i64::from(days.min(3_650)))).to_rfc3339())
        });
        let project_id = input
            .project_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut providers = BTreeMap::<String, ProviderUsageSummary>::new();

        {
            let mut statement = conn
                .prepare(
                    r#"
					SELECT provider_id, COUNT(*), MIN(created_at), MAX(updated_at)
					  FROM dcc_sessions
					 WHERE (?1 IS NULL OR created_at >= ?1)
					   AND (?2 IS NULL OR project_id = ?2)
					 GROUP BY provider_id
					"#,
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let rows = statement
                .query_map(params![period_started_at, project_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                })
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            for row in rows {
                let (provider_id, sessions, first, last) =
                    row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                providers.insert(
                    provider_id.clone(),
                    ProviderUsageSummary {
                        provider_id,
                        sessions,
                        first_used_at: first,
                        last_used_at: last,
                        ..ProviderUsageSummary::default()
                    },
                );
            }
        }

        {
            let mut statement = conn
                .prepare(
                    r#"
					SELECT s.provider_id, COUNT(*), MIN(e.occurred_at), MAX(e.occurred_at)
					  FROM dcc_session_events e
					  JOIN dcc_sessions s ON s.id = e.session_id
					 WHERE json_extract(e.kind_json, '$.type') = 'turn_completed'
					   AND (?1 IS NULL OR e.occurred_at >= ?1)
					   AND (?2 IS NULL OR s.project_id = ?2)
					 GROUP BY s.provider_id
					"#,
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let rows = statement
                .query_map(params![period_started_at, project_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                })
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            for row in rows {
                let (provider_id, turns, first, last) =
                    row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                let provider =
                    providers
                        .entry(provider_id.clone())
                        .or_insert_with(|| ProviderUsageSummary {
                            provider_id,
                            ..ProviderUsageSummary::default()
                        });
                provider.turns = turns;
                merge_earliest(&mut provider.first_used_at, first);
                merge_latest(&mut provider.last_used_at, last);
            }
        }

        {
            let mut statement = conn
                .prepare(
                    r#"
					SELECT s.provider_id,
					       COUNT(DISTINCT u.session_id || ':' || u.turn_id),
					       COALESCE(SUM(u.input_tokens), 0),
					       COALESCE(SUM(u.output_tokens), 0),
					       COALESCE(SUM(u.cached_input_tokens), 0),
					       COALESCE(SUM(u.cache_write_input_tokens), 0),
					       COALESCE(SUM(u.reasoning_output_tokens), 0),
					       COALESCE(SUM(u.total_tokens), 0),
					       SUM(u.cost_usd), MIN(u.recorded_at), MAX(u.recorded_at)
					  FROM dcc_turn_model_usage u
					  JOIN dcc_sessions s ON s.id = u.session_id
					 WHERE (?1 IS NULL OR u.recorded_at >= ?1)
					   AND (?2 IS NULL OR s.project_id = ?2)
					 GROUP BY s.provider_id
					"#,
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let rows = statement
                .query_map(params![period_started_at, project_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u64>(1)?,
                        row.get::<_, u64>(2)?,
                        row.get::<_, u64>(3)?,
                        row.get::<_, u64>(4)?,
                        row.get::<_, u64>(5)?,
                        row.get::<_, u64>(6)?,
                        row.get::<_, u64>(7)?,
                        row.get::<_, Option<f64>>(8)?,
                        row.get::<_, Option<String>>(9)?,
                        row.get::<_, Option<String>>(10)?,
                    ))
                })
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            for row in rows {
                let (
                    provider_id,
                    measured_turns,
                    input_tokens,
                    output_tokens,
                    cached_input_tokens,
                    cache_write_input_tokens,
                    reasoning_output_tokens,
                    total_tokens,
                    cost_usd,
                    first,
                    last,
                ) = row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                let provider =
                    providers
                        .entry(provider_id.clone())
                        .or_insert_with(|| ProviderUsageSummary {
                            provider_id,
                            ..ProviderUsageSummary::default()
                        });
                provider.measured_turns = measured_turns;
                provider.input_tokens = input_tokens;
                provider.output_tokens = output_tokens;
                provider.cached_input_tokens = cached_input_tokens;
                provider.cache_write_input_tokens = cache_write_input_tokens;
                provider.reasoning_output_tokens = reasoning_output_tokens;
                provider.total_tokens = total_tokens;
                provider.cost_usd = cost_usd;
                merge_earliest(&mut provider.first_used_at, first);
                merge_latest(&mut provider.last_used_at, last);
            }
        }

        let mut models = Vec::new();
        {
            let mut statement = conn
                .prepare(
                    r#"
					SELECT s.provider_id, u.model,
					       COUNT(DISTINCT u.session_id || ':' || u.turn_id),
					       COALESCE(SUM(u.input_tokens), 0),
					       COALESCE(SUM(u.output_tokens), 0),
					       COALESCE(SUM(u.cached_input_tokens), 0),
					       COALESCE(SUM(u.cache_write_input_tokens), 0),
					       COALESCE(SUM(u.reasoning_output_tokens), 0),
					       COALESCE(SUM(u.total_tokens), 0), SUM(u.cost_usd)
					  FROM dcc_turn_model_usage u
					  JOIN dcc_sessions s ON s.id = u.session_id
					 WHERE (?1 IS NULL OR u.recorded_at >= ?1)
					   AND (?2 IS NULL OR s.project_id = ?2)
					 GROUP BY s.provider_id, u.model
					"#,
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let rows = statement
                .query_map(params![period_started_at, project_id], |row| {
                    Ok(ModelUsageSummary {
                        provider_id: row.get(0)?,
                        model: row.get(1)?,
                        measured_turns: row.get(2)?,
                        input_tokens: row.get(3)?,
                        output_tokens: row.get(4)?,
                        cached_input_tokens: row.get(5)?,
                        cache_write_input_tokens: row.get(6)?,
                        reasoning_output_tokens: row.get(7)?,
                        total_tokens: row.get(8)?,
                        cost_usd: row.get(9)?,
                    })
                })
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            for row in rows {
                models
                    .push(row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?);
            }
        }
        models.sort_by(|left, right| {
            right
                .total_tokens
                .cmp(&left.total_tokens)
                .then_with(|| right.measured_turns.cmp(&left.measured_turns))
        });

        let mut daily = BTreeMap::<(String, String), DailyUsageSummary>::new();
        {
            let mut statement = conn
                .prepare(
                    r#"
					SELECT substr(e.occurred_at, 1, 10), s.provider_id, COUNT(*)
					  FROM dcc_session_events e
					  JOIN dcc_sessions s ON s.id = e.session_id
					 WHERE json_extract(e.kind_json, '$.type') = 'turn_completed'
					   AND (?1 IS NULL OR e.occurred_at >= ?1)
					   AND (?2 IS NULL OR s.project_id = ?2)
					 GROUP BY substr(e.occurred_at, 1, 10), s.provider_id
					"#,
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let rows = statement
                .query_map(params![period_started_at, project_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u64>(2)?,
                    ))
                })
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            for row in rows {
                let (date, provider_id, turns) =
                    row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                daily.insert(
                    (date.clone(), provider_id.clone()),
                    DailyUsageSummary {
                        date,
                        provider_id,
                        turns,
                        ..DailyUsageSummary::default()
                    },
                );
            }
        }
        {
            let mut statement = conn
                .prepare(
                    r#"
					SELECT substr(u.recorded_at, 1, 10), s.provider_id,
					       COUNT(DISTINCT u.session_id || ':' || u.turn_id),
					       COALESCE(SUM(u.total_tokens), 0)
					  FROM dcc_turn_model_usage u
					  JOIN dcc_sessions s ON s.id = u.session_id
					 WHERE (?1 IS NULL OR u.recorded_at >= ?1)
					   AND (?2 IS NULL OR s.project_id = ?2)
					 GROUP BY substr(u.recorded_at, 1, 10), s.provider_id
					"#,
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let rows = statement
                .query_map(params![period_started_at, project_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u64>(2)?,
                        row.get::<_, u64>(3)?,
                    ))
                })
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            for row in rows {
                let (date, provider_id, measured_turns, total_tokens) =
                    row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                let entry = daily
                    .entry((date.clone(), provider_id.clone()))
                    .or_insert_with(|| DailyUsageSummary {
                        date,
                        provider_id,
                        ..DailyUsageSummary::default()
                    });
                entry.measured_turns = measured_turns;
                entry.total_tokens = total_tokens;
            }
        }

        let mut providers = providers.into_values().collect::<Vec<_>>();
        providers.sort_by(|left, right| {
            right
                .total_tokens
                .cmp(&left.total_tokens)
                .then_with(|| right.turns.cmp(&left.turns))
                .then_with(|| right.sessions.cmp(&left.sessions))
        });
        let any_cost = providers.iter().any(|provider| provider.cost_usd.is_some());
        let totals = UsageTotals {
            sessions: providers.iter().map(|provider| provider.sessions).sum(),
            turns: providers.iter().map(|provider| provider.turns).sum(),
            measured_turns: providers
                .iter()
                .map(|provider| provider.measured_turns)
                .sum(),
            input_tokens: providers.iter().map(|provider| provider.input_tokens).sum(),
            output_tokens: providers
                .iter()
                .map(|provider| provider.output_tokens)
                .sum(),
            cached_input_tokens: providers
                .iter()
                .map(|provider| provider.cached_input_tokens)
                .sum(),
            cache_write_input_tokens: providers
                .iter()
                .map(|provider| provider.cache_write_input_tokens)
                .sum(),
            reasoning_output_tokens: providers
                .iter()
                .map(|provider| provider.reasoning_output_tokens)
                .sum(),
            total_tokens: providers.iter().map(|provider| provider.total_tokens).sum(),
            cost_usd: any_cost.then(|| {
                providers
                    .iter()
                    .filter_map(|provider| provider.cost_usd)
                    .sum()
            }),
        };

        Ok(UsageDashboard {
            generated_at: now.to_rfc3339(),
            period_started_at,
            totals,
            providers,
            models,
            daily: daily.into_values().collect(),
        })
    }
}

#[async_trait]
impl DelegationRepo for SqliteSessionRepo {
    async fn save_delegation(&self, delegation: &Delegation) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let context_policy_json = to_string(&delegation.context_policy)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let budget_json = to_string(&delegation.budget)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let touched_files_json = to_string(&delegation.touched_files)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        conn.execute(
            r#"
			INSERT INTO dcc_delegations (
				id, parent_session_id, parent_turn_id, child_session_id, workspace_id,
				target_provider_id, target_model_id, mode, status, prompt, context_policy_json, budget_json,
				result_summary, touched_files_json, diff_summary, validation_summary, created_at, updated_at
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
			ON CONFLICT(id) DO UPDATE SET
				parent_session_id = excluded.parent_session_id,
				parent_turn_id = excluded.parent_turn_id,
				child_session_id = excluded.child_session_id,
				workspace_id = excluded.workspace_id,
				target_provider_id = excluded.target_provider_id,
				target_model_id = excluded.target_model_id,
				mode = excluded.mode,
				status = excluded.status,
				prompt = excluded.prompt,
				context_policy_json = excluded.context_policy_json,
				budget_json = excluded.budget_json,
				result_summary = excluded.result_summary,
				touched_files_json = excluded.touched_files_json,
				diff_summary = excluded.diff_summary,
				validation_summary = excluded.validation_summary,
				created_at = excluded.created_at,
				updated_at = excluded.updated_at
			"#,
            params![
                delegation.id.0.clone(),
                delegation.parent_session_id.0.clone(),
                delegation
                    .parent_turn_id
                    .as_ref()
                    .map(|turn_id| turn_id.0.clone()),
                delegation
                    .child_session_id
                    .as_ref()
                    .map(|session_id| session_id.0.clone()),
                delegation.workspace_id.0.clone(),
                delegation.target_provider_id.0.clone(),
                delegation.target_model_id.clone(),
                Self::delegation_mode_as_str(&delegation.mode),
                Self::delegation_status_as_str(&delegation.status),
                delegation.prompt.clone(),
                context_policy_json,
                budget_json,
                delegation.result_summary.clone(),
                touched_files_json,
                delegation.diff_summary.clone(),
                delegation.validation_summary.clone(),
                delegation.created_at.clone(),
                delegation.updated_at.clone(),
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    async fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            r#"
			SELECT id, parent_session_id, parent_turn_id, child_session_id, workspace_id,
			       target_provider_id, target_model_id, mode, status, prompt, context_policy_json, budget_json,
			       result_summary, touched_files_json, diff_summary, validation_summary, created_at, updated_at
			  FROM dcc_delegations
			 WHERE id = ?1
			"#,
            params![id.0.clone()],
            Self::delegation_from_row,
        )
        .optional()
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    async fn list_delegations(
        &self,
        workspace_id: Option<&WorkspaceId>,
        parent_session_id: Option<&SessionId>,
    ) -> Result<Vec<Delegation>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let base_sql = r#"
			SELECT id, parent_session_id, parent_turn_id, child_session_id, workspace_id,
			       target_provider_id, target_model_id, mode, status, prompt, context_policy_json, budget_json,
			       result_summary, touched_files_json, diff_summary, validation_summary, created_at, updated_at
			  FROM dcc_delegations
		"#;
        let order_sql = " ORDER BY updated_at DESC, created_at DESC";
        let rows = match (workspace_id, parent_session_id) {
            (Some(workspace_id), Some(parent_session_id)) => {
                let sql = format!(
                    "{base_sql} WHERE workspace_id = ?1 AND parent_session_id = ?2{order_sql}"
                );
                let mut stmt = conn
                    .prepare(&sql)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                let rows = stmt
                    .query_map(
                        params![workspace_id.0.clone(), parent_session_id.0.clone()],
                        Self::delegation_from_row,
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
                    .collect::<rusqlite::Result<Vec<_>>>();
                rows
            }
            (Some(workspace_id), None) => {
                let sql = format!("{base_sql} WHERE workspace_id = ?1{order_sql}");
                let mut stmt = conn
                    .prepare(&sql)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                let rows = stmt
                    .query_map(params![workspace_id.0.clone()], Self::delegation_from_row)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
                    .collect::<rusqlite::Result<Vec<_>>>();
                rows
            }
            (None, Some(parent_session_id)) => {
                let sql = format!("{base_sql} WHERE parent_session_id = ?1{order_sql}");
                let mut stmt = conn
                    .prepare(&sql)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                let rows = stmt
                    .query_map(
                        params![parent_session_id.0.clone()],
                        Self::delegation_from_row,
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
                    .collect::<rusqlite::Result<Vec<_>>>();
                rows
            }
            (None, None) => {
                let sql = format!("{base_sql}{order_sql}");
                let mut stmt = conn
                    .prepare(&sql)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                let rows = stmt
                    .query_map([], Self::delegation_from_row)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
                    .collect::<rusqlite::Result<Vec<_>>>();
                rows
            }
        }
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(rows)
    }

    async fn update_delegation_status(
        &self,
        id: &DelegationId,
        status: DelegationStatus,
        updated_at: String,
    ) -> Result<Option<Delegation>> {
        {
            let conn = self
                .conn
                .lock()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            conn.execute(
                "UPDATE dcc_delegations SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![
                    Self::delegation_status_as_str(&status),
                    updated_at,
                    id.0.clone()
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        }
        self.get_delegation(id).await
    }
}

#[async_trait]
impl DelegationWorktreeOperationRepo for SqliteSessionRepo {
    async fn create_delegation_worktree_operation(
        &self,
        operation: &DelegationWorktreeOperation,
    ) -> Result<()> {
        operation
            .validate()
            .map_err(dcc_core::CoreError::InvalidInput)?;
        if operation.state != DelegationWorktreeOperationState::Preparing {
            return Err(dcc_core::CoreError::InvalidInput(
                "delegation worktree journal must be created in preparing state".to_string(),
            ));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            r#"
            INSERT INTO dcc_delegation_worktree_operations (
                operation_id, delegation_key, delegation_id, workspace_id,
                parent_session_id, child_session_id, source_root, worktree_path,
                branch, base_commit, expected_branch_oid, source_root_id,
                worktree_root_id, common_dir_id, state, last_error,
                recovery_owner, recovery_lease_until, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
                ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20
            )
            "#,
            params![
                operation.operation_id.0,
                operation.delegation_key,
                operation.delegation_id.as_ref().map(|id| id.0.as_str()),
                operation.workspace_id.0,
                operation.parent_session_id.as_ref().map(|id| id.0.as_str()),
                operation.child_session_id.as_ref().map(|id| id.0.as_str()),
                operation.source_root,
                operation.worktree_path,
                operation.branch,
                operation.base_commit,
                operation.expected_branch_oid,
                operation.source_root_id.as_ref().map(|id| id.0.as_slice()),
                operation
                    .worktree_root_id
                    .as_ref()
                    .map(|id| id.0.as_slice()),
                operation.common_dir_id.as_ref().map(|id| id.0.as_slice()),
                Self::delegation_worktree_state_as_str(&operation.state),
                operation.last_error,
                operation.recovery_owner,
                operation.recovery_lease_until,
                operation.created_at,
                operation.updated_at,
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    async fn get_delegation_worktree_operation(
        &self,
        id: &DelegationWorktreeOperationId,
    ) -> Result<Option<DelegationWorktreeOperation>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            &format!("{DELEGATION_WORKTREE_OPERATION_SELECT} WHERE operation_id = ?1"),
            params![id.0],
            Self::delegation_worktree_operation_from_row,
        )
        .optional()
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    async fn get_delegation_worktree_operation_by_delegation_id(
        &self,
        delegation_id: &DelegationId,
    ) -> Result<Option<DelegationWorktreeOperation>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            &format!("{DELEGATION_WORKTREE_OPERATION_SELECT} WHERE delegation_id = ?1"),
            params![delegation_id.0],
            Self::delegation_worktree_operation_from_row,
        )
        .optional()
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    async fn list_delegation_worktree_operations_by_workspace(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<Vec<DelegationWorktreeOperation>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut statement = conn
            .prepare(&format!(
                "{DELEGATION_WORKTREE_OPERATION_SELECT} \
                 WHERE workspace_id = ?1 ORDER BY created_at ASC, operation_id ASC"
            ))
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let operations = statement
            .query_map(
                params![workspace_id.0],
                Self::delegation_worktree_operation_from_row,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(operations)
    }

    async fn compare_and_swap_delegation_worktree_operation(
        &self,
        expected_state: DelegationWorktreeOperationState,
        operation: &DelegationWorktreeOperation,
    ) -> Result<bool> {
        operation
            .validate()
            .map_err(dcc_core::CoreError::InvalidInput)?;
        if expected_state == DelegationWorktreeOperationState::Removing
            || operation.state == DelegationWorktreeOperationState::Removing
        {
            return Err(dcc_core::CoreError::InvalidInput(
                "removing transitions require the recovery claim API".to_string(),
            ));
        }
        if operation.state == DelegationWorktreeOperationState::Applying
            || (expected_state == DelegationWorktreeOperationState::Applying
                && operation.state != DelegationWorktreeOperationState::CleanupRequired)
        {
            return Err(dcc_core::CoreError::InvalidInput(
                "applying transitions require the delegation apply transaction API".to_string(),
            ));
        }
        if !expected_state.can_transition_to(&operation.state) {
            return Err(dcc_core::CoreError::InvalidInput(format!(
                "invalid delegation worktree transition from {} to {}",
                Self::delegation_worktree_state_as_str(&expected_state),
                Self::delegation_worktree_state_as_str(&operation.state),
            )));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                r#"
                UPDATE dcc_delegation_worktree_operations
                   SET delegation_key = ?1,
                       delegation_id = ?2,
                       parent_session_id = ?3,
                       child_session_id = ?4,
                       expected_branch_oid = ?5,
                       source_root_id = ?6,
                       worktree_root_id = ?7,
                       common_dir_id = ?8,
                       state = ?9,
                       last_error = ?10,
                       updated_at = ?11
                 WHERE operation_id = ?12
                   AND state = ?13
                   AND workspace_id = ?14
                   AND source_root = ?15
                   AND worktree_path = ?16
                   AND branch = ?17
                   AND base_commit = ?18
                   AND created_at = ?19
                "#,
                params![
                    operation.delegation_key,
                    operation.delegation_id.as_ref().map(|id| id.0.as_str()),
                    operation.parent_session_id.as_ref().map(|id| id.0.as_str()),
                    operation.child_session_id.as_ref().map(|id| id.0.as_str()),
                    operation.expected_branch_oid,
                    operation.source_root_id.as_ref().map(|id| id.0.as_slice()),
                    operation
                        .worktree_root_id
                        .as_ref()
                        .map(|id| id.0.as_slice()),
                    operation.common_dir_id.as_ref().map(|id| id.0.as_slice()),
                    Self::delegation_worktree_state_as_str(&operation.state),
                    operation.last_error,
                    operation.updated_at,
                    operation.operation_id.0,
                    Self::delegation_worktree_state_as_str(&expected_state),
                    operation.workspace_id.0,
                    operation.source_root,
                    operation.worktree_path,
                    operation.branch,
                    operation.base_commit,
                    operation.created_at,
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(changed == 1)
    }

    async fn list_delegation_worktree_operations_requiring_recovery(
        &self,
    ) -> Result<Vec<DelegationWorktreeOperation>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut statement = conn
            .prepare(&format!(
                "{DELEGATION_WORKTREE_OPERATION_SELECT} \
                 WHERE state <> 'removed' ORDER BY updated_at ASC, created_at ASC, operation_id ASC"
            ))
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let operations = statement
            .query_map([], Self::delegation_worktree_operation_from_row)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(operations)
    }

    async fn claim_delegation_worktree_removal(
        &self,
        id: &DelegationWorktreeOperationId,
        recovery_owner: &str,
        now: &str,
        lease_until: &str,
    ) -> Result<Option<DelegationWorktreeOperation>> {
        let recovery_owner = recovery_owner.trim();
        if recovery_owner.is_empty() || recovery_owner.len() > 256 {
            return Err(dcc_core::CoreError::InvalidInput(
                "delegation worktree recovery owner is invalid".to_string(),
            ));
        }
        let now = Self::normalize_delegation_recovery_timestamp(now, "recovery claim time")?;
        let lease_until =
            Self::normalize_delegation_recovery_timestamp(lease_until, "recovery lease")?;
        if lease_until <= now {
            return Err(dcc_core::CoreError::InvalidInput(
                "delegation worktree recovery lease must end after claim time".to_string(),
            ));
        }

        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = transaction
            .execute(
                r#"
                UPDATE dcc_delegation_worktree_operations
                   SET state = 'removing',
                       recovery_owner = ?1,
                       recovery_lease_until = ?2,
                       updated_at = ?3
                 WHERE operation_id = ?4
                   AND (
                       state IN (
                           'prepared', 'bound', 'review_pending', 'applied',
                           'cleanup_required'
                       )
                       OR (
                           state = 'removing'
                           AND recovery_lease_until IS NOT NULL
                           AND recovery_lease_until <= ?3
                       )
                   )
                "#,
                params![recovery_owner, lease_until, now, id.0],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let operation = if changed == 1 {
            Some(
                transaction
                    .query_row(
                        &format!("{DELEGATION_WORKTREE_OPERATION_SELECT} WHERE operation_id = ?1"),
                        params![id.0],
                        Self::delegation_worktree_operation_from_row,
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?,
            )
        } else {
            None
        };
        transaction
            .commit()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(operation)
    }

    async fn finalize_delegation_worktree_removal(
        &self,
        id: &DelegationWorktreeOperationId,
        recovery_owner: &str,
        final_state: DelegationWorktreeOperationState,
        last_error: Option<String>,
        updated_at: &str,
    ) -> Result<Option<DelegationWorktreeOperation>> {
        let recovery_owner = recovery_owner.trim();
        if recovery_owner.is_empty() || recovery_owner.len() > 256 {
            return Err(dcc_core::CoreError::InvalidInput(
                "delegation worktree recovery owner is invalid".to_string(),
            ));
        }
        match (&final_state, last_error.as_deref()) {
            (DelegationWorktreeOperationState::Removed, None) => {}
            (DelegationWorktreeOperationState::CleanupRequired, Some(error))
                if !error.trim().is_empty() => {}
            (DelegationWorktreeOperationState::Removed, Some(_)) => {
                return Err(dcc_core::CoreError::InvalidInput(
                    "removed delegation worktree cannot retain a recovery error".to_string(),
                ));
            }
            (DelegationWorktreeOperationState::CleanupRequired, _) => {
                return Err(dcc_core::CoreError::InvalidInput(
                    "cleanup-required delegation worktree must retain its recovery error"
                        .to_string(),
                ));
            }
            _ => {
                return Err(dcc_core::CoreError::InvalidInput(
                    "removal finalization must select removed or cleanup_required".to_string(),
                ));
            }
        }
        let updated_at =
            Self::normalize_delegation_recovery_timestamp(updated_at, "recovery update time")?;
        let final_state_value = Self::delegation_worktree_state_as_str(&final_state);

        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = transaction
            .execute(
                r#"
                UPDATE dcc_delegation_worktree_operations
                   SET state = ?1,
                       last_error = ?2,
                       recovery_owner = NULL,
                       recovery_lease_until = NULL,
                       updated_at = ?3
                 WHERE operation_id = ?4
                   AND state = 'removing'
                   AND recovery_owner = ?5
                "#,
                params![
                    final_state_value,
                    last_error,
                    updated_at,
                    id.0,
                    recovery_owner,
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let operation = if changed == 1 {
            Some(
                transaction
                    .query_row(
                        &format!("{DELEGATION_WORKTREE_OPERATION_SELECT} WHERE operation_id = ?1"),
                        params![id.0],
                        Self::delegation_worktree_operation_from_row,
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?,
            )
        } else {
            None
        };
        transaction
            .commit()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(operation)
    }

    async fn delete_removed_delegation_worktree_operation(
        &self,
        id: &DelegationWorktreeOperationId,
    ) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                "DELETE FROM dcc_delegation_worktree_operations \
                 WHERE operation_id = ?1 AND state = 'removed'",
                params![id.0],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(changed == 1)
    }
}

#[async_trait]
impl DelegationApplyTransactionRepo for SqliteSessionRepo {
    async fn create_delegation_apply_transaction(
        &self,
        apply: &DelegationApplyTransaction,
    ) -> Result<()> {
        apply
            .validate()
            .map_err(dcc_core::CoreError::InvalidInput)?;
        if apply.state != DelegationApplyTransactionState::Preparing {
            return Err(dcc_core::CoreError::InvalidInput(
                "delegation apply transaction must be created in preparing state".to_string(),
            ));
        }
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let scope_matches = transaction
            .query_row(
                r#"SELECT COUNT(*) FROM dcc_delegation_worktree_operations
                    WHERE operation_id = ?1 AND delegation_id = ?2 AND workspace_id = ?3
                      AND state = 'review_pending'"#,
                params![
                    apply.operation_id.0,
                    apply.delegation_id.0,
                    apply.workspace_id.0
                ],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
            == 1;
        if !scope_matches {
            return Err(dcc_core::CoreError::InvalidInput(
                "delegation apply transaction scope is not review-pending".to_string(),
            ));
        }
        transaction
            .execute(
                r#"INSERT INTO dcc_delegation_apply_transactions (
                       transaction_id, operation_id, delegation_id, workspace_id,
                       source_head_oid, destination_head_oid, destination_ref,
                       destination_index_tree_oid, manifest_digest, file_count,
                       artifact_bytes, state, recovery_owner, recovery_lease_until,
                       last_error, created_at, updated_at
                   ) VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, NULL, NULL, 0, 0,
                             'preparing', NULL, NULL, NULL, ?5, ?6)"#,
                params![
                    apply.transaction_id.0,
                    apply.operation_id.0,
                    apply.delegation_id.0,
                    apply.workspace_id.0,
                    apply.created_at,
                    apply.updated_at,
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        transaction
            .commit()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    async fn get_delegation_apply_transaction(
        &self,
        id: &DelegationApplyTransactionId,
    ) -> Result<Option<DelegationApplyTransaction>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            &format!("{DELEGATION_APPLY_TRANSACTION_SELECT} WHERE transaction_id = ?1"),
            params![id.0],
            Self::delegation_apply_transaction_from_row,
        )
        .optional()
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    async fn get_delegation_apply_transaction_by_operation_id(
        &self,
        operation_id: &DelegationWorktreeOperationId,
    ) -> Result<Option<DelegationApplyTransaction>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            &format!(
                "{DELEGATION_APPLY_TRANSACTION_SELECT} WHERE operation_id = ?1 \
                 ORDER BY CASE WHEN state IN ('preparing','prepared','applying','recovery_required') \
                               THEN 0 ELSE 1 END, created_at DESC, transaction_id DESC LIMIT 1"
            ),
            params![operation_id.0],
            Self::delegation_apply_transaction_from_row,
        )
        .optional()
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    async fn compare_and_swap_delegation_apply_transaction(
        &self,
        expected_state: DelegationApplyTransactionState,
        apply: &DelegationApplyTransaction,
    ) -> Result<bool> {
        apply
            .validate()
            .map_err(dcc_core::CoreError::InvalidInput)?;
        if !expected_state.can_pre_apply_transition_to(&apply.state) {
            return Err(dcc_core::CoreError::InvalidInput(
                "invalid pre-apply transaction transition".to_string(),
            ));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                r#"UPDATE dcc_delegation_apply_transactions
                      SET source_head_oid = ?1, destination_head_oid = ?2,
                          destination_ref = ?3, destination_index_tree_oid = ?4,
                          manifest_digest = ?5, file_count = ?6, artifact_bytes = ?7,
                          state = ?8, last_error = ?9, updated_at = ?10
                    WHERE transaction_id = ?11 AND state = ?12
                      AND operation_id = ?13 AND delegation_id = ?14
                      AND workspace_id = ?15 AND created_at = ?16
                      AND recovery_owner IS NULL AND recovery_lease_until IS NULL"#,
                params![
                    apply.source_head_oid,
                    apply.destination_head_oid,
                    apply.destination_ref,
                    apply.destination_index_tree_oid,
                    apply.manifest_digest,
                    i64::from(apply.file_count),
                    i64::try_from(apply.artifact_bytes).map_err(|_| {
                        dcc_core::CoreError::InvalidInput(
                            "delegation apply artifact_bytes exceeds SQLite".to_string(),
                        )
                    })?,
                    Self::delegation_apply_state_as_str(&apply.state),
                    apply.last_error,
                    apply.updated_at,
                    apply.transaction_id.0,
                    Self::delegation_apply_state_as_str(&expected_state),
                    apply.operation_id.0,
                    apply.delegation_id.0,
                    apply.workspace_id.0,
                    apply.created_at,
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(changed == 1)
    }

    async fn claim_delegation_apply_transaction(
        &self,
        id: &DelegationApplyTransactionId,
        recovery_owner: &str,
        now: &str,
        lease_until: &str,
        operation_lock_held: bool,
    ) -> Result<Option<DelegationApplyTransaction>> {
        let recovery_owner = recovery_owner.trim();
        if recovery_owner.is_empty() || recovery_owner.len() > 256 {
            return Err(dcc_core::CoreError::InvalidInput(
                "delegation apply recovery owner is invalid".to_string(),
            ));
        }
        let now = Self::normalize_delegation_recovery_timestamp(now, "apply claim time")?;
        let lease_until =
            Self::normalize_delegation_recovery_timestamp(lease_until, "apply lease")?;
        if lease_until <= now {
            return Err(dcc_core::CoreError::InvalidInput(
                "delegation apply recovery lease must end after claim time".to_string(),
            ));
        }
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let current = transaction
            .query_row(
                &format!("{DELEGATION_APPLY_TRANSACTION_SELECT} WHERE transaction_id = ?1"),
                params![id.0],
                Self::delegation_apply_transaction_from_row,
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let Some(current) = current else {
            transaction
                .commit()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            return Ok(None);
        };
        let claimed = match current.state {
            DelegationApplyTransactionState::Prepared => {
                let operation_changed = transaction
                    .execute(
                        r#"UPDATE dcc_delegation_worktree_operations
                              SET state = 'applying', last_error = NULL, updated_at = ?1
                            WHERE operation_id = ?2 AND delegation_id = ?3
                              AND workspace_id = ?4 AND state = 'review_pending'"#,
                        params![
                            now,
                            current.operation_id.0,
                            current.delegation_id.0,
                            current.workspace_id.0,
                        ],
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                if operation_changed != 1 {
                    false
                } else {
                    transaction
                        .execute(
                            r#"UPDATE dcc_delegation_apply_transactions
                                  SET state = 'applying', recovery_owner = ?1,
                                      recovery_lease_until = ?2, updated_at = ?3
                                WHERE transaction_id = ?4 AND state = 'prepared'"#,
                            params![recovery_owner, lease_until, now, id.0],
                        )
                        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
                        == 1
                }
            }
            DelegationApplyTransactionState::Applying
                if operation_lock_held
                    || current
                        .recovery_lease_until
                        .as_deref()
                        .is_some_and(|lease| lease <= now.as_str()) =>
            {
                transaction
                    .execute(
                        r#"UPDATE dcc_delegation_apply_transactions
                              SET recovery_owner = ?1, recovery_lease_until = ?2, updated_at = ?3
                            WHERE transaction_id = ?4 AND state = 'applying'
                              AND recovery_lease_until IS NOT NULL
                              AND (?5 = 1 OR recovery_lease_until <= ?3)
                              AND EXISTS (
                                  SELECT 1 FROM dcc_delegation_worktree_operations op
                                   WHERE op.operation_id = dcc_delegation_apply_transactions.operation_id
                                     AND op.state = 'applying'
                              )"#,
                        params![recovery_owner, lease_until, now, id.0, operation_lock_held],
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
                    == 1
            }
            _ => false,
        };
        if !claimed {
            // Any earlier operation update is part of this transaction and is
            // therefore rolled back with the failed claim.
            return Ok(None);
        }
        let claimed = transaction
            .query_row(
                &format!("{DELEGATION_APPLY_TRANSACTION_SELECT} WHERE transaction_id = ?1"),
                params![id.0],
                Self::delegation_apply_transaction_from_row,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        transaction
            .commit()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(Some(claimed))
    }

    async fn finalize_delegation_apply_transaction(
        &self,
        id: &DelegationApplyTransactionId,
        recovery_owner: &str,
        final_state: DelegationApplyTransactionState,
        last_error: Option<String>,
        updated_at: &str,
    ) -> Result<Option<DelegationApplyTransaction>> {
        let recovery_owner = recovery_owner.trim();
        if recovery_owner.is_empty() || recovery_owner.len() > 256 {
            return Err(dcc_core::CoreError::InvalidInput(
                "delegation apply recovery owner is invalid".to_string(),
            ));
        }
        match (&final_state, last_error.as_deref()) {
            (DelegationApplyTransactionState::Applied, None) => {}
            (DelegationApplyTransactionState::RolledBack, _) => {}
            (DelegationApplyTransactionState::RecoveryRequired, Some(error))
                if !error.trim().is_empty() => {}
            _ => {
                return Err(dcc_core::CoreError::InvalidInput(
                    "invalid delegation apply finalization state or error".to_string(),
                ))
            }
        }
        let updated_at =
            Self::normalize_delegation_recovery_timestamp(updated_at, "apply update time")?;
        let operation_state = match final_state {
            DelegationApplyTransactionState::Applied => "applied",
            DelegationApplyTransactionState::RolledBack => "review_pending",
            DelegationApplyTransactionState::RecoveryRequired => "cleanup_required",
            _ => unreachable!(),
        };
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transaction = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let current = transaction
            .query_row(
                &format!("{DELEGATION_APPLY_TRANSACTION_SELECT} WHERE transaction_id = ?1"),
                params![id.0],
                Self::delegation_apply_transaction_from_row,
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let Some(mut current) = current else {
            return Ok(None);
        };
        if current.state != DelegationApplyTransactionState::Applying
            || current.recovery_owner.as_deref() != Some(recovery_owner)
        {
            return Ok(None);
        }
        let operation_error = if final_state == DelegationApplyTransactionState::RecoveryRequired {
            last_error.as_deref()
        } else {
            None
        };
        let operation_changed = transaction
            .execute(
                r#"UPDATE dcc_delegation_worktree_operations
                      SET state = ?1, last_error = ?2, updated_at = ?3
                    WHERE operation_id = ?4 AND state = 'applying'"#,
                params![
                    operation_state,
                    operation_error,
                    updated_at,
                    current.operation_id.0
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        if operation_changed != 1 {
            return Ok(None);
        }
        let changed = transaction
            .execute(
                r#"UPDATE dcc_delegation_apply_transactions
                      SET state = ?1, recovery_owner = NULL, recovery_lease_until = NULL,
                          last_error = ?2, updated_at = ?3
                    WHERE transaction_id = ?4 AND state = 'applying'
                      AND recovery_owner = ?5"#,
                params![
                    Self::delegation_apply_state_as_str(&final_state),
                    last_error,
                    updated_at,
                    id.0,
                    recovery_owner,
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        if changed != 1 {
            return Ok(None);
        }
        current.state = final_state;
        current.recovery_owner = None;
        current.recovery_lease_until = None;
        current.last_error = last_error;
        current.updated_at = updated_at;
        current
            .validate()
            .map_err(dcc_core::CoreError::InvalidInput)?;
        transaction
            .commit()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(Some(current))
    }

    async fn list_delegation_apply_transactions_requiring_recovery(
        &self,
    ) -> Result<Vec<DelegationApplyTransaction>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut statement = conn
            .prepare(&format!(
                "{DELEGATION_APPLY_TRANSACTION_SELECT} \
                 ORDER BY CASE WHEN state IN ('preparing','prepared','applying','recovery_required') \
                                   THEN 0 ELSE 1 END, \
                          updated_at ASC, created_at ASC, transaction_id ASC"
            ))
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let transactions = statement
            .query_map([], Self::delegation_apply_transaction_from_row)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(transactions)
    }

    async fn delete_terminal_delegation_apply_transaction(
        &self,
        id: &DelegationApplyTransactionId,
    ) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                "DELETE FROM dcc_delegation_apply_transactions \
                 WHERE transaction_id = ?1 AND state IN ('applied','rolled_back')",
                params![id.0],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(changed == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::{
        domain::{
            delegation::{
                Delegation, DelegationBudget, DelegationContextPolicy, DelegationId,
                DelegationMode, DelegationStatus,
            },
            delegation_apply::{
                DelegationApplyTransaction, DelegationApplyTransactionId,
                DelegationApplyTransactionState,
            },
            delegation_worktree::{
                DelegationWorktreeOperation, DelegationWorktreeOperationId,
                DelegationWorktreeOperationState,
            },
            guarded_undo::{
                canonical_restore_manifest_digest, ArtifactKey, CheckoutRefV1,
                GuardedUndoReasonCode, IndexIdentityV1, OpaqueRepoPath, PhysicalRootId,
                RestoreFileStatus, Sha256Digest, RESTORE_CAPTURE_VERSION,
                UNDO_JOURNAL_SCHEMA_VERSION,
            },
            provider::ProviderId,
            repository::{Repository, RepositoryId},
            session::{SessionEventKind, SessionState, TurnId},
            usage::{ModelTokenUsage, UsageDashboardInput},
            workspace::{
                WorkspaceId, WorkspacePushTarget, WorkspaceSetupReport, WorkspaceSetupStatus,
                WorkspaceSetupStepReport, WorkspaceSource, WorkspaceSourceKind, WorkspaceState,
            },
            workspace_bundle::{
                WorkspaceBundle, WorkspaceBundleId, WorkspaceBundleMember, WorkspaceBundleState,
            },
        },
        ports::{
            DelegationApplyTransactionRepo, DelegationRepo, DelegationWorktreeOperationRepo,
            RepositoryRepo, SessionEventRepo, SessionRepo, ThreadRepo, UsageRepo,
            WorkspaceBundleRepo, WorkspaceRepo,
        },
    };

    fn in_memory_conn() -> Arc<Mutex<Connection>> {
        Arc::new(Mutex::new(
            Connection::open_in_memory().expect("open in-memory sqlite"),
        ))
    }

    #[test]
    fn limited_session_event_read_is_ordered_and_does_not_materialize_past_sql_limit() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).expect("create repo");
        let session = Session {
            id: SessionId("limited-event-session".to_string()),
            project_id: ProjectId("limited-event-project".to_string()),
            workspace_id: WorkspaceId("limited-event-workspace".to_string()),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".to_string(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-09-01T00:00:00Z".to_string(),
            updated_at: "2026-09-01T00:00:00Z".to_string(),
        };
        futures::executor::block_on(repo.save_session(&session)).expect("save session");
        for sequence in 1..=3 {
            futures::executor::block_on(repo.append_event(&SessionEventRecord {
                event_id: format!("limited-event-{sequence}"),
                session_id: session.id.clone(),
                sequence,
                occurred_at: "2026-09-01T00:00:00Z".to_string(),
                kind: SessionEventKind::SessionResumed,
            }))
            .expect("append event");
        }

        let limited = repo
            .list_events_by_session_limited_sync(&session.id, 2)
            .expect("bounded query");
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0].sequence, 1);
        assert_eq!(limited[1].sequence, 2);
    }

    fn delegation_worktree_operation(id: &str) -> DelegationWorktreeOperation {
        DelegationWorktreeOperation {
            operation_id: DelegationWorktreeOperationId(id.to_string()),
            delegation_key: Some(format!("key-{id}")),
            delegation_id: None,
            workspace_id: WorkspaceId("journal-workspace".to_string()),
            parent_session_id: Some(SessionId("journal-parent".to_string())),
            child_session_id: None,
            source_root: "/tmp/journal-source".to_string(),
            worktree_path: format!("/tmp/journal-source/.dcc-worktrees/{id}"),
            branch: format!("dcc/delegation/{id}"),
            base_commit: "1111111111111111111111111111111111111111".to_string(),
            expected_branch_oid: None,
            source_root_id: None,
            worktree_root_id: None,
            common_dir_id: None,
            state: DelegationWorktreeOperationState::Preparing,
            last_error: None,
            recovery_owner: None,
            recovery_lease_until: None,
            created_at: "2026-08-28T10:00:00Z".to_string(),
            updated_at: "2026-08-28T10:00:00Z".to_string(),
        }
    }

    fn review_pending_delegation_operation(
        repo: &SqliteSessionRepo,
        id: &str,
    ) -> DelegationWorktreeOperation {
        let operation = delegation_worktree_operation(id);
        futures::executor::block_on(repo.create_delegation_worktree_operation(&operation))
            .expect("create apply operation");
        let mut prepared = operation;
        prepared.delegation_id = Some(DelegationId(format!("delegation-{id}")));
        prepared.state = DelegationWorktreeOperationState::Prepared;
        assert!(
            futures::executor::block_on(repo.compare_and_swap_delegation_worktree_operation(
                DelegationWorktreeOperationState::Preparing,
                &prepared,
            ))
            .expect("prepare apply operation")
        );
        let mut review = prepared;
        review.state = DelegationWorktreeOperationState::ReviewPending;
        assert!(
            futures::executor::block_on(repo.compare_and_swap_delegation_worktree_operation(
                DelegationWorktreeOperationState::Prepared,
                &review,
            ))
            .expect("review apply operation")
        );
        review
    }

    fn preparing_apply(
        operation: &DelegationWorktreeOperation,
        id: &str,
    ) -> DelegationApplyTransaction {
        DelegationApplyTransaction {
            transaction_id: DelegationApplyTransactionId(id.to_string()),
            operation_id: operation.operation_id.clone(),
            delegation_id: operation.delegation_id.clone().expect("delegation id"),
            workspace_id: operation.workspace_id.clone(),
            source_head_oid: None,
            destination_head_oid: None,
            destination_ref: None,
            destination_index_tree_oid: None,
            manifest_digest: None,
            file_count: 0,
            artifact_bytes: 0,
            state: DelegationApplyTransactionState::Preparing,
            recovery_owner: None,
            recovery_lease_until: None,
            last_error: None,
            created_at: "2026-08-28T11:00:00Z".to_string(),
            updated_at: "2026-08-28T11:00:00Z".to_string(),
        }
    }

    fn prepared_apply(mut apply: DelegationApplyTransaction) -> DelegationApplyTransaction {
        apply.source_head_oid = Some("a".repeat(40));
        apply.destination_head_oid = Some("b".repeat(40));
        apply.destination_ref = Some("refs/heads/main".to_string());
        apply.destination_index_tree_oid = Some("c".repeat(40));
        apply.manifest_digest = Some("d".repeat(64));
        apply.file_count = 2;
        apply.artifact_bytes = 42;
        apply.state = DelegationApplyTransactionState::Prepared;
        apply.updated_at = "2026-08-28T11:00:01Z".to_string();
        apply
    }

    fn prepared_delegation_worktree_operation(
        repo: &SqliteSessionRepo,
        id: &str,
    ) -> DelegationWorktreeOperation {
        let operation = delegation_worktree_operation(id);
        futures::executor::block_on(repo.create_delegation_worktree_operation(&operation))
            .expect("create delegation worktree operation");
        let mut prepared = operation;
        prepared.expected_branch_oid = Some("5555555555555555555555555555555555555555".to_string());
        prepared.state = DelegationWorktreeOperationState::Prepared;
        prepared.updated_at = "2026-08-28T10:00:01Z".to_string();
        assert!(
            futures::executor::block_on(repo.compare_and_swap_delegation_worktree_operation(
                DelegationWorktreeOperationState::Preparing,
                &prepared,
            ))
            .expect("prepare delegation worktree operation")
        );
        prepared
    }

    fn guarded_git_identity() -> GitIdentityV1 {
        GitIdentityV1 {
            schema_version: dcc_core::domain::guarded_undo::GIT_IDENTITY_SCHEMA_VERSION,
            worktree_identity: b"fixture-worktree".to_vec(),
            git_dir_identity: b"fixture-git-dir".to_vec(),
            common_dir_identity: b"fixture-common-dir".to_vec(),
            head_oid: vec![0x42; 20],
            checkout_ref: CheckoutRefV1::Symbolic {
                full_name: "refs/heads/main".to_owned(),
            },
            index: IndexIdentityV1 {
                sha256: Sha256Digest([0x33; 32]),
                size: 128,
                stat_identity: b"fixture-index-stat".to_vec(),
            },
        }
    }

    fn guarded_metadata() -> RegularFileMetadataV1 {
        RegularFileMetadataV1 {
            schema_version: 1,
            adapter: "fixture".to_owned(),
            file_identity: b"fixture-file-identity".to_vec(),
            link_count: 1,
            fields: BTreeMap::from([("mode".to_owned(), b"100644".to_vec())]),
        }
    }

    fn seed_guarded_undo_parents(conn: &Connection, suffix: &str) {
        conn.execute(
            "INSERT INTO dcc_workspaces (id, project_id, root_path, base_branch, state, created_at, updated_at) VALUES (?1, 'project', '/tmp/guarded', 'main', 'ready', 't0', 't0')",
            params![format!("workspace-{suffix}")],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO dcc_sessions (id, project_id, workspace_id, provider_id, state, created_at, updated_at) VALUES (?1, 'project', ?2, 'fixture', 'active', 't0', 't0')",
            params![format!("session-{suffix}"), format!("workspace-{suffix}")],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO dcc_turn_change_sets (snapshot_id, session_id, turn_id, workspace_id, capture_version, state, created_at) VALUES (?1, ?2, ?3, ?4, 1, 'available', 't0')",
            params![
                format!("snapshot-{suffix}"),
                format!("session-{suffix}"),
                format!("turn-{suffix}"),
                format!("workspace-{suffix}"),
            ],
        )
        .unwrap();
    }

    fn collecting_restore_set(suffix: &str) -> TurnRestoreSet {
        TurnRestoreSet {
            restore_set_id: RestoreSetId(format!("restore-{suffix}")),
            snapshot_id: format!("snapshot-{suffix}"),
            session_id: SessionId(format!("session-{suffix}")),
            turn_id: TurnId(format!("turn-{suffix}")),
            workspace_id: WorkspaceId(format!("workspace-{suffix}")),
            root_id: Some(PhysicalRootId(vec![1, 1, 7, 9])),
            capture_version: RESTORE_CAPTURE_VERSION,
            state: RestoreSetState::Collecting,
            reason_code: None,
            git_identity: Some(guarded_git_identity()),
            artifact_bytes: 0,
            file_count: 0,
            manifest_digest: None,
            created_at: "t1".to_owned(),
            completed_at: None,
            expires_at: None,
        }
    }

    fn eligible_restore_file(suffix: &str) -> TurnRestoreFile {
        TurnRestoreFile {
            restore_set_id: RestoreSetId(format!("restore-{suffix}")),
            ordinal: 0,
            path_bytes: OpaqueRepoPath::unix(b"src/lib-\xff.rs").unwrap(),
            status: RestoreFileStatus::Modified,
            pre_size: 4,
            pre_sha256: Sha256Digest([0x11; 32]),
            pre_artifact_key: ArtifactKey([0x21; 16]),
            result_size: 5,
            result_sha256: Sha256Digest([0x22; 32]),
            metadata_fingerprint: guarded_metadata(),
        }
    }

    fn persist_eligible_restore_set(
        repo: &SqliteSessionRepo,
        conn: &Arc<Mutex<Connection>>,
        suffix: &str,
        completed_at: &str,
    ) -> (TurnRestoreSet, TurnRestoreFile) {
        seed_guarded_undo_parents(&conn.lock().unwrap(), suffix);
        let collecting = collecting_restore_set(suffix);
        repo.create_turn_restore_set(&collecting).unwrap();
        let file = eligible_restore_file(suffix);
        let mut eligible = collecting;
        eligible.state = RestoreSetState::Eligible;
        eligible.file_count = 1;
        eligible.artifact_bytes = file.pre_size;
        eligible.manifest_digest =
            Some(canonical_restore_manifest_digest(std::slice::from_ref(&file)).unwrap());
        eligible.completed_at = Some(completed_at.to_owned());
        eligible.expires_at = Some("t9".to_owned());
        assert!(repo
            .finalize_turn_restore_set(&eligible, std::slice::from_ref(&file))
            .unwrap());
        (eligible, file)
    }

    #[test]
    fn guarded_undo_summary_lookup_is_content_free_and_fails_closed() {
        let conn = in_memory_conn();
        let repo = SqliteSessionRepo::from_connection(conn.clone()).unwrap();
        assert_eq!(
            repo.get_guarded_undo_capture_summary("snapshot-missing")
                .unwrap(),
            None
        );

        let (eligible, _file) = persist_eligible_restore_set(&repo, &conn, "summary", "t2");
        assert_eq!(
            repo.get_guarded_undo_capture_summary(&eligible.snapshot_id)
                .unwrap(),
            Some(GuardedUndoCaptureSummary {
                state: "eligible".to_owned(),
                reason_code: None,
                file_count: 1,
                artifact_bytes: 4,
                completed_at: Some("t2".to_owned()),
                expires_at: Some("t9".to_owned()),
            })
        );

        conn.lock()
            .unwrap()
            .execute(
                "UPDATE dcc_turn_restore_sets SET state = 'future_state' WHERE snapshot_id = ?1",
                params![eligible.snapshot_id],
            )
            .unwrap();
        assert!(repo
            .get_guarded_undo_capture_summary("snapshot-summary")
            .is_err());
    }

    #[test]
    fn workspace_history_deletion_discards_ineligible_capture_and_unblocks_workspace() {
        let conn = in_memory_conn();
        let session_repo = SqliteSessionRepo::from_connection(conn.clone()).unwrap();
        let workspace_repo = SqliteWorkspaceRepo::from_connection(conn.clone()).unwrap();
        seed_guarded_undo_parents(&conn.lock().unwrap(), "retire-ineligible");
        let collecting = collecting_restore_set("retire-ineligible");
        session_repo.create_turn_restore_set(&collecting).unwrap();
        let mut ineligible = collecting;
        ineligible.state = RestoreSetState::Ineligible;
        ineligible.reason_code = Some(GuardedUndoReasonCode::UntrackedPath);
        ineligible.completed_at = Some("t2".to_owned());
        assert!(session_repo
            .finalize_turn_restore_set(&ineligible, &[])
            .unwrap());

        let workspace_id = WorkspaceId("workspace-retire-ineligible".to_owned());
        session_repo
            .ensure_workspace_history_deletable(std::slice::from_ref(&workspace_id))
            .expect("an ineligible capture is discardable workspace history");
        session_repo
            .delete_workspace_history(std::slice::from_ref(&workspace_id))
            .expect("delete workspace history in dependency order");
        session_repo
            .delete_workspace_history(std::slice::from_ref(&workspace_id))
            .expect("retrying history deletion is idempotent");
        futures::executor::block_on(workspace_repo.delete_workspace(&workspace_id))
            .expect("guarded undo no longer blocks the workspace row");

        let locked = conn.lock().unwrap();
        for table in [
            "dcc_turn_restore_sets",
            "dcc_turn_change_sets",
            "dcc_sessions",
            "dcc_workspaces",
        ] {
            assert_eq!(
                locked
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap(),
                0,
                "{table} must not retain the retired workspace"
            );
        }
    }

    #[test]
    fn workspace_history_deletion_discards_eligible_capture_and_terminal_undo_journal() {
        let conn = in_memory_conn();
        let session_repo = SqliteSessionRepo::from_connection(conn.clone()).unwrap();
        let workspace_repo = SqliteWorkspaceRepo::from_connection(conn.clone()).unwrap();
        let (eligible, _) =
            persist_eligible_restore_set(&session_repo, &conn, "retire-terminal", "t2");
        conn.lock()
            .unwrap()
            .execute(
                r#"INSERT INTO dcc_undo_operations (
                       operation_id, restore_set_id, journal_version, state, active,
                       cleanup_pending, prepared_identity_json, reason_code,
                       created_at, updated_at, completed_at
                   ) VALUES ('operation-retire-terminal', ?1, 1, 'blocked', 0, 0, '{}',
                             'workspace_changed', 't3', 't4', 't4')"#,
                params![eligible.restore_set_id.0],
            )
            .unwrap();

        let workspace_id = WorkspaceId("workspace-retire-terminal".to_owned());
        session_repo
            .delete_workspace_history(std::slice::from_ref(&workspace_id))
            .expect("eligible capture and terminal journal are discardable history");
        futures::executor::block_on(workspace_repo.delete_workspace(&workspace_id))
            .expect("terminal Undo history no longer blocks workspace deletion");

        let locked = conn.lock().unwrap();
        for table in [
            "dcc_undo_operations",
            "dcc_turn_restore_files",
            "dcc_turn_restore_sets",
            "dcc_turn_change_sets",
            "dcc_sessions",
            "dcc_workspaces",
        ] {
            assert_eq!(
                locked
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap(),
                0,
                "{table} must not retain terminal history"
            );
        }
    }

    #[test]
    fn workspace_history_deletion_blocks_active_undo_before_changing_rows() {
        let conn = in_memory_conn();
        let repo = SqliteSessionRepo::from_connection(conn.clone()).unwrap();
        let (eligible, _) = persist_eligible_restore_set(&repo, &conn, "retire-active", "t2");
        conn.lock()
            .unwrap()
            .execute(
                r#"INSERT INTO dcc_undo_operations (
                       operation_id, restore_set_id, journal_version, state, active,
                       cleanup_pending, prepared_identity_json, created_at, updated_at
                   ) VALUES ('operation-retire-active', ?1, 1, 'preparing', 1, 1, '{}', 't3', 't3')"#,
                params![eligible.restore_set_id.0],
            )
            .unwrap();
        let workspace_id = WorkspaceId("workspace-retire-active".to_owned());

        let preflight = repo
            .ensure_workspace_history_deletable(std::slice::from_ref(&workspace_id))
            .expect_err("active recovery must block before external deletion");
        assert!(preflight
            .to_string()
            .contains("Undo recovery is still active"));
        assert!(repo
            .delete_workspace_history(std::slice::from_ref(&workspace_id))
            .is_err());

        let locked = conn.lock().unwrap();
        for table in [
            "dcc_undo_operations",
            "dcc_turn_restore_sets",
            "dcc_turn_change_sets",
            "dcc_sessions",
            "dcc_workspaces",
        ] {
            assert_eq!(
                locked
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap(),
                1,
                "{table} must remain intact after blocked preflight"
            );
        }
    }

    #[test]
    fn sqlite_session_repo_persists_session_thread_and_events() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).expect("create repo");
        let session = Session {
            id: SessionId("session-1".to_string()),
            project_id: ProjectId("project-1".to_string()),
            workspace_id: WorkspaceId("workspace-1".to_string()),
            additional_workspace_ids: vec![WorkspaceId("workspace-2".to_string())],
            provider_id: "codex".to_string(),
            model: Some("gpt-5".to_string()),
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let thread = Thread {
            id: ThreadId("thread-1".to_string()),
            project_id: ProjectId("project-1".to_string()),
            session_id: Some(SessionId("session-1".to_string())),
            title: "Thread".to_string(),
            archived_at: None,
        };
        let events = [
            SessionEventRecord {
                event_id: "event-1".to_string(),
                session_id: SessionId("session-1".to_string()),
                sequence: 1,
                occurred_at: "2026-01-01T00:00:00Z".to_string(),
                kind: SessionEventKind::SessionStarted {
                    workspace_id: WorkspaceId("workspace-1".to_string()),
                    project_id: ProjectId("project-1".to_string()),
                    provider_id: "codex".to_string(),
                    model: Some("gpt-5".to_string()),
                },
            },
            SessionEventRecord {
                event_id: "event-2".to_string(),
                session_id: SessionId("session-1".to_string()),
                sequence: 2,
                occurred_at: "2026-01-01T00:00:02Z".to_string(),
                kind: SessionEventKind::TurnStarted {
                    turn_id: TurnId("turn-1".to_string()),
                    prompt: "Implement the workspace recap".to_string(),
                    plan_mode: Some(false),
                    model: None,
                },
            },
            SessionEventRecord {
                event_id: "event-3".to_string(),
                session_id: SessionId("session-1".to_string()),
                sequence: 3,
                occurred_at: "2026-01-01T00:00:09Z".to_string(),
                kind: SessionEventKind::TurnCompleted {
                    turn_id: TurnId("turn-1".to_string()),
                },
            },
        ];

        futures::executor::block_on(repo.save_session(&session)).expect("save session");
        futures::executor::block_on(repo.save_thread(&thread)).expect("save thread");
        for event in &events {
            futures::executor::block_on(repo.append_event(event)).expect("append event");
        }

        let summary = repo
            .list_workspace_sessions(&WorkspaceId("workspace-1".to_string()))
            .expect("list summaries");
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].session.id.0, "session-1");
        assert_eq!(
            summary[0].session.additional_workspace_ids,
            vec![WorkspaceId("workspace-2".to_string())]
        );
        assert_eq!(summary[0].thread.id.0, "thread-1");
        assert_eq!(summary[0].projection.state, SessionState::Active);
        assert_eq!(
            summary[0].last_turn_started_at.as_deref(),
            Some("2026-01-01T00:00:02Z")
        );
        assert_eq!(
            summary[0].last_turn_completed_at.as_deref(),
            Some("2026-01-01T00:00:09Z")
        );
    }

    #[test]
    fn sqlite_session_repo_roundtrips_turn_change_sets_without_session_events() {
        let conn = in_memory_conn();
        let workspace_repo =
            SqliteWorkspaceRepo::from_connection(conn.clone()).expect("workspace repo");
        let repo = SqliteSessionRepo::from_connection(conn).expect("session repo");
        let workspace = Workspace {
            id: WorkspaceId("review-workspace".to_string()),
            project_id: ProjectId("review-project".to_string()),
            name: Some("Review".to_string()),
            root_path: "/tmp/review-workspace".to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some("/tmp/review-workspace".to_string()),
            source: None,
            state: WorkspaceState::Ready,
            setup_report: None,
            pinned_at: None,
            created_at: "2026-08-27T10:00:00Z".to_string(),
            updated_at: "2026-08-27T10:00:00Z".to_string(),
        };
        futures::executor::block_on(workspace_repo.save_workspace(&workspace)).unwrap();
        let session = Session {
            id: SessionId("review-session".to_string()),
            project_id: workspace.project_id.clone(),
            workspace_id: workspace.id.clone(),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".to_string(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-08-27T10:00:00Z".to_string(),
            updated_at: "2026-08-27T10:00:00Z".to_string(),
        };
        futures::executor::block_on(repo.save_session(&session)).unwrap();
        let mut diffs = BTreeMap::new();
        diffs.insert("src/lib.rs".to_string(), "captured diff".to_string());
        let change_set = TurnChangeSet {
            snapshot_id: "snapshot-1".to_string(),
            session_id: session.id.clone(),
            turn_id: TurnId("turn-1".to_string()),
            workspace_id: workspace.id.clone(),
            capture_version: 1,
            state: "available".to_string(),
            base_tree: Some("base".to_string()),
            result_tree: Some("result".to_string()),
            baseline_untracked: Vec::new(),
            result_untracked: Vec::new(),
            files: vec![dcc_core::domain::session::TurnReviewFile {
                path: "src/lib.rs".to_string(),
                old_path: None,
                status: "M".to_string(),
                insertions: 2,
                deletions: 1,
                untracked: false,
                binary: false,
                preview_unavailable: false,
            }],
            file_diffs: diffs,
            observed_validations: vec!["test".to_string()],
            diff_truncated: false,
            turn_outcome: Some("completed".to_string()),
            outcome_reason: None,
            error: None,
            created_at: "2026-08-27T10:00:01Z".to_string(),
            completed_at: Some("2026-08-27T10:00:02Z".to_string()),
        };
        repo.save_turn_change_set(&change_set).unwrap();

        let loaded = repo.get_turn_change_set("snapshot-1").unwrap().unwrap();
        assert_eq!(loaded.session_id, session.id);
        assert_eq!(loaded.files[0].path, "src/lib.rs");
        assert_eq!(loaded.turn_outcome.as_deref(), Some("completed"));
        assert_eq!(
            loaded.file_diffs.get("src/lib.rs").map(String::as_str),
            Some("captured diff")
        );
        let summaries = repo.list_turn_change_sets_by_session(&session.id).unwrap();
        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].file_diffs.is_empty());
        let mut competing_terminal = loaded.clone();
        competing_terminal.state = "partial".to_string();
        competing_terminal.turn_outcome = Some("aborted".to_string());
        competing_terminal.outcome_reason = Some("late abort".to_string());
        repo.save_turn_change_set(&competing_terminal).unwrap();
        let stable = repo.get_turn_change_set("snapshot-1").unwrap().unwrap();
        assert_eq!(stable.state, "available");
        assert_eq!(stable.turn_outcome.as_deref(), Some("completed"));
        assert!(stable.outcome_reason.is_none());
        let mut orphaned = stable.clone();
        orphaned.snapshot_id = "snapshot-orphaned".to_string();
        orphaned.turn_id = TurnId("turn-orphaned".to_string());
        orphaned.state = "collecting".to_string();
        orphaned.turn_outcome = None;
        orphaned.completed_at = None;
        repo.save_turn_change_set(&orphaned).unwrap();
        assert_eq!(
            repo.recover_interrupted_turn_change_sets("2026-08-27T10:10:00Z")
                .unwrap(),
            vec!["snapshot-orphaned"]
        );
        let recovered = repo
            .get_turn_change_set("snapshot-orphaned")
            .unwrap()
            .unwrap();
        assert_eq!(recovered.state, "interrupted");
        assert_eq!(recovered.turn_outcome.as_deref(), Some("aborted"));
        assert!(
            futures::executor::block_on(repo.list_events_by_session(&session.id))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn usage_dashboard_aggregates_and_replaces_turn_telemetry() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).expect("create repo");
        let session = Session {
            id: SessionId("usage-session".to_string()),
            project_id: ProjectId("usage-project".to_string()),
            workspace_id: WorkspaceId("usage-workspace".to_string()),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".to_string(),
            model: Some("gpt-5.6-codex".to_string()),
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-08-18T10:00:00Z".to_string(),
            updated_at: "2026-08-18T10:01:00Z".to_string(),
        };
        futures::executor::block_on(repo.save_session(&session)).expect("save usage session");
        futures::executor::block_on(repo.append_event(&SessionEventRecord {
            event_id: "usage-completed".to_string(),
            session_id: session.id.clone(),
            sequence: 1,
            occurred_at: "2026-08-18T10:01:00Z".to_string(),
            kind: SessionEventKind::TurnCompleted {
                turn_id: TurnId("usage-turn".to_string()),
            },
        }))
        .expect("append completed turn");

        let first = ModelTokenUsage {
            model: None,
            input_tokens: 100,
            output_tokens: 20,
            cached_input_tokens: 40,
            cache_write_input_tokens: 0,
            reasoning_output_tokens: 8,
            total_tokens: 120,
            cost_usd: None,
        };
        futures::executor::block_on(repo.replace_turn_usage(
            &session.id,
            &TurnId("usage-turn".to_string()),
            "2026-08-18T10:00:59Z",
            &[first],
        ))
        .expect("record first usage");
        let replacement = ModelTokenUsage {
            model: None,
            input_tokens: 150,
            output_tokens: 30,
            cached_input_tokens: 50,
            cache_write_input_tokens: 5,
            reasoning_output_tokens: 10,
            total_tokens: 180,
            cost_usd: None,
        };
        futures::executor::block_on(repo.replace_turn_usage(
            &session.id,
            &TurnId("usage-turn".to_string()),
            "2026-08-18T10:01:00Z",
            &[replacement],
        ))
        .expect("replace usage");

        let dashboard = futures::executor::block_on(repo.usage_dashboard(&UsageDashboardInput {
            period_days: None,
            project_id: None,
        }))
        .expect("load usage dashboard");
        assert_eq!(dashboard.totals.sessions, 1);
        assert_eq!(dashboard.totals.turns, 1);
        assert_eq!(dashboard.totals.measured_turns, 1);
        assert_eq!(dashboard.totals.total_tokens, 180);
        assert_eq!(dashboard.providers[0].provider_id, "codex");
        assert_eq!(dashboard.models[0].model, "gpt-5.6-codex");
        assert_eq!(dashboard.daily[0].turns, 1);
        assert_eq!(dashboard.daily[0].total_tokens, 180);
    }

    #[test]
    fn sqlite_session_repo_cleans_only_orphaned_multi_workspace_sessions() {
        let conn = in_memory_conn();
        let session_repo =
            SqliteSessionRepo::from_connection(conn.clone()).expect("create session repo");
        let workspace_repo =
            SqliteWorkspaceRepo::from_connection(conn.clone()).expect("create workspace repo");
        let workspace = Workspace {
            id: WorkspaceId("removed-workspace".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: Some("Removed Workspace".to_string()),
            root_path: "/tmp/removed-workspace".to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some("/tmp/removed-workspace".to_string()),
            source: None,
            state: WorkspaceState::Ready,
            setup_report: None,
            pinned_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        futures::executor::block_on(workspace_repo.save_workspace(&workspace))
            .expect("save workspace");

        let session = |id: &str, additional_workspace_ids: Vec<WorkspaceId>| Session {
            id: SessionId(id.to_string()),
            project_id: ProjectId("project-1".to_string()),
            workspace_id: workspace.id.clone(),
            additional_workspace_ids,
            provider_id: "codex".to_string(),
            model: Some("gpt-5".to_string()),
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Completed,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let multi_session = session(
            "orphaned-multi",
            vec![WorkspaceId("secondary-workspace".to_string())],
        );
        let single_session = session("orphaned-single", Vec::new());
        futures::executor::block_on(session_repo.save_session(&multi_session))
            .expect("save multi session");
        futures::executor::block_on(session_repo.save_session(&single_session))
            .expect("save single session");
        futures::executor::block_on(workspace_repo.delete_workspace(&workspace.id))
            .expect("delete workspace");

        let reopened = SqliteSessionRepo::from_connection(conn).expect("reopen session repo");
        assert!(
            futures::executor::block_on(reopened.get_session(&multi_session.id))
                .expect("read multi session")
                .is_none()
        );
        assert!(
            futures::executor::block_on(reopened.get_session(&single_session.id))
                .expect("read single session")
                .is_some()
        );
    }

    #[test]
    fn sqlite_session_repo_supports_full_text_search_over_past_sessions() {
        let conn = in_memory_conn();
        let repo = SqliteSessionRepo::from_connection(conn.clone()).expect("create session repo");
        let workspace_repo =
            SqliteWorkspaceRepo::from_connection(conn).expect("create workspace repo");
        let workspace = Workspace {
            id: WorkspaceId("workspace-1".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: Some("Searchable Workspace".to_string()),
            root_path: "/tmp/searchable".to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some("/tmp/searchable".to_string()),
            source: None,
            state: WorkspaceState::Ready,
            setup_report: None,
            pinned_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let session = Session {
            id: SessionId("session-search".to_string()),
            project_id: ProjectId("project-1".to_string()),
            workspace_id: WorkspaceId("workspace-1".to_string()),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".to_string(),
            model: Some("gpt-5".to_string()),
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let thread = Thread {
            id: ThreadId("thread-search".to_string()),
            project_id: ProjectId("project-1".to_string()),
            session_id: Some(session.id.clone()),
            title: "Authentication bugfix".to_string(),
            archived_at: Some("2026-01-01T00:10:00Z".to_string()),
        };
        let events = vec![
            SessionEventRecord {
                event_id: "event-1".to_string(),
                session_id: session.id.clone(),
                sequence: 1,
                occurred_at: "2026-01-01T00:00:00Z".to_string(),
                kind: SessionEventKind::SessionStarted {
                    workspace_id: WorkspaceId("workspace-1".to_string()),
                    project_id: ProjectId("project-1".to_string()),
                    provider_id: "codex".to_string(),
                    model: Some("gpt-5".to_string()),
                },
            },
            SessionEventRecord {
                event_id: "event-2".to_string(),
                session_id: session.id.clone(),
                sequence: 2,
                occurred_at: "2026-01-01T00:00:05Z".to_string(),
                kind: SessionEventKind::TurnStarted {
                    turn_id: TurnId("turn-1".to_string()),
                    prompt: "Find the authentication race condition in login".to_string(),
                    plan_mode: Some(false),
                    model: None,
                },
            },
            SessionEventRecord {
                event_id: "event-3".to_string(),
                session_id: session.id.clone(),
                sequence: 3,
                occurred_at: "2026-01-01T00:00:10Z".to_string(),
                kind: SessionEventKind::TurnAssistantMessageStarted {
                    turn_id: TurnId("turn-1".to_string()),
                    message_id: "final-1".to_string(),
                    phase: AssistantMessagePhase::FinalAnswer,
                },
            },
            SessionEventRecord {
                event_id: "event-4".to_string(),
                session_id: session.id.clone(),
                sequence: 4,
                occurred_at: "2026-01-01T00:00:11Z".to_string(),
                kind: SessionEventKind::TurnAssistantMessageDelta {
                    turn_id: TurnId("turn-1".to_string()),
                    message_id: "final-1".to_string(),
                    content: "The login handler drops the session token during retry.".to_string(),
                },
            },
            SessionEventRecord {
                event_id: "event-5".to_string(),
                session_id: session.id.clone(),
                sequence: 5,
                occurred_at: "2026-01-01T00:00:14Z".to_string(),
                kind: SessionEventKind::TurnAssistantMessageCompleted {
                    turn_id: TurnId("turn-1".to_string()),
                    message_id: "final-1".to_string(),
                    phase: AssistantMessagePhase::FinalAnswer,
                    content: Some(
                        "The login handler drops the session token during retry.".to_string(),
                    ),
                },
            },
            SessionEventRecord {
                event_id: "event-6".to_string(),
                session_id: session.id.clone(),
                sequence: 6,
                occurred_at: "2026-01-01T00:00:15Z".to_string(),
                kind: SessionEventKind::TurnCompleted {
                    turn_id: TurnId("turn-1".to_string()),
                },
            },
        ];

        futures::executor::block_on(workspace_repo.save_workspace(&workspace))
            .expect("save workspace");
        futures::executor::block_on(repo.save_session(&session)).expect("save session");
        futures::executor::block_on(repo.save_thread(&thread)).expect("save thread");
        for event in &events {
            futures::executor::block_on(repo.append_event(event)).expect("append event");
        }

        let matches = repo
            .search_sessions("authentication retry", 10)
            .expect("search sessions");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].session_id.0, "session-search");
        assert_eq!(matches[0].thread_title, "Authentication bugfix");
        assert_eq!(
            matches[0].workspace_name.as_deref(),
            Some("Searchable Workspace")
        );
        assert_eq!(
            matches[0].archived_at.as_deref(),
            Some("2026-01-01T00:10:00Z")
        );
        assert!(matches[0].snippet.to_lowercase().contains("authentication"));

        let recents = repo.search_sessions("", 10).expect("recent sessions");
        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0].session_id.0, "session-search");
    }

    #[test]
    fn sqlite_session_repo_persists_delegations_and_status_updates() {
        let conn = in_memory_conn();
        let repo = SqliteSessionRepo::from_connection(conn.clone()).expect("create session repo");
        let workspace_repo =
            SqliteWorkspaceRepo::from_connection(conn).expect("create workspace repo");
        let workspace = Workspace {
            id: WorkspaceId("workspace-1".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: Some("Delegation Workspace".to_string()),
            root_path: "/tmp/delegation".to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some("/tmp/delegation".to_string()),
            source: None,
            state: WorkspaceState::Ready,
            setup_report: None,
            pinned_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let parent_session = Session {
            id: SessionId("parent-session".to_string()),
            project_id: ProjectId("project-1".to_string()),
            workspace_id: workspace.id.clone(),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".to_string(),
            model: Some("gpt-5".to_string()),
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let child_session = Session {
            id: SessionId("child-session".to_string()),
            project_id: ProjectId("project-1".to_string()),
            workspace_id: workspace.id.clone(),
            additional_workspace_ids: Vec::new(),
            provider_id: "gemini".to_string(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Draft,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        futures::executor::block_on(workspace_repo.save_workspace(&workspace))
            .expect("save workspace");
        futures::executor::block_on(repo.save_session(&parent_session))
            .expect("save parent session");
        futures::executor::block_on(repo.save_session(&child_session)).expect("save child session");

        let delegation = Delegation {
            id: DelegationId("delegation-1".to_string()),
            parent_session_id: parent_session.id.clone(),
            parent_turn_id: Some(TurnId("turn-1".to_string())),
            child_session_id: Some(child_session.id.clone()),
            workspace_id: workspace.id.clone(),
            target_provider_id: ProviderId("gemini".to_string()),
            target_model_id: Some("gemini-2.5-pro".to_string()),
            mode: DelegationMode::Review,
            status: DelegationStatus::Draft,
            prompt: "Review the current diff".to_string(),
            context_policy: DelegationContextPolicy::ReviewCurrentDiff,
            budget: DelegationBudget {
                turn_limit: Some(1),
                timeout_seconds: Some(300),
                allow_file_edits: false,
            },
            result_summary: Some("No blocking issues.".to_string()),
            touched_files: vec!["src/lib.rs".to_string()],
            diff_summary: Some("1 file changed".to_string()),
            validation_summary: Some("cargo test -p dcc-core".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        futures::executor::block_on(repo.save_delegation(&delegation)).expect("save delegation");
        let fetched = futures::executor::block_on(repo.get_delegation(&delegation.id))
            .expect("get delegation")
            .expect("delegation exists");
        assert_eq!(fetched.parent_session_id.0, "parent-session");
        assert_eq!(fetched.target_model_id.as_deref(), Some("gemini-2.5-pro"));
        assert_eq!(
            fetched.child_session_id.as_ref().map(|id| id.0.as_str()),
            Some("child-session")
        );
        assert_eq!(
            fetched.context_policy,
            DelegationContextPolicy::ReviewCurrentDiff
        );
        assert_eq!(
            fetched.result_summary.as_deref(),
            Some("No blocking issues.")
        );
        assert_eq!(fetched.touched_files, vec!["src/lib.rs".to_string()]);
        assert_eq!(fetched.diff_summary.as_deref(), Some("1 file changed"));
        assert_eq!(
            fetched.validation_summary.as_deref(),
            Some("cargo test -p dcc-core")
        );

        let listed = futures::executor::block_on(
            repo.list_delegations(Some(&workspace.id), Some(&parent_session.id)),
        )
        .expect("list delegations");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id.0, "delegation-1");

        let cancelled = futures::executor::block_on(repo.update_delegation_status(
            &delegation.id,
            DelegationStatus::Cancelled,
            "2026-01-01T00:01:00Z".to_string(),
        ))
        .expect("cancel delegation")
        .expect("delegation exists after update");
        assert_eq!(cancelled.status, DelegationStatus::Cancelled);
        assert_eq!(cancelled.updated_at, "2026-01-01T00:01:00Z");
    }

    #[test]
    fn delegation_worktree_journal_migrates_and_roundtrips_physical_identity() {
        let conn = in_memory_conn();
        let repo = SqliteSessionRepo::from_connection(conn.clone()).expect("create session repo");
        let schema_objects = conn
            .lock()
            .expect("lock database")
            .prepare(
                "SELECT name FROM sqlite_master \
                 WHERE tbl_name = 'dcc_delegation_worktree_operations' ORDER BY name",
            )
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .expect("inspect journal migration");
        assert!(schema_objects
            .iter()
            .any(|name| name == "dcc_delegation_worktree_operations"));
        assert!(schema_objects
            .iter()
            .any(|name| name == "idx_dcc_delegation_worktree_recovery"));

        let operation = delegation_worktree_operation("operation-roundtrip");
        futures::executor::block_on(repo.create_delegation_worktree_operation(&operation))
            .expect("create journal operation");

        let mut prepared = operation.clone();
        prepared.delegation_id = Some(DelegationId("delegation-roundtrip".to_string()));
        prepared.child_session_id = Some(SessionId("journal-child".to_string()));
        prepared.expected_branch_oid = Some("2222222222222222222222222222222222222222".to_string());
        prepared.source_root_id = Some(PhysicalRootId(vec![1, 1, 7, 1]));
        prepared.worktree_root_id = Some(PhysicalRootId(vec![1, 1, 7, 2]));
        prepared.common_dir_id = Some(PhysicalRootId(vec![1, 1, 7, 3]));
        prepared.state = DelegationWorktreeOperationState::Prepared;
        prepared.updated_at = "2026-08-28T10:00:01Z".to_string();
        assert!(
            futures::executor::block_on(repo.compare_and_swap_delegation_worktree_operation(
                DelegationWorktreeOperationState::Preparing,
                &prepared,
            ))
            .expect("prepare with CAS")
        );

        let loaded = futures::executor::block_on(
            repo.get_delegation_worktree_operation(&operation.operation_id),
        )
        .expect("read journal operation")
        .expect("journal operation exists");
        assert_eq!(loaded, prepared);
        assert_eq!(
            futures::executor::block_on(repo.get_delegation_worktree_operation_by_delegation_id(
                prepared.delegation_id.as_ref().expect("delegation id"),
            ),)
            .expect("lookup by delegation"),
            Some(prepared.clone())
        );
        assert_eq!(
            futures::executor::block_on(
                repo.list_delegation_worktree_operations_by_workspace(&prepared.workspace_id),
            )
            .expect("list by workspace"),
            vec![prepared]
        );
    }

    #[test]
    fn delegation_worktree_journal_cas_is_stale_safe_and_recovery_is_deterministic() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).expect("create repo");
        let first = delegation_worktree_operation("operation-first");
        let mut second = delegation_worktree_operation("operation-second");
        second.updated_at = "2026-08-28T10:00:02Z".to_string();
        futures::executor::block_on(repo.create_delegation_worktree_operation(&first))
            .expect("create first operation");
        futures::executor::block_on(repo.create_delegation_worktree_operation(&second))
            .expect("create second operation");

        let mut prepared = first.clone();
        prepared.expected_branch_oid = Some("3333333333333333333333333333333333333333".to_string());
        prepared.state = DelegationWorktreeOperationState::Prepared;
        prepared.updated_at = "2026-08-28T10:00:01Z".to_string();
        assert!(
            futures::executor::block_on(repo.compare_and_swap_delegation_worktree_operation(
                DelegationWorktreeOperationState::Preparing,
                &prepared,
            ))
            .expect("winning CAS")
        );

        let mut stale = prepared.clone();
        stale.state = DelegationWorktreeOperationState::CleanupRequired;
        stale.last_error = Some("stale worker".to_string());
        assert!(
            !futures::executor::block_on(repo.compare_and_swap_delegation_worktree_operation(
                DelegationWorktreeOperationState::Preparing,
                &stale,
            ))
            .expect("stale CAS is a normal miss")
        );

        let mut wrong_scope = prepared.clone();
        wrong_scope.worktree_path = "/tmp/replaced-by-stale-worker".to_string();
        wrong_scope.state = DelegationWorktreeOperationState::Bound;
        assert!(
            !futures::executor::block_on(repo.compare_and_swap_delegation_worktree_operation(
                DelegationWorktreeOperationState::Prepared,
                &wrong_scope,
            ))
            .expect("immutable scope mismatch is rejected")
        );

        let recovery = futures::executor::block_on(
            repo.list_delegation_worktree_operations_requiring_recovery(),
        )
        .expect("list recovery");
        assert_eq!(
            recovery
                .iter()
                .map(|operation| operation.operation_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["operation-first", "operation-second"]
        );
        assert!(!futures::executor::block_on(
            repo.delete_removed_delegation_worktree_operation(&first.operation_id),
        )
        .expect("live journal cannot be purged"));
    }

    #[test]
    fn delegation_worktree_generic_cas_cannot_enter_or_leave_applying() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).expect("create repo");
        let operation = delegation_worktree_operation("operation-retry");
        futures::executor::block_on(repo.create_delegation_worktree_operation(&operation))
            .expect("create operation");

        let mut prepared = operation.clone();
        prepared.expected_branch_oid = Some("4444444444444444444444444444444444444444".to_string());
        prepared.state = DelegationWorktreeOperationState::Prepared;
        assert!(
            futures::executor::block_on(repo.compare_and_swap_delegation_worktree_operation(
                DelegationWorktreeOperationState::Preparing,
                &prepared,
            ))
            .unwrap()
        );
        let mut review = prepared.clone();
        review.state = DelegationWorktreeOperationState::ReviewPending;
        assert!(
            futures::executor::block_on(repo.compare_and_swap_delegation_worktree_operation(
                DelegationWorktreeOperationState::Prepared,
                &review,
            ))
            .unwrap()
        );
        let mut applying = review.clone();
        applying.state = DelegationWorktreeOperationState::Applying;
        assert!(
            futures::executor::block_on(repo.compare_and_swap_delegation_worktree_operation(
                DelegationWorktreeOperationState::ReviewPending,
                &applying,
            ))
            .is_err()
        );
        assert_eq!(
            futures::executor::block_on(
                repo.get_delegation_worktree_operation(&operation.operation_id),
            )
            .unwrap()
            .unwrap()
            .state,
            DelegationWorktreeOperationState::ReviewPending
        );
    }

    #[test]
    fn delegation_apply_roundtrip_rollback_allows_a_new_active_attempt() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).expect("create repo");
        let operation = review_pending_delegation_operation(&repo, "apply-roundtrip");
        let preparing = preparing_apply(&operation, "apply-tx-one");
        futures::executor::block_on(repo.create_delegation_apply_transaction(&preparing))
            .expect("create preparing apply");

        let mut invalid = prepared_apply(preparing.clone());
        invalid.manifest_digest = Some("not-a-digest".to_string());
        assert!(
            futures::executor::block_on(repo.compare_and_swap_delegation_apply_transaction(
                DelegationApplyTransactionState::Preparing,
                &invalid,
            ))
            .is_err()
        );

        let prepared = prepared_apply(preparing);
        assert!(
            futures::executor::block_on(repo.compare_and_swap_delegation_apply_transaction(
                DelegationApplyTransactionState::Preparing,
                &prepared,
            ))
            .expect("prepare transaction")
        );
        let claimed = futures::executor::block_on(repo.claim_delegation_apply_transaction(
            &prepared.transaction_id,
            "worker-one",
            "2026-08-28T11:00:02Z",
            "2026-08-28T11:01:02Z",
            false,
        ))
        .expect("claim transaction")
        .expect("claim won");
        assert_eq!(claimed.state, DelegationApplyTransactionState::Applying);
        assert_eq!(
            futures::executor::block_on(
                repo.get_delegation_worktree_operation(&operation.operation_id)
            )
            .unwrap()
            .unwrap()
            .state,
            DelegationWorktreeOperationState::Applying
        );

        assert!(
            futures::executor::block_on(repo.finalize_delegation_apply_transaction(
                &prepared.transaction_id,
                "wrong-worker",
                DelegationApplyTransactionState::RolledBack,
                Some("pre-mutation rollback".to_string()),
                "2026-08-28T11:00:03Z",
            ))
            .unwrap()
            .is_none()
        );
        let rolled_back = futures::executor::block_on(repo.finalize_delegation_apply_transaction(
            &prepared.transaction_id,
            "worker-one",
            DelegationApplyTransactionState::RolledBack,
            Some("pre-mutation rollback".to_string()),
            "2026-08-28T11:00:03Z",
        ))
        .expect("finalize rollback")
        .expect("owner finalized");
        assert_eq!(
            rolled_back.state,
            DelegationApplyTransactionState::RolledBack
        );
        assert_eq!(
            futures::executor::block_on(
                repo.get_delegation_worktree_operation(&operation.operation_id)
            )
            .unwrap()
            .unwrap()
            .state,
            DelegationWorktreeOperationState::ReviewPending
        );

        let mut retry = preparing_apply(&operation, "apply-tx-two");
        retry.created_at = "2026-08-28T11:00:04Z".to_string();
        retry.updated_at = retry.created_at.clone();
        futures::executor::block_on(repo.create_delegation_apply_transaction(&retry))
            .expect("terminal history permits retry");
        assert_eq!(
            futures::executor::block_on(
                repo.get_delegation_apply_transaction_by_operation_id(&operation.operation_id)
            )
            .unwrap()
            .unwrap()
            .transaction_id,
            retry.transaction_id
        );
    }

    #[test]
    fn delegation_apply_claim_has_one_owner_and_expired_lease_allows_takeover() {
        let directory = tempfile::tempdir().expect("temporary database directory");
        let path = directory.path().join("apply-claim.sqlite");
        let first_repo = SqliteSessionRepo::open(&path).expect("open first repo");
        let operation = review_pending_delegation_operation(&first_repo, "apply-claim");
        let prepared = prepared_apply(preparing_apply(&operation, "apply-claim-tx"));
        futures::executor::block_on(
            first_repo.create_delegation_apply_transaction(&preparing_apply(
                &operation,
                "apply-claim-tx",
            )),
        )
        .expect("create transaction");
        assert!(futures::executor::block_on(
            first_repo.compare_and_swap_delegation_apply_transaction(
                DelegationApplyTransactionState::Preparing,
                &prepared,
            )
        )
        .unwrap());
        let second_repo = SqliteSessionRepo::open(&path).expect("open second repo");
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let first_barrier = Arc::clone(&barrier);
        let first_id = prepared.transaction_id.clone();
        let first_thread = std::thread::spawn(move || {
            first_barrier.wait();
            futures::executor::block_on(first_repo.claim_delegation_apply_transaction(
                &first_id,
                "owner-one",
                "2026-08-28T12:00:00Z",
                "2026-08-28T12:01:00Z",
                false,
            ))
            .expect("first claim")
        });
        let second_barrier = barrier;
        let second_id = prepared.transaction_id.clone();
        let second_for_thread = second_repo.clone();
        let second_thread = std::thread::spawn(move || {
            second_barrier.wait();
            futures::executor::block_on(second_for_thread.claim_delegation_apply_transaction(
                &second_id,
                "owner-two",
                "2026-08-28T12:00:00Z",
                "2026-08-28T12:01:00Z",
                false,
            ))
            .expect("second claim")
        });
        let claims = [first_thread.join().unwrap(), second_thread.join().unwrap()];
        assert_eq!(claims.iter().filter(|claim| claim.is_some()).count(), 1);
        let winner = claims
            .iter()
            .find_map(|claim| claim.as_ref())
            .unwrap()
            .recovery_owner
            .clone()
            .unwrap();

        let takeover = futures::executor::block_on(second_repo.claim_delegation_apply_transaction(
            &prepared.transaction_id,
            "takeover-owner",
            "2026-08-28T12:01:00Z",
            "2026-08-28T12:02:00Z",
            false,
        ))
        .expect("takeover claim")
        .expect("expired lease permits takeover");
        assert_eq!(takeover.recovery_owner.as_deref(), Some("takeover-owner"));
        assert_ne!(winner, "takeover-owner");
        assert!(
            futures::executor::block_on(second_repo.finalize_delegation_apply_transaction(
                &prepared.transaction_id,
                &winner,
                DelegationApplyTransactionState::Applied,
                None,
                "2026-08-28T12:01:01Z",
            ))
            .unwrap()
            .is_none()
        );
        let applied =
            futures::executor::block_on(second_repo.finalize_delegation_apply_transaction(
                &prepared.transaction_id,
                "takeover-owner",
                DelegationApplyTransactionState::Applied,
                None,
                "2026-08-28T12:01:01Z",
            ))
            .unwrap()
            .unwrap();
        assert_eq!(applied.state, DelegationApplyTransactionState::Applied);
        assert_eq!(
            futures::executor::block_on(
                second_repo.get_delegation_worktree_operation(&operation.operation_id)
            )
            .unwrap()
            .unwrap()
            .state,
            DelegationWorktreeOperationState::Applied
        );
    }

    #[test]
    fn delegation_apply_future_lease_takeover_requires_operation_lock() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).expect("create repo");
        let operation = review_pending_delegation_operation(&repo, "apply-locked-takeover");
        let preparing = preparing_apply(&operation, "apply-locked-takeover-tx");
        futures::executor::block_on(repo.create_delegation_apply_transaction(&preparing))
            .expect("create transaction");
        let prepared = prepared_apply(preparing);
        assert!(
            futures::executor::block_on(repo.compare_and_swap_delegation_apply_transaction(
                DelegationApplyTransactionState::Preparing,
                &prepared,
            ))
            .expect("prepare transaction")
        );

        let first = futures::executor::block_on(repo.claim_delegation_apply_transaction(
            &prepared.transaction_id,
            "first-owner",
            "2026-08-28T14:00:00Z",
            "2026-08-28T14:15:00Z",
            false,
        ))
        .expect("initial claim")
        .expect("initial owner wins");
        assert_eq!(first.recovery_owner.as_deref(), Some("first-owner"));

        assert!(
            futures::executor::block_on(repo.claim_delegation_apply_transaction(
                &prepared.transaction_id,
                "unlocked-takeover",
                "2026-08-28T14:01:00Z",
                "2026-08-28T14:16:00Z",
                false,
            ))
            .expect("normal claim respects future lease")
            .is_none()
        );

        let takeover = futures::executor::block_on(repo.claim_delegation_apply_transaction(
            &prepared.transaction_id,
            "locked-takeover",
            "2026-08-28T14:01:00Z",
            "2026-08-28T14:16:00Z",
            true,
        ))
        .expect("operation-lock claim")
        .expect("operation lock permits immediate takeover");
        assert_eq!(takeover.recovery_owner.as_deref(), Some("locked-takeover"));
        assert_eq!(
            takeover.recovery_lease_until.as_deref(),
            Some("2026-08-28T14:16:00.000000000Z")
        );
    }

    #[test]
    fn delegation_apply_claim_is_atomic_with_operation_scope() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).expect("create repo");
        let operation = review_pending_delegation_operation(&repo, "apply-atomic");
        let preparing = preparing_apply(&operation, "apply-atomic-tx");
        futures::executor::block_on(repo.create_delegation_apply_transaction(&preparing)).unwrap();
        let prepared = prepared_apply(preparing);
        assert!(
            futures::executor::block_on(repo.compare_and_swap_delegation_apply_transaction(
                DelegationApplyTransactionState::Preparing,
                &prepared,
            ))
            .unwrap()
        );

        let mut cleanup = operation.clone();
        cleanup.state = DelegationWorktreeOperationState::CleanupRequired;
        cleanup.last_error = Some("independent lifecycle failure".to_string());
        assert!(
            futures::executor::block_on(repo.compare_and_swap_delegation_worktree_operation(
                DelegationWorktreeOperationState::ReviewPending,
                &cleanup,
            ))
            .unwrap()
        );
        assert!(
            futures::executor::block_on(repo.claim_delegation_apply_transaction(
                &prepared.transaction_id,
                "worker",
                "2026-08-28T13:00:00Z",
                "2026-08-28T13:01:00Z",
                false,
            ))
            .unwrap()
            .is_none()
        );
        assert_eq!(
            futures::executor::block_on(
                repo.get_delegation_apply_transaction(&prepared.transaction_id)
            )
            .unwrap()
            .unwrap()
            .state,
            DelegationApplyTransactionState::Prepared
        );
    }

    #[test]
    fn delegation_apply_schema_migrates_idempotently_without_rewriting_legacy_applying() {
        let directory = tempfile::tempdir().expect("temporary database directory");
        let path = directory.path().join("apply-migration.sqlite");
        let repo = SqliteSessionRepo::open(&path).expect("create legacy-shaped database");
        let operation = review_pending_delegation_operation(&repo, "legacy-apply");
        drop(repo);
        let connection = Connection::open(&path).expect("open legacy database directly");
        connection
            .execute(
                "UPDATE dcc_delegation_worktree_operations SET state = 'applying' \
                 WHERE operation_id = ?1",
                params![operation.operation_id.0],
            )
            .expect("seed legacy applying state");
        connection
            .execute_batch("DROP TABLE dcc_delegation_apply_transactions;")
            .expect("remove new table to model legacy schema");
        drop(connection);

        let migrated = SqliteSessionRepo::open(&path).expect("migrate apply transaction schema");
        let legacy = futures::executor::block_on(
            migrated.get_delegation_worktree_operation(&operation.operation_id),
        )
        .unwrap()
        .unwrap();
        assert_eq!(legacy.state, DelegationWorktreeOperationState::Applying);
        assert!(futures::executor::block_on(
            migrated.get_delegation_apply_transaction_by_operation_id(&operation.operation_id)
        )
        .unwrap()
        .is_none());
        drop(migrated);
        SqliteSessionRepo::open(&path).expect("migration is idempotent");
    }

    #[test]
    fn delegation_worktree_journal_enforces_one_operation_per_delegation() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).expect("create repo");
        let mut first = delegation_worktree_operation("operation-owner-one");
        first.delegation_id = Some(DelegationId("delegation-owner".to_string()));
        let mut second = delegation_worktree_operation("operation-owner-two");
        second.delegation_id = first.delegation_id.clone();
        futures::executor::block_on(repo.create_delegation_worktree_operation(&first))
            .expect("create first owner");
        let error = futures::executor::block_on(repo.create_delegation_worktree_operation(&second))
            .expect_err("delegation id must have one journal owner");
        assert!(error.to_string().contains("UNIQUE constraint failed"));
    }

    #[test]
    fn delegation_worktree_removal_claim_has_one_cross_connection_owner() {
        let directory = tempfile::tempdir().expect("temporary database directory");
        let path = directory.path().join("delegation-claim.sqlite");
        let first_repo = SqliteSessionRepo::open(&path).expect("open first repo");
        let operation = prepared_delegation_worktree_operation(&first_repo, "operation-claim-race");
        let second_repo = SqliteSessionRepo::open(&path).expect("open second repo");
        let barrier = Arc::new(std::sync::Barrier::new(2));

        let first_barrier = Arc::clone(&barrier);
        let first_id = operation.operation_id.clone();
        let first = std::thread::spawn(move || {
            first_barrier.wait();
            futures::executor::block_on(first_repo.claim_delegation_worktree_removal(
                &first_id,
                "owner-one-secret",
                "2026-08-28T11:00:00Z",
                "2026-08-28T11:05:00Z",
            ))
            .expect("first claim")
        });
        let second_barrier = Arc::clone(&barrier);
        let second_id = operation.operation_id.clone();
        let second = std::thread::spawn(move || {
            second_barrier.wait();
            futures::executor::block_on(second_repo.claim_delegation_worktree_removal(
                &second_id,
                "owner-two-secret",
                "2026-08-28T11:00:00Z",
                "2026-08-28T11:05:00Z",
            ))
            .expect("second claim")
        });
        let first = first.join().expect("first claim thread");
        let second = second.join().expect("second claim thread");
        assert_eq!(
            usize::from(first.is_some()) + usize::from(second.is_some()),
            1
        );
        let claimed = first.or(second).expect("one claim winner");
        let winning_owner = claimed.recovery_owner.clone().expect("winning owner");
        assert_eq!(claimed.state, DelegationWorktreeOperationState::Removing);
        assert_eq!(
            claimed.recovery_lease_until.as_deref(),
            Some("2026-08-28T11:05:00.000000000Z")
        );
        assert!(!format!("{claimed:?}").contains(&winning_owner));

        let verifier = SqliteSessionRepo::open(&path).expect("open verifier");
        assert!(
            futures::executor::block_on(verifier.finalize_delegation_worktree_removal(
                &operation.operation_id,
                "wrong-owner",
                DelegationWorktreeOperationState::Removed,
                None,
                "2026-08-28T11:01:00Z",
            ))
            .expect("wrong owner finalize is a normal miss")
            .is_none()
        );
        let removed = futures::executor::block_on(verifier.finalize_delegation_worktree_removal(
            &operation.operation_id,
            &winning_owner,
            DelegationWorktreeOperationState::Removed,
            None,
            "2026-08-28T11:01:00Z",
        ))
        .expect("winning owner finalizes")
        .expect("removed operation");
        assert_eq!(removed.state, DelegationWorktreeOperationState::Removed);
        assert!(removed.recovery_owner.is_none());
        assert!(removed.recovery_lease_until.is_none());
    }

    #[test]
    fn delegation_worktree_expired_removal_lease_allows_takeover() {
        let directory = tempfile::tempdir().expect("temporary database directory");
        let path = directory.path().join("delegation-takeover.sqlite");
        let first_repo = SqliteSessionRepo::open(&path).expect("open first repo");
        let operation = prepared_delegation_worktree_operation(&first_repo, "operation-takeover");
        let claimed = futures::executor::block_on(first_repo.claim_delegation_worktree_removal(
            &operation.operation_id,
            "expired-owner",
            "2026-08-28T12:00:00Z",
            "2026-08-28T12:01:00Z",
        ))
        .expect("initial claim")
        .expect("initial owner");
        assert_eq!(claimed.recovery_owner.as_deref(), Some("expired-owner"));

        let second_repo = SqliteSessionRepo::open(&path).expect("open takeover repo");
        assert!(
            futures::executor::block_on(second_repo.claim_delegation_worktree_removal(
                &operation.operation_id,
                "too-early-owner",
                "2026-08-28T12:00:59Z",
                "2026-08-28T12:02:00Z",
            ))
            .expect("unexpired claim is a normal miss")
            .is_none()
        );
        let takeover = futures::executor::block_on(second_repo.claim_delegation_worktree_removal(
            &operation.operation_id,
            "takeover-owner",
            "2026-08-28T12:01:00Z",
            "2026-08-28T12:06:00Z",
        ))
        .expect("expired lease takeover")
        .expect("takeover owner");
        assert_eq!(takeover.recovery_owner.as_deref(), Some("takeover-owner"));

        assert!(
            futures::executor::block_on(first_repo.finalize_delegation_worktree_removal(
                &operation.operation_id,
                "expired-owner",
                DelegationWorktreeOperationState::Removed,
                None,
                "2026-08-28T12:01:01Z",
            ))
            .expect("expired owner finalize is a normal miss")
            .is_none()
        );
        let failed = futures::executor::block_on(second_repo.finalize_delegation_worktree_removal(
            &operation.operation_id,
            "takeover-owner",
            DelegationWorktreeOperationState::CleanupRequired,
            Some("filesystem removal was partial".to_string()),
            "2026-08-28T12:01:01Z",
        ))
        .expect("takeover failure finalize")
        .expect("cleanup-required operation");
        assert_eq!(
            failed.state,
            DelegationWorktreeOperationState::CleanupRequired
        );
        assert_eq!(
            failed.last_error.as_deref(),
            Some("filesystem removal was partial")
        );
        assert!(failed.recovery_owner.is_none());
        assert!(failed.recovery_lease_until.is_none());
    }

    #[test]
    fn delegation_worktree_recovery_lease_migration_is_idempotent() {
        let conn = in_memory_conn();
        conn.lock()
            .expect("lock old database")
            .execute_batch(
                r#"
                CREATE TABLE dcc_delegation_worktree_operations (
                    operation_id TEXT PRIMARY KEY NOT NULL,
                    delegation_key TEXT NULL,
                    delegation_id TEXT NULL,
                    workspace_id TEXT NOT NULL,
                    parent_session_id TEXT NULL,
                    child_session_id TEXT NULL,
                    source_root TEXT NOT NULL,
                    worktree_path TEXT NOT NULL,
                    branch TEXT NOT NULL,
                    base_commit TEXT NOT NULL,
                    expected_branch_oid TEXT NULL,
                    source_root_id BLOB NULL,
                    worktree_root_id BLOB NULL,
                    common_dir_id BLOB NULL,
                    state TEXT NOT NULL,
                    last_error TEXT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                INSERT INTO dcc_delegation_worktree_operations (
                    operation_id, workspace_id, source_root, worktree_path,
                    branch, base_commit, state, created_at, updated_at
                ) VALUES (
                    'legacy-removing', 'legacy-workspace', '/tmp/legacy-source',
                    '/tmp/legacy-child', 'dcc/delegation/legacy',
                    '6666666666666666666666666666666666666666', 'removing',
                    '2026-08-28T09:00:00Z', '2026-08-28T09:01:00Z'
                );
                "#,
            )
            .expect("create pre-lease journal schema");

        let first = SqliteSessionRepo::from_connection(Arc::clone(&conn))
            .expect("apply first lease migration");
        let second = SqliteSessionRepo::from_connection(conn).expect("reapply lease migration");
        let migrated = futures::executor::block_on(second.get_delegation_worktree_operation(
            &DelegationWorktreeOperationId("legacy-removing".to_string()),
        ))
        .expect("read migrated operation")
        .expect("legacy operation remains");
        assert_eq!(
            migrated.state,
            DelegationWorktreeOperationState::CleanupRequired
        );
        assert_eq!(
            migrated.last_error.as_deref(),
            Some("legacy removing operation requires recovery ownership")
        );
        assert!(migrated.recovery_owner.is_none());
        assert!(migrated.recovery_lease_until.is_none());
        drop(first);
    }

    #[test]
    fn sqlite_workspace_repo_persists_repositories_and_deletes_linked_workspaces() {
        let repo = SqliteWorkspaceRepo::from_connection(in_memory_conn()).expect("create repo");
        let repository = Repository {
            id: RepositoryId("/tmp/repo".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: "repo".to_string(),
            display_name: None,
            icon: None,
            color: None,
            pinned_at: None,
            root_path: "/tmp/repo".to_string(),
            base_branch: "main".to_string(),
            remote: Some("origin".to_string()),
            remote_url: Some("git@github.com:acme/repo.git".to_string()),
            forge_provider: Some("github".to_string()),
            forge_login: Some("octocat".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let workspace = Workspace {
            id: WorkspaceId("workspace-1".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: Some("Workspace".to_string()),
            root_path: "/tmp/repo".to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some("/tmp/repo/.dcc-worktrees/main".to_string()),
            source: None,
            state: WorkspaceState::Ready,
            setup_report: None,
            pinned_at: Some("2026-01-01T00:00:30Z".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        futures::executor::block_on(repo.save_repository(&repository)).expect("save repository");
        futures::executor::block_on(repo.save_workspace(&workspace)).expect("save workspace");

        let repositories =
            futures::executor::block_on(repo.list_repositories()).expect("list repositories");
        assert_eq!(repositories.len(), 1);
        assert_eq!(repositories[0].root_path, "/tmp/repo");
        assert_eq!(repositories[0].remote.as_deref(), Some("origin"));
        assert_eq!(repositories[0].forge_login.as_deref(), Some("octocat"));
        assert_eq!(
            futures::executor::block_on(repo.get_workspace(&workspace.id))
                .expect("read workspace")
                .expect("workspace exists")
                .pinned_at
                .as_deref(),
            Some("2026-01-01T00:00:30Z")
        );

        assert!(repo
            .update_repository_identity(
                &RepositoryId("/tmp/repo".to_string()),
                Some("Customer Portal"),
                Some("rocket"),
                Some("violet"),
            )
            .expect("rename project"));
        let renamed = futures::executor::block_on(
            repo.get_repository(&RepositoryId("/tmp/repo".to_string())),
        )
        .expect("read renamed project")
        .expect("renamed project exists");
        assert_eq!(renamed.display_name.as_deref(), Some("Customer Portal"));
        assert_eq!(renamed.icon.as_deref(), Some("rocket"));
        assert_eq!(renamed.color.as_deref(), Some("violet"));
        assert!(repo
            .update_repository_pinned_at(
                &RepositoryId("/tmp/repo".to_string()),
                Some("2026-01-01T00:00:40Z"),
            )
            .expect("pin project"));

        futures::executor::block_on(repo.save_repository(&repository))
            .expect("re-register repository");
        let rediscovered = futures::executor::block_on(
            repo.get_repository(&RepositoryId("/tmp/repo".to_string())),
        )
        .expect("read rediscovered project")
        .expect("rediscovered project exists");
        assert_eq!(
            rediscovered.display_name.as_deref(),
            Some("Customer Portal")
        );
        assert_eq!(rediscovered.icon.as_deref(), Some("rocket"));
        assert_eq!(rediscovered.color.as_deref(), Some("violet"));
        assert_eq!(
            rediscovered.pinned_at.as_deref(),
            Some("2026-01-01T00:00:40Z")
        );

        futures::executor::block_on(repo.delete_repository(&RepositoryId("/tmp/repo".to_string())))
            .expect("delete repository");

        let repositories =
            futures::executor::block_on(repo.list_repositories()).expect("list repositories");
        assert!(repositories.is_empty());
        let workspaces =
            futures::executor::block_on(repo.list_workspaces()).expect("list workspaces");
        assert!(workspaces.is_empty());
        let mut stale_refresh = repository.clone();
        stale_refresh.remote = Some("origin".to_string());
        stale_refresh.remote_url = Some("git@gitlab.example.com:acme/repo.git".to_string());
        stale_refresh.forge_provider = Some("gitlab".to_string());
        assert!(!repo
            .update_repository_forge_metadata_if_exists(&stale_refresh)
            .expect("ignore refresh for deleted repository"));
        assert!(
            futures::executor::block_on(repo.get_repository(&repository.id))
                .expect("read repository after stale refresh")
                .is_none()
        );

        let mut replacement = repository.clone();
        replacement.remote = Some("replacement".to_string());
        replacement.remote_url = Some("git@github.com:acme/replacement.git".to_string());
        replacement.forge_provider = Some("github".to_string());
        replacement.created_at = "2026-01-01T00:01:00Z".to_string();
        replacement.updated_at = "2026-01-01T00:01:00Z".to_string();
        futures::executor::block_on(repo.save_repository(&replacement))
            .expect("re-register replacement repository");
        assert!(!repo
            .update_repository_forge_metadata_if_exists(&stale_refresh)
            .expect("ignore refresh for recreated repository"));
        let current = futures::executor::block_on(repo.get_repository(&replacement.id))
            .expect("read recreated repository")
            .expect("recreated repository exists");
        assert_eq!(current.remote.as_deref(), Some("replacement"));
    }

    #[test]
    fn sqlite_workspace_repo_round_trips_imported_source_context() {
        let repo = SqliteWorkspaceRepo::from_connection(in_memory_conn()).expect("create repo");
        let workspace = Workspace {
            id: WorkspaceId("workspace-source".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: Some("Review #42".to_string()),
            root_path: "/tmp/repo".to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some("/tmp/repo/.dcc-worktrees/review-42".to_string()),
            source: Some(WorkspaceSource {
                kind: WorkspaceSourceKind::PullRequest,
                url: "https://github.com/acme/widgets/pull/42".to_string(),
                provider: "github".to_string(),
                remote_name: "origin".to_string(),
                head_branch: "feature/review".to_string(),
                head_sha: "abc123".to_string(),
                base_branch: "main".to_string(),
                change_request_number: Some(42),
                title: Some("Improve review flow".to_string()),
                author: Some("octocat".to_string()),
                source_repository: Some("acme/widgets".to_string()),
                push_target: Some(WorkspacePushTarget {
                    remote_name: "origin".to_string(),
                    branch_name: "feature/review".to_string(),
                    remote_url: None,
                    remote_created: false,
                }),
            }),
            state: WorkspaceState::Ready,
            setup_report: None,
            pinned_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        futures::executor::block_on(repo.save_workspace(&workspace)).expect("save workspace");
        let restored = futures::executor::block_on(repo.get_workspace(&workspace.id))
            .expect("read workspace")
            .expect("workspace exists");

        assert_eq!(
            restored
                .source
                .as_ref()
                .map(|source| source.head_branch.as_str()),
            Some("feature/review")
        );
        assert_eq!(
            restored
                .source
                .as_ref()
                .and_then(|source| source.change_request_number),
            Some(42)
        );
        assert_eq!(
            restored
                .source
                .as_ref()
                .and_then(|source| source.push_target.as_ref())
                .map(|target| target.remote_name.as_str()),
            Some("origin")
        );

        let legacy_source = serde_json::json!({
            "kind": "pull_request",
            "url": "https://github.com/acme/widgets/pull/42",
            "provider": "github",
            "remoteName": "origin",
            "headBranch": "feature/review",
            "headSha": "abc123",
            "baseBranch": "main",
            "changeRequestNumber": 42,
            "title": "Improve review flow",
            "author": "octocat"
        });
        repo.conn
            .lock()
            .expect("workspace database lock")
            .execute(
                "UPDATE dcc_workspaces SET source_json = ?2 WHERE id = ?1",
                params![&workspace.id.0, legacy_source.to_string()],
            )
            .expect("write legacy source");
        let restored_legacy = futures::executor::block_on(repo.get_workspace(&workspace.id))
            .expect("read legacy workspace")
            .expect("legacy workspace exists");
        assert!(restored_legacy
            .source
            .as_ref()
            .and_then(|source| source.push_target.as_ref())
            .is_none());
    }

    #[test]
    fn workspace_base_branch_partial_update_preserves_concurrent_metadata() {
        let repo = SqliteWorkspaceRepo::from_connection(in_memory_conn()).expect("create repo");
        let workspace = Workspace {
            id: WorkspaceId("workspace-branch-update".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: Some("Initial task".to_string()),
            root_path: "/tmp/repo".to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some("/tmp/repo/.dcc-worktrees/initial".to_string()),
            source: Some(WorkspaceSource {
                kind: WorkspaceSourceKind::PullRequest,
                url: "https://github.com/acme/widgets/pull/42".to_string(),
                provider: "github".to_string(),
                remote_name: "origin".to_string(),
                head_branch: "feature/initial".to_string(),
                head_sha: "initial-sha".to_string(),
                base_branch: "main".to_string(),
                change_request_number: Some(42),
                title: Some("Initial title".to_string()),
                author: Some("octocat".to_string()),
                source_repository: Some("acme/widgets".to_string()),
                push_target: Some(WorkspacePushTarget {
                    remote_name: "origin".to_string(),
                    branch_name: "feature/initial".to_string(),
                    remote_url: Some("https://github.com/acme/widgets.git".to_string()),
                    remote_created: false,
                }),
            }),
            state: WorkspaceState::Ready,
            setup_report: Some(WorkspaceSetupReport {
                status: WorkspaceSetupStatus::Completed,
                steps: vec![WorkspaceSetupStepReport {
                    label: "Initial setup".to_string(),
                    command: "initial-command".to_string(),
                    source_path: ".dcc.toml".to_string(),
                    status: WorkspaceSetupStatus::Completed,
                    detail: None,
                }],
                message: Some("initial setup".to_string()),
            }),
            pinned_at: Some("2026-01-01T00:00:00Z".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        futures::executor::block_on(repo.save_workspace(&workspace)).expect("save workspace");

        // Simulate a concurrent metadata update after a caller observed the
        // original aggregate but before it persists its new base branch.
        let mut concurrent = workspace.clone();
        concurrent.name = Some("Concurrent task".to_string());
        concurrent.worktree_path = Some("/tmp/repo/.dcc-worktrees/concurrent".to_string());
        concurrent.state = WorkspaceState::Archived;
        concurrent.setup_report = Some(WorkspaceSetupReport {
            status: WorkspaceSetupStatus::Warning,
            steps: vec![WorkspaceSetupStepReport {
                label: "Concurrent setup".to_string(),
                command: "concurrent-command".to_string(),
                source_path: "concurrent.toml".to_string(),
                status: WorkspaceSetupStatus::Warning,
                detail: Some("concurrent detail".to_string()),
            }],
            message: Some("concurrent setup".to_string()),
        });
        concurrent.pinned_at = None;
        concurrent.updated_at = "2026-01-01T00:01:00Z".to_string();
        concurrent.source.as_mut().expect("source").head_branch = "feature/concurrent".to_string();
        futures::executor::block_on(repo.save_workspace(&concurrent))
            .expect("save concurrent metadata");

        repo.update_workspace_base_branch(&workspace.id, "release/2026", "2026-01-01T00:02:00Z")
            .expect("partial base branch update");

        let restored = futures::executor::block_on(repo.get_workspace(&workspace.id))
            .expect("read workspace")
            .expect("workspace exists");
        assert_eq!(restored.base_branch, "release/2026");
        assert_eq!(restored.updated_at, "2026-01-01T00:02:00Z");
        assert_eq!(restored.name.as_deref(), Some("Concurrent task"));
        assert_eq!(
            restored.worktree_path.as_deref(),
            Some("/tmp/repo/.dcc-worktrees/concurrent")
        );
        assert_eq!(restored.state, WorkspaceState::Archived);
        assert_eq!(restored.pinned_at, None);
        assert_eq!(
            restored
                .source
                .as_ref()
                .map(|source| source.head_branch.as_str()),
            Some("feature/concurrent")
        );
        assert_eq!(
            restored
                .setup_report
                .as_ref()
                .and_then(|report| report.message.as_deref()),
            Some("concurrent setup")
        );

        let missing = repo
            .update_workspace_base_branch(
                &WorkspaceId("missing-workspace".to_string()),
                "main",
                "2026-01-01T00:03:00Z",
            )
            .expect_err("missing workspace must fail");
        assert!(matches!(
            missing,
            dcc_core::CoreError::Repository(message) if message == "workspace not found"
        ));
    }

    #[test]
    fn sqlite_workspace_repo_lists_and_updates_forge_bindings() {
        let repo = SqliteWorkspaceRepo::from_connection(in_memory_conn()).expect("create repo");
        let repository = Repository {
            id: RepositoryId("/tmp/repo".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: "repo".to_string(),
            display_name: None,
            icon: None,
            color: None,
            pinned_at: None,
            root_path: "/tmp/repo".to_string(),
            base_branch: "main".to_string(),
            remote: Some("origin".to_string()),
            remote_url: Some("git@github.com:acme/repo.git".to_string()),
            forge_provider: Some("github".to_string()),
            forge_login: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        futures::executor::block_on(repo.save_repository(&repository)).expect("save repository");

        let unbound = repo
            .list_repositories_needing_forge_binding()
            .expect("list unbound repositories");
        assert_eq!(unbound, vec![RepositoryId("/tmp/repo".to_string())]);

        repo.update_repository_forge_login(&RepositoryId("/tmp/repo".to_string()), Some("octocat"))
            .expect("update forge login");
        let bound = repo
            .list_forge_bound_repositories()
            .expect("list bound repositories");
        assert_eq!(bound.len(), 1);
        assert_eq!(bound[0].id.0, "/tmp/repo");
        assert_eq!(bound[0].login, "octocat");

        let mut repository_without_login = repository.clone();
        repository_without_login.forge_login = None;
        futures::executor::block_on(repo.save_repository(&repository_without_login))
            .expect("save repository snapshot without login");
        let preserved = futures::executor::block_on(
            repo.get_repository(&RepositoryId("/tmp/repo".to_string())),
        )
        .expect("read preserved forge login")
        .expect("repository exists after snapshot update");
        assert_eq!(preserved.forge_login.as_deref(), Some("octocat"));

        repo.update_repository_forge_login(&RepositoryId("/tmp/repo".to_string()), None)
            .expect("clear forge login explicitly");
        let cleared = futures::executor::block_on(
            repo.get_repository(&RepositoryId("/tmp/repo".to_string())),
        )
        .expect("read cleared forge login")
        .expect("repository exists after explicit logout");
        assert_eq!(cleared.forge_login, None);
    }

    #[test]
    fn sqlite_workspace_repo_roundtrips_setup_report() {
        let repo = SqliteWorkspaceRepo::from_connection(in_memory_conn()).expect("create repo");
        let workspace = Workspace {
            id: WorkspaceId("workspace-setup".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: Some("Workspace".to_string()),
            root_path: "/tmp/repo".to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some("/tmp/repo/.dcc-worktrees/main".to_string()),
            source: None,
            state: WorkspaceState::SetupPending,
            setup_report: Some(WorkspaceSetupReport {
                status: WorkspaceSetupStatus::Warning,
                steps: vec![WorkspaceSetupStepReport {
                    label: "Install dependencies".to_string(),
                    command: "pnpm install".to_string(),
                    source_path: "/tmp/repo/package.json".to_string(),
                    status: WorkspaceSetupStatus::Warning,
                    detail: Some("pnpm: command not found".to_string()),
                }],
                message: Some("Workspace was created, but setup needs attention.".to_string()),
            }),
            pinned_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        futures::executor::block_on(repo.save_workspace(&workspace)).expect("save workspace");
        let workspaces =
            futures::executor::block_on(repo.list_workspaces()).expect("list workspaces");

        assert_eq!(workspaces.len(), 1);
        let restored = &workspaces[0];
        assert_eq!(restored.state, WorkspaceState::SetupPending);
        let setup_report = restored
            .setup_report
            .as_ref()
            .expect("workspace setup report should persist");
        assert_eq!(setup_report.status, WorkspaceSetupStatus::Warning);
        assert_eq!(setup_report.steps.len(), 1);
        assert_eq!(setup_report.steps[0].command, "pnpm install");
        assert_eq!(
            setup_report.steps[0].detail.as_deref(),
            Some("pnpm: command not found")
        );
    }

    #[test]
    fn sqlite_workspace_repo_roundtrips_workspace_bundle_and_members() {
        let repo = SqliteWorkspaceRepo::from_connection(in_memory_conn()).expect("create repo");
        let workspaces = [
            Workspace {
                id: WorkspaceId("workspace-backend".to_string()),
                project_id: ProjectId("backend".to_string()),
                name: Some("Backend".to_string()),
                root_path: "/tmp/backend".to_string(),
                base_branch: "main".to_string(),
                worktree_path: Some("/tmp/backend/.dcc-worktrees/main-a".to_string()),
                source: None,
                state: WorkspaceState::SetupPending,
                setup_report: None,
                pinned_at: Some("2026-01-01T00:00:30Z".to_string()),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            },
            Workspace {
                id: WorkspaceId("workspace-frontend".to_string()),
                project_id: ProjectId("frontend".to_string()),
                name: Some("Frontend".to_string()),
                root_path: "/tmp/frontend".to_string(),
                base_branch: "main".to_string(),
                worktree_path: Some("/tmp/frontend/.dcc-worktrees/main-b".to_string()),
                source: None,
                state: WorkspaceState::Ready,
                setup_report: None,
                pinned_at: Some("2026-01-01T00:00:30Z".to_string()),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            },
        ];
        for workspace in &workspaces {
            futures::executor::block_on(repo.save_workspace(workspace)).expect("save workspace");
        }

        let bundle = WorkspaceBundle {
            id: WorkspaceBundleId("bundle-1".to_string()),
            name: "Checkout flow".to_string(),
            primary_workspace_id: workspaces[0].id.clone(),
            state: WorkspaceBundleState::Ready,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let members = workspaces
            .iter()
            .enumerate()
            .map(|(position, workspace)| WorkspaceBundleMember {
                bundle_id: bundle.id.clone(),
                workspace_id: workspace.id.clone(),
                created_for_bundle: true,
                position: position as u32,
            })
            .collect::<Vec<_>>();
        futures::executor::block_on(repo.save_workspace_bundle(&bundle, &members))
            .expect("save workspace bundle");

        let restored = futures::executor::block_on(repo.get_workspace_bundle(&bundle.id))
            .expect("get workspace bundle")
            .expect("workspace bundle exists");
        assert_eq!(restored.bundle.name, "Checkout flow");
        assert_eq!(restored.members.len(), 2);
        assert_eq!(restored.members[0].workspace_id, workspaces[0].id);
        assert_eq!(restored.members[1].workspace_id, workspaces[1].id);
        let found_by_member =
            futures::executor::block_on(repo.get_workspace_bundle_for_workspace(&workspaces[1].id))
                .expect("get workspace bundle by member")
                .expect("workspace bundle exists for member");
        assert_eq!(found_by_member.bundle.id, bundle.id);

        let listed = futures::executor::block_on(repo.list_workspace_bundles())
            .expect("list workspace bundles");
        assert_eq!(listed.len(), 1);

        let archived = futures::executor::block_on(repo.set_workspace_bundle_state(
            &bundle.id,
            WorkspaceBundleState::Archived,
            "2026-01-01T00:01:00Z".to_string(),
        ))
        .expect("archive workspace bundle")
        .expect("workspace bundle exists");
        assert_eq!(archived.bundle.state, WorkspaceBundleState::Archived);
        let archived_workspaces =
            futures::executor::block_on(repo.list_workspaces()).expect("list workspaces");
        assert!(archived_workspaces
            .iter()
            .all(|workspace| workspace.state == WorkspaceState::Archived));
        assert!(archived_workspaces
            .iter()
            .all(|workspace| workspace.pinned_at.is_none()));

        let ready = futures::executor::block_on(repo.set_workspace_bundle_state(
            &bundle.id,
            WorkspaceBundleState::Ready,
            "2026-01-01T00:02:00Z".to_string(),
        ))
        .expect("restore workspace bundle")
        .expect("workspace bundle exists");
        assert_eq!(ready.bundle.state, WorkspaceBundleState::Ready);
        let restored_workspaces =
            futures::executor::block_on(repo.list_workspaces()).expect("list restored workspaces");
        let restored_states = restored_workspaces
            .into_iter()
            .map(|workspace| (workspace.id.0, workspace.state))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            restored_states.get("workspace-backend"),
            Some(&WorkspaceState::SetupPending)
        );
        assert_eq!(
            restored_states.get("workspace-frontend"),
            Some(&WorkspaceState::Ready)
        );

        let completed = futures::executor::block_on(repo.set_workspace_bundle_state(
            &bundle.id,
            WorkspaceBundleState::Completed,
            "2026-01-01T00:03:00Z".to_string(),
        ))
        .expect("complete workspace bundle")
        .expect("workspace bundle exists");
        assert_eq!(completed.bundle.state, WorkspaceBundleState::Completed);
        let completed_workspaces =
            futures::executor::block_on(repo.list_workspaces()).expect("list workspaces");
        assert!(completed_workspaces
            .iter()
            .all(|workspace| workspace.state == WorkspaceState::Completed));

        futures::executor::block_on(repo.delete_workspace(&workspaces[0].id))
            .expect("delete primary workspace");
        let deleted = futures::executor::block_on(repo.get_workspace_bundle(&bundle.id))
            .expect("get deleted workspace bundle");
        assert!(deleted.is_none());
    }

    #[test]
    fn guarded_undo_phase_zero_roundtrip_lifecycle_and_cascades() {
        let conn = in_memory_conn();
        let repo = SqliteSessionRepo::from_connection(conn.clone()).unwrap();
        seed_guarded_undo_parents(&conn.lock().unwrap(), "one");
        let mut collecting = collecting_restore_set("one");
        collecting.root_id = None;
        collecting.git_identity = None;
        repo.create_turn_restore_set(&collecting).unwrap();
        assert_eq!(
            repo.get_turn_restore_set(&collecting.restore_set_id)
                .unwrap()
                .unwrap(),
            (collecting.clone(), Vec::new())
        );

        let restore_file = eligible_restore_file("one");
        let mut eligible = collecting.clone();
        eligible.root_id = Some(PhysicalRootId(vec![1, 1, 7, 9]));
        eligible.git_identity = Some(guarded_git_identity());
        eligible.state = RestoreSetState::Eligible;
        eligible.file_count = 1;
        eligible.artifact_bytes = restore_file.pre_size;
        eligible.manifest_digest =
            Some(canonical_restore_manifest_digest(std::slice::from_ref(&restore_file)).unwrap());
        eligible.completed_at = Some("t2".to_owned());
        eligible.expires_at = Some("t9".to_owned());
        assert!(repo
            .finalize_turn_restore_set(&eligible, std::slice::from_ref(&restore_file))
            .unwrap());
        assert_eq!(
            repo.get_turn_restore_set(&eligible.restore_set_id)
                .unwrap()
                .unwrap()
                .1[0]
                .path_bytes,
            restore_file.path_bytes,
            "SQLite BLOB must round-trip non-UTF-8 path bytes exactly"
        );
        assert!(!repo
            .finalize_turn_restore_set(&eligible, std::slice::from_ref(&restore_file))
            .unwrap());
        let mut conflicting = eligible.clone();
        conflicting.expires_at = Some("different".to_owned());
        assert!(repo
            .finalize_turn_restore_set(&conflicting, std::slice::from_ref(&restore_file))
            .is_err());

        let operation = UndoOperation {
            operation_id: UndoOperationId("operation-one".to_owned()),
            restore_set_id: eligible.restore_set_id.clone(),
            journal_version: UNDO_JOURNAL_SCHEMA_VERSION,
            state: UndoOperationState::Preparing,
            active: true,
            preview_token_digest: Some(Sha256Digest([0x44; 32])),
            prepared_identity: PreparedIdentityV1 {
                schema_version: dcc_core::domain::guarded_undo::PREPARED_IDENTITY_SCHEMA_VERSION,
                root_id: eligible.root_id.clone().unwrap(),
                git: eligible.git_identity.clone().unwrap(),
                manifest_digest: eligible.manifest_digest.unwrap(),
                coordinator_generation: 7,
                git_dir_generation: 8,
                common_dir_generation: 9,
            },
            reason_code: None,
            recovery_details: None,
            created_at: "t3".to_owned(),
            updated_at: "t3".to_owned(),
            completed_at: None,
        };
        let mut operation_file = UndoOperationFile {
            operation_id: operation.operation_id.clone(),
            restore_set_id: operation.restore_set_id.clone(),
            ordinal: 0,
            path_bytes: restore_file.path_bytes.clone(),
            exchange_artifact_key: ArtifactKey([0x55; 16]),
            expected_result_size: restore_file.result_size,
            expected_result_sha256: restore_file.result_sha256,
            expected_metadata: restore_file.metadata_fingerprint.clone(),
            pre_size: restore_file.pre_size,
            pre_sha256: restore_file.pre_sha256,
            staged_metadata: None,
            displaced_size: None,
            displaced_sha256: None,
            displaced_metadata: None,
            state: UndoOperationFileState::Planned,
            verification_outcome: VerificationOutcome::Pending,
            recovery_details: None,
            updated_at: "t3".to_owned(),
        };
        repo.create_undo_operation(&operation, std::slice::from_ref(&operation_file))
            .unwrap();
        let clone_journal_row = |restore_set_sql: &str, ordinal: i64| {
            conn.lock().unwrap().execute(
                &format!(
                    r#"INSERT INTO dcc_undo_operation_files (
                        operation_id, restore_set_id, ordinal, path_bytes,
                        exchange_artifact_key, expected_result_size,
                        expected_result_sha256, expected_metadata_json, pre_size,
                        pre_sha256, displaced_size, displaced_sha256,
                        displaced_metadata_json, state, verification_outcome,
                        recovery_details_json, updated_at
                    ) SELECT operation_id, {restore_set_sql}, ?1, path_bytes,
                             exchange_artifact_key, expected_result_size,
                             expected_result_sha256, expected_metadata_json,
                             pre_size, pre_sha256, displaced_size, displaced_sha256,
                             displaced_metadata_json, state, verification_outcome,
                             recovery_details_json, updated_at
                        FROM dcc_undo_operation_files
                       WHERE operation_id = 'operation-one' AND ordinal = 0"#
                ),
                params![ordinal],
            )
        };
        assert!(
            clone_journal_row("restore_set_id", 99).is_err(),
            "journal child without a source restore_file must violate its composite FK"
        );
        assert!(
            clone_journal_row("'restore-mismatch'", 98).is_err(),
            "journal child with a mismatched restore_set must violate its parent composite FK"
        );
        assert!(conn
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM dcc_undo_operations WHERE operation_id = 'operation-one'",
                []
            )
            .is_err());
        assert!(conn
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM dcc_undo_operation_files WHERE operation_id = 'operation-one'",
                [],
            )
            .is_err());
        assert!(conn
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM dcc_turn_restore_files WHERE restore_set_id = 'restore-one'",
                [],
            )
            .is_err());
        operation_file.staged_metadata = Some(guarded_metadata());
        operation_file.state = UndoOperationFileState::Staged;
        operation_file.updated_at = "t4".to_owned();
        assert!(repo
            .transition_undo_operation_file(&UndoOperationFileState::Planned, &operation_file,)
            .unwrap());
        assert!(repo
            .transition_undo_operation(
                &operation.operation_id,
                &UndoOperationState::Preparing,
                &UndoOperationState::Prepared,
                None,
                None,
                "t4"
            )
            .unwrap());
        assert!(!repo
            .transition_undo_operation(
                &operation.operation_id,
                &UndoOperationState::Prepared,
                &UndoOperationState::Prepared,
                None,
                None,
                "ignored"
            )
            .unwrap());
        let mut tampered_metadata = restore_file.metadata_fingerprint.clone();
        tampered_metadata
            .fields
            .insert("mode".to_owned(), b"100755".to_vec());
        conn.lock()
            .unwrap()
            .execute(
                r#"UPDATE dcc_undo_operation_files
                      SET path_bytes = ?1, expected_result_sha256 = ?2,
                          expected_metadata_json = ?3
                    WHERE operation_id = 'operation-one' AND ordinal = 0"#,
                params![
                    OpaqueRepoPath::unix(b"src/tampered.rs").unwrap().0,
                    vec![0x99_u8; 32],
                    serde_json::to_string(&tampered_metadata).unwrap(),
                ],
            )
            .unwrap();
        assert!(repo
            .transition_undo_operation(
                &operation.operation_id,
                &UndoOperationState::Prepared,
                &UndoOperationState::Prepared,
                None,
                None,
                "tampered-idempotent-replay",
            )
            .is_err());
        conn.lock()
            .unwrap()
            .execute(
                r#"UPDATE dcc_undo_operation_files
                      SET path_bytes = ?1, expected_result_sha256 = ?2,
                          expected_metadata_json = ?3
                    WHERE operation_id = 'operation-one' AND ordinal = 0"#,
                params![
                    restore_file.path_bytes.0.clone(),
                    restore_file.result_sha256.0.to_vec(),
                    serde_json::to_string(&restore_file.metadata_fingerprint).unwrap(),
                ],
            )
            .unwrap();
        conn.lock()
            .unwrap()
            .execute(
                "UPDATE dcc_undo_operation_files SET state = 'future_state' WHERE operation_id = 'operation-one' AND ordinal = 0",
                [],
            )
            .unwrap();
        assert!(repo
            .transition_undo_operation(
                &operation.operation_id,
                &UndoOperationState::Prepared,
                &UndoOperationState::Applying,
                None,
                None,
                "unknown-child-state",
            )
            .is_err());
        conn.lock()
            .unwrap()
            .execute(
                "UPDATE dcc_undo_operation_files SET state = 'staged' WHERE operation_id = 'operation-one' AND ordinal = 0",
                [],
            )
            .unwrap();
        conn.lock()
            .unwrap()
            .execute(
                "UPDATE dcc_undo_operation_files SET verification_outcome = 'future_outcome' WHERE operation_id = 'operation-one' AND ordinal = 0",
                [],
            )
            .unwrap();
        assert!(repo
            .transition_undo_operation(
                &operation.operation_id,
                &UndoOperationState::Prepared,
                &UndoOperationState::Applying,
                None,
                None,
                "unknown-child-outcome",
            )
            .is_err());
        conn.lock()
            .unwrap()
            .execute(
                "UPDATE dcc_undo_operation_files SET verification_outcome = 'pending' WHERE operation_id = 'operation-one' AND ordinal = 0",
                [],
            )
            .unwrap();
        repo.transition_undo_operation(
            &operation.operation_id,
            &UndoOperationState::Prepared,
            &UndoOperationState::Applying,
            None,
            None,
            "t5",
        )
        .unwrap();

        operation_file.state = UndoOperationFileState::Applied;
        operation_file.displaced_size = Some(restore_file.result_size);
        operation_file.displaced_sha256 = Some(restore_file.result_sha256);
        operation_file.displaced_metadata = Some(restore_file.metadata_fingerprint.clone());
        operation_file.updated_at = "t6".to_owned();
        repo.transition_undo_operation_file(&UndoOperationFileState::Staged, &operation_file)
            .unwrap();
        operation_file.state = UndoOperationFileState::Verified;
        operation_file.verification_outcome = VerificationOutcome::Verified;
        operation_file.updated_at = "t7".to_owned();
        repo.transition_undo_operation_file(&UndoOperationFileState::Applied, &operation_file)
            .unwrap();
        repo.transition_undo_operation(
            &operation.operation_id,
            &UndoOperationState::Applying,
            &UndoOperationState::Verifying,
            None,
            None,
            "t8",
        )
        .unwrap();
        repo.transition_undo_operation(
            &operation.operation_id,
            &UndoOperationState::Verifying,
            &UndoOperationState::Completed,
            None,
            None,
            "t9",
        )
        .unwrap();
        let completed = repo
            .get_undo_operation(&operation.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(completed.0.state, UndoOperationState::Completed);
        assert!(!completed.0.active);
        assert_eq!(
            repo.get_turn_restore_set(&eligible.restore_set_id)
                .unwrap()
                .unwrap()
                .0
                .state,
            RestoreSetState::Consumed
        );

        let locked = conn.lock().unwrap();
        locked
            .execute(
                "DELETE FROM dcc_undo_operations WHERE operation_id = 'operation-one'",
                [],
            )
            .unwrap();
        assert_eq!(locked.query_row("SELECT COUNT(*) FROM dcc_undo_operation_files WHERE operation_id = 'operation-one'", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
        locked
            .execute(
                "DELETE FROM dcc_turn_restore_sets WHERE restore_set_id = 'restore-one'",
                [],
            )
            .unwrap();
        assert_eq!(locked.query_row("SELECT COUNT(*) FROM dcc_turn_restore_files WHERE restore_set_id = 'restore-one'", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
    }

    #[test]
    fn guarded_undo_unknown_values_preserve_rows_and_corrupt_schema_fails_closed() {
        let conn = in_memory_conn();
        let repo = SqliteSessionRepo::from_connection(conn.clone()).unwrap();
        seed_guarded_undo_parents(&conn.lock().unwrap(), "future");
        let collecting = collecting_restore_set("future");
        repo.create_turn_restore_set(&collecting).unwrap();
        conn.lock()
            .unwrap()
            .execute(
                "UPDATE dcc_turn_restore_sets SET state = 'future_state' WHERE restore_set_id = ?1",
                params![collecting.restore_set_id.0],
            )
            .unwrap();
        assert!(repo
            .get_turn_restore_set(&collecting.restore_set_id)
            .is_err());
        assert!(repo
            .finalize_turn_restore_set(
                &TurnRestoreSet {
                    state: RestoreSetState::Failed,
                    reason_code: Some(GuardedUndoReasonCode::InvalidPersistedRecord),
                    completed_at: Some("t2".to_owned()),
                    ..collecting.clone()
                },
                &[]
            )
            .is_err());

        let mut corrupt = guarded_git_identity();
        corrupt.schema_version = 99;
        conn.lock().unwrap().execute(
            "UPDATE dcc_turn_restore_sets SET state = 'collecting', git_identity_json = ?1 WHERE restore_set_id = ?2",
            params![serde_json::to_string(&corrupt).unwrap(), collecting.restore_set_id.0],
        ).unwrap();
        assert!(repo
            .get_turn_restore_set(&collecting.restore_set_id)
            .is_err());
    }

    #[test]
    fn guarded_undo_finalization_idempotency_is_independent_of_input_order() {
        let conn = in_memory_conn();
        let repo = SqliteSessionRepo::from_connection(conn.clone()).unwrap();
        seed_guarded_undo_parents(&conn.lock().unwrap(), "order");
        let collecting = collecting_restore_set("order");
        repo.create_turn_restore_set(&collecting).unwrap();
        let mut first = eligible_restore_file("order");
        first.path_bytes = OpaqueRepoPath::unix(b"a.txt").unwrap();
        let mut second = first.clone();
        second.ordinal = 1;
        second.path_bytes = OpaqueRepoPath::unix(b"z.txt").unwrap();
        second.pre_artifact_key = ArtifactKey([0x23; 16]);
        let reversed = vec![second, first];
        let mut eligible = collecting;
        eligible.state = RestoreSetState::Eligible;
        eligible.file_count = 2;
        eligible.artifact_bytes = reversed.iter().map(|file| file.pre_size).sum();
        eligible.manifest_digest = Some(canonical_restore_manifest_digest(&reversed).unwrap());
        eligible.completed_at = Some("t2".to_owned());
        eligible.expires_at = Some("t9".to_owned());
        assert!(repo
            .finalize_turn_restore_set(&eligible, &reversed)
            .unwrap());
        assert!(!repo
            .finalize_turn_restore_set(&eligible, &reversed)
            .unwrap());
    }

    #[test]
    fn guarded_undo_schema_migrates_an_existing_m3_database() {
        let conn = in_memory_conn();
        {
            let locked = conn.lock().unwrap();
            locked.execute_batch(&format!("PRAGMA foreign_keys = ON;\n{WORKSPACE_TABLE_SQL}\n{SESSION_TABLE_SQL}\n{TURN_CHANGE_SET_TABLE_SQL}")).unwrap();
            assert_eq!(
                locked
                    .query_row(
                        "SELECT COUNT(*) FROM sqlite_master WHERE name = 'dcc_turn_restore_sets'",
                        [],
                        |row| row.get::<_, i64>(0)
                    )
                    .unwrap(),
                0
            );
        }
        SqliteSessionRepo::from_connection(conn.clone()).unwrap();
        let locked = conn.lock().unwrap();
        for table in [
            "dcc_turn_restore_sets",
            "dcc_turn_restore_files",
            "dcc_undo_operations",
            "dcc_undo_operation_files",
        ] {
            assert_eq!(
                locked
                    .query_row(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                        params![table],
                        |row| row.get::<_, i64>(0)
                    )
                    .unwrap(),
                1,
                "missing migrated table {table}"
            );
        }
    }

    #[test]
    fn phase1b_recovery_is_authorized_cas_and_idempotent() {
        let conn = in_memory_conn();
        let repo = SqliteSessionRepo::from_connection(conn.clone()).unwrap();
        seed_guarded_undo_parents(&conn.lock().unwrap(), "recover");
        let collecting = collecting_restore_set("recover");
        repo.create_turn_restore_set(&collecting).unwrap();
        let authority = MaintenanceAuthority::new();

        assert_eq!(
            repo.recover_interrupted_restore_sets(&authority, "t2")
                .unwrap(),
            vec!["restore-recover".to_owned()]
        );
        assert!(repo
            .recover_interrupted_restore_sets(&authority, "t3")
            .unwrap()
            .is_empty());
        let (set, files) = repo
            .get_turn_restore_set(&collecting.restore_set_id)
            .unwrap()
            .unwrap();
        assert_eq!(set.state, RestoreSetState::Failed);
        assert_eq!(
            set.reason_code,
            Some(GuardedUndoReasonCode::CaptureInterrupted)
        );
        assert_eq!(set.completed_at.as_deref(), Some("t2"));
        assert!(files.is_empty());
    }

    #[test]
    fn phase1b_expiry_preserves_audit_rows_and_rejects_active_journal() {
        let conn = in_memory_conn();
        let repo = SqliteSessionRepo::from_connection(conn.clone()).unwrap();
        let (eligible, file) = persist_eligible_restore_set(&repo, &conn, "expire", "t2");
        let authority = MaintenanceAuthority::new();
        let before = repo
            .get_turn_restore_set(&eligible.restore_set_id)
            .unwrap()
            .unwrap();
        conn.lock()
            .unwrap()
            .execute(
                r#"INSERT INTO dcc_undo_operations (
                    operation_id, restore_set_id, journal_version, state, active,
                    prepared_identity_json, created_at, updated_at
                ) VALUES ('op-expire', ?1, 1, 'preparing', 1, '{}', 't3', 't3')"#,
                params![eligible.restore_set_id.0],
            )
            .unwrap();
        assert!(repo
            .expire_eligible_restore_set(&authority, &eligible.restore_set_id, "t4")
            .is_err());
        conn.lock()
            .unwrap()
            .execute(
                "UPDATE dcc_undo_operations SET state = 'completed', active = 0 WHERE operation_id = 'op-expire'",
                [],
            )
            .unwrap();
        assert!(repo
            .expire_eligible_restore_set(&authority, &eligible.restore_set_id, "t4")
            .unwrap());
        assert!(!repo
            .expire_eligible_restore_set(&authority, &eligible.restore_set_id, "t5")
            .unwrap());
        let (expired, files) = repo
            .get_turn_restore_set(&eligible.restore_set_id)
            .unwrap()
            .unwrap();
        assert_eq!(expired.state, RestoreSetState::Expired);
        assert_eq!(
            expired.reason_code,
            Some(GuardedUndoReasonCode::RetentionExpired)
        );
        assert_eq!(expired.artifact_bytes, before.0.artifact_bytes);
        assert_eq!(expired.file_count, before.0.file_count);
        assert_eq!(expired.manifest_digest, before.0.manifest_digest);
        assert_eq!(files, vec![file]);
        assert_eq!(
            conn.lock()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM dcc_turn_restore_files WHERE restore_set_id = ?1",
                    params![eligible.restore_set_id.0],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
    }

    #[test]
    fn phase1b_integrity_failure_is_reason_restricted_and_keeps_manifest() {
        let conn = in_memory_conn();
        let repo = SqliteSessionRepo::from_connection(conn.clone()).unwrap();
        let (eligible, file) = persist_eligible_restore_set(&repo, &conn, "integrity", "t2");
        let authority = MaintenanceAuthority::new();
        assert!(repo
            .fail_eligible_restore_set_integrity(
                &authority,
                &eligible.restore_set_id,
                &GuardedUndoReasonCode::HeadChanged,
                "t3",
            )
            .is_err());
        assert!(repo
            .fail_eligible_restore_set_integrity(
                &authority,
                &eligible.restore_set_id,
                &GuardedUndoReasonCode::ArtifactCorrupt,
                "t4",
            )
            .unwrap());
        assert!(!repo
            .fail_eligible_restore_set_integrity(
                &authority,
                &eligible.restore_set_id,
                &GuardedUndoReasonCode::ArtifactCorrupt,
                "t5",
            )
            .unwrap());
        let (failed, files) = repo
            .get_turn_restore_set(&eligible.restore_set_id)
            .unwrap()
            .unwrap();
        assert_eq!(failed.state, RestoreSetState::Failed);
        assert_eq!(
            failed.reason_code,
            Some(GuardedUndoReasonCode::ArtifactCorrupt)
        );
        assert_eq!(files, vec![file]);
    }

    #[test]
    fn phase1b_retention_candidates_are_ordered_and_bounded() {
        let conn = in_memory_conn();
        let repo = SqliteSessionRepo::from_connection(conn.clone()).unwrap();
        let (first, _) = persist_eligible_restore_set(&repo, &conn, "candidate-a", "t1");
        let (second, _) = persist_eligible_restore_set(&repo, &conn, "candidate-b", "t1");
        let (third, _) = persist_eligible_restore_set(&repo, &conn, "candidate-c", "t2");
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO dcc_workspaces (id, project_id, root_path, base_branch, state, created_at, updated_at) VALUES ('workspace-retention', 'project', '/tmp/retention', 'main', 'ready', 't0', 't0')",
                [],
            )
            .unwrap();
        conn.lock()
            .unwrap()
            .execute(
                "UPDATE dcc_turn_restore_sets SET workspace_id = 'workspace-retention' WHERE restore_set_id IN ('restore-candidate-a', 'restore-candidate-b', 'restore-candidate-c')",
                [],
            )
            .unwrap();
        let authority = MaintenanceAuthority::new();
        let candidates = repo
            .list_retention_candidates(
                &authority,
                &WorkspaceId("workspace-retention".to_owned()),
                2,
                8,
            )
            .unwrap();
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.restore_set_id.0.as_str())
                .collect::<Vec<_>>(),
            vec![
                first.restore_set_id.0.as_str(),
                second.restore_set_id.0.as_str()
            ]
        );
        let byte_bounded = repo
            .list_retention_candidates(
                &authority,
                &WorkspaceId("workspace-retention".to_owned()),
                20,
                7,
            )
            .unwrap();
        assert_eq!(byte_bounded.len(), 1);
        assert_eq!(byte_bounded[0].artifact_bytes, 4);
        assert_eq!(third.artifact_bytes, 4);
    }

    #[test]
    fn phase1b_artifact_references_are_opaque_and_deterministic() {
        let conn = in_memory_conn();
        let repo = SqliteSessionRepo::from_connection(conn.clone()).unwrap();
        let (first, first_file) = persist_eligible_restore_set(&repo, &conn, "refs-a", "t1");
        let (second, second_file) = persist_eligible_restore_set(&repo, &conn, "refs-b", "t1");
        let authority = MaintenanceAuthority::new();
        let keys = repo
            .list_referenced_artifact_keys(
                &authority,
                &[second.restore_set_id.clone(), first.restore_set_id.clone()],
            )
            .unwrap();
        assert_eq!(
            keys,
            vec![second_file.pre_artifact_key, first_file.pre_artifact_key]
        );
        assert!(repo
            .list_referenced_artifact_keys(&authority, &[RestoreSetId("missing".to_owned())],)
            .is_err());
    }

    fn event_session(repo: &SqliteSessionRepo, session_id: &str) {
        futures::executor::block_on(repo.save_session(&Session {
            id: SessionId(session_id.to_owned()),
            project_id: ProjectId("project-events".to_owned()),
            workspace_id: WorkspaceId("workspace-events".to_owned()),
            additional_workspace_ids: Vec::new(),
            provider_id: "fixture".to_owned(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "t0".to_owned(),
            updated_at: "t0".to_owned(),
        }))
        .expect("persist event test session");
    }

    fn event_record(
        event_id: &str,
        session_id: &str,
        sequence: u64,
        kind: SessionEventKind,
    ) -> SessionEventRecord {
        SessionEventRecord {
            event_id: event_id.to_owned(),
            session_id: SessionId(session_id.to_owned()),
            sequence,
            occurred_at: "2026-01-01T00:00:00Z".to_owned(),
            kind,
        }
    }

    #[test]
    fn terminal_precheck_returns_none_completed_and_aborted_canonical_events() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).expect("create repo");
        event_session(&repo, "terminal-precheck-session");
        let session_id = SessionId("terminal-precheck-session".to_owned());
        let missing_turn = TurnId("missing-turn".to_owned());
        assert!(
            futures::executor::block_on(repo.find_terminal_event(&session_id, &missing_turn))
                .expect("query missing terminal")
                .is_none()
        );

        let completed = event_record(
            "precheck-completed",
            &session_id.0,
            99,
            SessionEventKind::TurnCompleted {
                turn_id: TurnId("completed-turn".to_owned()),
            },
        );
        let aborted = event_record(
            "precheck-aborted",
            &session_id.0,
            99,
            SessionEventKind::TurnAborted {
                turn_id: TurnId("aborted-turn".to_owned()),
                reason: Some("test-only reason".to_owned()),
            },
        );
        futures::executor::block_on(repo.append_event(&completed)).expect("append completed");
        futures::executor::block_on(repo.append_event(&aborted)).expect("append aborted");

        let completed_result = futures::executor::block_on(
            repo.find_terminal_event(&session_id, &TurnId("completed-turn".to_owned())),
        )
        .expect("find completed")
        .expect("completed exists");
        assert_eq!(completed_result.event_id, "precheck-completed");
        assert!(matches!(
            completed_result.kind,
            SessionEventKind::TurnCompleted { .. }
        ));

        let aborted_result = futures::executor::block_on(
            repo.find_terminal_event(&session_id, &TurnId("aborted-turn".to_owned())),
        )
        .expect("find aborted")
        .expect("aborted exists");
        assert_eq!(aborted_result.event_id, "precheck-aborted");
        assert!(matches!(
            aborted_result.kind,
            SessionEventKind::TurnAborted { .. }
        ));

        let conn = repo.conn.lock().expect("lock database");
        let plan = conn
            .prepare(
                "EXPLAIN QUERY PLAN SELECT event_id FROM dcc_session_events WHERE session_id = ?1 AND terminal_turn_id = ?2 LIMIT 2",
            )
            .and_then(|mut statement| {
                statement
                    .query_map(params![session_id.0, "completed-turn"], |row| {
                        row.get::<_, String>(3)
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .expect("explain terminal lookup");
        assert!(plan
            .iter()
            .any(|detail| detail.contains("idx_dcc_session_events_terminal_turn")));
    }

    #[test]
    fn terminal_precheck_rejects_corrupt_or_nonterminal_metadata() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).expect("create repo");
        event_session(&repo, "terminal-corrupt-session");
        let session_id = SessionId("terminal-corrupt-session".to_owned());
        let completed = event_record(
            "corrupt-terminal",
            &session_id.0,
            1,
            SessionEventKind::TurnCompleted {
                turn_id: TurnId("corrupt-turn".to_owned()),
            },
        );
        futures::executor::block_on(repo.append_event(&completed)).expect("append completed");
        repo.conn
            .lock()
            .expect("lock database")
            .execute(
                "UPDATE dcc_session_events SET terminal_kind = 'aborted' WHERE event_id = ?1",
                params![completed.event_id],
            )
            .expect("corrupt metadata");
        assert!(futures::executor::block_on(
            repo.find_terminal_event(&session_id, &TurnId("corrupt-turn".to_owned()),)
        )
        .expect_err("corrupt terminal metadata must fail")
        .to_string()
        .contains("terminal metadata is inconsistent"));

        let nonterminal_kind = to_string(&SessionEventKind::TurnDelta {
            turn_id: TurnId("nonterminal-turn".to_owned()),
            content: "delta".to_owned(),
        })
        .expect("serialize nonterminal");
        repo.conn
            .lock()
            .expect("lock database")
            .execute(
                r#"INSERT INTO dcc_session_events
                   (event_id, session_id, sequence, occurred_at, kind_json,
                    terminal_turn_id, terminal_kind)
                   VALUES ('nonterminal-corrupt', ?1, 2, 't2', ?2, 'nonterminal-turn', 'completed')"#,
                params![session_id.0, nonterminal_kind],
            )
            .expect("insert nonterminal metadata corruption");
        assert!(futures::executor::block_on(repo.find_terminal_event(
            &SessionId("terminal-corrupt-session".to_owned()),
            &TurnId("nonterminal-turn".to_owned()),
        ))
        .is_err());
    }

    #[test]
    fn append_event_returns_canonical_record_for_duplicate_event_id() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).expect("create repo");
        event_session(&repo, "event-id-session");
        let first = event_record(
            "same-event-id",
            "event-id-session",
            99,
            SessionEventKind::TurnDelta {
                turn_id: TurnId("turn-1".to_owned()),
                content: "first".to_owned(),
            },
        );
        let mismatched = event_record(
            "same-event-id",
            "event-id-session",
            1,
            SessionEventKind::TurnDelta {
                turn_id: TurnId("turn-1".to_owned()),
                content: "different retry payload".to_owned(),
            },
        );
        let second = event_record(
            "same-event-id",
            "event-id-session",
            1,
            SessionEventKind::TurnDelta {
                turn_id: TurnId("turn-1".to_owned()),
                content: "first".to_owned(),
            },
        );
        let inserted = futures::executor::block_on(repo.append_event(&first)).expect("insert");
        let mismatch = futures::executor::block_on(repo.append_event(&mismatched))
            .expect_err("semantic mismatch must be rejected");
        assert!(mismatch
            .to_string()
            .contains("event identity conflicts with existing event"));
        let existing = futures::executor::block_on(repo.append_event(&second)).expect("retry");
        assert!(matches!(inserted, AppendEventOutcome::Inserted(ref event) if event.sequence == 1));
        match existing {
            AppendEventOutcome::Existing(event) => {
                assert_eq!(event.event_id, "same-event-id");
                assert_eq!(event.sequence, 1);
                assert!(
                    matches!(event.kind, SessionEventKind::TurnDelta { content, .. } if content == "first")
                );
            }
            AppendEventOutcome::Inserted(_) => panic!("retry must return existing canonical event"),
        }
    }

    #[test]
    fn concurrent_terminal_appends_have_one_durable_winner() {
        let directory = tempfile::tempdir().expect("temporary database directory");
        let path = directory.path().join("events.sqlite");
        let first_repo = SqliteSessionRepo::open(&path).expect("open first repo");
        event_session(&first_repo, "terminal-race-session");
        let second_repo = SqliteSessionRepo::open(&path).expect("open second repo");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let completed = event_record(
            "completed-race",
            "terminal-race-session",
            10,
            SessionEventKind::TurnCompleted {
                turn_id: TurnId("terminal-race-turn".to_owned()),
            },
        );
        let aborted = event_record(
            "aborted-race",
            "terminal-race-session",
            10,
            SessionEventKind::TurnAborted {
                turn_id: TurnId("terminal-race-turn".to_owned()),
                reason: Some("race".to_owned()),
            },
        );
        let first_barrier = barrier.clone();
        let first_repo_for_thread = first_repo.clone();
        let first_thread = std::thread::spawn(move || {
            first_barrier.wait();
            futures::executor::block_on(first_repo_for_thread.append_event(&completed))
                .expect("complete")
        });
        let second_barrier = barrier;
        let second_thread = std::thread::spawn(move || {
            second_barrier.wait();
            futures::executor::block_on(second_repo.append_event(&aborted)).expect("abort")
        });
        let outcomes = [
            first_thread.join().expect("complete thread"),
            second_thread.join().expect("abort thread"),
        ];
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, AppendEventOutcome::Inserted(_)))
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, AppendEventOutcome::Existing(_)))
                .count(),
            1
        );
        let events = futures::executor::block_on(SessionEventRepo::list_events_by_session(
            &first_repo,
            &SessionId("terminal-race-session".to_owned()),
        ))
        .expect("list terminal events");
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].kind,
            SessionEventKind::TurnCompleted { .. } | SessionEventKind::TurnAborted { .. }
        ));
    }

    #[test]
    fn concurrent_nonterminal_appends_allocate_unique_monotonic_sequences() {
        let directory = tempfile::tempdir().expect("temporary database directory");
        let path = directory.path().join("events.sqlite");
        let first_repo = SqliteSessionRepo::open(&path).expect("open first repo");
        event_session(&first_repo, "sequence-race-session");
        let second_repo = SqliteSessionRepo::open(&path).expect("open second repo");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let first = event_record(
            "delta-one",
            "sequence-race-session",
            100,
            SessionEventKind::TurnDelta {
                turn_id: TurnId("sequence-turn".to_owned()),
                content: "one".to_owned(),
            },
        );
        let second = event_record(
            "delta-two",
            "sequence-race-session",
            100,
            SessionEventKind::TurnDelta {
                turn_id: TurnId("sequence-turn".to_owned()),
                content: "two".to_owned(),
            },
        );
        let first_barrier = barrier.clone();
        let first_repo_for_thread = first_repo.clone();
        let first_thread = std::thread::spawn(move || {
            first_barrier.wait();
            futures::executor::block_on(first_repo_for_thread.append_event(&first))
                .expect("first delta")
        });
        let second_barrier = barrier;
        let second_thread = std::thread::spawn(move || {
            second_barrier.wait();
            futures::executor::block_on(second_repo.append_event(&second)).expect("second delta")
        });
        let _ = first_thread.join().expect("first thread");
        let _ = second_thread.join().expect("second thread");
        let events = futures::executor::block_on(SessionEventRepo::list_events_by_session(
            &first_repo,
            &SessionId("sequence-race-session".to_owned()),
        ))
        .expect("list sequence events");
        assert_eq!(
            events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    fn create_legacy_event_table(path: &std::path::Path, rows: &[(&str, &str, &str)]) {
        let conn = Connection::open(path).expect("open legacy database");
        conn.execute_batch(
            "CREATE TABLE dcc_session_events (event_id TEXT PRIMARY KEY, session_id TEXT NOT NULL, sequence INTEGER NOT NULL, occurred_at TEXT NOT NULL, kind_json TEXT NOT NULL);",
        )
        .expect("create legacy event table");
        for (event_id, kind_json, sequence) in rows {
            conn.execute(
                "INSERT INTO dcc_session_events(event_id, session_id, sequence, occurred_at, kind_json) VALUES (?1, 'legacy-session', ?2, 't0', ?3)",
                params![event_id, sequence.parse::<i64>().expect("sequence"), kind_json],
            )
            .expect("insert legacy event");
        }
    }

    #[test]
    fn migration_backfills_known_terminal_keys_without_deleting_history() {
        let directory = tempfile::tempdir().expect("temporary database directory");
        let path = directory.path().join("legacy.sqlite");
        create_legacy_event_table(
            &path,
            &[(
                "legacy-complete",
                r#"{"type":"turn_completed","turnId":"legacy-turn"}"#,
                "4",
            )],
        );
        let repo = SqliteSessionRepo::open(&path).expect("migrate legacy database");
        drop(repo);
        let conn = Connection::open(&path).expect("reopen migrated database");
        let row = conn
            .query_row(
                "SELECT terminal_turn_id, terminal_kind FROM dcc_session_events WHERE event_id = 'legacy-complete'",
                [],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .expect("read migrated terminal key");
        assert_eq!(row.0.as_deref(), Some("legacy-turn"));
        assert_eq!(row.1.as_deref(), Some("completed"));
    }

    #[test]
    fn migration_rejects_duplicate_known_terminal_keys_without_deleting_rows() {
        let directory = tempfile::tempdir().expect("temporary database directory");
        let path = directory.path().join("duplicate.sqlite");
        create_legacy_event_table(
            &path,
            &[
                (
                    "legacy-complete",
                    r#"{"type":"turn_completed","turnId":"same-turn"}"#,
                    "1",
                ),
                (
                    "legacy-abort",
                    r#"{"type":"turn_aborted","turnId":"same-turn","reason":null}"#,
                    "2",
                ),
            ],
        );
        assert!(SqliteSessionRepo::open(&path).is_err());
        let conn = Connection::open(&path).expect("reopen duplicate database");
        let count = conn
            .query_row("SELECT COUNT(*) FROM dcc_session_events", [], |row| {
                row.get::<_, i64>(0)
            })
            .expect("count preserved rows");
        assert_eq!(count, 2);
        let columns = conn
            .prepare("PRAGMA table_info(dcc_session_events)")
            .expect("inspect legacy columns")
            .query_map([], |row| row.get::<_, String>(1))
            .expect("list legacy columns")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect legacy columns");
        assert!(!columns.iter().any(|column| column == "terminal_turn_id"));
        assert!(!columns.iter().any(|column| column == "terminal_kind"));
        let indexes = conn
            .prepare("PRAGMA index_list(dcc_session_events)")
            .expect("inspect legacy indexes")
            .query_map([], |row| row.get::<_, String>(1))
            .expect("list legacy indexes")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect legacy indexes");
        assert!(!indexes
            .iter()
            .any(|index| index == "idx_dcc_session_events_terminal_turn"));
        let kinds = conn
            .prepare("SELECT kind_json FROM dcc_session_events ORDER BY sequence")
            .expect("inspect legacy rows")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("read legacy rows")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect legacy rows");
        assert_eq!(kinds.len(), 2);
        assert!(kinds[0].contains("turn_completed"));
        assert!(kinds[1].contains("turn_aborted"));
    }

    #[test]
    fn migration_leaves_unknown_future_event_unclassified() {
        let directory = tempfile::tempdir().expect("temporary database directory");
        let path = directory.path().join("future.sqlite");
        create_legacy_event_table(
            &path,
            &[(
                "future-event",
                r#"{"type":"future_terminal","turnId":"future-turn"}"#,
                "1",
            )],
        );
        let repo = SqliteSessionRepo::open(&path).expect("migrate future database");
        drop(repo);
        let conn = Connection::open(&path).expect("reopen future database");
        let row = conn
            .query_row(
                "SELECT terminal_turn_id, terminal_kind, kind_json FROM dcc_session_events WHERE event_id = 'future-event'",
                [],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, String>(2)?)),
            )
            .expect("read future event");
        assert!(row.0.is_none());
        assert!(row.1.is_none());
        assert!(row.2.contains("future_terminal"));
    }

    #[test]
    fn browser_location_round_trip_is_scoped_and_expires_opportunistically() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).unwrap();
        repo.save_browser_location("workspace", None, "https://example.test/a", 100, 200)
            .unwrap();
        repo.save_browser_location(
            "workspace",
            Some("session"),
            "https://example.test/b",
            100,
            200,
        )
        .unwrap();
        assert_eq!(
            repo.load_browser_location("workspace", None, 199).unwrap(),
            Some("https://example.test/a".to_string())
        );
        assert_eq!(
            repo.load_browser_location("workspace", Some("session"), 199)
                .unwrap(),
            Some("https://example.test/b".to_string())
        );
        assert_eq!(
            repo.load_browser_location("workspace", None, 200).unwrap(),
            None
        );
        assert_eq!(
            repo.load_browser_location("workspace", None, 200).unwrap(),
            None
        );
        assert_eq!(
            repo.load_browser_location("workspace", Some("session"), 200)
                .unwrap(),
            None
        );
    }

    #[test]
    fn browser_location_rejects_unbounded_input_and_stale_upserts() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).unwrap();
        assert!(repo
            .save_browser_location("", None, "https://example.test", 0, 1)
            .is_err());
        assert!(repo
            .save_browser_location("workspace\n", None, "https://example.test", 0, 1)
            .is_err());
        assert!(repo
            .save_browser_location("workspace", None, "https://example.test\n", 0, 1)
            .is_err());
        assert!(repo
            .save_browser_location("workspace", None, "https://example.test", 0, 0,)
            .is_err());
        let long_url = format!("https://example.test/{}", "x".repeat(2040));
        assert!(repo
            .save_browser_location("workspace", None, &long_url, 0, 1)
            .is_err());

        repo.save_browser_location("workspace", None, "https://example.test/new", 20, 30)
            .unwrap();
        repo.save_browser_location("workspace", None, "https://example.test/old", 10, 30)
            .unwrap();
        assert_eq!(
            repo.load_browser_location("workspace", None, 20).unwrap(),
            Some("https://example.test/new".to_string())
        );
    }

    #[test]
    fn browser_location_cleanup_removes_workspace_and_session_rows() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).unwrap();
        repo.save_browser_location("workspace", None, "https://example.test/a", 0, 10)
            .unwrap();
        repo.save_browser_location(
            "workspace",
            Some("session"),
            "https://example.test/b",
            0,
            10,
        )
        .unwrap();
        repo.save_browser_location("other", Some("session"), "https://example.test/c", 0, 10)
            .unwrap();
        assert_eq!(
            repo.delete_browser_location("workspace", None).unwrap(),
            true
        );
        assert_eq!(
            repo.load_browser_location("workspace", None, 1).unwrap(),
            None
        );
        assert_eq!(
            repo.delete_browser_locations_for_session("session")
                .unwrap(),
            2
        );
        assert_eq!(
            repo.load_browser_location("other", Some("session"), 1)
                .unwrap(),
            None
        );
    }

    #[test]
    fn provider_availability_defaults_enabled_and_migrates_without_touching_legacy_table() {
        let conn = in_memory_conn();
        conn.lock()
            .unwrap()
            .execute_batch(
                "CREATE TABLE providers (id TEXT PRIMARY KEY, is_active INTEGER NOT NULL);\
                 INSERT INTO providers (id, is_active) VALUES ('codex', 0);",
            )
            .unwrap();
        let repo = SqliteSessionRepo::from_connection(Arc::clone(&conn)).unwrap();
        assert_eq!(repo.load_provider_availability("codex").unwrap(), None);
        repo.save_provider_availability(&ProviderAvailabilityRecord {
            provider_id: "codex".to_string(),
            enabled: false,
            generation: 7,
            updated_at_ms: 42,
        })
        .unwrap();
        assert_eq!(
            repo.load_provider_availability("codex").unwrap(),
            Some(ProviderAvailabilityRecord {
                provider_id: "codex".to_string(),
                enabled: false,
                generation: 7,
                updated_at_ms: 42,
            })
        );
        assert_eq!(
            conn.lock()
                .unwrap()
                .query_row(
                    "SELECT is_active FROM providers WHERE id = 'codex'",
                    [],
                    |row| { row.get::<_, i64>(0) }
                )
                .unwrap(),
            0,
            "the legacy application provider table is not authority for DCC runtime availability"
        );
        SqliteSessionRepo::from_connection(conn).expect("migration remains idempotent");
    }

    #[test]
    fn provider_availability_rejects_unbounded_or_regressing_records() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).unwrap();
        assert!(repo.load_provider_availability("\n").is_err());
        assert!(repo
            .save_provider_availability(&ProviderAvailabilityRecord {
                provider_id: "x".repeat(PROVIDER_AVAILABILITY_MAX_PROVIDER_ID_CHARS + 1),
                enabled: true,
                generation: 0,
                updated_at_ms: 0,
            })
            .is_err());
        repo.save_provider_availability(&ProviderAvailabilityRecord {
            provider_id: "codex".to_string(),
            enabled: false,
            generation: 3,
            updated_at_ms: 30,
        })
        .unwrap();
        repo.save_provider_availability(&ProviderAvailabilityRecord {
            provider_id: "codex".to_string(),
            enabled: true,
            generation: 2,
            updated_at_ms: 40,
        })
        .expect_err("a stale generation must not silently overwrite availability");
        repo.save_provider_availability(&ProviderAvailabilityRecord {
            provider_id: "codex".to_string(),
            enabled: true,
            generation: 3,
            updated_at_ms: 40,
        })
        .expect_err("the same generation cannot reverse availability");
        assert_eq!(
            repo.load_provider_availability("codex")
                .unwrap()
                .expect("record")
                .enabled,
            false,
            "a stale writer cannot re-enable a newer disabled record"
        );
    }
}
