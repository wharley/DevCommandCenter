use std::io::Write;

use dcc_core::domain::usage_logs::{EventKind, Runtime, UsageRow, Via};

use super::claude::{self, ClaudeFile};
use super::codex::{self, CodexFile};
use super::{DccHints, Resolver, Sources, UsageIndex};

fn claude_assistant(message: &str, request: &str, entrypoint: &str, sidechain: bool) -> String {
    format!(
        r#"{{"parentUuid":null,"isSidechain":{sidechain},"message":{{"id":"{message}","model":"claude-opus-5-5","content":[{{"type":"text","text":"x"}}],"usage":{{"input_tokens":10,"cache_creation_input_tokens":20,"cache_read_input_tokens":300,"output_tokens":40,"output_tokens_details":{{"thinking_tokens":5}}}}}},"type":"assistant","uuid":"u-{message}-{request}","timestamp":"2026-11-10T12:00:00.000Z","entrypoint":"{entrypoint}","cwd":"/nowhere/proj","sessionId":"sess-1","requestId":"{request}","attributionAgent":"Explore"}}"#
    )
}

fn claude_user(uuid: &str, content: &str) -> String {
    format!(
        r#"{{"parentUuid":null,"isSidechain":false,"promptId":"p","type":"user","message":{{"role":"user","content":{content}}},"uuid":"{uuid}","timestamp":"2026-11-10T11:59:00.000Z","entrypoint":"cli","cwd":"/nowhere/proj","sessionId":"sess-1"}}"#
    )
}

fn resolver() -> Resolver {
    Resolver::new(DccHints::default())
}

#[test]
fn claude_repeats_one_call_per_block_and_counts_prompts_as_turns() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sess-1.jsonl");
    let lines = [
        claude_user("u1", r#""fix the bug""#),
        // One response written once per content block: one call.
        claude_assistant("msg_1", "req_1", "cli", false),
        claude_assistant("msg_1", "req_1", "cli", false),
        claude_assistant("msg_1", "req_1", "cli", false),
        claude_user(
            "u2",
            r#"[{"type":"tool_result","tool_use_id":"t","content":"ok"}]"#,
        ),
        claude_assistant("msg_2", "req_2", "cli", false),
    ];
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();
    let mut index = UsageIndex::open_in_memory().unwrap();
    let mut resolver = resolver();
    index
        .index_path(&path, Runtime::Claude, &mut resolver)
        .unwrap();
    let rows = index.load_rows(None).unwrap();
    let calls: Vec<&UsageRow> = rows
        .rows
        .iter()
        .filter(|row| row.kind == EventKind::Call)
        .collect();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        rows.rows
            .iter()
            .filter(|row| row.kind == EventKind::Turn)
            .count(),
        1
    );
    let call = calls[0];
    assert_eq!(call.tokens.input, 10);
    assert_eq!(call.tokens.cache_write, 20);
    assert_eq!(call.tokens.cache_read, 300);
    assert_eq!(call.tokens.output, 40);
    assert_eq!(call.tokens.reasoning, 5);
    assert_eq!(call.via, Via::Cli);
    assert!(!call.subagent);
    assert_eq!(rows.string(call.project), "proj");
    assert_eq!(rows.string(call.model), "claude-opus-5-5");
}

#[test]
fn claude_subagents_by_folder_or_sidechain_and_via_from_entrypoint() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("sess-1").join("subagents");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("agent-abc.jsonl");
    std::fs::write(
        folder.join("agent-abc.meta.json"),
        r#"{"agentType":"general-purpose","description":"d"}"#,
    )
    .unwrap();
    let mut file = ClaudeFile::for_path(&path);
    assert!(file.subagent);
    assert_eq!(file.agent.as_deref(), Some("general-purpose"));
    let mut resolver = resolver();
    let event = claude::parse_line(
        &claude_assistant("m", "r", "claude-desktop", true),
        &mut file,
        &mut resolver,
    )
    .unwrap();
    assert!(event.subagent);
    assert_eq!(event.agent, "general-purpose");
    assert_eq!(event.session, "sess-1/agent-abc");
    assert_eq!(event.via, Via::Desktop);

    // A sidechain line in a main file is a subagent's, named by attribution.
    let mut main = ClaudeFile::default();
    let event = claude::parse_line(
        &claude_assistant("m2", "r2", "sdk-ts", true),
        &mut main,
        &mut resolver,
    )
    .unwrap();
    assert!(event.subagent);
    assert_eq!(event.agent, "Explore");
    assert_eq!(event.via, Via::Sdk);
    assert_eq!(resolver.claude_via("cli", "s", "/x"), Via::Cli);
    assert_eq!(resolver.claude_via("claude-vscode", "s", "/x"), Via::Ide);
    assert_eq!(resolver.claude_via("", "s", "/x"), Via::Other);
}

