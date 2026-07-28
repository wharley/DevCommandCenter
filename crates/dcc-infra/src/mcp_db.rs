use std::{
    fmt,
    path::Path,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{de::DeserializeOwned, Serialize};

use dcc_core::{
    domain::mcp::{
        McpBinding, McpBindingId, McpBindingScope, McpDefinition, McpDefinitionId,
        McpDefinitionOwnership, McpToolPolicy, McpToolPolicyDecision, McpTransport, McpTrust,
    },
    ports::McpRepo,
    CoreError, Result,
};

const MCP_SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_mcp_definitions (
    id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL,
    transport_kind TEXT NOT NULL CHECK (transport_kind IN ('stdio', 'http')),
    transport_json TEXT NOT NULL,
    secret_refs_json TEXT NOT NULL DEFAULT '[]',
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    ownership_kind TEXT NOT NULL CHECK (ownership_kind IN ('dcc_managed', 'imported_read_only')),
    ownership_json TEXT NOT NULL,
    current_trust_fingerprint TEXT NOT NULL CHECK (length(current_trust_fingerprint) = 64),
    trust_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_dcc_mcp_definitions_updated_at
ON dcc_mcp_definitions(updated_at DESC, id ASC);

CREATE INDEX IF NOT EXISTS idx_dcc_mcp_definitions_enabled
ON dcc_mcp_definitions(enabled);

CREATE TABLE IF NOT EXISTS dcc_mcp_bindings (
    id TEXT PRIMARY KEY NOT NULL,
    definition_id TEXT NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('session', 'project', 'global')),
    scope_target_id TEXT NOT NULL DEFAULT '',
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    provider_exclusions_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (definition_id) REFERENCES dcc_mcp_definitions(id) ON DELETE CASCADE,
    CHECK (
        (scope_kind = 'global' AND scope_target_id = '')
        OR (scope_kind IN ('session', 'project') AND length(scope_target_id) > 0)
    ),
    UNIQUE(definition_id, scope_kind, scope_target_id)
);

CREATE INDEX IF NOT EXISTS idx_dcc_mcp_bindings_definition
ON dcc_mcp_bindings(definition_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_dcc_mcp_bindings_scope
ON dcc_mcp_bindings(scope_kind, scope_target_id, enabled);

CREATE TABLE IF NOT EXISTS dcc_mcp_tool_policies (
    definition_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    decision TEXT NOT NULL CHECK (decision IN ('allow', 'deny')),
    updated_at TEXT NOT NULL,
    PRIMARY KEY (definition_id, tool_name),
    FOREIGN KEY (definition_id) REFERENCES dcc_mcp_definitions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dcc_mcp_tool_policies_definition
ON dcc_mcp_tool_policies(definition_id, tool_name);
"#;

#[derive(Clone)]
pub struct SqliteMcpRepo {
    conn: Arc<Mutex<Connection>>,
}

impl fmt::Debug for SqliteMcpRepo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqliteMcpRepo")
            .finish_non_exhaustive()
    }
}

impl SqliteMcpRepo {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path.as_ref()).map_err(repository_error)?;
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

    pub fn apply_migrations(conn: &Connection) -> Result<()> {
        conn.execute_batch(&format!(
            "PRAGMA foreign_keys = ON;\nPRAGMA busy_timeout = 5000;\n{MCP_SCHEMA_SQL}"
        ))
        .map_err(repository_error)
    }

    fn ensure_schema(&self) -> Result<()> {
        let conn = self.lock_connection()?;
        Self::apply_migrations(&conn)
    }

    fn lock_connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|error| CoreError::Repository(error.to_string()))
    }

    fn definition_from_row(row: &Row<'_>) -> rusqlite::Result<McpDefinition> {
        let stored_transport_kind = row.get::<_, String>(2)?;
        let transport: McpTransport = deserialize_column(row, 3)?;
        if transport_kind(&transport) != stored_transport_kind {
            return Err(invalid_column(
                2,
                "MCP transport kind column does not match transport JSON",
            ));
        }
        let stored_ownership_kind = row.get::<_, String>(6)?;
        let ownership: McpDefinitionOwnership = deserialize_column(row, 7)?;
        if ownership_kind(&ownership) != stored_ownership_kind {
            return Err(invalid_column(
                6,
                "MCP ownership kind column does not match ownership JSON",
            ));
        }
        let stored_fingerprint = row.get::<_, String>(8)?;
        let trust: McpTrust = deserialize_column(row, 9)?;
        if trust.current_fingerprint.0 != stored_fingerprint {
            return Err(invalid_column(
                8,
                "MCP trust fingerprint column does not match trust JSON",
            ));
        }
        let definition = McpDefinition {
            id: McpDefinitionId(row.get::<_, String>(0)?),
            display_name: row.get(1)?,
            transport,
            secret_refs: deserialize_column(row, 4)?,
            enabled: row.get(5)?,
            ownership,
            trust,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        };
        definition.validate().map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
        Ok(definition)
    }

    fn binding_from_row(row: &Row<'_>) -> rusqlite::Result<McpBinding> {
        let scope_kind = row.get::<_, String>(2)?;
        let scope_target_id = row.get::<_, String>(3)?;
        let scope = binding_scope_from_columns(&scope_kind, scope_target_id, 2)?;
        let binding = McpBinding {
            id: McpBindingId(row.get::<_, String>(0)?),
            definition_id: McpDefinitionId(row.get::<_, String>(1)?),
            scope,
            enabled: row.get(4)?,
            provider_exclusions: deserialize_column(row, 5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        };
        binding.validate().map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
        Ok(binding)
    }

    fn tool_policy_from_row(row: &Row<'_>) -> rusqlite::Result<McpToolPolicy> {
        let decision = match row.get::<_, String>(2)?.as_str() {
            "allow" => McpToolPolicyDecision::Allow,
            "deny" => McpToolPolicyDecision::Deny,
            value => {
                return Err(invalid_column(
                    2,
                    format!("invalid MCP tool policy decision: {value}"),
                ));
            }
        };
        let policy = McpToolPolicy {
            definition_id: McpDefinitionId(row.get(0)?),
            tool_name: row.get(1)?,
            decision,
            updated_at: row.get(3)?,
        };
        policy.validate().map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
        Ok(policy)
    }
}

