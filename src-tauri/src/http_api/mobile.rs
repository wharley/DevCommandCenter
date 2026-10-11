//! Mobile operations reuse the same domain use cases and provider authority as desktop.
use super::*;
use dcc_core::{
    application::{
        create_workspace_for_repo, CreateWorkspaceForRepoInput, WorkspaceIsolationMode,
    },
    domain::{provider::ProviderApprovalPolicy, workspace::WorkspaceState},
    ports::{RepositoryRepo, WorkspaceRepo},
};
use dcc_infra::{db::SqliteWorkspaceRepo, git::CommandGitOps};
pub(super) use dcc_tauri::delegation_runtime::provider_approval_policies;
use rusqlite::OptionalExtension;

pub(super) async fn catalog(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    let providers =
        dcc_tauri::commands::provider_commands::list_providers_for_state(&backend.sessions)
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
    /// The phone's "Worktree" checkbox (checked by default, like the desktop).
    /// Omitted by older apps: protected worktree, the core's default.
    #[serde(default)]
    isolation_mode: Option<WorkspaceIsolationMode>,
    /// The phone's access selector. Omitted by older apps, which always asked.
    /// Skipped when absent so journaled requests from those apps still match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    approval_policy: Option<ProviderApprovalPolicy>,
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
    task_approval_policy(input)?;
    Ok(())
}

/// The level a phone turn runs with. A choice must be one the provider
/// enforces; without one the session keeps its latest turn's level (what the
/// desktop or the phone last chose), so a continuation neither drops back to
/// asking nor rises above it. Delegated children cap themselves at that same
/// recorded level (`delegation_runtime::child_approval_policy`).
pub(super) fn turn_approval_policy(
    supported: &[ProviderApprovalPolicy],
    chosen: Option<ProviderApprovalPolicy>,
    latest: Option<ProviderApprovalPolicy>,
) -> Result<Option<ProviderApprovalPolicy>, HttpApiError> {
    match chosen {
        Some(policy) if supported.contains(&policy) => Ok(Some(policy)),
        Some(_) => Err(HttpApiError::BadRequest(
            "Este agente não oferece esse nível de acesso.".into(),
        )),
        None => Ok(latest.filter(|policy| supported.contains(policy))),
    }
}

/// A new task has no earlier turn: older apps (no choice) keep asking.
fn task_approval_policy(
    input: &CreateTask,
) -> Result<Option<ProviderApprovalPolicy>, HttpApiError> {
    let supported = provider_approval_policies(&input.provider_id);
    let legacy = supported
        .contains(&ProviderApprovalPolicy::Ask)
        .then_some(ProviderApprovalPolicy::Ask);
    Ok(turn_approval_policy(&supported, input.approval_policy, None)?.or(legacy))
}

