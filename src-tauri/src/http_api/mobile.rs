//! Mobile operations reuse the same domain use cases and provider authority as desktop.
use super::*;
use dcc_core::{
    application::{create_workspace_for_repo, CreateWorkspaceForRepoInput},
    domain::{provider::ProviderApprovalPolicy, workspace::WorkspaceState},
    ports::{RepositoryRepo, WorkspaceRepo},
};
use dcc_infra::{db::SqliteWorkspaceRepo, git::CommandGitOps};
use rusqlite::OptionalExtension;

pub(super) async fn catalog(
    State(config): State<Arc<RwLock<HttpConfig>>>,
) -> Result<Json<Value>, HttpApiError> {
    let state = headless_session_state(config.clone()).await?;
    let providers = dcc_tauri::commands::provider_commands::list_providers_for_state(&state)
        .await
        .map_err(HttpApiError::internal)?;
    let repo = SqliteWorkspaceRepo::open(&config.read().await.db_path)
        .map_err(|e| HttpApiError::internal(e.to_string()))?;
    let repositories = repo.list_repositories().await.map_err(core_error)?;
    let workspaces = repo.list_workspaces().await.map_err(core_error)?;
    Ok(Json(
        json!({ "providers": providers.catalog.providers, "repositories": repositories, "workspaces": workspaces }),
    ))
}

fn core_error(error: dcc_core::CoreError) -> HttpApiError {
    classify_session_error(error.to_string())
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CreateTask {
    request_id: String,
    workspace_id: Option<String>,
    repository_id: Option<String>,
    title: String,
    prompt: String,
    provider_id: String,
    model: Option<String>,
    #[serde(default)]
    plan_mode: bool,
}

fn validate_task(input: &CreateTask) -> Result<(), HttpApiError> {
    if Uuid::parse_str(&input.request_id).is_err() {
        return Err(HttpApiError::BadRequest(
            "Identificador de criação inválido.".into(),
        ));
    }
    if input.workspace_id.is_some() == input.repository_id.is_some() {
        return Err(HttpApiError::BadRequest(
            "Escolha um workspace ou um repositório para uma nova área isolada.".into(),
        ));
    }
    if input.prompt.trim().is_empty()
        || input.prompt.chars().count() > 100_000
        || input.title.chars().count() > 240
    {
        return Err(HttpApiError::BadRequest(
            "Descreva a tarefa (até 100 mil caracteres) e use um título curto.".into(),
        ));
    }
    Ok(())
}

fn journal(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_task_requests (
        id TEXT PRIMARY KEY, input TEXT NOT NULL, result TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
    )",
    )
    .map_err(|e| e.to_string())
}

