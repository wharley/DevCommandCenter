use super::*;
use crate::test_cli::TestCli;
use dcc_core::domain::workspace::WorkspaceId;

fn adapter(cli: &TestCli) -> CodexAppServerAdapter {
    let mut adapter = CodexAppServerAdapter::with_runtime_detection(
        crate::codex::stable_codex_capabilities(),
        CodexMcpProjection::from_cli_output("codex-cli 0.153.4"),
        false,
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
        mcp_servers: Vec::new(),
        native_resume_id: None,
        native_rewind_checkpoint: None,
    }
}

fn turn(prompt: &str) -> Input {
    Input::Turn(ProviderTurnInput {
        prompt: prompt.to_string(),
        tool_instructions: None,
        plan_mode: None,
        effort: None,
        fast_mode: None,
        approval_policy: None,
        resume_fallback_context: None,
    })
}

async fn stop(adapter: &CodexAppServerAdapter) {
    let sessions = std::mem::take(&mut *adapter.state.sessions.lock().await);
    for runtime in sessions.values() {
        let _ = runtime.child.lock().await.kill().await;
    }
}

/// Events until the stream is quiet for `quiet`.
async fn drain(
    events: &mut BoxStream<'static, Result<ProviderEvent>>,
    quiet: Duration,
) -> Vec<ProviderEvent> {
    let mut seen = Vec::new();
    while let Ok(Some(event)) = timeout(quiet, events.next()).await {
        if let Ok(event) = event {
            seen.push(event);
        }
    }
    seen
}

fn goal_statuses(events: &[ProviderEvent]) -> Vec<Option<GoalStatus>> {
    events
        .iter()
        .filter_map(|event| match event {
            ProviderEvent::Goal {
                signal: GoalSignal::Snapshot { goal },
                ..
            } => Some(goal.as_ref().map(|goal| goal.status)),
            _ => None,
        })
        .collect()
}

fn completions(events: &[ProviderEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, ProviderEvent::Completed { .. }))
        .count()
}

#[test]
fn goal_commands_parse_like_the_codex_cli() {
    assert_eq!(
        parse_codex_goal_command("/goal"),
        Some(CodexGoalCommand::Show)
    );
    assert_eq!(
        parse_codex_goal_command(" /goal  "),
        Some(CodexGoalCommand::Show)
    );
    assert_eq!(
        parse_codex_goal_command("/goal Clear"),
        Some(CodexGoalCommand::Clear)
    );
    assert_eq!(
        parse_codex_goal_command("/goal pause"),
        Some(CodexGoalCommand::Pause)
    );
    assert_eq!(
        parse_codex_goal_command("/goal resume"),
        Some(CodexGoalCommand::Resume)
    );
    assert_eq!(
        parse_codex_goal_command("/goal  all tests pass\nand lint is clean"),
        Some(CodexGoalCommand::Set(
            "all tests pass\nand lint is clean".to_string()
        ))
    );
    assert_eq!(parse_codex_goal_command("/goals"), None);
    assert_eq!(parse_codex_goal_command("set a /goal"), None);
}

#[test]
fn goals_need_codex_0_128() {
    assert!(codex_supports_goals("0.128.0"));
    assert!(codex_supports_goals("0.160.0"));
    assert!(!codex_supports_goals("0.127.9"));
    assert!(!codex_supports_goals("0.128.0-alpha.1"));
}

#[test]
fn codex_goal_maps_every_status_and_its_usage() {
    let goal = |status: &str| json!({ "objective": " ship it ", "status": status, "tokensUsed": 1200, "tokenBudget": 5000, "timeUsedSeconds": 90 });
    for (wire, status) in [
        ("active", GoalStatus::Active),
        ("paused", GoalStatus::Paused),
        ("blocked", GoalStatus::Blocked),
        ("usageLimited", GoalStatus::UsageLimited),
        ("budgetLimited", GoalStatus::BudgetLimited),
        ("complete", GoalStatus::Complete),
    ] {
        let snapshot = codex_goal_snapshot(&goal(wire)).expect(wire);
        assert_eq!(snapshot.status, status);
        assert_eq!(snapshot.objective, "ship it");
        assert_eq!(snapshot.tokens_used, Some(1200));
        assert_eq!(snapshot.token_budget, Some(5000));
        assert_eq!(snapshot.time_used_seconds, Some(90));
    }
    assert_eq!(codex_goal_snapshot(&goal("someday")), None);
    assert_eq!(
        codex_goal_snapshot(&json!({ "objective": "", "status": "active" })),
        None
    );
}