#[test]
fn claude_sdk_sessions_in_dcc_read_as_dcc() {
    let resolver = Resolver::new(DccHints {
        claude_sessions: ["known".to_string()].into_iter().collect(),
        paths: vec!["/work/app".into()],
        worktrees: vec![("/work/.dcc-worktrees/main-1".into(), "app".into())],
    });
    assert_eq!(
        resolver.claude_via("sdk-ts", "s", "/work/app/src"),
        Via::Dcc
    );
    assert_eq!(
        resolver.claude_via("sdk-ts", "s", "/work/application"),
        Via::Sdk
    );
    assert_eq!(
        resolver.claude_via("sdk-ts", "s", "/x/.dcc-worktrees/b/src"),
        Via::Dcc
    );
    assert_eq!(resolver.claude_via("cli", "known", "/x"), Via::Dcc);
    let mut resolver = resolver;
    assert_eq!(
        resolver.project("/work/.dcc-worktrees/main-1/crates"),
        "app"
    );
}

fn codex_meta(originator: &str, source: &str) -> String {
    format!(
        r#"{{"timestamp":"2026-11-10T10:00:00.000Z","type":"session_meta","payload":{{"id":"cx-1","timestamp":"2026-11-10T10:00:00.000Z","cwd":"/nowhere/repo","originator":"{originator}","cli_version":"0.1","source":{source},"model_provider":"openai","base_instructions":{{"text":"long"}}}}}}"#
    )
}

fn codex_turn(turn: &str, model: &str) -> String {
    format!(
        r#"{{"timestamp":"2026-11-10T10:00:01.000Z","type":"turn_context","payload":{{"turn_id":"{turn}","cwd":"/nowhere/repo","model":"{model}"}}}}"#
    )
}

fn codex_started(turn: &str) -> String {
    format!(
        r#"{{"timestamp":"2026-11-10T10:00:01.000Z","type":"event_msg","payload":{{"type":"task_started","turn_id":"{turn}"}}}}"#
    )
}

fn codex_tokens(second: u32, input: u64, cached: u64, output: u64, total: u64) -> String {
    format!(
        r#"{{"timestamp":"2026-11-10T10:00:{second:02}.000Z","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":0,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0,"total_tokens":{total}}},"last_token_usage":{{"input_tokens":{input},"cached_input_tokens":{cached},"cache_write_input_tokens":0,"output_tokens":{output},"reasoning_output_tokens":7,"total_tokens":{}}}}}}}}}"#,
        input + output
    )
}

#[test]
fn codex_skips_repeated_totals_and_takes_input_without_its_cache() {
    let mut file = CodexFile::default();
    let mut resolver = resolver();
    let lines = [
        codex_meta("codex-tui", r#""cli""#),
        codex_turn("t1", "gpt-5.5"),
        codex_started("t1"),
        codex_tokens(2, 1000, 800, 50, 1050),
        // The same total again: nothing new was billed.
        codex_tokens(3, 1000, 800, 50, 1050),
        r#"{"timestamp":"2026-11-10T10:00:04.000Z","type":"event_msg","payload":{"type":"token_count","info":null}}"#.to_string(),
        codex_tokens(5, 2000, 1500, 100, 3150),
    ];
    let events: Vec<_> = lines
        .iter()
        .filter_map(|line| codex::parse_line(line, &mut file, &mut resolver))
        .collect();
    // turn_context and task_started name the same turn: one key.
    let turns: Vec<_> = events
        .iter()
        .filter(|e| e.kind == EventKind::Turn)
        .collect();
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].key, turns[1].key);
    let calls: Vec<_> = events
        .iter()
        .filter(|e| e.kind == EventKind::Call)
        .collect();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].tokens.input, 200);
    assert_eq!(calls[0].tokens.cache_read, 800);
    assert_eq!(calls[0].tokens.output, 50);
    assert_eq!(calls[0].tokens.reasoning, 7);
    assert_eq!(calls[0].tokens.new_tokens(), 250);
    assert_eq!(calls[0].model, "gpt-5.5");
    assert_eq!(calls[0].via, Via::Tui);
    assert_eq!(calls[0].project, "repo");
    assert_eq!(calls[0].session, "cx-1");
    assert_eq!(calls[1].tokens.input, 500);
}