/// The level recorded on the session's newest turn that has one.
fn latest_session_approval_policy(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> Result<Option<ProviderApprovalPolicy>, String> {
    let policy = conn
        .query_row(
            "SELECT json_extract(kind_json, '$.approvalPolicy') FROM dcc_session_events
             WHERE session_id = ?1 AND json_extract(kind_json, '$.type') = 'turn_started'
               AND json_extract(kind_json, '$.approvalPolicy') IS NOT NULL
             ORDER BY sequence DESC LIMIT 1",
            [session_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(policy.and_then(|policy| serde_json::from_value(Value::String(policy)).ok()))
}

/// Fills in (or checks) the level of a turn sent over HTTP. Unknown sessions
/// fall through to the send pipeline, which reports them.
pub(super) async fn resolve_turn_approval_policy(
    config: Arc<RwLock<HttpConfig>>,
    backend: &HttpBackend,
    input: &mut SendTurnInput,
) -> Result<(), HttpApiError> {
    let Some(session) = backend
        .sessions
        .peek_session(&input.session_id)
        .await
        .map_err(core_error)?
    else {
        return Ok(());
    };
    let provider_id = input.provider_id.clone().unwrap_or(session.provider_id);
    let latest = if input.approval_policy.is_none() {
        let id = input.session_id.0.clone();
        db_read(config, move |conn| {
            latest_session_approval_policy(conn, &id)
        })
        .await?
    } else {
        None
    };
    input.approval_policy = turn_approval_policy(
        &provider_approval_policies(&provider_id),
        input.approval_policy,
        latest,
    )?;
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
    State(backend): State<Arc<HttpBackend>>,
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
            if let Err(error) = execute_task(config.clone(), backend, &input, &mut result).await {
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
    backend: Arc<HttpBackend>,
    input: &CreateTask,
    result: &mut Value,
) -> Result<(), HttpApiError> {
    let state = &backend.sessions;
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
            state,
            CreateWorkspaceForRepoInput {
                project_id: repository.project_id,
                workspace_root: repository.root_path,
                base_branch: repository.base_branch,
                name: Some(input.title.trim().to_owned()),
                isolation_mode: input.isolation_mode.clone(),
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
    let output = start_thread(state, state, state, state, start)
        .await
        .map_err(core_error)?;
    let session_id = output.session.id.0;
    result["sessionId"] = json!(session_id);
    save_result(config.clone(), input.request_id.clone(), result.clone()).await?;
    let turn: SendTurnInput = serde_json::from_value(json!({
        "sessionId":session_id, "prompt":input.prompt.trim(), "planMode":input.plan_mode,
        "approvalPolicy": task_approval_policy(input)?,
    }))
    .map_err(|e| HttpApiError::BadRequest(e.to_string()))?;
    let _ = send_turn_handler(
        State(config.clone()),
        State(backend.clone()),
        Path(session_id),
        Json(turn),
    )
    .await?;
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

/// Same cap as the desktop's `@` menu (`file-mention-plugin.tsx`).
const MAX_FILE_RESULTS: usize = 50;
const MAX_FILE_QUERY: usize = 200;
/// The phone asks on every keystroke; `git ls-files` runs at most this often per tree.
const TRACKED_FILES_TTL: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
pub(super) struct FilesQuery {
    #[serde(default)]
    q: String,
    limit: Option<usize>,
}

/// `@` in the phone's composer: tracked files of a workspace, found by id only.
pub(super) async fn workspace_files(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(id): Path<String>,
    Query(query): Query<FilesQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let repo = SqliteWorkspaceRepo::open(&config.read().await.db_path).map_err(core_error)?;
    let workspace = repo
        .get_workspace(&WorkspaceId(id))
        .await
        .map_err(core_error)?
        .ok_or_else(|| HttpApiError::NotFound("Workspace não encontrado.".into()))?;
    let root = workspace.worktree_path.unwrap_or(workspace.root_path);
    tracked_files_matching(PathBuf::from(root), query)
        .await
        .map(Json)
}

/// `@` in "Nova tarefa": the project's checkout, before its worktree exists.
pub(super) async fn repository_files(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(id): Path<String>,
    Query(query): Query<FilesQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let repo = SqliteWorkspaceRepo::open(&config.read().await.db_path).map_err(core_error)?;
    let repository = repo
        .list_repositories()
        .await
        .map_err(core_error)?
        .into_iter()
        .find(|r| r.id.0 == id)
        .ok_or_else(|| HttpApiError::NotFound("Repositório não encontrado no DCC.".into()))?;
    tracked_files_matching(PathBuf::from(repository.root_path), query)
        .await
        .map(Json)
}

async fn tracked_files_matching(root: PathBuf, query: FilesQuery) -> Result<Value, HttpApiError> {
    if query.q.chars().count() > MAX_FILE_QUERY || query.q.contains('\0') {
        return Err(HttpApiError::BadRequest(
            "Busca de arquivo inválida.".into(),
        ));
    }
    let root = tokio::fs::canonicalize(&root)
        .await
        .map_err(|_| HttpApiError::NotFound("A pasta do workspace não existe mais.".into()))?;
    let paths = tracked_files(root).await?;
    let limit = query
        .limit
        .unwrap_or(MAX_FILE_RESULTS)
        .clamp(1, MAX_FILE_RESULTS);
    let files: Vec<Value> = filter_tracked_files(&paths, &query.q, limit)
        .into_iter()
        .map(|path| json!({ "path": path }))
        .collect();
    Ok(json!({ "files": files, "total": paths.len() }))
}

async fn tracked_files(root: PathBuf) -> Result<Arc<Vec<String>>, HttpApiError> {
    type Cache = std::sync::Mutex<HashMap<PathBuf, (std::time::Instant, Arc<Vec<String>>)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some((at, paths)) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&root) {
        if at.elapsed() < TRACKED_FILES_TTL {
            return Ok(paths.clone());
        }
    }
    let dir = root.to_string_lossy().into_owned();
    let paths = tokio::task::spawn_blocking(move || {
        dcc_tauri::commands::workspace_commands::git_tracked_files(&dir)
    })
    .await
    .map_err(|e| HttpApiError::internal(e.to_string()))?
    .map_err(HttpApiError::internal)?;
    let paths = Arc::new(paths);
    let mut entries = cache.lock().unwrap_or_else(|e| e.into_inner());
    entries.retain(|_, (at, _)| at.elapsed() < TRACKED_FILES_TTL);
    entries.insert(root, (std::time::Instant::now(), paths.clone()));
    Ok(paths)
}

/// The desktop's `@` ranking (`rankFile`/`filterFiles` in `file-mention-plugin.tsx`):
/// an exact name or path wins alone; then path suffix, name prefix, name
/// substring, path substring — case-insensitive, stable within a rank.
fn filter_tracked_files<'a>(paths: &'a [String], query: &str, limit: usize) -> Vec<&'a str> {
    if query.is_empty() {
        return paths.iter().take(limit).map(String::as_str).collect();
    }
    let q = query.to_lowercase();
    let suffix = format!("/{q}");
    let rank = |path: &str| -> u8 {
        let path = path.to_lowercase();
        let name = path.rsplit('/').next().unwrap_or(&path);
        if name == q || path == q {
            5
        } else if path.ends_with(&suffix) {
            4
        } else if name.starts_with(&q) {
            3
        } else if name.contains(&q) {
            2
        } else if path.contains(&q) {
            1
        } else {
            0
        }
    };
    let mut ranked: Vec<(u8, &str)> = paths
        .iter()
        .map(|path| (rank(path), path.as_str()))
        .filter(|(score, _)| *score > 0)
        .collect();
    if ranked.iter().any(|(score, _)| *score == 5) {
        ranked.retain(|(score, _)| *score == 5);
    }
    ranked.sort_by(|a, b| b.0.cmp(&a.0));
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, path)| path)
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UploadAttachment {
    /// The name the phone shows; kept for documents so the agent reads it.
    name: String,
    /// Base64 of the file, already compressed by the phone.
    data: String,
}

/// Bytes of an upload within the phone limit (an image is sniffed by content).
fn decode_attachment(input: &UploadAttachment) -> Result<Vec<u8>, HttpApiError> {
    use base64::Engine as _;
    let limit = crate::prompt_attachments::MAX_PHONE_ATTACHMENT_BYTES;
    // Base64 is 4 chars per 3 bytes: refuse before decoding something huge.
    if input.data.len() / 4 * 3 > limit + 3 {
        return Err(attachment_too_large());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input.data.trim())
        .map_err(|_| HttpApiError::BadRequest("Anexo inválido.".into()))?;
    if bytes.is_empty() {
        return Err(HttpApiError::BadRequest("O anexo está vazio.".into()));
    }
    if bytes.len() > limit {
        return Err(attachment_too_large());
    }
    Ok(bytes)
}

fn attachment_too_large() -> HttpApiError {
    HttpApiError::BadRequest(format!(
        "O anexo passa de {} MB.",
        crate::prompt_attachments::MAX_PHONE_ATTACHMENT_BYTES / (1024 * 1024)
    ))
}

/// Saves a photo or document from the phone where the desktop keeps pasted
/// images. The phone then sends `@path` in the prompt, as the composer does,
/// so turns and new tasks need nothing else.
pub(super) async fn upload_attachment(
    Json(input): Json<UploadAttachment>,
) -> Result<Json<Value>, HttpApiError> {
    let bytes = decode_attachment(&input)?;
    let saved = tokio::task::spawn_blocking(move || {
        let size = bytes.len();
        let (kind, path) = match crate::prompt_attachments::sniff_image(&bytes) {
            Some((_, extension)) => (
                "image",
                crate::prompt_attachments::save_temp_image(&bytes, extension),
            ),
            None => (
                "file",
                crate::prompt_attachments::save_temp_file(&bytes, &input.name),
            ),
        };
        path.map(|path| (kind, path, size))
    })
    .await
    .map_err(|e| HttpApiError::internal(e.to_string()))?
    .map_err(|e| HttpApiError::internal(format!("Não foi possível guardar o anexo: {e}")))?;
    let (kind, path, size) = saved;
    Ok(Json(json!({
        "kind": kind,
        "path": path.to_string_lossy(),
        "name": path.file_name().map(|name| name.to_string_lossy().to_string()),
        "size": size,
    })))
}

#[derive(Deserialize)]
pub(super) struct AttachmentQuery {
    path: String,
}

/// Largest image the phone pulls a thumbnail for over the relay.
const PHONE_PREVIEW_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Whether one of the session's prompts attached `path` (`@path`).
fn session_references_attachment(
    conn: &rusqlite::Connection,
    session_id: &str,
    path: &str,
) -> Result<bool, String> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM dcc_session_events
         WHERE session_id = ?1 AND json_extract(kind_json, '$.type') = 'turn_started'
           AND instr(json_extract(kind_json, '$.prompt'), '@' || ?2) > 0)",
        rusqlite::params![session_id, path],
        |row| row.get::<_, bool>(0),
    )
    .map_err(|e| e.to_string())
}

