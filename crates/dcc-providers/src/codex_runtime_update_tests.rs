use super::*;
use crate::test_cli::TestCli;
use dcc_core::{
    domain::workspace::WorkspaceId,
    ports::{ProviderMcpServerConfig, ProviderMcpTransport},
};

fn adapter(cli: &TestCli) -> CodexAppServerAdapter {
    let mut adapter = CodexAppServerAdapter::with_runtime_detection(
        crate::codex::stable_codex_capabilities(),
        CodexMcpProjection::from_cli_output("codex-cli 0.153.4"),
        true,
    );
    adapter.binary = cli.binary.clone();
    adapter
}

fn config(session: &str) -> SessionConfig {
    SessionConfig {
        workspace_id: WorkspaceId("fixture".to_string()),
        session_id: SessionId(session.to_string()),
        model: None,
        working_directory: None,
        additional_working_directories: Vec::new(),
        provider_runtime: None,
        mcp_servers: vec![ProviderMcpServerConfig {
            definition_id: McpDefinitionId("fixture".to_string()),
            server_name: "dcc-fixture".to_string(),
            transport: ProviderMcpTransport::Stdio {
                executable: "fixture".to_string(),
                args: Vec::new(),
                cwd: None,
                environment: Vec::new(),
            },
            oauth_state: None,
            tool_policies: Vec::new(),
        }],
        native_resume_id: None,
    }
}

async fn stop(adapter: &CodexAppServerAdapter) {
    let sessions = std::mem::take(&mut *adapter.state.sessions.lock().await);
    for runtime in sessions.values() {
        let _ = runtime.child.lock().await.kill().await;
    }
}

#[tokio::test]
async fn cli_update_refreshes_new_sessions_and_preserves_running_sessions() {
    let cli = TestCli::new("codex-cli 0.153.4");
    let adapter = adapter(&cli);
    let old_handle = adapter.prepare_session(config("old")).await.unwrap();
    let old_runtime = adapter
        .session_runtime(&old_handle.session_id)
        .await
        .unwrap();
    cli.write("version", "codex-cli 0.154.0");
    cli.write("features", "");
    let new_handle = adapter.prepare_session(config("new")).await.unwrap();
    let new_runtime = adapter
        .session_runtime(&new_handle.session_id)
        .await
        .unwrap();

    assert_eq!(
        adapter.dcc_mcp_projection_version(),
        Some(codex_mcp_runtime_version("0.154.0"))
    );
    assert!(!adapter.capabilities().supports_native_subagent_steering);
    assert!(old_runtime.multi_agent_v2_supported);
    assert!(!new_runtime.multi_agent_v2_supported);
    for (handle, version) in [(&old_handle, "0.153.4"), (&new_handle, "0.154.0")] {
        assert_eq!(
            adapter.session_mcp_projection_version(handle).await,
            Some(codex_mcp_runtime_version(version))
        );
        let event = adapter.stream_events(handle).next().await.unwrap().unwrap();
        let ProviderEvent::McpRuntimeStatusSnapshot { statuses } = event else {
            panic!("missing MCP snapshot")
        };
        assert!(!statuses.is_empty());
        assert!(statuses
            .iter()
            .all(|status| status.provider_version == codex_mcp_runtime_version(version)));
        adapter
            .send_input(handle, Input::Text("hello".to_string()))
            .await
            .unwrap();
    }
    assert_eq!(cli.log().matches("\"method\":\"turn/start\"").count(), 2);

    // A delayed reader from an earlier attempt must not remove its replacement.
    adapter
        .state
        .remove_runtime(&new_handle.session_id.0, &old_runtime)
        .await;
    assert_eq!(
        adapter
            .resume(&new_handle.session_id)
            .await
            .unwrap()
            .handle_id,
        new_handle.handle_id
    );
    stop(&adapter).await;
}

#[tokio::test]
async fn update_during_handshake_retries_before_starting_thread_or_sending_input() {
    let cli = TestCli::new("codex-cli 0.153.4");
    cli.write("next-version", "codex-cli 0.154.0");
    let adapter = adapter(&cli);
    let handle = adapter.prepare_session(config("race")).await.unwrap();
    assert_eq!(
        adapter.session_mcp_projection_version(&handle).await,
        Some(codex_mcp_runtime_version("0.154.0"))
    );
    adapter
        .send_input(&handle, Input::Text("hello".to_string()))
        .await
        .unwrap();
    let log = cli.log();
    assert_eq!(log.matches("\"method\":\"initialize\"").count(), 2);
    assert_eq!(log.matches("\"method\":\"thread/start\"").count(), 1);
    assert_eq!(log.matches("\"method\":\"turn/start\"").count(), 1);
    assert_eq!(adapter.state.sessions.lock().await.len(), 1);
    stop(&adapter).await;
}

#[tokio::test]
async fn persistent_version_mismatch_stops_after_one_retry_without_user_input() {
    let cli = TestCli::new("codex-cli 0.153.4");
    cli.write("user-agent", "dcc/0.154.0");
    let adapter = adapter(&cli);
    let error = adapter
        .prepare_session(config("mismatch"))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("automatic retry"));
    assert_eq!(cli.log().matches("\"method\":\"initialize\"").count(), 2);
    assert!(!cli.log().contains("\"method\":\"thread/start\""));
    assert!(!cli.log().contains("\"method\":\"turn/start\""));
    assert!(adapter.state.sessions.lock().await.is_empty());
}