fn reserve_task(
    conn: &rusqlite::Connection,
    id: &str,
    encoded: &str,
) -> Result<(bool, bool, String), String> {
    journal(conn)?;
    let inserted = conn
        .execute(
            "INSERT OR IGNORE INTO mobile_task_requests (id,input,result) VALUES (?1,?2,?3)",
            rusqlite::params![id, encoded, json!({"state":"creating"}).to_string()],
        )
        .map_err(|e| e.to_string())?;
    let (saved_input, result) = conn
        .query_row(
            "SELECT input,result FROM mobile_task_requests WHERE id=?1",
            [id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .map_err(|e| e.to_string())?;
    Ok((inserted == 1, saved_input == encoded, result))
}

async fn save_result(
    config: Arc<RwLock<HttpConfig>>,
    id: String,
    result: Value,
) -> Result<(), HttpApiError> {
    db_read(config, move |conn| {
        conn.execute(
            "UPDATE mobile_task_requests SET result = ?2 WHERE id = ?1",
            rusqlite::params![id, result.to_string()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
}

pub(super) async fn task_status(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let result = db_read(config, move |conn| {
        journal(conn)?;
        conn.query_row(
            "SELECT result FROM mobile_task_requests WHERE id = ?1",
            [id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())
    })
    .await?
    .ok_or_else(|| HttpApiError::NotFound("Criação não encontrada.".into()))?;
    Ok(Json(
        serde_json::from_str(&result).map_err(|e| HttpApiError::internal(e.to_string()))?,
    ))
}

pub(super) async fn create_task(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Json(input): Json<CreateTask>,
) -> Result<Json<Value>, HttpApiError> {
    validate_task(&input)?;
    let id = input.request_id.clone();
    let encoded =
        serde_json::to_string(&input).map_err(|e| HttpApiError::internal(e.to_string()))?;
    let reservation = db_read(config.clone(), move |conn| {
        reserve_task(conn, &id, &encoded)
    })
    .await?;
    if !reservation.1 {
        return Err(HttpApiError::BadRequest(
            "Esta criação já foi usada com outra descrição.".into(),
        ));
    }
    if reservation.0 {
        // Detach execution from the HTTP request: switching networks must not cancel creation.
        tokio::spawn(async move {
            let mut result = json!({"state":"creating"});
            if let Err(error) = execute_task(config.clone(), &input, &mut result).await {
                result["state"] = json!("failed");
                result["error"] = json!(error.message());
            }
            if let Err(error) = save_result(config, input.request_id, result).await {
                eprintln!(
                    "[DCC mobile] could not save task outcome: {}",
                    error.message()
                );
            }
        });
    }
    Ok(Json(
        serde_json::from_str(&reservation.2).map_err(|e| HttpApiError::internal(e.to_string()))?,
    ))
}

async fn execute_task(
    config: Arc<RwLock<HttpConfig>>,
    input: &CreateTask,
    result: &mut Value,
) -> Result<(), HttpApiError> {
    let state = headless_session_state(config.clone()).await?;
    // Validate provider/model before creating any worktree. The real workspace is checked below.
    let mut start: StartThreadInput = serde_json::from_value(json!({
        "workspaceId": input.workspace_id.as_deref().unwrap_or("pending"), "projectId": "pending",
        "providerId": input.provider_id, "model": input.model, "title": input.title.trim(),
    }))
    .map_err(|e| HttpApiError::BadRequest(e.to_string()))?;
    state
        .validate_start_thread_scope(&start)
        .await
        .map_err(core_error)?;
    let repo = SqliteWorkspaceRepo::open(&config.read().await.db_path).map_err(core_error)?;
    let workspace = if let Some(id) = &input.workspace_id {
        let workspace = repo
            .get_workspace(&WorkspaceId(id.clone()))
            .await
            .map_err(core_error)?
            .ok_or_else(|| HttpApiError::NotFound("Workspace não encontrado.".into()))?;
        if workspace.state != WorkspaceState::Ready {
            return Err(HttpApiError::BadRequest(
                "Escolha um workspace pronto para trabalhar.".into(),
            ));
        }
        workspace
    } else {
        let repositories = repo.list_repositories().await.map_err(core_error)?;
        let repository = repositories
            .into_iter()
            .find(|r| Some(&r.id.0) == input.repository_id.as_ref())
            .ok_or_else(|| HttpApiError::NotFound("Repositório não encontrado no DCC.".into()))?;
        let created = create_workspace_for_repo(
            &repo,
            &CommandGitOps::new(),
            &*state,
            CreateWorkspaceForRepoInput {
                project_id: repository.project_id,
                workspace_root: repository.root_path,
                base_branch: repository.base_branch,
                name: Some(input.title.trim().to_owned()),
                isolation_mode: None,
            },
        )
        .await
        .map_err(core_error)?;
        created.workspace
    };
    result["workspaceId"] = json!(workspace.id.0);
    save_result(config.clone(), input.request_id.clone(), result.clone()).await?;
    start.workspace_id = workspace.id;
    start.project_id = workspace.project_id;
    // Attach only when sending: start_thread persists the session before provider startup.
    let output = start_thread(&*state, &*state, &*state, &*state, start)
        .await
        .map_err(core_error)?;
    let session_id = output.session.id.0;
    result["sessionId"] = json!(session_id);
    save_result(config.clone(), input.request_id.clone(), result.clone()).await?;
    let turn: SendTurnInput = serde_json::from_value(json!({
        "sessionId":session_id, "prompt":input.prompt.trim(), "planMode":input.plan_mode,
        "approvalPolicy": ProviderApprovalPolicy::Ask,
    }))
    .map_err(|e| HttpApiError::BadRequest(e.to_string()))?;
    let _ = send_turn_handler(State(config), Path(session_id), Json(turn)).await?;
    result["state"] = json!("started");
    Ok(())
}

#[derive(Deserialize)]
pub(super) struct PatchQuery {
    path: String,
}

fn validate_relative_path(path: &str) -> Result<(), HttpApiError> {
    if path.is_empty()
        || path.contains('\0')
        || path.contains('\\')
        || path.starts_with('/')
        || path
            .split('/')
            .any(|part| part == ".." || part == "." || part.is_empty())
    {
        return Err(HttpApiError::BadRequest(
            "Caminho de arquivo inválido.".into(),
        ));
    }
    Ok(())
}

async fn git_output(root: &FsPath, args: &[&str]) -> Result<(bool, String, bool), HttpApiError> {
    use tokio::io::AsyncReadExt;
    const MAX_PATCH: u64 = 512_000;
    let mut child = tokio::process::Command::new("git")
        .arg("--literal-pathspecs")
        .args(args)
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| HttpApiError::internal(e.to_string()))?;
    let mut output = Vec::new();
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| HttpApiError::internal("Git sem saída"))?
        .take(MAX_PATCH + 1);
    tokio::time::timeout(Duration::from_secs(15), stdout.read_to_end(&mut output))
        .await
        .map_err(|_| HttpApiError::GatewayTimeout("O Git demorou para responder.".into()))?
        .map_err(|e| HttpApiError::internal(e.to_string()))?;
    let truncated = output.len() as u64 > MAX_PATCH;
    if truncated {
        let _ = child.kill().await;
        output.truncate(MAX_PATCH as usize);
    }
    let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .map_err(|_| HttpApiError::GatewayTimeout("O Git não encerrou.".into()))?
        .map_err(|e| HttpApiError::internal(e.to_string()))?;
    Ok((
        status.success(),
        String::from_utf8_lossy(&output).into_owned(),
        truncated,
    ))
}

pub(super) async fn patch(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(id): Path<String>,
    Query(query): Query<PatchQuery>,
) -> Result<Json<Value>, HttpApiError> {
    validate_relative_path(&query.path)?;
    let repo = SqliteWorkspaceRepo::open(&config.read().await.db_path).map_err(core_error)?;
    let workspace = repo
        .get_workspace(&WorkspaceId(id))
        .await
        .map_err(core_error)?
        .ok_or_else(|| HttpApiError::NotFound("Workspace não encontrado.".into()))?;
    let root = PathBuf::from(
        workspace
            .worktree_path
            .as_deref()
            .unwrap_or(&workspace.root_path),
    );
    workspace_patch(&root, &query.path).await.map(Json)
}

async fn workspace_patch(root: &FsPath, path: &str) -> Result<Value, HttpApiError> {
    validate_relative_path(path)?;
    let root = tokio::fs::canonicalize(root)
        .await
        .map_err(|e| HttpApiError::BadRequest(e.to_string()))?;
    let target = root.join(path);
    // Canonicalize the nearest existing ancestor too, so deleted paths cannot traverse symlinks.
    let mut ancestor = target.as_path();
    loop {
        match tokio::fs::canonicalize(ancestor).await {
            Ok(resolved) => {
                if !resolved.starts_with(&root) {
                    return Err(HttpApiError::BadRequest(
                        "Arquivo fora do workspace.".into(),
                    ));
                }
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| HttpApiError::BadRequest("Arquivo inválido.".into()))?;
            }
            Err(e) => return Err(HttpApiError::BadRequest(e.to_string())),
        }
    }
    let (in_index, _, _) = git_output(&root, &["ls-files", "--error-unmatch", "--", path]).await?;
    let (has_head, _, _) = git_output(&root, &["rev-parse", "--verify", "HEAD"]).await?;
    // Staged deletions are absent from the index but still need a patch against HEAD.
    let tracked = in_index
        || (has_head
            && git_output(&root, &["cat-file", "-e", &format!("HEAD:{path}")])
                .await?
                .0);
    let (ok, text, truncated) = if tracked {
        let base = if has_head { "HEAD" } else { "--cached" };
        git_output(
            &root,
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--unified=3",
                base,
                "--",
                path,
            ],
        )
        .await?
    } else {
        if !target.is_file() {
            return Err(HttpApiError::NotFound("Arquivo não encontrado.".into()));
        }
        // Git's no-index handles binaries and returns 1 when there are differences.
        git_output(
            &root,
            &[
                "diff",
                "--no-index",
                "--no-ext-diff",
                "--no-textconv",
                "--unified=3",
                "--",
                "/dev/null",
                &target.to_string_lossy(),
            ],
        )
        .await?
    };
    if !ok && text.is_empty() && !truncated {
        return Err(HttpApiError::BadRequest(
            "Não foi possível ler o diff deste arquivo.".into(),
        ));
    }
    Ok(json!({ "path":path, "patch":text, "truncated":truncated }))
}

/// Called once at HTTP process startup. An interrupted mutation is never silently replayed.
pub(super) fn recover_interrupted_tasks(db_path: &FsPath) -> Result<(), String> {
    let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
    journal(&conn)?;
    conn.execute("UPDATE mobile_task_requests SET result=json_set(result,'$.state','failed','$.error',
        'O serviço reiniciou durante a criação. Confira a conversa ou o workspace criado antes de iniciar outra tarefa.')
        WHERE json_extract(result,'$.state')='creating'", []).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retry_keeps_original_outcome_and_rejects_changed_input() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        assert_eq!(reserve_task(&conn, "request", "original").unwrap().0, true);
        conn.execute(
            "UPDATE mobile_task_requests SET result=?1",
            [r#"{"state":"started","sessionId":"only-session"}"#],
        )
        .unwrap();
        let retry = reserve_task(&conn, "request", "original").unwrap();
        assert!(!retry.0);
        assert!(retry.1);
        assert_eq!(
            serde_json::from_str::<Value>(&retry.2).unwrap()["sessionId"],
            "only-session"
        );
        let changed = reserve_task(&conn, "request", "different").unwrap();
        assert!(!changed.0 && !changed.1);
        assert_eq!(
            conn.query_row("SELECT count(*) FROM mobile_task_requests", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn patch_reads_real_changes_and_treats_git_patterns_literally() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for args in [
            vec!["init"],
            vec!["config", "user.email", "test@example.invalid"],
            vec!["config", "user.name", "Test"],
        ] {
            assert!(git_output(root, &args).await.unwrap().0);
        }
        std::fs::write(root.join("tracked.txt"), "before\n").unwrap();
        assert!(git_output(root, &["add", "."]).await.unwrap().0);
        assert!(
            git_output(
                root,
                &[
                    "-c",
                    "commit.gpgsign=false",
                    "-c",
                    "core.hooksPath=/dev/null",
                    "commit",
                    "-m",
                    "fixture"
                ]
            )
            .await
            .unwrap()
            .0
        );
        std::fs::write(root.join("tracked.txt"), "after\n").unwrap();
        let patch = workspace_patch(root, "tracked.txt").await.unwrap();
        assert!(patch["patch"].as_str().unwrap().contains("-before\n+after"));
        std::fs::write(root.join(":(glob)*"), "literal file\n").unwrap();
        let patch = workspace_patch(root, ":(glob)*").await.unwrap();
        assert!(patch["patch"].as_str().unwrap().contains("+literal file"));
        assert!(!patch["patch"].as_str().unwrap().contains("+after"));
        std::fs::remove_file(root.join("tracked.txt")).unwrap();
        let patch = workspace_patch(root, "tracked.txt").await.unwrap();
        assert!(patch["patch"].as_str().unwrap().contains("-before"));
        assert!(git_output(root, &["add", "tracked.txt"]).await.unwrap().0);
        let staged_deletion = workspace_patch(root, "tracked.txt").await.unwrap();
        assert!(staged_deletion["patch"]
            .as_str()
            .unwrap()
            .contains("-before"));
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            std::fs::write(outside.path().join("secret"), "secret").unwrap();
            std::os::unix::fs::symlink(outside.path(), root.join("escape")).unwrap();
            assert!(workspace_patch(root, "escape/secret").await.is_err());
            assert!(workspace_patch(root, "escape/deleted").await.is_err());
        }
    }

    #[test]
    fn rejects_paths_outside_workspace() {
        for path in [
            "../secret",
            "/tmp/file",
            "dir/../../secret",
            "dir\\secret",
            "",
            "a/./b",
        ] {
            assert!(validate_relative_path(path).is_err(), "{path}");
        }
        for path in ["src/main.rs", "a file.txt", ":(glob)*"] {
            assert!(validate_relative_path(path).is_ok());
        }
    }
    #[test]
    fn restart_preserves_partial_result_without_replaying_it() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("test.sqlite");
        let conn = rusqlite::Connection::open(&db).unwrap();
        journal(&conn).unwrap();
        conn.execute(
            "INSERT INTO mobile_task_requests (id,input,result) VALUES ('one','{}',?1)",
            [
                json!({"state":"creating","sessionId":"session-one","workspaceId":"workspace-one"})
                    .to_string(),
            ],
        )
        .unwrap();
        recover_interrupted_tasks(&db).unwrap();
        let result: String = conn
            .query_row("SELECT result FROM mobile_task_requests", [], |r| r.get(0))
            .unwrap();
        let result: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(result["state"], "failed");
        assert_eq!(result["sessionId"], "session-one");
        assert_eq!(result["workspaceId"], "workspace-one");
    }
}
