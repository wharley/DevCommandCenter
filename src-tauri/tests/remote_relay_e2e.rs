//! End to end through a real relay: `wrangler dev` (apps/relay) + this
//! process serving the API like the desktop app does (embedded backend over a
//! `SessionCommandState`, temporary database) + a Node "phone" using
//! `packages/remote-protocol`.
//!
//! Run it with `node scripts/e2e-remote.mjs`, which starts the relay and sets
//! `DCC_E2E_RELAY_URL`. Without that variable the test does nothing.
//!
//! Push notifications go to a fake Expo push service started here: the
//! phone registers its token over the channel, and a permission request
//! reaches the fake only while the phone says it is in the background.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use dcc_core::domain::project::ProjectId;
use dcc_core::domain::session::{
    Session, SessionEventKind, SessionEventRecord, SessionId, SessionState, TurnId,
};
use dcc_core::domain::thread::{Thread, ThreadId};
use dcc_core::domain::workspace::WorkspaceId;
use dcc_core::ports::events::CoreEvent;
use dcc_core::ports::repository::{AppendEventOutcome, SessionEventRepo, SessionRepo, ThreadRepo};
use dcc_core::ports::EventBus;
use dcc_tauri::state::SessionCommandState;
use dev_command_center_tauri::http_api::{build_router, HttpBackend};
use dev_command_center_tauri::http_config::HttpConfig;
use dev_command_center_tauri::pairing;
use dev_command_center_tauri::remote_connector::{RemoteEvent, RemoteHub};
use dev_command_center_tauri::remote_push::PushConfig;
use serde_json::{json, Value};
use tokio::sync::RwLock;

const SESSION: &str = "remote-e2e-session";
const WORKSPACE: &str = "remote-e2e-workspace";
const REPLAY_AFTER: u64 = 2;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

async fn append(state: &SessionCommandState, kind: SessionEventKind) -> SessionEventRecord {
    let record = SessionEventRecord {
        event_id: uuid::Uuid::new_v4().to_string(),
        session_id: SessionId(SESSION.to_string()),
        sequence: 0,
        occurred_at: chrono::Utc::now().to_rfc3339(),
        kind,
    };
    match SessionEventRepo::append_event(state, &record)
        .await
        .expect("append event")
    {
        AppendEventOutcome::Inserted(record) | AppendEventOutcome::Existing(record) => record,
    }
}

/// Appends like a provider would and publishes the live envelope.
async fn append_and_publish(state: &SessionCommandState, kind: SessionEventKind, event: CoreEvent) {
    let record = append(state, kind).await;
    EventBus::publish_durable_session(state, &record, event)
        .await
        .expect("publish");
}

async fn seed(state: &SessionCommandState) {
    SessionRepo::save_session(
        state,
        &Session {
            id: SessionId(SESSION.to_string()),
            project_id: ProjectId("remote-e2e-project".to_string()),
            workspace_id: WorkspaceId(WORKSPACE.to_string()),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".to_string(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-10-09T00:00:00Z".to_string(),
            updated_at: "2026-10-09T00:00:00Z".to_string(),
        },
    )
    .await
    .expect("save session");
    ThreadRepo::save_thread(
        state,
        &Thread {
            id: ThreadId("remote-e2e-thread".to_string()),
            project_id: ProjectId("remote-e2e-project".to_string()),
            session_id: Some(SessionId(SESSION.to_string())),
            title: "Sessão do e2e remoto".to_string(),
            archived_at: None,
        },
    )
    .await
    .expect("save thread");
    let turn = || TurnId("t1".to_string());
    // Larger than one Noise message, so reading the history fragments.
    append(
        state,
        SessionEventKind::TurnAssistantMessageDelta {
            turn_id: turn(),
            message_id: "m0".to_string(),
            content: "relatório ".repeat(12_000),
        },
    )
    .await;
    append(
        state,
        SessionEventKind::TurnPermissionRequested {
            turn_id: turn(),
            request_id: "p1".to_string(),
            tool_name: "Bash".to_string(),
            title: None,
            description: None,
            command: Some("yarn test".to_string()),
            file: None,
        },
    )
    .await;
    append(
        state,
        SessionEventKind::TurnPermissionResolved {
            turn_id: turn(),
            request_id: "p1".to_string(),
            behavior: "allow".to_string(),
        },
    )
    .await;
    for piece in ["Rodando ", "os ", "testes."] {
        append(
            state,
            SessionEventKind::TurnAssistantMessageDelta {
                turn_id: turn(),
                message_id: "m0".to_string(),
                content: piece.to_string(),
            },
        )
        .await;
    }
}