#[async_trait]
impl McpRepo for SqliteMcpRepo {
    async fn save_mcp_definition(&self, definition: &McpDefinition) -> Result<()> {
        definition
            .validate()
            .map_err(|error| CoreError::InvalidInput(error.to_string()))?;
        let transport_json = serialize_json(&definition.transport)?;
        let secret_refs_json = serialize_json(&definition.secret_refs)?;
        let ownership_json = serialize_json(&definition.ownership)?;
        let trust_json = serialize_json(&definition.trust)?;
        let transport_kind = transport_kind(&definition.transport);
        let ownership_kind = ownership_kind(&definition.ownership);

        let conn = self.lock_connection()?;
        conn.execute(
            r#"
            INSERT INTO dcc_mcp_definitions (
                id, display_name, transport_kind, transport_json,
                secret_refs_json, enabled, ownership_kind, ownership_json,
                current_trust_fingerprint, trust_json, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(id) DO UPDATE SET
                display_name = excluded.display_name,
                transport_kind = excluded.transport_kind,
                transport_json = excluded.transport_json,
                secret_refs_json = excluded.secret_refs_json,
                enabled = excluded.enabled,
                ownership_kind = excluded.ownership_kind,
                ownership_json = excluded.ownership_json,
                current_trust_fingerprint = excluded.current_trust_fingerprint,
                trust_json = excluded.trust_json,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at
            "#,
            params![
                definition.id.0,
                definition.display_name,
                transport_kind,
                transport_json,
                secret_refs_json,
                definition.enabled,
                ownership_kind,
                ownership_json,
                definition.trust.current_fingerprint.0,
                trust_json,
                definition.created_at,
                definition.updated_at,
            ],
        )
        .map_err(repository_error)?;
        Ok(())
    }

    async fn get_mcp_definition(&self, id: &McpDefinitionId) -> Result<Option<McpDefinition>> {
        let conn = self.lock_connection()?;
        conn.query_row(
            r#"
            SELECT id, display_name, transport_kind, transport_json,
                   secret_refs_json, enabled, ownership_kind, ownership_json,
                   current_trust_fingerprint, trust_json,
                   created_at, updated_at
              FROM dcc_mcp_definitions
             WHERE id = ?1
            "#,
            params![id.0],
            Self::definition_from_row,
        )
        .optional()
        .map_err(repository_error)
    }

    async fn list_mcp_definitions(&self) -> Result<Vec<McpDefinition>> {
        let conn = self.lock_connection()?;
        let mut statement = conn
            .prepare(
                r#"
                SELECT id, display_name, transport_kind, transport_json,
                       secret_refs_json, enabled, ownership_kind, ownership_json,
                       current_trust_fingerprint, trust_json,
                       created_at, updated_at
                  FROM dcc_mcp_definitions
                 ORDER BY updated_at DESC, id ASC
                "#,
            )
            .map_err(repository_error)?;
        let rows = statement
            .query_map([], Self::definition_from_row)
            .map_err(repository_error)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(repository_error)
    }

    async fn delete_mcp_definition(&self, id: &McpDefinitionId) -> Result<()> {
        let conn = self.lock_connection()?;
        conn.execute(
            "DELETE FROM dcc_mcp_definitions WHERE id = ?1",
            params![id.0],
        )
        .map_err(repository_error)?;
        Ok(())
    }

    async fn save_mcp_binding(&self, binding: &McpBinding) -> Result<()> {
        binding
            .validate()
            .map_err(|error| CoreError::InvalidInput(error.to_string()))?;
        let (scope_kind, scope_target_id) = binding_scope_columns(&binding.scope);
        let provider_exclusions_json = serialize_json(&binding.provider_exclusions)?;

        let conn = self.lock_connection()?;
        conn.execute(
            r#"
            INSERT INTO dcc_mcp_bindings (
                id, definition_id, scope_kind, scope_target_id, enabled,
                provider_exclusions_json, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
                definition_id = excluded.definition_id,
                scope_kind = excluded.scope_kind,
                scope_target_id = excluded.scope_target_id,
                enabled = excluded.enabled,
                provider_exclusions_json = excluded.provider_exclusions_json,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at
            "#,
            params![
                binding.id.0,
                binding.definition_id.0,
                scope_kind,
                scope_target_id,
                binding.enabled,
                provider_exclusions_json,
                binding.created_at,
                binding.updated_at,
            ],
        )
        .map_err(repository_error)?;
        Ok(())
    }

    async fn get_mcp_binding(&self, id: &McpBindingId) -> Result<Option<McpBinding>> {
        let conn = self.lock_connection()?;
        conn.query_row(
            r#"
            SELECT id, definition_id, scope_kind, scope_target_id, enabled,
                   provider_exclusions_json, created_at, updated_at
              FROM dcc_mcp_bindings
             WHERE id = ?1
            "#,
            params![id.0],
            Self::binding_from_row,
        )
        .optional()
        .map_err(repository_error)
    }

    async fn list_mcp_bindings(
        &self,
        definition_id: Option<&McpDefinitionId>,
    ) -> Result<Vec<McpBinding>> {
        let conn = self.lock_connection()?;
        let sql = if definition_id.is_some() {
            r#"
            SELECT id, definition_id, scope_kind, scope_target_id, enabled,
                   provider_exclusions_json, created_at, updated_at
              FROM dcc_mcp_bindings
             WHERE definition_id = ?1
             ORDER BY updated_at DESC, id ASC
            "#
        } else {
            r#"
            SELECT id, definition_id, scope_kind, scope_target_id, enabled,
                   provider_exclusions_json, created_at, updated_at
              FROM dcc_mcp_bindings
             ORDER BY updated_at DESC, id ASC
            "#
        };
        let mut statement = conn.prepare(sql).map_err(repository_error)?;
        let rows = if let Some(definition_id) = definition_id {
            statement
                .query_map(params![definition_id.0], Self::binding_from_row)
                .map_err(repository_error)?
        } else {
            statement
                .query_map([], Self::binding_from_row)
                .map_err(repository_error)?
        };
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(repository_error)
    }

    async fn delete_mcp_binding(&self, id: &McpBindingId) -> Result<()> {
        let conn = self.lock_connection()?;
        conn.execute("DELETE FROM dcc_mcp_bindings WHERE id = ?1", params![id.0])
            .map_err(repository_error)?;
        Ok(())
    }

    async fn save_mcp_tool_policy(&self, policy: &McpToolPolicy) -> Result<()> {
        policy
            .validate()
            .map_err(|error| CoreError::InvalidInput(error.to_string()))?;
        let decision = match policy.decision {
            McpToolPolicyDecision::Allow => "allow",
            McpToolPolicyDecision::Deny => "deny",
            McpToolPolicyDecision::Ask => {
                return Err(CoreError::InvalidInput(
                    "Ask is the implicit MCP tool policy and must not be persisted".to_string(),
                ));
            }
        };
        let conn = self.lock_connection()?;
        conn.execute(
            r#"
            INSERT INTO dcc_mcp_tool_policies (
                definition_id, tool_name, decision, updated_at
            )
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(definition_id, tool_name) DO UPDATE SET
                decision = excluded.decision,
                updated_at = excluded.updated_at
            "#,
            params![
                policy.definition_id.0,
                policy.tool_name,
                decision,
                policy.updated_at,
            ],
        )
        .map_err(repository_error)?;
        Ok(())
    }

    async fn list_mcp_tool_policies(
        &self,
        definition_id: Option<&McpDefinitionId>,
    ) -> Result<Vec<McpToolPolicy>> {
        let conn = self.lock_connection()?;
        let sql = if definition_id.is_some() {
            r#"
            SELECT definition_id, tool_name, decision, updated_at
              FROM dcc_mcp_tool_policies
             WHERE definition_id = ?1
             ORDER BY tool_name ASC
            "#
        } else {
            r#"
            SELECT definition_id, tool_name, decision, updated_at
              FROM dcc_mcp_tool_policies
             ORDER BY definition_id ASC, tool_name ASC
            "#
        };
        let mut statement = conn.prepare(sql).map_err(repository_error)?;
        let rows = if let Some(definition_id) = definition_id {
            statement
                .query_map(params![definition_id.0], Self::tool_policy_from_row)
                .map_err(repository_error)?
        } else {
            statement
                .query_map([], Self::tool_policy_from_row)
                .map_err(repository_error)?
        };
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(repository_error)
    }

    async fn delete_mcp_tool_policy(
        &self,
        definition_id: &McpDefinitionId,
        tool_name: &str,
    ) -> Result<()> {
        let conn = self.lock_connection()?;
        conn.execute(
            "DELETE FROM dcc_mcp_tool_policies WHERE definition_id = ?1 AND tool_name = ?2",
            params![definition_id.0, tool_name],
        )
        .map_err(repository_error)?;
        Ok(())
    }
}

fn serialize_json(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value).map_err(repository_error)
}

fn deserialize_column<T: DeserializeOwned>(row: &Row<'_>, column: usize) -> rusqlite::Result<T> {
    let json = row.get::<_, String>(column)?;
    serde_json::from_str(&json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn transport_kind(transport: &McpTransport) -> &'static str {
    match transport {
        McpTransport::Stdio { .. } => "stdio",
        McpTransport::Http { .. } => "http",
    }
}

fn ownership_kind(ownership: &McpDefinitionOwnership) -> &'static str {
    match ownership {
        McpDefinitionOwnership::DccManaged => "dcc_managed",
        McpDefinitionOwnership::ImportedReadOnly { .. } => "imported_read_only",
    }
}

fn binding_scope_columns(scope: &McpBindingScope) -> (&'static str, &str) {
    match scope {
        McpBindingScope::Session { session_id } => ("session", &session_id.0),
        McpBindingScope::Project { project_id } => ("project", &project_id.0),
        McpBindingScope::Global => ("global", ""),
    }
}

fn binding_scope_from_columns(
    kind: &str,
    target_id: String,
    column: usize,
) -> rusqlite::Result<McpBindingScope> {
    match kind {
        "session" => Ok(McpBindingScope::Session {
            session_id: dcc_core::domain::session::SessionId(target_id),
        }),
        "project" => Ok(McpBindingScope::Project {
            project_id: dcc_core::domain::project::ProjectId(target_id),
        }),
        "global" if target_id.is_empty() => Ok(McpBindingScope::Global),
        _ => Err(invalid_column(
            column,
            format!("invalid MCP binding scope: {kind}"),
        )),
    }
}

fn invalid_column(column: usize, message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message.into(),
        )),
    )
}

