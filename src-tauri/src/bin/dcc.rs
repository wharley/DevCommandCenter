use dev_command_center_tauri::daemon_client::{
    default_app_data_dir, read_runtime_info, rpc_with_db_path_timeout, rpc_with_info,
    sidecar_binary_candidates, DaemonRuntimeInfo,
};
use serde_json::Value;
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn locate_sidecar_binary() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    let binary_names = sidecar_binary_candidates();
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            for name in &binary_names {
                candidates.push(parent.join(name));
            }
            if let Some(grand_parent) = parent.parent() {
                for name in &binary_names {
                    candidates.push(grand_parent.join(name));
                }
            }
        }
    }
    candidates.into_iter().find(|candidate| candidate.exists())
}

fn wait_for_runtime_info(app_data_dir: &Path, timeout: Duration) -> Option<DaemonRuntimeInfo> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if let Some(info) = read_runtime_info(app_data_dir) {
            return Some(info);
        }
        thread::sleep(Duration::from_millis(150));
    }
    None
}

fn ensure_daemon_running() -> Result<DaemonRuntimeInfo, String> {
    let app_data_dir = default_app_data_dir();
    if let Some(info) = read_runtime_info(&app_data_dir) {
        if rpc_with_db_path_timeout(
            std::path::Path::new(&info.db_path),
            "daemon.getStatus",
            serde_json::json!({}),
            Duration::from_secs(2),
        )
        .is_ok()
        {
            return Ok(info);
        }
    }

    let db_path = app_data_dir.join("database.sqlite");
    let mut command = if let Some(binary) = locate_sidecar_binary() {
        Command::new(binary)
    } else {
        let mut cargo = Command::new("cargo");
        cargo
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env(
                "CARGO_TARGET_DIR",
                std::env::temp_dir().join("dcc-sidecar-target"),
            )
            .args(["run", "--quiet", "--bin", "dccd", "--"]);
        cargo
    };
    let child = command
        .env("DCC_DAEMON_DB_PATH", &db_path)
        .env("DCC_DAEMON_APP_DATA_DIR", &app_data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let _ = child.id();

    wait_for_runtime_info(&app_data_dir, Duration::from_secs(30))
        .ok_or_else(|| "daemon started but did not publish runtime info".to_string())
}

fn call_daemon(method: &str, params: Value) -> Result<Value, String> {
    let info = ensure_daemon_running()?;
    rpc_with_info(&info, method, params)
}

fn print_json(value: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
    );
}

fn print_usage() {
    eprintln!("Uso:");
    eprintln!("  dcc daemon status");
    eprintln!("  dcc daemon tasks");
    eprintln!("  dcc daemon run <project-id> <task-id>");
    eprintln!("  dcc daemon attach <project-id> <task-id>");
    eprintln!("  dcc daemon detach <project-id> <task-id>");
    eprintln!("  dcc mcp");
}

fn read_mcp_message(stdin: &mut io::StdinLock<'_>) -> Option<Value> {
    let mut content_length = None;
    let mut line = String::new();

    loop {
        line.clear();
        if stdin.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse::<usize>().ok();
            }
        }
    }

    let len = content_length?;
    let mut body = vec![0u8; len];
    stdin.read_exact(&mut body).ok()?;
    serde_json::from_slice::<Value>(&body).ok()
}

fn write_mcp_message(stdout: &mut io::StdoutLock<'_>, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message).unwrap_or_else(|_| b"{}".to_vec());
    write!(stdout, "Content-Length: {}\r\n\r\n", body.len())?;
    stdout.write_all(&body)?;
    stdout.flush()
}