/// Thumbnail of an image a prompt of this session attached, like the
/// desktop's `read_image_preview` — but only for paths the session's own
/// prompts reference, so a phone cannot read arbitrary images off the Mac.
pub(super) async fn attachment_preview(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(session_id): Path<String>,
    Query(query): Query<AttachmentQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let path = PathBuf::from(&query.path);
    if !path.is_absolute()
        || query.path.contains('\0')
        || crate::prompt_attachments::image_media_type(&path).is_none()
    {
        return Err(HttpApiError::NotFound("Prévia indisponível.".into()));
    }
    let wanted = query.path.clone();
    if !db_read(config, move |conn| {
        session_references_attachment(conn, &session_id, &wanted)
    })
    .await?
    {
        return Err(HttpApiError::NotFound("Prévia indisponível.".into()));
    }
    let data_url = tokio::task::spawn_blocking(move || {
        crate::prompt_attachments::read_image_data_url(&path, PHONE_PREVIEW_MAX_BYTES)
    })
    .await
    .map_err(|e| HttpApiError::internal(e.to_string()))?
    .ok_or_else(|| HttpApiError::NotFound("Prévia indisponível.".into()))?;
    Ok(Json(json!({ "dataUrl": data_url })))
}

/// The branch checked out in a workspace's folder (its worktree when it has
/// one), read from `HEAD` without spawning git: the list asks for every
/// workspace at once. `None` while the worktree is still detached.
pub(super) fn checked_out_branch(folder: &FsPath) -> Option<String> {
    let dot_git = folder.join(".git");
    let git_dir = if dot_git.is_file() {
        let pointer = std::fs::read_to_string(&dot_git).ok()?;
        let target = PathBuf::from(pointer.trim().strip_prefix("gitdir:")?.trim());
        if target.is_absolute() {
            target
        } else {
            folder.join(target)
        }
    } else {
        dot_git
    };
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    head.trim()
        .strip_prefix("ref: refs/heads/")
        .map(str::to_string)
        .filter(|branch| !branch.is_empty())
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
    fn task_input_carries_the_worktree_choice() {
        let base = json!({
            "requestId": "8f0d2f4e-8a59-4f0e-9a43-0d1f6f3f2b11", "repositoryId": "repo",
            "title": "t", "prompt": "p", "providerId": "claude",
        });
        let mut direct = base.clone();
        direct["isolationMode"] = json!("localDirect");
        let input: CreateTask = serde_json::from_value(direct).unwrap();
        assert!(matches!(
            input.isolation_mode,
            Some(WorkspaceIsolationMode::LocalDirect)
        ));
        // Older apps omit it: the core falls back to a protected worktree.
        let input: CreateTask = serde_json::from_value(base).unwrap();
        assert!(input.isolation_mode.is_none());
    }

    #[test]
    fn access_level_is_chosen_inherited_or_refused() {
        use ProviderApprovalPolicy::*;
        let all = [ReadOnly, Ask, Auto, FullAccess];
        assert_eq!(
            turn_approval_policy(&all, Some(FullAccess), Some(Ask)).unwrap(),
            Some(FullAccess)
        );
        assert_eq!(
            turn_approval_policy(&all, None, Some(FullAccess)).unwrap(),
            Some(FullAccess)
        );
        assert_eq!(turn_approval_policy(&all, None, None).unwrap(), None);
        assert!(turn_approval_policy(&[Ask, Auto], Some(FullAccess), None).is_err());
        assert!(turn_approval_policy(&[], Some(Ask), None).is_err());
        // An inherited level the provider can't enforce is dropped, never widened.
        assert_eq!(
            turn_approval_policy(&[Ask], None, Some(FullAccess)).unwrap(),
            None
        );
    }

    #[test]
    fn task_input_carries_the_access_level_without_changing_old_requests() {
        let base = json!({
            "requestId": "8f0d2f4e-8a59-4f0e-9a43-0d1f6f3f2b11", "repositoryId": "repo",
            "title": "t", "prompt": "p", "providerId": "codex",
        });
        let old: CreateTask = serde_json::from_value(base.clone()).unwrap();
        assert!(!serde_json::to_string(&old)
            .unwrap()
            .contains("approvalPolicy"));
        assert_eq!(
            task_approval_policy(&old).unwrap(),
            Some(ProviderApprovalPolicy::Ask)
        );
        let mut chosen = base.clone();
        chosen["approvalPolicy"] = json!("full_access");
        let chosen: CreateTask = serde_json::from_value(chosen).unwrap();
        assert!(validate_task(&chosen).is_ok());
        assert_eq!(
            task_approval_policy(&chosen).unwrap(),
            Some(ProviderApprovalPolicy::FullAccess)
        );
        let mut unknown = base;
        unknown["providerId"] = json!("no-such-provider");
        unknown["approvalPolicy"] = json!("auto");
        let unknown: CreateTask = serde_json::from_value(unknown).unwrap();
        assert!(validate_task(&unknown).is_err());
    }

    #[test]
    fn file_search_ranks_like_the_desktop_mention_menu() {
        let paths: Vec<String> = [
            "README.md",
            "src/app/payment.ts",
            "src/app/paymentPix.ts",
            "src/lib/redis.ts",
            "docs/payment-flow.md",
            "src/PayButton.tsx",
        ]
        .map(String::from)
        .to_vec();
        // A complete file name stands alone.
        assert_eq!(
            filter_tracked_files(&paths, "payment.ts", 50),
            ["src/app/payment.ts"]
        );
        // Name prefix before name substring before path substring.
        assert_eq!(
            filter_tracked_files(&paths, "PAY", 50),
            [
                "src/app/payment.ts",
                "src/app/paymentPix.ts",
                "docs/payment-flow.md",
                "src/PayButton.tsx"
            ]
        );
        assert_eq!(
            filter_tracked_files(&paths, "app/", 50),
            ["src/app/payment.ts", "src/app/paymentPix.ts"]
        );
        assert_eq!(
            filter_tracked_files(&paths, "", 2),
            ["README.md", "src/app/payment.ts"]
        );
        assert_eq!(filter_tracked_files(&paths, "pay", 1).len(), 1);
        assert!(filter_tracked_files(&paths, "zzz", 50).is_empty());
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