#[tokio::test]
async fn goal_run_stays_one_turn_until_codex_completes_the_goal() {
    let cli = TestCli::new("codex-cli 0.153.4");
    cli.write("goal-run", "ok");
    let adapter = adapter(&cli);
    assert!(adapter.capabilities().supports_native_goal);
    let handle = adapter.prepare_session(config("goal")).await.unwrap();
    let mut events = adapter.stream_events(&handle);
    adapter
        .send_input(&handle, turn("/goal fixture goal"))
        .await
        .unwrap();
    let seen = drain(&mut events, Duration::from_millis(1500)).await;

    assert_eq!(completions(&seen), 1, "{seen:?}");
    assert!(
        matches!(seen.last(), Some(ProviderEvent::Completed { .. })),
        "{seen:?}"
    );
    let statuses = goal_statuses(&seen);
    assert_eq!(
        statuses.first(),
        Some(&None),
        "the old goal is cleared first"
    );
    assert!(statuses.contains(&Some(GoalStatus::Active)));
    assert_eq!(statuses.last(), Some(&Some(GoalStatus::Complete)));

    let log = cli.log();
    let set = log.find("\"method\":\"thread/goal/set\"").unwrap();
    let start = log.find("\"method\":\"turn/start\"").unwrap();
    let activate = log.find("\"status\":\"active\"").unwrap();
    assert!(
        set < start && start < activate,
        "paused, then the turn, then active"
    );
    assert!(
        log.contains("\"text\":\"fixture goal\""),
        "the objective is the turn's input"
    );
    stop(&adapter).await;
}

#[tokio::test]
async fn a_failed_goal_turn_pauses_the_goal_and_stops_the_unseen_next_one() {
    let cli = TestCli::new("codex-cli 0.153.4");
    cli.write("goal-run", "fail");
    let adapter = adapter(&cli);
    let handle = adapter.prepare_session(config("goal-fail")).await.unwrap();
    let mut events = adapter.stream_events(&handle);
    adapter
        .send_input(&handle, turn("/goal fixture goal"))
        .await
        .unwrap();
    let seen = drain(&mut events, Duration::from_millis(1500)).await;

    assert!(seen
        .iter()
        .any(|event| matches!(event, ProviderEvent::Failed { .. })));
    assert_eq!(completions(&seen), 0);
    assert_eq!(goal_statuses(&seen).last(), Some(&Some(GoalStatus::Paused)));
    let log = cli.log();
    let activate = log.find("\"status\":\"active\"").unwrap();
    assert!(log[activate..].contains("\"status\":\"paused\""));
    assert!(log.contains("\"method\":\"turn/interrupt\"") && log.contains("\"turnId\":\"turn-2\""));
    stop(&adapter).await;
}

#[tokio::test]
async fn goal_controls_answer_without_a_model_turn() {
    let cli = TestCli::new("codex-cli 0.153.4");
    let adapter = adapter(&cli);
    let handle = adapter.prepare_session(config("goal-show")).await.unwrap();
    let mut events = adapter.stream_events(&handle);
    adapter.send_input(&handle, turn("/goal")).await.unwrap();
    let seen = drain(&mut events, Duration::from_millis(500))
        .await
        .into_iter()
        .filter(|event| !matches!(event, ProviderEvent::NativeSessionChanged { .. }))
        .collect::<Vec<_>>();
    assert!(
        matches!(
            &seen[..],
            [
                ProviderEvent::Goal { signal: GoalSignal::Snapshot { goal: None }, .. },
                ProviderEvent::AssistantMessageCompleted { content: Some(text), .. },
                ProviderEvent::Completed { .. },
            ] if text.starts_with("Nenhum objetivo")
        ),
        "{seen:?}"
    );
    adapter
        .send_input(&handle, turn("/goal resume"))
        .await
        .unwrap();
    let seen = drain(&mut events, Duration::from_millis(500)).await;
    assert_eq!(
        completions(&seen),
        1,
        "resume without a goal just reports it"
    );
    assert!(!cli.log().contains("\"method\":\"turn/start\""));
    stop(&adapter).await;
}

#[test]
fn codex_without_goals_does_not_offer_them() {
    let adapter = CodexAppServerAdapter::with_runtime_detection(
        crate::codex::stable_codex_capabilities(),
        CodexMcpProjection::from_cli_output("codex-cli 0.127.0"),
        false,
    );
    assert!(!adapter.capabilities().supports_native_goal);
}