fn repository_error(error: impl ToString) -> CoreError {
    CoreError::Repository(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc};

    use dcc_core::{
        domain::{
            mcp::{
                McpImportSource, McpImportSourceKind, McpSecretBinding, McpSecretReferenceId,
                McpSecretTarget, McpTrustDecision, McpTrustFingerprint,
            },
            project::ProjectId,
            provider::ProviderId,
            session::SessionId,
        },
        ports::McpRepo,
    };
    use futures::executor::block_on;
    use tempfile::tempdir;

    use super::*;

    fn fingerprint(character: char) -> McpTrustFingerprint {
        McpTrustFingerprint(character.to_string().repeat(64))
    }

    fn http_definition(id: &str) -> McpDefinition {
        let mut definition = McpDefinition {
            id: McpDefinitionId(id.to_string()),
            display_name: "Figma".to_string(),
            transport: McpTransport::Http {
                url: "https://mcp.figma.example/v1".to_string(),
            },
            secret_refs: vec![McpSecretBinding {
                target: McpSecretTarget::HttpHeader {
                    name: "Authorization".to_string(),
                },
                secret_ref: McpSecretReferenceId("credential:figma".to_string()),
            }],
            enabled: true,
            ownership: McpDefinitionOwnership::DccManaged,
            trust: McpTrust {
                current_fingerprint: fingerprint('a'),
                decision: McpTrustDecision::Untrusted,
            },
            created_at: "2026-07-28T00:00:00Z".to_string(),
            updated_at: "2026-07-28T00:00:00Z".to_string(),
        };
        definition.synchronize_trust_fingerprint();
        definition
    }

    fn binding(id: &str, definition_id: &str, scope: McpBindingScope) -> McpBinding {
        McpBinding {
            id: McpBindingId(id.to_string()),
            definition_id: McpDefinitionId(definition_id.to_string()),
            scope,
            enabled: true,
            provider_exclusions: Vec::new(),
            created_at: "2026-07-28T00:00:00Z".to_string(),
            updated_at: "2026-07-28T00:00:00Z".to_string(),
        }
    }

    fn in_memory_repo() -> (SqliteMcpRepo, Arc<Mutex<Connection>>) {
        let conn = Arc::new(Mutex::new(
            Connection::open_in_memory().expect("open in-memory sqlite"),
        ));
        let repo = SqliteMcpRepo::from_connection(conn.clone()).expect("create MCP repo");
        (repo, conn)
    }

    #[test]
    fn round_trips_definition_and_stores_only_opaque_secret_references() {
        let (repo, conn) = in_memory_repo();
        let definition = http_definition("figma");

        block_on(repo.save_mcp_definition(&definition)).expect("save definition");
        let fetched = block_on(repo.get_mcp_definition(&definition.id))
            .expect("get definition")
            .expect("definition exists");
        assert_eq!(fetched, definition);

        let conn = conn.lock().expect("lock sqlite");
        let (transport_json, secret_refs_json) = conn
            .query_row(
                "SELECT transport_json, secret_refs_json FROM dcc_mcp_definitions WHERE id = ?1",
                params![definition.id.0],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .expect("read persisted definition");
        assert!(transport_json.contains("mcp.figma.example"));
        assert!(secret_refs_json.contains("credential:figma"));
        assert!(!transport_json.contains("super-secret-token"));
        assert!(!secret_refs_json.contains("super-secret-token"));

        let columns = conn
            .prepare("PRAGMA table_info(dcc_mcp_definitions)")
            .expect("prepare table info")
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query table info")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect columns");
        assert!(!columns.iter().any(|column| {
            matches!(
                column.as_str(),
                "secret" | "secret_value" | "token" | "password"
            )
        }));
    }

    #[test]
    fn persists_all_binding_scopes_and_provider_exclusions() {
        let (repo, _) = in_memory_repo();
        let definition = http_definition("figma");
        block_on(repo.save_mcp_definition(&definition)).expect("save definition");

        let session = binding(
            "session-binding",
            "figma",
            McpBindingScope::Session {
                session_id: SessionId("session-1".to_string()),
            },
        );
        let mut project = binding(
            "project-binding",
            "figma",
            McpBindingScope::Project {
                project_id: ProjectId("project-1".to_string()),
            },
        );
        project.provider_exclusions = vec![ProviderId("cursor".to_string())];
        let global = binding("global-binding", "figma", McpBindingScope::Global);

        for binding in [&session, &project, &global] {
            block_on(repo.save_mcp_binding(binding)).expect("save binding");
        }

        let bindings = block_on(repo.list_mcp_bindings(Some(&definition.id)))
            .expect("list definition bindings");
        assert_eq!(bindings.len(), 3);
        assert!(bindings.contains(&session));
        assert!(bindings.contains(&project));
        assert!(bindings.contains(&global));
    }

    #[test]
    fn persists_tool_policy_overrides_and_cascades_with_definition() {
        let (repo, _) = in_memory_repo();
        let definition = http_definition("figma");
        block_on(repo.save_mcp_definition(&definition)).expect("save definition");
        let policy = McpToolPolicy {
            definition_id: definition.id.clone(),
            tool_name: "update_design".to_string(),
            decision: McpToolPolicyDecision::Deny,
            updated_at: "2026-07-28T01:00:00Z".to_string(),
        };

        block_on(repo.save_mcp_tool_policy(&policy)).expect("save tool policy");
        assert_eq!(
            block_on(repo.list_mcp_tool_policies(Some(&definition.id)))
                .expect("list tool policies"),
            vec![policy.clone()]
        );

        block_on(repo.delete_mcp_definition(&definition.id)).expect("delete definition");
        assert!(block_on(repo.list_mcp_tool_policies(Some(&definition.id)))
            .expect("list cascaded policies")
            .is_empty());
    }

    #[test]
    fn definitions_and_bindings_survive_reopen_and_definition_delete_cascades() {
        let directory = tempdir().expect("create temp dir");
        let database_path = directory.path().join("dcc.sqlite");
        let definition = http_definition("figma");
        let global = binding("global-binding", "figma", McpBindingScope::Global);

        {
            let repo = SqliteMcpRepo::open(&database_path).expect("open MCP repo");
            block_on(repo.save_mcp_definition(&definition)).expect("save definition");
            block_on(repo.save_mcp_binding(&global)).expect("save binding");
        }

        let repo = SqliteMcpRepo::open(&database_path).expect("reopen MCP repo");
        assert_eq!(
            block_on(repo.get_mcp_definition(&definition.id)).expect("get definition"),
            Some(definition.clone())
        );
        assert_eq!(
            block_on(repo.get_mcp_binding(&global.id)).expect("get binding"),
            Some(global.clone())
        );

        block_on(repo.delete_mcp_definition(&definition.id)).expect("delete definition");
        assert_eq!(
            block_on(repo.get_mcp_binding(&global.id)).expect("get cascaded binding"),
            None
        );
    }

    #[test]
    fn disabling_preserves_definition_and_bindings() {
        let (repo, _) = in_memory_repo();
        let mut definition = http_definition("figma");
        let global = binding("global-binding", "figma", McpBindingScope::Global);
        block_on(repo.save_mcp_definition(&definition)).expect("save definition");
        block_on(repo.save_mcp_binding(&global)).expect("save binding");

        definition.enabled = false;
        definition.updated_at = "2026-07-28T01:00:00Z".to_string();
        block_on(repo.save_mcp_definition(&definition)).expect("disable definition");

        assert_eq!(
            block_on(repo.get_mcp_definition(&definition.id)).expect("get disabled definition"),
            Some(definition)
        );
        assert_eq!(
            block_on(repo.get_mcp_binding(&global.id)).expect("get preserved binding"),
            Some(global)
        );
    }

    #[test]
    fn deleting_an_imported_definition_never_modifies_its_source() {
        let directory = tempdir().expect("create temp dir");
        let source_path = directory.path().join("mcp.json");
        let source_contents = r#"{"payments":{"command":"payment-mcp"}}"#;
        fs::write(&source_path, source_contents).expect("write imported source");

        let (repo, _) = in_memory_repo();
        let mut definition = http_definition("payments");
        definition.ownership = McpDefinitionOwnership::ImportedReadOnly {
            source: McpImportSource {
                kind: McpImportSourceKind::ProjectFile,
                locator: source_path.to_string_lossy().to_string(),
                definition_key: Some("payments".to_string()),
            },
        };
        definition.synchronize_trust_fingerprint();

        block_on(repo.save_mcp_definition(&definition)).expect("save imported definition");
        block_on(repo.delete_mcp_definition(&definition.id)).expect("delete imported definition");
        assert_eq!(
            fs::read_to_string(&source_path).expect("read imported source"),
            source_contents
        );
    }

    #[test]
    fn invalid_definitions_fail_before_any_row_is_written() {
        let (repo, conn) = in_memory_repo();
        let mut definition = http_definition("figma");
        definition.transport = McpTransport::Http {
            url: "file:///tmp/not-an-http-server".to_string(),
        };

        let error = block_on(repo.save_mcp_definition(&definition))
            .expect_err("invalid definition must fail");
        assert!(matches!(error, CoreError::InvalidInput(_)));

        let count = conn
            .lock()
            .expect("lock sqlite")
            .query_row("SELECT COUNT(*) FROM dcc_mcp_definitions", [], |row| {
                row.get::<_, u64>(0)
            })
            .expect("count definitions");
        assert_eq!(count, 0);
    }

    #[test]
    fn duplicate_definition_scope_bindings_are_rejected() {
        let (repo, _) = in_memory_repo();
        let definition = http_definition("figma");
        block_on(repo.save_mcp_definition(&definition)).expect("save definition");
        let first = binding("global-one", "figma", McpBindingScope::Global);
        let duplicate = binding("global-two", "figma", McpBindingScope::Global);

        block_on(repo.save_mcp_binding(&first)).expect("save first binding");
        let error =
            block_on(repo.save_mcp_binding(&duplicate)).expect_err("duplicate scope must fail");
        assert!(matches!(error, CoreError::Repository(_)));
    }

    #[test]
    fn inconsistent_discriminator_columns_fail_closed() {
        let (repo, conn) = in_memory_repo();
        let definition = http_definition("figma");
        block_on(repo.save_mcp_definition(&definition)).expect("save definition");
        conn.lock()
            .expect("lock sqlite")
            .execute(
                "UPDATE dcc_mcp_definitions SET transport_kind = 'stdio' WHERE id = ?1",
                params![definition.id.0],
            )
            .expect("tamper transport kind");

        let error = block_on(repo.get_mcp_definition(&definition.id))
            .expect_err("inconsistent definition must fail");
        assert!(matches!(error, CoreError::Repository(_)));
    }

    #[test]
    fn runtime_status_is_not_part_of_the_durable_schema() {
        let (_, conn) = in_memory_repo();
        let table_names = conn
            .lock()
            .expect("lock sqlite")
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .expect("prepare table list")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query tables")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect table names");

        assert!(table_names.contains(&"dcc_mcp_definitions".to_string()));
        assert!(table_names.contains(&"dcc_mcp_bindings".to_string()));
        assert!(!table_names
            .iter()
            .any(|name| name.contains("runtime") || name.contains("status")));
    }
}