#[tokio::test]
async fn malformed_handshake_and_thread_errors_are_not_retried() {
    for (suffix, value) in [("user-agent", "malformed"), ("thread-error", "fail")] {
        let cli = TestCli::new("codex-cli 0.153.4");
        cli.write(suffix, value);
        let adapter = adapter(&cli);
        assert!(adapter.prepare_session(config("invalid")).await.is_err());
        assert_eq!(cli.log().matches("\"method\":\"initialize\"").count(), 1);
        assert!(!cli.log().contains("\"method\":\"turn/start\""));
        assert!(adapter.state.sessions.lock().await.is_empty());
    }
}

#[tokio::test]
async fn metadata_refresh_recovers_after_cli_becomes_available() {
    let cli = TestCli::new("invalid version output");
    let adapter = adapter(&cli);
    assert!(adapter.refresh_runtime_metadata().await.is_err());
    assert_eq!(adapter.dcc_mcp_projection_version(), None);
    assert!(adapter
        .prepare_session(config("unavailable"))
        .await
        .is_err());
    assert!(cli.log().is_empty());
    cli.write("version", "codex-cli 0.154.0");
    adapter.refresh_runtime_metadata().await.unwrap();
    assert_eq!(
        adapter.dcc_mcp_projection_version(),
        Some(codex_mcp_runtime_version("0.154.0"))
    );
    let handle = adapter.prepare_session(config("recovered")).await.unwrap();
    assert_eq!(
        adapter.session_mcp_projection_version(&handle).await,
        Some(codex_mcp_runtime_version("0.154.0"))
    );
    stop(&adapter).await;
}

#[tokio::test]
async fn native_thread_resumes_and_falls_back_to_a_fresh_thread_with_the_snapshot() {
    for (native_id, expected_thread, resumed) in [
        (None, "thread-fixture", false),
        (Some("thread-kept"), "thread-kept", true),
        (Some("thread-gone"), "thread-fixture", false),
    ] {
        let cli = TestCli::new("codex-cli 0.153.4");
        let adapter = adapter(&cli);
        assert!(adapter.supports_native_resume());
        let mut cfg = config("native");
        cfg.native_resume_id = native_id.map(str::to_string);
        let handle = adapter.prepare_session(cfg).await.unwrap();
        let mut events = adapter.stream_events(&handle);
        adapter
            .send_input(
                &handle,
                Input::Turn(ProviderTurnInput {
                    prompt: "next".to_string(),
                    tool_instructions: None,
                    plan_mode: None,
                    effort: None,
                    fast_mode: None,
                    approval_policy: None,
                    resume_fallback_context: native_id
                        .map(|_| "<dcc_reanchor>snapshot</dcc_reanchor>".to_string()),
                }),
            )
            .await
            .unwrap();

        // DCC learns the thread to persist on the first input.
        let reported = loop {
            match tokio::time::timeout(Duration::from_secs(5), events.next())
                .await
                .expect("native session event")
            {
                Some(Ok(ProviderEvent::NativeSessionChanged { native_session_id })) => {
                    break native_session_id
                }
                Some(_) => continue,
                None => panic!("stream ended"),
            }
        };
        assert_eq!(reported.as_deref(), Some(expected_thread));

        let log = cli.log();
        let resume_attempts = log.matches("\"method\":\"thread/resume\"").count();
        assert_eq!(resume_attempts, usize::from(native_id.is_some()));
        assert_eq!(
            log.matches("\"method\":\"thread/start\"").count(),
            usize::from(!resumed)
        );
        let turn = log
            .lines()
            .find(|line| line.contains("\"method\":\"turn/start\""))
            .expect("turn/start");
        assert!(turn.contains(&format!("\"threadId\":\"{expected_thread}\"")));
        // The held-back snapshot travels only when the resume was refused.
        assert_eq!(
            turn.contains("<dcc_reanchor>snapshot"),
            native_id.is_some() && !resumed
        );
        if resumed {
            let resume = log
                .lines()
                .find(|line| line.contains("\"method\":\"thread/resume\""))
                .unwrap();
            assert!(resume.contains("\"excludeTurns\":true"));
            assert!(resume.contains("\"mcp_servers\""));
        }
        stop(&adapter).await;
    }
}

#[test]
fn native_thread_ids_are_bounded_opaque_tokens() {
    assert_eq!(
        valid_codex_thread_id(" 019a5f3e-7b1c-7d40-a2b9-3c1d2e4f5a6b "),
        Some("019a5f3e-7b1c-7d40-a2b9-3c1d2e4f5a6b")
    );
    assert_eq!(valid_codex_thread_id("../../etc/passwd"), None);
    assert_eq!(valid_codex_thread_id(""), None);
    assert_eq!(valid_codex_thread_id(&"a".repeat(257)), None);

    let params = thread_resume_params("thread-1", "/repo", &["/extra".to_string()], Some("gpt-5"));
    assert_eq!(params["threadId"], "thread-1");
    assert_eq!(params["excludeTurns"], true);
    assert_eq!(params["runtimeWorkspaceRoots"], json!(["/repo", "/extra"]));
    assert!(params.get("experimentalRawEvents").is_none());
}