fn mcp_tool_list() -> Value {
    serde_json::json!({
        "tools": [
            {
                "name": "daemon_status",
                "description": "Retorna o resumo do daemon local.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "daemon_tasks",
                "description": "Lista as tasks registradas no daemon.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "daemon_run_task",
                "description": "Executa uma task do daemon agora.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "projectId": { "type": "string" },
                        "taskId": { "type": "string" }
                    },
                    "required": ["projectId", "taskId"]
                }
            },
            {
                "name": "daemon_attach_task",
                "description": "Marca a task como anexada.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "projectId": { "type": "string" },
                        "taskId": { "type": "string" }
                    },
                    "required": ["projectId", "taskId"]
                }
            },
            {
                "name": "daemon_detach_task",
                "description": "Desmarca a task como anexada.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "projectId": { "type": "string" },
                        "taskId": { "type": "string" }
                    },
                    "required": ["projectId", "taskId"]
                }
            },
            {
                "name": "combs_list",
                "description": "Lista combs existentes, opcionalmente filtrando por projeto.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "projectId": { "type": "string" }
                    }
                }
            },
            {
                "name": "panes_list",
                "description": "Lista panes existentes, opcionalmente filtrando por projeto ou comb.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "projectId": { "type": "string" },
                        "combId": { "type": "string" }
                    }
                }
            },
            {
                "name": "diffs_bundle",
                "description": "Agrupa diffs de worktrees e combs em um bundle de revisão.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "worktreePaths": {
                            "type": "array",
                            "items": { "type": "string" }
                        },
                        "combIds": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    }
                }
            },
            {
                "name": "daemon_health",
                "description": "Snapshot de saúde do daemon (CPU/memória, processos vivos).",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "processes_list",
                "description": "Lista processos de longa duração gerenciados (dev servers, watchers, agentes) e seu status, opcionalmente por projeto.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "projectId": { "type": "string" }
                    }
                }
            },
            {
                "name": "process_start",
                "description": "Inicia um processo gerenciado.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "projectId": { "type": "string" },
                        "processId": { "type": "string" }
                    },
                    "required": ["projectId", "processId"]
                }
            },
            {
                "name": "process_stop",
                "description": "Para um processo gerenciado.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "projectId": { "type": "string" },
                        "processId": { "type": "string" }
                    },
                    "required": ["projectId", "processId"]
                }
            },
            {
                "name": "process_restart",
                "description": "Reinicia um processo gerenciado.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "projectId": { "type": "string" },
                        "processId": { "type": "string" }
                    },
                    "required": ["projectId", "processId"]
                }
            }
        ]
    })
}