#[test]
fn codex_via_and_subagents_from_the_meta() {
    assert_eq!(super::codex_via("dcc", "vscode"), Via::Dcc);
    assert_eq!(super::codex_via("dcc-probe", "exec"), Via::Dcc);
    assert_eq!(super::codex_via("codex_exec", "exec"), Via::Exec);
    assert_eq!(super::codex_via("codex_vscode", "vscode"), Via::Desktop);
    assert_eq!(super::codex_via("Codex Desktop", "vscode"), Via::Desktop);
    assert_eq!(super::codex_via("codex_cli_rs", "cli"), Via::Cli);
    assert_eq!(super::codex_via("other", "exec"), Via::Exec);
    assert_eq!(super::codex_via("other", "mcp"), Via::Other);

    let mut file = CodexFile::default();
    let mut resolver = resolver();
    let spawned = r#"{"subagent":{"thread_spawn":{"parent_thread_id":"p","depth":1,"agent_role":"reviewer"}}}"#;
    codex::parse_line(&codex_meta("dcc", spawned), &mut file, &mut resolver);
    assert!(file.subagent);
    assert_eq!(file.agent, "reviewer");
    assert_eq!(file.via, "dcc");
    // A parent's meta repeated later in the file changes nothing.
    codex::parse_line(
        &codex_meta("codex-tui", r#""cli""#),
        &mut file,
        &mut resolver,
    );
    assert_eq!(file.via, "dcc");
    assert!(file.subagent);
}

#[test]
fn a_refresh_reads_only_appended_lines_and_rereads_a_rewritten_file() {
    let dir = tempfile::tempdir().unwrap();
    let sessions = dir
        .path()
        .join("sessions")
        .join("2026")
        .join("11")
        .join("10");
    std::fs::create_dir_all(&sessions).unwrap();
    let path = sessions.join("rollout-2026-11-10T10-00-00-cx-1.jsonl");
    let first = [
        codex_meta("codex-tui", r#""cli""#),
        codex_turn("t1", "gpt-5.5"),
        codex_tokens(2, 1000, 800, 50, 1050),
    ];
    std::fs::write(&path, first.join("\n") + "\n").unwrap();
    let sources = Sources {
        claude: Vec::new(),
        codex: vec![dir.path().join("sessions")],
    };
    let db = dir.path().join("usage-logs.sqlite");
    let mut index = UsageIndex::open(&db).unwrap();
    let mut resolver = resolver();
    let stats = index
        .refresh(&sources, &mut resolver, &mut |_, _| {})
        .unwrap();
    assert_eq!((stats.files, stats.parsed, stats.added), (1, 1, 2));
    let size = std::fs::metadata(&path).unwrap().len();
    assert_eq!(stats.bytes_read, size);

    // Unchanged: not read at all.
    let stats = index
        .refresh(&sources, &mut resolver, &mut |_, _| {})
        .unwrap();
    assert_eq!((stats.parsed, stats.bytes_read), (0, 0));

    // Appended: one complete line and one still being written.
    let appended = codex_tokens(9, 2000, 1500, 100, 3150) + "\n";
    let partial = r#"{"timestamp":"2026-11-10T10:00:10.000Z","type":"event_msg","#;
    let mut handle = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    write!(handle, "{appended}{partial}").unwrap();
    drop(handle);
    std::thread::sleep(std::time::Duration::from_millis(5));
    let stats = index
        .refresh(&sources, &mut resolver, &mut |_, _| {})
        .unwrap();
    assert_eq!(stats.bytes_read, appended.len() as u64);
    assert_eq!(stats.added, 1);
    // The model learned before the append is still known after it.
    let rows = index.load_rows(None).unwrap();
    let last = rows.rows.iter().max_by_key(|row| row.at_ms).unwrap();
    assert_eq!(rows.string(last.model), "gpt-5.5");
    assert_eq!(index.counts().unwrap(), (2, 1, 1));

    // Rewritten shorter: read again from the start, old rows dropped.
    std::fs::write(&path, first[..2].join("\n") + "\n").unwrap();
    let stats = index
        .refresh(&sources, &mut resolver, &mut |_, _| {})
        .unwrap();
    assert_eq!(stats.parsed, 1);
    assert_eq!(index.counts().unwrap(), (0, 1, 1));

    // Reopened: the schema and the rows stay.
    drop(index);
    let index = UsageIndex::open(&db).unwrap();
    assert_eq!(index.counts().unwrap(), (0, 1, 1));
}

/// A read-only run over this machine's real logs, printing only counts and
/// timings: `cargo test -p dcc-infra --lib usage_logs::tests::smoke -- --ignored --nocapture`.
#[test]
#[ignore]
fn smoke_real_logs() {
    use chrono::Local;
    use dcc_core::domain::usage_logs::{aggregate, GroupBy, Metric, UsageFilter};

    let home = std::path::PathBuf::from(std::env::var("HOME").unwrap());
    let dcc_db = home.join("Library/Application Support/com.devcommandcenter.app/database.sqlite");
    let (sources, hints) = super::discover(Some(&dcc_db));
    println!(
        "claude roots {}, codex roots {}",
        sources.claude.len(),
        sources.codex.len()
    );
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("usage-logs.sqlite");
    let mut resolver = Resolver::new(hints);
    let mut index = UsageIndex::open(&db).unwrap();
    let cold = index
        .refresh(&sources, &mut resolver, &mut |_, _| {})
        .unwrap();
    println!(
        "cold: files {} parsed {} MB {} rows {} in {:?}",
        cold.files,
        cold.parsed,
        cold.bytes_read / 1_000_000,
        cold.added,
        cold.elapsed
    );
    let warm = index
        .refresh(&sources, &mut resolver, &mut |_, _| {})
        .unwrap();
    println!(
        "warm: files {} parsed {} KB {} rows {} in {:?}",
        warm.files,
        warm.parsed,
        warm.bytes_read / 1000,
        warm.added,
        warm.elapsed
    );
    let (calls, turns, files) = index.counts().unwrap();
    println!(
        "index: calls {calls} turns {turns} files {files} db {} MB",
        std::fs::metadata(&db).unwrap().len() / 1_000_000
    );
    let now = chrono::Utc::now().timestamp_millis();
    let started = std::time::Instant::now();
    let rows = index.load_rows(Some(now - 32 * 86_400_000)).unwrap();
    let loaded = started.elapsed();
    let started = std::time::Instant::now();
    let summary = aggregate(
        &rows,
        &UsageFilter::default(),
        Metric::TotalTokens,
        GroupBy::Runtime,
        &Local,
        now,
    );
    println!(
        "30d rows {} load {:?} aggregate {:?}",
        rows.rows.len(),
        loaded,
        started.elapsed()
    );
    println!(
        "30d: total {} new {} output {} reasoning {} cache_read {} cost ${:.0} (claude ${:.0} codex ${:.0}) calls {} turns {} sessions {} subagent sessions {} dcc {:.1}% subagent {:.1}%",
        summary.tokens.total(),
        summary.tokens.new_tokens(),
        summary.tokens.output,
        summary.tokens.reasoning,
        summary.tokens.cache_read,
        summary.cost_usd,
        summary.cost_by_runtime[0],
        summary.cost_by_runtime[1],
        summary.calls,
        summary.turns,
        summary.sessions,
        summary.subagent_sessions,
        summary.dcc_percent,
        summary.subagent_percent,
    );
    println!(
        "active {}/{} streak {} peak day {:?} peak hour {:?} vias {:?} unknown models {:?} unpriced calls {}",
        summary.active_days,
        summary.period_days,
        summary.longest_streak,
        summary.peak_day,
        summary.peak_hour,
        summary.vias_present,
        summary.unknown_models,
        summary.unpriced_calls
    );
    for (group, value) in &summary.groups {
        println!("  {group}: {value}");
    }
}