async fn publish_live(state: &SessionCommandState) {
    for piece in ["Hello ", "from ", "the ", "desktop", "!"] {
        append_and_publish(
            state,
            SessionEventKind::TurnAssistantMessageDelta {
                turn_id: TurnId("t1".to_string()),
                message_id: "m1".to_string(),
                content: piece.to_string(),
            },
            CoreEvent::SessionTurnAssistantMessageDelta {
                session_id: SESSION.to_string(),
                turn_id: "t1".to_string(),
                message_id: "m1".to_string(),
                content: piece.to_string(),
            },
        )
        .await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    append_and_publish(
        state,
        SessionEventKind::TurnPermissionRequested {
            turn_id: TurnId("t1".to_string()),
            request_id: "p2".to_string(),
            tool_name: "Bash".to_string(),
            title: None,
            description: None,
            command: Some("rm -rf build".to_string()),
            file: None,
        },
        CoreEvent::SessionTurnPermissionRequested {
            session_id: SESSION.to_string(),
            turn_id: "t1".to_string(),
            request_id: "p2".to_string(),
            tool_name: "Bash".to_string(),
            title: None,
            description: None,
            command: Some("rm -rf build".to_string()),
            file: None,
        },
    )
    .await;
}

async fn publish_permission(state: &SessionCommandState, request_id: &str) {
    append_and_publish(
        state,
        SessionEventKind::TurnPermissionRequested {
            turn_id: TurnId("t1".to_string()),
            request_id: request_id.to_string(),
            tool_name: "Bash".to_string(),
            title: None,
            description: None,
            command: Some("cat ~/.ssh/id_rsa".to_string()),
            file: None,
        },
        CoreEvent::SessionTurnPermissionRequested {
            session_id: SESSION.to_string(),
            turn_id: "t1".to_string(),
            request_id: request_id.to_string(),
            tool_name: "Bash".to_string(),
            title: None,
            description: None,
            command: Some("cat ~/.ssh/id_rsa".to_string()),
            file: None,
        },
    )
    .await;
}

type Pushes = Arc<std::sync::Mutex<Vec<Value>>>;

/// Stands in for `https://exp.host/--/api/v2/push`: records every message
/// and accepts it.
async fn fake_expo() -> (String, Pushes) {
    use axum::{extract::State, routing::post, Json, Router};
    let pushes: Pushes = Arc::default();
    let app = Router::new()
        .route(
            "/send",
            post(
                |State(pushes): State<Pushes>, Json(body): Json<Value>| async move {
                    let messages = body.as_array().cloned().unwrap_or_default();
                    let tickets: Vec<Value> = messages
                        .iter()
                        .enumerate()
                        .map(|(n, _)| json!({ "status": "ok", "id": format!("ticket-{n}") }))
                        .collect();
                    pushes.lock().unwrap().extend(messages);
                    Json(json!({ "data": tickets }))
                },
            ),
        )
        .route(
            "/getReceipts",
            post(|| async { Json(json!({ "data": {} })) }),
        )
        .with_state(pushes.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), pushes)
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn phone_pairs_reconnects_streams_and_answers_through_the_relay() {
    let Ok(relay) = std::env::var("DCC_E2E_RELAY_URL") else {
        eprintln!("DCC_E2E_RELAY_URL not set; run `node scripts/e2e-remote.mjs`");
        return;
    };
    let relay_http = relay.replacen("ws://", "http://", 1);
    let node = std::env::var("DCC_E2E_NODE").unwrap_or_else(|_| "node".to_string());

    // The desktop side, on a temporary database.
    let root = tempfile::tempdir().expect("temp root");
    let physical = std::fs::canonicalize(root.path()).expect("physical root");
    let db_path = physical.join("state.sqlite");
    let state = SessionCommandState::new_headless(db_path.clone(), physical.join("app-data"));
    seed(&state).await;
    // The headless runtime does not create the repositories table (the
    // desktop app does); the push names the project from it.
    rusqlite::Connection::open(&db_path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS dcc_repositories (id TEXT PRIMARY KEY NOT NULL,
                 project_id TEXT NOT NULL, name TEXT NOT NULL, display_name TEXT NULL,
                 root_path TEXT NOT NULL UNIQUE, created_at TEXT NOT NULL);
             INSERT INTO dcc_repositories (id, project_id, name, display_name, root_path, created_at)
             VALUES ('repo', 'remote-e2e-project', 'e2e', 'Projeto E2E', '/tmp/e2e-repo',
                 '2026-10-09T00:00:00Z');",
        )
        .expect("project row");
    let (push_api, pushes) = fake_expo().await;
    let config = Arc::new(RwLock::new(HttpConfig {
        db_path: db_path.clone(),
        api_key: "e2e-operator-key".to_string(),
        ..HttpConfig::default()
    }));
    let backend = Arc::new(HttpBackend::embedded(state.clone()).expect("embedded backend"));
    let router = build_router(config, backend.clone());
    let hub = RemoteHub::new(router, db_path.clone()).expect("remote hub");
    hub.configure_push(PushConfig {
        api_base: push_api,
        batch_window: Duration::from_millis(50),
        receipt_delay: Duration::from_millis(100),
        ..PushConfig::default()
    });
    backend.attach_remote(&hub);
    hub.set_relay_url(Some(relay.clone()))
        .await
        .expect("relay url");
    let mut events = hub.subscribe();
    hub.start();

    // The person at the desktop: approves "approve-*", refuses the rest, and
    // remembers which fingerprints were shown.
    let shown = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    {
        let hub = hub.clone();
        let shown = shown.clone();
        tokio::spawn(async move {
            while let Ok(event) = events.recv().await {
                if let RemoteEvent::ApprovalRequested { approval } = event {
                    shown.lock().unwrap().push(approval.fingerprint.clone());
                    let approve = approval.device_name.starts_with("approve-");
                    hub.respond_approval(&approval.id, approve);
                }
            }
        });
    }

    let expired = hub
        .create_offer_expiring_in(Duration::ZERO)
        .await
        .expect("expired offer");
    let deny = hub.create_offer().await.expect("deny offer");
    let approve = hub.create_offer().await.expect("approve offer");
    let room = hub.status().room_id.expect("room id");

    // Wait until the control connection is up before the phone dials.
    for _ in 0..100 {
        if hub.status().connected {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(hub.status().connected, "desktop never reached the relay");

    let phone_config = json!({
        "offers": { "approve": approve.url, "deny": deny.url, "expired": expired.url },
        "workspaceId": WORKSPACE,
        "sessionId": SESSION,
        "replayAfter": REPLAY_AFTER,
        "resolvedRequestId": "p1",
        "pendingRequestId": "p2",
    });
    let mut phone = Command::new(&node)
        .arg(repo_root().join("packages/remote-protocol/test/e2e/phone.ts"))
        .current_dir(repo_root())
        .env("DCC_E2E_CONFIG", phone_config.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn the Node phone");
    let mut stdin = phone.stdin.take().expect("phone stdin");
    let stdout = phone.stdout.take().expect("phone stdout");
    let (lines_tx, mut lines) = tokio::sync::mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if lines_tx.send(line).is_err() {
                break;
            }
        }
    });

    let mut done = None;
    while let Some(line) = tokio::time::timeout(Duration::from_secs(90), lines.recv())
        .await
        .expect("phone went quiet")
    {
        println!("{line}");
        let Some(command) = line.strip_prefix("@@") else {
            continue;
        };
        let (name, payload) = command.split_once(' ').unwrap_or((command, "{}"));
        let payload: Value = serde_json::from_str(payload).unwrap_or(Value::Null);
        match name {
            "permission-in-foreground" => {
                // The phone said it is looking: no push.
                publish_permission(&state, "p-foreground").await;
                tokio::time::sleep(Duration::from_millis(800)).await;
                let seen = pushes.lock().unwrap().clone();
                let reply = if seen.is_empty() {
                    "go".to_string()
                } else {
                    format!("pushed while in the foreground: {seen:?}")
                };
                writeln!(stdin, "{reply}").unwrap();
            }
            "live" => {
                publish_live(&state).await;
                // In the background now: the permission request in
                // `publish_live` reaches the fake Expo.
                let mut seen = Vec::new();
                for _ in 0..100 {
                    seen = pushes.lock().unwrap().clone();
                    if !seen.is_empty() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                assert_eq!(
                    seen.len(),
                    1,
                    "one push for the permission request: {seen:?}"
                );
                let push = &seen[0];
                assert_eq!(push["to"], payload["token"]);
                assert_eq!(push["title"], "DCC");
                assert_eq!(push["body"], "Projeto E2E precisa de você");
                assert_eq!(
                    push["data"],
                    json!({ "sessionId": SESSION, "kind": "permission" })
                );
                let wire = push.to_string();
                assert!(!wire.contains("rm -rf") && !wire.contains("Bash"), "{wire}");
                println!("[desktop] push sent to the fake Expo: {wire}");
                writeln!(stdin, "go").unwrap();
            }
            "permission-answered" => {
                // No agent CLI runs in this test, so the answer cannot be
                // forwarded: the reservation is released and the request
                // stays answerable, exactly as on the desktop when the agent
                // is gone. The phone's call went through the same rule.
                let status = payload["status"].as_u64().unwrap_or_default();
                let body = payload["body"].as_str().unwrap_or_default();
                let reply = if status == 500 && body.contains("provider") {
                    "go".to_string()
                } else {
                    format!("unexpected {status} {body}")
                };
                writeln!(stdin, "{reply}").unwrap();
            }
            "revoke" => {
                let device_id = payload["deviceId"].as_str().expect("device id").to_string();
                // A QR left open keeps the connector on the relay after the
                // last phone is revoked, so the revoked key meets the desktop
                // (and is refused) instead of an offline room.
                hub.create_offer().await.expect("keep-alive offer");
                let conn = rusqlite::Connection::open(&db_path).unwrap();
                assert!(pairing::revoke_device(&conn, &device_id, None).unwrap());
                hub.device_revoked(&device_id);
                writeln!(stdin, "go").unwrap();
            }
            "done" => {
                done = Some(payload);
                break;
            }
            other => panic!("unknown phone command {other}"),
        }
    }
    let status = phone.wait().expect("phone exit");
    assert!(status.success(), "the phone failed: {status}");
    let done = done.expect("phone finished");

    // The person saw the same fingerprint the phone computed.
    let shown = shown.lock().unwrap().clone();
    assert!(
        shown.contains(&done["fingerprint"].as_str().unwrap().to_string()),
        "fingerprints shown on the desktop: {shown:?}"
    );

    // The device was pinned (relay kind) and later revoked; refusals were
    // audited.
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let devices = pairing::list_paired_devices(&conn, true).unwrap();
    let device = devices
        .iter()
        .find(|device| device.device_id == done["deviceId"].as_str().unwrap())
        .expect("paired device row");
    assert_eq!(device.kind, "relay");
    assert!(device.revoked);
    let token: Option<String> = conn
        .query_row(
            "SELECT push_token FROM paired_devices WHERE device_id = ?1",
            [&device.device_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(token, None, "revoking took the push token with it");
    assert_eq!(devices.len(), 1, "refused phones are not stored");
    let audit: Vec<String> = pairing::list_audit_log(&conn, 100)
        .unwrap()
        .into_iter()
        .map(|entry| entry.event)
        .collect();
    for event in [
        "pair",
        "push_token_set",
        "push_token_cleared",
        "relay_offer_invalid",
        "relay_pair_denied",
        "relay_refused",
        "revoke",
    ] {
        assert!(
            audit.iter().any(|e| e == event),
            "audit has {event}: {audit:?}"
        );
    }

    // Everything the relay saw was ciphertext.
    let traffic: Value = reqwest::Client::new()
        .get(format!("{relay_http}/debug/rooms/{room}/traffic"))
        .send()
        .await
        .expect("traffic dump")
        .json()
        .await
        .expect("traffic json");
    let entries = traffic["entries"].as_array().expect("entries");
    assert!(
        entries.len() > 20,
        "dump has the session's frames: {}",
        entries.len()
    );
    let markers = [
        SESSION,
        WORKSPACE,
        "/api/v1",
        "approve-phone",
        "deny-phone",
        "Hello",
        "desktop!",
        "relat",
        "yarn test",
        "rm -rf",
        "sessionTurn",
        "permission",
        "\"t\":",
        "requestId",
        "PushToken",
        "presence",
        approve.url.split("otp=").nth(1).unwrap(),
    ];
    let mut bytes_seen = 0usize;
    for entry in entries {
        let frame = base64::engine::general_purpose::STANDARD
            .decode(entry["base64"].as_str().unwrap())
            .unwrap();
        bytes_seen += frame.len();
        assert!(
            matches!(frame.first(), Some(1..=3)),
            "every frame is a handshake or transport envelope"
        );
        for marker in markers {
            assert!(!contains(&frame, marker), "relay saw plaintext {marker:?}");
        }
    }
    println!(
        "[desktop] relay dump: {} frames, {bytes_seen} bytes, no plaintext",
        entries.len()
    );
}
