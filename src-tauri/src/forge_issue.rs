//! Resolução de referências a issues (GitHub / GitLab) e pedidos HTTP às APIs públicas.
//! Tokens: `GITHUB_TOKEN` / `GH_TOKEN`, `GITLAB_TOKEN` / `GITLAB_ACCESS_TOKEN`, ou override opcional vindo da UI.
//!
//! Quando `gh` (GitHub CLI) ou `glab` (GitLab CLI) estão no `PATH` e autenticados, estes são
//! tentados **antes** do REST — reutilizam a sessão do utilizador sem precisar de PAT na UI.

use chrono::Utc;
use dcc_tauri::git::configure_git_command;
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

use crate::ApiError;

lazy_static! {
    static ref RE_GITHUB_ISSUE_URL: Regex =
        Regex::new(r"(?i)^https?://([^/]+)/([^/]+)/([^/]+)/issues/(\d+)(?:[\s/?#].*)?$").unwrap();
    static ref RE_GITLAB_ISSUE_URL: Regex =
        Regex::new(r"(?i)^https?://([^/]+)/(.+)/-/issues/(\d+)(?:[\s/?#].*)?$").unwrap();
    static ref RE_SHORTHAND: Regex = Regex::new(r"^([^#\s]+)#(\d+)\s*$").unwrap();
}

#[derive(Debug, Clone)]
enum ForgeTarget {
    GitHub {
        api_base: String,
        owner: String,
        repo: String,
        number: u32,
    },
    GitLab {
        api_base: String,
        project_path: String,
        iid: u32,
    },
}

fn github_api_base(host: &str) -> String {
    let h = host.trim().to_lowercase();
    if h == "github.com" {
        "https://api.github.com".to_string()
    } else {
        format!("https://{}/api/v3", h)
    }
}

fn gitlab_api_base(host: &str) -> String {
    format!("https://{}/api/v4", host.trim())
}