fn handle_mcp_request(request: Value) -> Option<Value> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method")?.as_str()?.to_string();

    if method == "initialize" {
        return Some(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "serverInfo": {
                    "name": "dcc",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {
                    "tools": {}
                }
            }
        }));
    }

    if method == "notifications/initialized" {
        return None;
    }

    if method == "tools/list" {
        return Some(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": mcp_tool_list()
        }));
    }

    if method == "tools/call" {
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        let tool_name = params
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
        let result = match tool_name {
            "daemon_status" => call_daemon("daemon.getStatus", serde_json::json!({})),
            "daemon_tasks" => call_daemon("daemon.listTasks", serde_json::json!({})),
            "daemon_run_task" => {
                let project_id = arguments
                    .get("projectId")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let task_id = arguments
                    .get("taskId")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                call_daemon(
                    "daemon.runTask",
                    serde_json::json!({ "projectId": project_id, "taskId": task_id }),
                )
            }
            "daemon_attach_task" => {
                let project_id = arguments
                    .get("projectId")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let task_id = arguments
                    .get("taskId")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                call_daemon(
                    "daemon.attachTask",
                    serde_json::json!({ "projectId": project_id, "taskId": task_id }),
                )
            }
            "daemon_detach_task" => {
                let project_id = arguments
                    .get("projectId")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let task_id = arguments
                    .get("taskId")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                call_daemon(
                    "daemon.detachTask",
                    serde_json::json!({ "projectId": project_id, "taskId": task_id }),
                )
            }
            "combs_list" => {
                let project_id = arguments.get("projectId").and_then(|value| value.as_str());
                call_daemon("combs.list", serde_json::json!({ "projectId": project_id }))
            }
            "panes_list" => {
                let project_id = arguments.get("projectId").and_then(|value| value.as_str());
                let comb_id = arguments.get("combId").and_then(|value| value.as_str());
                call_daemon(
                    "panes.list",
                    serde_json::json!({ "projectId": project_id, "combId": comb_id }),
                )
            }
            "diffs_bundle" => {
                let worktree_paths = arguments
                    .get("worktreePaths")
                    .and_then(|value| value.as_array())
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|value| value.as_str().map(|s| s.to_string()))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let comb_ids = arguments
                    .get("combIds")
                    .and_then(|value| value.as_array())
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|value| value.as_str().map(|s| s.to_string()))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                call_daemon(
                    "diffs.bundle",
                    serde_json::json!({ "worktreePaths": worktree_paths, "combIds": comb_ids }),
                )
            }
            "daemon_health" => call_daemon("daemon.health", serde_json::json!({})),
            "processes_list" => {
                let project_id = arguments.get("projectId").and_then(|value| value.as_str());
                call_daemon(
                    "daemon.listProcesses",
                    serde_json::json!({ "projectId": project_id }),
                )
            }
            "process_start" | "process_stop" | "process_restart" => {
                let project_id = arguments
                    .get("projectId")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let process_id = arguments
                    .get("processId")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let method = match tool_name {
                    "process_start" => "daemon.startProcess",
                    "process_stop" => "daemon.stopProcess",
                    _ => "daemon.restartProcess",
                };
                call_daemon(
                    method,
                    serde_json::json!({ "projectId": project_id, "processId": process_id }),
                )
            }
            _ => Err(format!("tool desconhecida: {tool_name}")),
        };

        return Some(match result {
            Ok(payload) => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [
                        {
                            "type": "text",
                            "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
                        }
                    ]
                }
            }),
            Err(error) => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32000,
                    "message": error
                }
            }),
        });
    }

    if method == "shutdown" {
        return Some(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": Value::Null
        }));
    }

    Some(serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": -32601,
            "message": format!("método MCP não suportado: {method}")
        }
    }))
}

fn serve_mcp() -> Result<(), String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdin = stdin.lock();
    let mut stdout = stdout.lock();
    while let Some(message) = read_mcp_message(&mut stdin) {
        if let Some(response) = handle_mcp_request(message) {
            write_mcp_message(&mut stdout, &response).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn main() {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        print_usage();
        std::process::exit(1);
    }

    match args.remove(0).as_str() {
        "daemon" => {
            if args.is_empty() {
                print_usage();
                std::process::exit(1);
            }
            let subcommand = args.remove(0);
            let result = match subcommand.as_str() {
                "status" => call_daemon("daemon.getStatus", serde_json::json!({})),
                "tasks" => call_daemon("daemon.listTasks", serde_json::json!({})),
                "run" if args.len() >= 2 => call_daemon(
                    "daemon.runTask",
                    serde_json::json!({ "projectId": args[0].clone(), "taskId": args[1].clone() }),
                ),
                "attach" if args.len() >= 2 => call_daemon(
                    "daemon.attachTask",
                    serde_json::json!({ "projectId": args[0].clone(), "taskId": args[1].clone() }),
                ),
                "detach" if args.len() >= 2 => call_daemon(
                    "daemon.detachTask",
                    serde_json::json!({ "projectId": args[0].clone(), "taskId": args[1].clone() }),
                ),
                _ => {
                    print_usage();
                    std::process::exit(1);
                }
            };

            match result {
                Ok(value) => print_json(&value),
                Err(error) => {
                    eprintln!("{error}");
                    std::process::exit(1);
                }
            }
        }
        "mcp" => {
            if let Err(error) = serve_mcp() {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        _ => {
            print_usage();
            std::process::exit(1);
        }
    }
}