fn git_remote_origin(project_path: &str) -> Option<String> {
    let mut command = std::process::Command::new("git");
    configure_git_command(&mut command);
    command.args(["remote", "get-url", "origin"]);
    command.current_dir(project_path);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn normalize_git_remote(raw: &str) -> String {
    raw.trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_string()
}

/// Extrai host + path tipo `owner/repo` ou `group/sub/repo` a partir de um remote comum.
fn parse_remote_path(remote: &str) -> Option<(String, Vec<String>)> {
    let r = remote.trim();
    // git@host:path
    if let Some(idx) = r.find('@') {
        if let Some(colon) = r[idx..].find(':') {
            let host = r[idx + 1..idx + colon].to_string();
            let path_part = &r[idx + colon + 1..];
            let path = path_part.trim_end_matches(".git");
            let segments: Vec<String> = path
                .split('/')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            if host.is_empty() || segments.is_empty() {
                return None;
            }
            return Some((host, segments));
        }
    }
    // https://host/path
    if r.starts_with("http://") || r.starts_with("https://") {
        let without_scheme = r.split("//").nth(1)?;
        let mut parts = without_scheme.splitn(2, '/');
        let host = parts.next()?.to_string();
        let rest = parts.next()?.trim_end_matches(".git");
        let segments: Vec<String> = rest
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        if host.is_empty() || segments.is_empty() {
            return None;
        }
        return Some((host, segments));
    }
    None
}

fn parse_issue_input(input: &str) -> Result<ForgeTarget, ApiError> {
    let t = input.trim();
    if t.is_empty() {
        return Err(ApiError {
            code: "VALIDATION_ERROR",
            message: "Referência de issue vazia.".into(),
        });
    }

    if let Some(c) = RE_GITHUB_ISSUE_URL.captures(t) {
        let host = c.get(1).map(|m| m.as_str()).unwrap_or("github.com");
        let owner = c.get(2).map(|m| m.as_str()).unwrap_or("");
        let repo = c.get(3).map(|m| m.as_str()).unwrap_or("");
        let num: u32 = c
            .get(4)
            .and_then(|m| m.as_str().parse().ok())
            .ok_or_else(|| ApiError {
                code: "VALIDATION_ERROR",
                message: "Número de issue inválido.".into(),
            })?;
        let host_l = host.to_lowercase();
        if host_l.contains("gitlab") {
            return Err(ApiError {
                code: "VALIDATION_ERROR",
                message: "URL semelhante a GitHub mas o host parece GitLab; use o formato GitLab (/issues/ ou /-/issues/).".into(),
            });
        }
        return Ok(ForgeTarget::GitHub {
            api_base: github_api_base(host),
            owner: owner.to_string(),
            repo: repo.to_string(),
            number: num,
        });
    }

    if let Some(c) = RE_GITLAB_ISSUE_URL.captures(t) {
        let host = c.get(1).map(|m| m.as_str()).unwrap_or("");
        let path = c.get(2).map(|m| m.as_str()).unwrap_or("");
        let num: u32 = c
            .get(3)
            .and_then(|m| m.as_str().parse().ok())
            .ok_or_else(|| ApiError {
                code: "VALIDATION_ERROR",
                message: "Número de issue inválido.".into(),
            })?;
        return Ok(ForgeTarget::GitLab {
            api_base: gitlab_api_base(host),
            project_path: path.to_string(),
            iid: num,
        });
    }

    if let Some(c) = RE_SHORTHAND.captures(t) {
        let path = c.get(1).map(|m| m.as_str()).unwrap_or("");
        let num: u32 = c
            .get(2)
            .and_then(|m| m.as_str().parse().ok())
            .ok_or_else(|| ApiError {
                code: "VALIDATION_ERROR",
                message: "Número de issue inválido.".into(),
            })?;
        let segments: Vec<String> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        if segments.len() < 2 {
            return Err(ApiError {
                code: "VALIDATION_ERROR",
                message: "Use owner/repositorio#123 ou um URL completo da issue.".into(),
            });
        }
        // Heurística: 2 segmentos → GitHub; 3+ → GitLab
        if segments.len() == 2 {
            return Ok(ForgeTarget::GitHub {
                api_base: "https://api.github.com".to_string(),
                owner: segments[0].clone(),
                repo: segments[1].clone(),
                number: num,
            });
        }
        return Ok(ForgeTarget::GitLab {
            api_base: "https://gitlab.com/api/v4".to_string(),
            project_path: segments.join("/"),
            iid: num,
        });
    }

    Err(ApiError {
        code: "VALIDATION_ERROR",
        message:
            "Formato não reconhecido. Cole o URL da issue (GitHub ou GitLab) ou owner/repo#123."
                .into(),
    })
}

fn align_shorthand_with_remote(
    target: ForgeTarget,
    remote_url: Option<&str>,
) -> Result<ForgeTarget, ApiError> {
    let Some(remote_raw) = remote_url.filter(|s| !s.trim().is_empty()) else {
        return Ok(target);
    };
    let norm = normalize_git_remote(remote_raw);
    let Some((host, segments)) = parse_remote_path(&norm) else {
        return Ok(target);
    };

    match &target {
        ForgeTarget::GitHub {
            api_base: _,
            owner,
            repo,
            number,
        } => {
            if segments.len() == 2 {
                let ro = segments[0].to_lowercase();
                let rr = segments[1].to_lowercase();
                if host.to_lowercase().contains("github") || host == "github.com" {
                    if ro == owner.to_lowercase() && rr == repo.to_lowercase() {
                        return Ok(ForgeTarget::GitHub {
                            api_base: github_api_base(&host),
                            owner: owner.clone(),
                            repo: repo.clone(),
                            number: *number,
                        });
                    }
                }
            }
            Ok(target)
        }
        ForgeTarget::GitLab {
            api_base: _,
            project_path,
            iid,
        } => {
            let path_join = segments.join("/");
            if path_join.to_lowercase() == project_path.to_lowercase() {
                return Ok(ForgeTarget::GitLab {
                    api_base: gitlab_api_base(&host),
                    project_path: project_path.clone(),
                    iid: *iid,
                });
            }
            Ok(target)
        }
    }
}

fn github_token(explicit: Option<&str>) -> Option<String> {
    explicit
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::var("GITHUB_TOKEN")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .or_else(|| {
            std::env::var("GH_TOKEN")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
}

fn gitlab_token(explicit: Option<&str>) -> Option<String> {
    explicit
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::var("GITLAB_TOKEN")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .or_else(|| {
            std::env::var("GITLAB_ACCESS_TOKEN")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
}

async fn http_github_issue(
    api_base: &str,
    owner: &str,
    repo: &str,
    number: u32,
    token: Option<&str>,
) -> Result<Value, ApiError> {
    let url = format!(
        "{}/repos/{}/{}/issues/{}",
        api_base.trim_end_matches('/'),
        owner,
        repo,
        number
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|e| ApiError {
            code: "HTTP_ERROR",
            message: e.to_string(),
        })?;

    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/vnd.github+json"),
    );
    if let Some(t) = github_token(token) {
        if let Ok(h) = HeaderValue::from_str(&format!("Bearer {}", t.trim())) {
            headers.insert(AUTHORIZATION, h);
        }
    }

    let resp = client
        .get(&url)
        .headers(headers)
        .header("User-Agent", "DevCommandCenter/forge-issue")
        .send()
        .await
        .map_err(|e| ApiError {
            code: "HTTP_ERROR",
            message: format!("GitHub: {e}"),
        })?;

    let status = resp.status();
    let body_text = resp.text().await.map_err(|e| ApiError {
        code: "HTTP_ERROR",
        message: e.to_string(),
    })?;

    if !status.is_success() {
        return Err(ApiError {
            code: "FORGE_HTTP_ERROR",
            message: format!(
                "GitHub API {}: {}",
                status.as_u16(),
                truncate_body(&body_text, 280)
            ),
        });
    }

    let v: Value = serde_json::from_str(&body_text).map_err(|e| ApiError {
        code: "SERIALIZE_ERROR",
        message: format!("Resposta GitHub inválida: {e}"),
    })?;

    if v.get("pull_request").is_some() {
        return Err(ApiError {
            code: "VALIDATION_ERROR",
            message:
                "Este número é um pull request no GitHub, não uma issue. Use o URL de uma issue."
                    .into(),
        });
    }

    Ok(v)
}

async fn http_gitlab_issue(
    api_base: &str,
    project_path: &str,
    iid: u32,
    token: Option<&str>,
) -> Result<Value, ApiError> {
    let enc = project_path.replace('/', "%2F");
    let url = format!(
        "{}/projects/{}/issues?iids%5B%5D={}",
        api_base.trim_end_matches('/'),
        enc,
        iid
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|e| ApiError {
            code: "HTTP_ERROR",
            message: e.to_string(),
        })?;

    let mut req = client
        .get(&url)
        .header("User-Agent", "DevCommandCenter/forge-issue");

    if let Some(t) = gitlab_token(token) {
        req = req.header("PRIVATE-TOKEN", t.trim());
    }

    let resp = req.send().await.map_err(|e| ApiError {
        code: "HTTP_ERROR",
        message: format!("GitLab: {e}"),
    })?;

    let status = resp.status();
    let body_text = resp.text().await.map_err(|e| ApiError {
        code: "HTTP_ERROR",
        message: e.to_string(),
    })?;

    if !status.is_success() {
        return Err(ApiError {
            code: "FORGE_HTTP_ERROR",
            message: format!(
                "GitLab API {}: {}",
                status.as_u16(),
                truncate_body(&body_text, 280)
            ),
        });
    }

    let arr: Vec<Value> = serde_json::from_str(&body_text).map_err(|e| ApiError {
        code: "SERIALIZE_ERROR",
        message: format!("Resposta GitLab inválida: {e}"),
    })?;

    arr.into_iter().next().ok_or_else(|| ApiError {
        code: "NOT_FOUND",
        message: "Issue GitLab não encontrada para este projeto.".into(),
    })
}

fn truncate_body(s: &str, max: usize) -> String {
    let t = s.trim();
    if t.len() <= max {
        t.to_string()
    } else {
        format!("{}…", &t[..max])
    }
}

/// Executa um binário no `PATH` com `cwd` no repositório (para `gh`/`glab` lerem remotes e auth).
async fn run_forge_cli_stdout(
    program: &str,
    project_path: &str,
    args: &[&str],
) -> Result<Vec<u8>, String> {
    let output = Command::new(program)
        .current_dir(project_path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("{program}: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "{} exit {:?}: {}",
            program,
            output.status.code(),
            err.trim()
        ));
    }
    Ok(output.stdout)
}

/// `true` se existir PR com este número (GitHub usa o mesmo contador que issues).
async fn github_number_is_pull_request_via_gh(
    project_path: &str,
    owner: &str,
    repo: &str,
    number: u32,
) -> bool {
    let repo_spec = format!("{}/{}", owner, repo);
    let n = number.to_string();
    let args = [
        "pr",
        "view",
        n.as_str(),
        "--repo",
        repo_spec.as_str(),
        "--json",
        "number",
    ];
    run_forge_cli_stdout("gh", project_path, &args)
        .await
        .is_ok()
}

async fn fetch_github_issue_via_gh(
    project_path: &str,
    owner: &str,
    repo: &str,
    number: u32,
) -> Result<Value, String> {
    let repo_spec = format!("{}/{}", owner, repo);
    let n = number.to_string();
    let args = [
        "issue",
        "view",
        n.as_str(),
        "--repo",
        repo_spec.as_str(),
        "--json",
        "title,body,url",
    ];
    let out = run_forge_cli_stdout("gh", project_path, &args).await?;
    serde_json::from_slice(&out).map_err(|e| format!("JSON gh issue view: {e}"))
}

async fn fetch_gitlab_issue_via_glab(
    project_path: &str,
    project_path_ns: &str,
    iid: u32,
) -> Result<Value, String> {
    let n = iid.to_string();
    let args = [
        "issue",
        "view",
        n.as_str(),
        "-R",
        project_path_ns,
        "-F",
        "json",
    ];
    let out = run_forge_cli_stdout("glab", project_path, &args).await?;
    serde_json::from_slice(&out).map_err(|e| format!("JSON glab issue view: {e}"))
}

async fn resolve_github_pr_via_gh(
    project_path: &str,
    owner: &str,
    repo: &str,
    branch: &str,
) -> Option<Value> {
    let head = format!("{}:{}", owner, branch);
    let repo_spec = format!("{}/{}", owner, repo);
    let args = [
        "pr",
        "list",
        "--repo",
        repo_spec.as_str(),
        "--state",
        "open",
        "--head",
        head.as_str(),
        "--json",
        "number,title,url",
        "--limit",
        "5",
    ];
    let out = run_forge_cli_stdout("gh", project_path, &args).await.ok()?;
    let arr: Vec<Value> = serde_json::from_slice(&out).ok()?;
    let p = arr.into_iter().next()?;
    let number = p.get("number").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
    let title = p
        .get("title")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let url = p
        .get("url")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    if url.is_empty() || number == 0 {
        return None;
    }
    let synced = Utc::now().to_rfc3339();
    Some(json!({
        "forge": "github",
        "number": number,
        "url": url,
        "title": title,
        "branch": branch,
        "syncedAt": synced,
    }))
}

async fn resolve_gitlab_mr_via_glab(
    project_path: &str,
    project_path_ns: &str,
    branch: &str,
) -> Option<Value> {
    let args = [
        "mr",
        "list",
        "-R",
        project_path_ns,
        "--source-branch",
        branch,
        "-F",
        "json",
        "-P",
        "20",
    ];
    let out = run_forge_cli_stdout("glab", project_path, &args)
        .await
        .ok()?;
    let arr: Vec<Value> = serde_json::from_slice(&out).ok()?;
    for m in arr {
        let src = m.get("source_branch").and_then(|x| x.as_str());
        if src != Some(branch) {
            continue;
        }
        let iid = m.get("iid").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        let title = m
            .get("title")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let url = m
            .get("web_url")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        if url.is_empty() || iid == 0 {
            continue;
        }
        let synced = Utc::now().to_rfc3339();
        return Some(json!({
            "forge": "gitlab",
            "number": iid,
            "url": url,
            "title": title,
            "branch": branch,
            "syncedAt": synced,
        }));
    }
    None
}

fn build_suggested_workspace_name(title: &str, num_label: &str) -> String {
    let t = title.trim();
    if t.is_empty() {
        format!("Issue {num_label}")
    } else {
        format!("{num_label} · {t}")
    }
}

/// Busca metadados da issue e devolve payload para pré-preencher o «Novo Workspace».
pub async fn fetch_issue_for_project(
    project_path: &str,
    stored_remote: Option<&str>,
    issue_ref: &str,
    token_override: Option<&str>,
) -> Result<Value, ApiError> {
    let remote = stored_remote
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .or_else(|| git_remote_origin(project_path));

    let mut target = parse_issue_input(issue_ref)?;
    target = align_shorthand_with_remote(target, remote.as_deref())?;

    match target {
        ForgeTarget::GitHub {
            api_base,
            owner,
            repo,
            number,
        } => {
            if github_number_is_pull_request_via_gh(project_path, &owner, &repo, number).await {
                return Err(ApiError {
                    code: "VALIDATION_ERROR",
                    message: "Este número é um pull request no GitHub, não uma issue. Use o URL de uma issue.".into(),
                });
            }
            let v = match fetch_github_issue_via_gh(project_path, &owner, &repo, number).await {
                Ok(v) => v,
                Err(_) => {
                    http_github_issue(&api_base, &owner, &repo, number, token_override).await?
                }
            };
            let title = v
                .get("title")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let body = v
                .get("body")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let web_url = v
                .get("html_url")
                .or_else(|| v.get("url"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let num_label = format!("#{number}");
            let suggested_name = build_suggested_workspace_name(&title, &num_label);
            Ok(json!({
                "forge": "github",
                "owner": owner,
                "repo": repo,
                "issueNumber": number,
                "title": title,
                "body": body,
                "webUrl": web_url,
                "suggestedWorkspaceName": suggested_name,
                "suggestedDescription": body,
            }))
        }
        ForgeTarget::GitLab {
            api_base,
            project_path: gl_project_path,
            iid,
        } => {
            let v = match fetch_gitlab_issue_via_glab(project_path, &gl_project_path, iid).await {
                Ok(v) => v,
                Err(_) => {
                    http_gitlab_issue(&api_base, &gl_project_path, iid, token_override).await?
                }
            };
            let title = v
                .get("title")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let body = v
                .get("description")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let web_url = v
                .get("web_url")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let num_label = format!("#{iid}");
            let suggested_name = build_suggested_workspace_name(&title, &num_label);
            Ok(json!({
                "forge": "gitlab",
                "projectPath": gl_project_path,
                "issueNumber": iid,
                "title": title,
                "body": body,
                "webUrl": web_url,
                "suggestedWorkspaceName": suggested_name,
                "suggestedDescription": body,
            }))
        }
    }
}

#[derive(Debug, Clone)]
enum ForgeRemoteKind {
    GitHub {
        api_base: String,
        owner: String,
        repo: String,
    },
    GitLab {
        api_base: String,
        project_path: String,
    },
}

/// Infere GitHub vs GitLab a partir do remote `origin` (mesma heurística que o fluxo de issues).
fn classify_git_remote_for_pr(remote: &str) -> Option<ForgeRemoteKind> {
    let norm = normalize_git_remote(remote);
    let (host, segments) = parse_remote_path(&norm)?;
    if segments.len() < 2 {
        return None;
    }
    let owner = segments[0].clone();
    let repo = segments[1].clone();
    let hl = host.to_lowercase();
    if hl == "github.com" || hl.ends_with(".github.com") {
        return Some(ForgeRemoteKind::GitHub {
            api_base: github_api_base(&host),
            owner,
            repo,
        });
    }
    if hl.contains("gitlab") {
        let project_path = segments.join("/");
        return Some(ForgeRemoteKind::GitLab {
            api_base: gitlab_api_base(&host),
            project_path,
        });
    }
    Some(ForgeRemoteKind::GitHub {
        api_base: format!("https://{}/api/v3", hl.trim()),
        owner,
        repo,
    })
}

async fn http_github_list_open_pulls_for_head(
    api_base: &str,
    owner: &str,
    repo: &str,
    branch: &str,
    token: Option<&str>,
) -> Result<Vec<Value>, ApiError> {
    let url = format!(
        "{}/repos/{}/{}/pulls",
        api_base.trim_end_matches('/'),
        owner,
        repo
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|e| ApiError {
            code: "HTTP_ERROR",
            message: e.to_string(),
        })?;
    let head = format!("{}:{}", owner, branch);
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/vnd.github+json"),
    );
    if let Some(t) = github_token(token) {
        if let Ok(h) = HeaderValue::from_str(&format!("Bearer {}", t.trim())) {
            headers.insert(AUTHORIZATION, h);
        }
    }
    let resp = client
        .get(&url)
        .headers(headers)
        .query(&[
            ("state", "open"),
            ("per_page", "20"),
            ("head", head.as_str()),
        ])
        .header("User-Agent", "DevCommandCenter/forge-pr")
        .send()
        .await
        .map_err(|e| ApiError {
            code: "HTTP_ERROR",
            message: format!("GitHub: {e}"),
        })?;
    let status = resp.status();
    let body_text = resp.text().await.map_err(|e| ApiError {
        code: "HTTP_ERROR",
        message: e.to_string(),
    })?;
    if !status.is_success() {
        return Err(ApiError {
            code: "FORGE_HTTP_ERROR",
            message: format!(
                "GitHub API {}: {}",
                status.as_u16(),
                truncate_body(&body_text, 280)
            ),
        });
    }
    serde_json::from_str(&body_text).map_err(|e| ApiError {
        code: "SERIALIZE_ERROR",
        message: format!("Resposta GitHub inválida: {e}"),
    })
}

async fn http_gitlab_list_open_mrs_for_branch(
    api_base: &str,
    project_path: &str,
    branch: &str,
    token: Option<&str>,
) -> Result<Vec<Value>, ApiError> {
    let enc = project_path.replace('/', "%2F");
    let url = format!(
        "{}/projects/{}/merge_requests",
        api_base.trim_end_matches('/'),
        enc
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|e| ApiError {
            code: "HTTP_ERROR",
            message: e.to_string(),
        })?;
    let mut req = client
        .get(&url)
        .query(&[
            ("state", "opened"),
            ("per_page", "20"),
            ("source_branch", branch),
        ])
        .header("User-Agent", "DevCommandCenter/forge-pr");
    if let Some(t) = gitlab_token(token) {
        req = req.header("PRIVATE-TOKEN", t.trim());
    }
    let resp = req.send().await.map_err(|e| ApiError {
        code: "HTTP_ERROR",
        message: format!("GitLab: {e}"),
    })?;
    let status = resp.status();
    let body_text = resp.text().await.map_err(|e| ApiError {
        code: "HTTP_ERROR",
        message: e.to_string(),
    })?;
    if !status.is_success() {
        return Err(ApiError {
            code: "FORGE_HTTP_ERROR",
            message: format!(
                "GitLab API {}: {}",
                status.as_u16(),
                truncate_body(&body_text, 280)
            ),
        });
    }
    serde_json::from_str(&body_text).map_err(|e| ApiError {
        code: "SERIALIZE_ERROR",
        message: format!("Resposta GitLab inválida: {e}"),
    })
}

fn pick_github_pr_for_branch(pulls: Vec<Value>, branch: &str) -> Option<Value> {
    for p in pulls {
        let head_ref = p
            .get("head")
            .and_then(|h| h.get("ref"))
            .and_then(|x| x.as_str());
        if head_ref == Some(branch) {
            return Some(p);
        }
    }
    None
}

fn pick_gitlab_mr_for_branch(mrs: Vec<Value>, branch: &str) -> Option<Value> {
    for m in mrs {
        let src = m.get("source_branch").and_then(|x| x.as_str());
        if src == Some(branch) {
            return Some(m);
        }
    }
    None
}

/// Procura um PR/MR **aberto** cuja branch de origem coincide com a branch do worktree.
/// Tenta primeiro `gh` / `glab` (auth local); se não houver resultado, usa a API REST (tokens via env ou override).
pub async fn resolve_open_pr_mr_for_branch(
    project_path: &str,
    stored_remote: Option<&str>,
    branch: &str,
    token: Option<&str>,
) -> Result<Option<Value>, ApiError> {
    let b = branch.trim();
    if b.is_empty() {
        return Ok(None);
    }
    let remote = stored_remote
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .or_else(|| git_remote_origin(project_path));
    let Some(remote_raw) = remote else {
        return Ok(None);
    };
    let Some(kind) = classify_git_remote_for_pr(&remote_raw) else {
        return Ok(None);
    };
    match kind {
        ForgeRemoteKind::GitHub {
            api_base,
            owner,
            repo,
        } => {
            if let Some(v) = resolve_github_pr_via_gh(project_path, &owner, &repo, b).await {
                return Ok(Some(v));
            }
            let pulls =
                http_github_list_open_pulls_for_head(&api_base, &owner, &repo, b, token).await?;
            let pr = pick_github_pr_for_branch(pulls, b);
            let Some(p) = pr else {
                return Ok(None);
            };
            let number = p.get("number").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            let title = p
                .get("title")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let url = p
                .get("html_url")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            if url.is_empty() || number == 0 {
                return Ok(None);
            }
            let synced = Utc::now().to_rfc3339();
            Ok(Some(json!({
                "forge": "github",
                "number": number,
                "url": url,
                "title": title,
                "branch": b,
                "syncedAt": synced,
            })))
        }
        ForgeRemoteKind::GitLab {
            api_base,
            project_path: gl_project_path,
        } => {
            if let Some(v) = resolve_gitlab_mr_via_glab(project_path, &gl_project_path, b).await {
                return Ok(Some(v));
            }
            let mrs =
                http_gitlab_list_open_mrs_for_branch(&api_base, &gl_project_path, b, token).await?;
            let mr = pick_gitlab_mr_for_branch(mrs, b);
            let Some(m) = mr else {
                return Ok(None);
            };
            let iid = m.get("iid").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            let title = m
                .get("title")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let url = m
                .get("web_url")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            if url.is_empty() || iid == 0 {
                return Ok(None);
            }
            let synced = Utc::now().to_rfc3339();
            Ok(Some(json!({
                "forge": "gitlab",
                "number": iid,
                "url": url,
                "title": title,
                "branch": b,
                "syncedAt": synced,
            })))
        }
    }
}

fn normalize_github_review_comment(c: &Value) -> Value {
    let path = c
        .get("path")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let line = c
        .get("line")
        .and_then(|x| x.as_u64())
        .map(|x| x as u32)
        .or_else(|| {
            c.get("original_line")
                .and_then(|x| x.as_u64())
                .map(|x| x as u32)
        });
    let body = c
        .get("body")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let author = c
        .get("user")
        .and_then(|u| u.get("login"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let url = c
        .get("html_url")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let created = c
        .get("created_at")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let side = c.get("side").and_then(|x| x.as_str()).unwrap_or("RIGHT");
    json!({
        "path": path,
        "line": line,
        "side": side,
        "body": body,
        "author": author,
        "url": url,
        "createdAt": created,
    })
}

async fn github_pr_review_comments_via_gh(
    project_path: &str,
    owner: &str,
    repo: &str,
    pull_number: u32,
) -> Result<Vec<Value>, String> {
    let endpoint = format!("repos/{}/{}/pulls/{}/comments", owner, repo, pull_number);
    let out = run_forge_cli_stdout(
        "gh",
        project_path,
        &["api", endpoint.as_str(), "--paginate"],
    )
    .await?;
    serde_json::from_slice(&out).map_err(|e| format!("JSON gh api: {e}"))
}

async fn http_github_pull_review_comments(
    api_base: &str,
    owner: &str,
    repo: &str,
    pull_number: u32,
    token: Option<&str>,
) -> Result<Vec<Value>, ApiError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| ApiError {
            code: "HTTP_ERROR".into(),
            message: e.to_string(),
        })?;
    let mut all = Vec::new();
    let mut page = 1u32;
    loop {
        let url = format!(
            "{}/repos/{}/{}/pulls/{}/comments",
            api_base.trim_end_matches('/'),
            owner,
            repo,
            pull_number
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        if let Some(t) = github_token(token) {
            if let Ok(h) = HeaderValue::from_str(&format!("Bearer {}", t.trim())) {
                headers.insert(AUTHORIZATION, h);
            }
        }
        let resp = client
            .get(&url)
            .headers(headers)
            .query(&[("per_page", "100"), ("page", &page.to_string())])
            .header("User-Agent", "DevCommandCenter/forge-pr-comments")
            .send()
            .await
            .map_err(|e| ApiError {
                code: "HTTP_ERROR".into(),
                message: format!("GitHub: {e}"),
            })?;
        let status = resp.status();
        let body_text = resp.text().await.map_err(|e| ApiError {
            code: "HTTP_ERROR".into(),
            message: e.to_string(),
        })?;
        if !status.is_success() {
            return Err(ApiError {
                code: "FORGE_HTTP_ERROR".into(),
                message: format!(
                    "GitHub API {}: {}",
                    status.as_u16(),
                    truncate_body(&body_text, 280)
                ),
            });
        }
        let page_items: Vec<Value> = serde_json::from_str(&body_text).map_err(|e| ApiError {
            code: "SERIALIZE_ERROR".into(),
            message: format!("Resposta GitHub inválida: {e}"),
        })?;
        let n = page_items.len();
        all.extend(page_items);
        if n < 100 {
            break;
        }
        page += 1;
        if page > 40 {
            break;
        }
    }
    Ok(all)
}

async fn gitlab_mr_discussions_via_glab(
    project_path: &str,
    gl_project: &str,
    iid: u32,
) -> Result<Vec<Value>, String> {
    let enc = gl_project.replace('/', "%2F");
    let path = format!("projects/{}/merge_requests/{}/discussions", enc, iid);
    let out = run_forge_cli_stdout("glab", project_path, &["api", path.as_str()]).await?;
    serde_json::from_slice(&out).map_err(|e| format!("JSON glab api: {e}"))
}

async fn http_gitlab_mr_discussions_pages(
    api_base: &str,
    project_path: &str,
    iid: u32,
    token: Option<&str>,
) -> Result<Vec<Value>, ApiError> {
    let enc = project_path.replace('/', "%2F");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| ApiError {
            code: "HTTP_ERROR".into(),
            message: e.to_string(),
        })?;
    let mut all = Vec::new();
    let mut page = 1u32;
    loop {
        let url = format!(
            "{}/projects/{}/merge_requests/{}/discussions",
            api_base.trim_end_matches('/'),
            enc,
            iid
        );
        let mut req = client
            .get(&url)
            .query(&[("per_page", "100"), ("page", &page.to_string())])
            .header("User-Agent", "DevCommandCenter/forge-mr-discussions");
        if let Some(t) = gitlab_token(token) {
            req = req.header("PRIVATE-TOKEN", t.trim());
        }
        let resp = req.send().await.map_err(|e| ApiError {
            code: "HTTP_ERROR".into(),
            message: format!("GitLab: {e}"),
        })?;
        let status = resp.status();
        let body_text = resp.text().await.map_err(|e| ApiError {
            code: "HTTP_ERROR".into(),
            message: e.to_string(),
        })?;
        if !status.is_success() {
            return Err(ApiError {
                code: "FORGE_HTTP_ERROR".into(),
                message: format!(
                    "GitLab API {}: {}",
                    status.as_u16(),
                    truncate_body(&body_text, 280)
                ),
            });
        }
        let page_items: Vec<Value> = serde_json::from_str(&body_text).map_err(|e| ApiError {
            code: "SERIALIZE_ERROR".into(),
            message: format!("Resposta GitLab inválida: {e}"),
        })?;
        let n = page_items.len();
        all.extend(page_items);
        if n < 100 {
            break;
        }
        page += 1;
        if page > 40 {
            break;
        }
    }
    Ok(all)
}

fn collect_gitlab_inline_notes(discussions: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    for d in discussions {
        let Some(notes) = d.get("notes").and_then(|x| x.as_array()) else {
            continue;
        };
        for n in notes {
            if n.get("system").and_then(|x| x.as_bool()) == Some(true) {
                continue;
            }
            let body = n
                .get("body")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            if body.trim().is_empty() {
                continue;
            }
            let author = n
                .get("author")
                .and_then(|a| a.get("username"))
                .and_then(|x| x.as_str())
                .or_else(|| {
                    n.get("author")
                        .and_then(|a| a.get("name"))
                        .and_then(|x| x.as_str())
                })
                .unwrap_or("")
                .to_string();
            let pos = n.get("position");
            let (path, line) = if let Some(p) = pos {
                let new_path = p.get("new_path").and_then(|x| x.as_str()).unwrap_or("");
                let new_line = p.get("new_line").and_then(|x| x.as_u64()).map(|x| x as u32);
                if new_path.is_empty() {
                    continue;
                }
                (new_path.to_string(), new_line)
            } else {
                continue;
            };
            let url = n
                .get("web_url")
                .or_else(|| n.get("url"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let created = n
                .get("created_at")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            out.push(json!({
                "path": path,
                "line": line,
                "side": "RIGHT",
                "body": body,
                "author": author,
                "url": url,
                "createdAt": created,
            }));
        }
    }
    out
}

/// Comentários inline no PR (GitHub) ou discussões posicionadas no MR (GitLab).
/// Tenta `gh api` / `glab api` primeiro; depois REST com tokens (`GITHUB_TOKEN`, `GITLAB_TOKEN`, ou override).
pub async fn fetch_pr_review_comments(
    project_path: &str,
    stored_remote: Option<&str>,
    forge_link: &Value,
    token: Option<&str>,
) -> Result<Value, ApiError> {
    let number = forge_link
        .get("number")
        .and_then(|x| x.as_u64())
        .unwrap_or(0) as u32;
    if number == 0 {
        return Ok(json!({
            "success": false,
            "skipped": true,
            "reason": "invalid_forge_link",
            "comments": [],
        }));
    }
    let remote = stored_remote
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .or_else(|| git_remote_origin(project_path));
    let Some(remote_raw) = remote else {
        return Ok(json!({
            "success": false,
            "skipped": true,
            "reason": "no_remote",
            "comments": [],
        }));
    };
    let Some(kind) = classify_git_remote_for_pr(&remote_raw) else {
        return Ok(json!({
            "success": false,
            "skipped": true,
            "reason": "unknown_remote",
            "comments": [],
        }));
    };
    match kind {
        ForgeRemoteKind::GitHub {
            api_base,
            owner,
            repo,
        } => {
            let raw = match github_pr_review_comments_via_gh(&project_path, &owner, &repo, number)
                .await
            {
                Ok(v) => v,
                Err(_) => {
                    http_github_pull_review_comments(&api_base, &owner, &repo, number, token)
                        .await?
                }
            };
            let normalized: Vec<Value> = raw
                .iter()
                .map(|c| normalize_github_review_comment(c))
                .collect();
            Ok(json!({
                "success": true,
                "forge": "github",
                "comments": normalized,
            }))
        }
        ForgeRemoteKind::GitLab {
            api_base,
            project_path: gl_path,
        } => {
            let discussions =
                match gitlab_mr_discussions_via_glab(&project_path, &gl_path, number).await {
                    Ok(v) => v,
                    Err(_) => {
                        http_gitlab_mr_discussions_pages(&api_base, &gl_path, number, token).await?
                    }
                };
            let normalized = collect_gitlab_inline_notes(&discussions);
            Ok(json!({
                "success": true,
                "forge": "gitlab",
                "comments": normalized,
            }))
        }
    }
}
