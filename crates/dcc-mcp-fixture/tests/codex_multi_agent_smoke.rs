use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use dcc_core::{
    domain::{
        provider::{NativeSubagentStatus, ProviderEvent},
        session::{AssistantMessagePhase, SessionId},
        workspace::WorkspaceId,
    },
    ports::{Input, Provider, ProviderTurnInput, SessionConfig},
};
use futures::{stream::BoxStream, StreamExt};
use tokio::time::{sleep, timeout};

const RUN_ENV: &str = "DCC_RUN_CODEX_MULTI_AGENT_SMOKE";
const PARENT_MODEL: &str = "gpt-5.6-sol";
const CHILD_MODEL: &str = "gpt-5.6-terra";
const CHILD_SENTINEL: &str = "DCC_TERRA_CHILD_OK";
const PARENT_SENTINEL: &str = "DCC_SOL_RECEIVED_TERRA_RESULT";
const ORIGINAL_CHILD_SENTINEL: &str = "DCC_TERRA_ORIGINAL_RESULT";
const INTERRUPTED_PARENT_SENTINEL: &str = "DCC_SOL_CONFIRMED_TERRA_INTERRUPTED";
const TEST_TIMEOUT: Duration = Duration::from_secs(300);

const ORCHESTRATION_SKILL: &str =
    include_str!("../../../apps/desktop/src/features/skills/presets/dcc-orchestration/SKILL.md");
const ORCHESTRATION_OPENAI_CONFIG: &str = include_str!(
    "../../../apps/desktop/src/features/skills/presets/dcc-orchestration/agents/openai.yaml"
);

type ProviderStream = BoxStream<'static, dcc_core::Result<ProviderEvent>>;

#[derive(Default, Debug)]
struct Observation {
    requested_terra: HashSet<String>,
    confirmed_terra: HashSet<String>,
    running_children: HashSet<String>,
    completed_children: HashSet<String>,
    child_paths: HashMap<String, String>,
    final_answer: Option<String>,
}

impl Observation {
    fn terra_child_id(&self) -> Result<&str, String> {
        if self.requested_terra.len() != 1 {
            return Err(format!(
                "expected exactly one requested Terra child; observed {}",
                self.requested_terra.len()
            ));
        }
        let child_id = self
            .requested_terra
            .iter()
            .next()
            .expect("checked one child above");
        if !self.running_children.contains(child_id) {
            return Err("the requested Terra child never reported running".to_string());
        }
        if !self
            .child_paths
            .get(child_id)
            .is_some_and(|path| path.starts_with("/root/") || path.starts_with("root/"))
        {
            return Err(
                "the requested Terra child did not report a canonical root path".to_string(),
            );
        }
        Ok(child_id)
    }

    fn validate(self) -> Result<(), String> {
        let completed_terra_children = self
            .requested_terra
            .intersection(&self.confirmed_terra)
            .filter(|id| {
                self.running_children.contains(*id) && self.completed_children.contains(*id)
            })
            .count();
        if completed_terra_children != 1 {
            return Err(format!(
                "expected exactly one requested, confirmed, running, and completed Terra child; \
                 observed requested={}, confirmed={}, running={}, completed={}, matched={}",
                self.requested_terra.len(),
                self.confirmed_terra.len(),
                self.running_children.len(),
                self.completed_children.len(),
                completed_terra_children,
            ));
        }
        if self.final_answer.as_deref() != Some(PARENT_SENTINEL) {
            return Err(format!(
                "Sol did not return the exact integration sentinel; final_message_present={}, \
                 final_message_chars={}",
                self.final_answer.is_some(),
                self.final_answer
                    .as_deref()
                    .map(|content| content.chars().count())
                    .unwrap_or_default(),
            ));
        }
        Ok(())
    }
}

fn codex_home() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("CODEX_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|path| path.join(".codex"))
        .ok_or_else(|| "could not resolve the Codex home used by the smoke".to_string())
}

fn find_child_rollout(codex_home: &Path, child_id: &str) -> Result<PathBuf, String> {
    let sessions = codex_home.join("sessions");
    let suffix = format!("-{child_id}.jsonl");
    let mut pending = vec![sessions];
    let mut visited = 0usize;

    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory)
            .map_err(|_| "could not read Codex session history for E2E evidence".to_string())?;
        for entry in entries {
            let entry =
                entry.map_err(|_| "could not inspect a Codex session history entry".to_string())?;
            visited += 1;
            if visited > 100_000 {
                return Err("Codex session history exceeded the bounded E2E scan".to_string());
            }
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(&suffix))
            {
                return Ok(path);
            }
        }
    }
    Err("Codex did not persist the spawned child rollout".to_string())
}

fn add_persisted_child_evidence(observation: &mut Observation) -> Result<(), String> {
    let child_id = observation.terra_child_id()?.to_string();
    let rollout = find_child_rollout(&codex_home()?, &child_id)?;
    let file = std::fs::File::open(rollout)
        .map_err(|_| "could not open the spawned child rollout".to_string())?;
    let mut model_confirmed = false;
    let mut task_completed = false;

    for line in BufReader::new(file).lines() {
        let line = line.map_err(|_| "could not read the spawned child rollout".to_string())?;
        let value = serde_json::from_str::<serde_json::Value>(&line)
            .map_err(|_| "spawned child rollout contained invalid JSON".to_string())?;
        if value.get("type").and_then(serde_json::Value::as_str) == Some("turn_context") {
            model_confirmed |= value
                .get("payload")
                .and_then(|payload| payload.get("model"))
                .and_then(serde_json::Value::as_str)
                == Some(CHILD_MODEL);
        }
        if value.get("type").and_then(serde_json::Value::as_str) == Some("event_msg") {
            task_completed |= value
                .get("payload")
                .and_then(|payload| payload.get("type"))
                .and_then(serde_json::Value::as_str)
                == Some("task_complete");
        }
    }

    if !model_confirmed {
        return Err("the spawned child rollout did not confirm the Terra model".to_string());
    }
    if !task_completed {
        return Err("the spawned Terra child rollout did not complete".to_string());
    }
    // Completion must also arrive through the adapter's structured event
    // stream. The rollout is independent evidence, not a substitute for the
    // terminal event consumed by the DCC timeline.
    observation.confirmed_terra.insert(child_id);
    Ok(())
}

fn exact_opt_in() -> bool {
    std::env::var(RUN_ENV).ok().as_deref() == Some("1")
}

fn disposable_workspace() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "dcc-codex-multi-agent-smoke-{}-{nonce}",
        std::process::id()
    ))
}

fn install_orchestration_preset(workspace: &Path) -> std::io::Result<()> {
    let preset = workspace.join(".agents/skills/dcc-orchestration");
    std::fs::create_dir_all(preset.join("agents"))?;
    std::fs::write(preset.join("SKILL.md"), ORCHESTRATION_SKILL)?;
    std::fs::write(
        preset.join("agents/openai.yaml"),
        ORCHESTRATION_OPENAI_CONFIG,
    )?;
    Ok(())
}

fn child_key(id: &str, agent_thread_id: Option<&str>) -> String {
    agent_thread_id.unwrap_or(id).to_string()
}

async fn observe_turn(events: &mut ProviderStream) -> Result<Observation, String> {
    let mut observation = Observation::default();
    let mut message_deltas = HashMap::<String, String>::new();

    loop {
        let event = events
            .next()
            .await
            .ok_or_else(|| "Codex event stream ended before turn completion".to_string())?
            .map_err(|_| "Codex event stream returned an adapter error".to_string())?;

        match event {
            ProviderEvent::NativeSubagentActivity {
                id,
                agent_thread_id,
                path,
                model,
                status,
                ..
            } => {
                let key = child_key(&id, agent_thread_id.as_deref());
                if let Some(path) = path {
                    observation.child_paths.insert(key.clone(), path);
                }
                if model.as_deref() == Some(CHILD_MODEL) {
                    observation.confirmed_terra.insert(key.clone());
                }
                match status {
                    NativeSubagentStatus::Running => {
                        observation.running_children.insert(key);
                    }
                    NativeSubagentStatus::Completed => {
                        observation.completed_children.insert(key);
                    }
                    NativeSubagentStatus::Failed => {
                        return Err("the native Codex subagent reported failure".to_string());
                    }
                }
            }
            ProviderEvent::NativeSubagentModelRequested {
                correlation_id,
                model,
                ..
            } => {
                if model != CHILD_MODEL {
                    return Err(format!("Sol requested an unexpected child model: {model}"));
                }
                observation.requested_terra.insert(correlation_id);
            }
            ProviderEvent::NativeSubagentModelConfirmed {
                correlation_id,
                model,
                ..
            } => {
                if model != CHILD_MODEL {
                    return Err(format!(
                        "Codex confirmed an unexpected child model: {model}"
                    ));
                }
                observation.confirmed_terra.insert(correlation_id);
            }
            ProviderEvent::AssistantMessageStarted { id, .. } => {
                message_deltas.entry(id).or_default();
            }
            ProviderEvent::AssistantMessageDelta { id, content } => {
                message_deltas.entry(id).or_default().push_str(&content);
            }
            ProviderEvent::AssistantMessageCompleted {
                id, phase, content, ..
            } if phase != AssistantMessagePhase::Commentary => {
                // Older app-server builds omit the phase on agentMessage
                // items. DCC represents that as Unknown and, like its
                // timeline reducer, treats the last non-commentary completed
                // message as the final response.
                observation.final_answer = content.or_else(|| message_deltas.remove(&id));
            }
            ProviderEvent::ToolCallStarted { .. }
            | ProviderEvent::PermissionRequested { .. }
            | ProviderEvent::UserInputRequested { .. } => {
                return Err(
                    "the smoke used an unexpected non-collaboration tool or interaction"
                        .to_string(),
                );
            }
            ProviderEvent::Failed { .. } => {
                return Err("the Sol turn reported failure".to_string());
            }
            ProviderEvent::Completed { .. } => return Ok(observation),
            _ => {}
        }
    }
}

async fn observe_interrupted_turn(events: &mut ProviderStream) -> Result<(), String> {
    let mut requested_terra = HashSet::new();
    let mut running_children = HashSet::new();
    let mut interrupted_children = HashSet::new();
    let mut message_deltas = HashMap::<String, String>::new();
    let mut final_answer = None;

    loop {
        let event = events
            .next()
            .await
            .ok_or_else(|| "Codex event stream ended before interruption completed".to_string())?
            .map_err(|error| format!("Codex interruption stream failed: {error}"))?;
        match event {
            ProviderEvent::NativeSubagentActivity {
                id,
                agent_thread_id,
                status,
                ..
            } => {
                let key = child_key(&id, agent_thread_id.as_deref());
                match status {
                    NativeSubagentStatus::Running => {
                        running_children.insert(key);
                    }
                    NativeSubagentStatus::Failed => {
                        interrupted_children.insert(key);
                    }
                    NativeSubagentStatus::Completed => {
                        return Err("the Terra child completed instead of being interrupted".into());
                    }
                }
            }
            ProviderEvent::NativeSubagentModelRequested {
                correlation_id,
                model,
                ..
            } => {
                if model != CHILD_MODEL {
                    return Err(format!("Sol requested an unexpected child model: {model}"));
                }
                requested_terra.insert(correlation_id);
            }
            ProviderEvent::AssistantMessageStarted { id, .. } => {
                message_deltas.entry(id).or_default();
            }
            ProviderEvent::AssistantMessageDelta { id, content } => {
                message_deltas.entry(id).or_default().push_str(&content);
            }
            ProviderEvent::AssistantMessageCompleted {
                id, phase, content, ..
            } if phase != AssistantMessagePhase::Commentary => {
                final_answer = content.or_else(|| message_deltas.remove(&id));
            }
            ProviderEvent::Completed { .. } => {
                let interrupted_requested_child = requested_terra.iter().any(|thread_id| {
                    running_children.contains(thread_id) && interrupted_children.contains(thread_id)
                });
                if !interrupted_requested_child {
                    return Err("the requested Terra child was not observed as interrupted".into());
                }
                if final_answer.as_deref() != Some(INTERRUPTED_PARENT_SENTINEL) {
                    return Err("Sol did not confirm the child interruption exactly".into());
                }
                return Ok(());
            }
            ProviderEvent::Failed { .. } => {
                return Err("interrupting Terra also failed the Sol parent turn".into());
            }
            _ => {}
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires explicit opt-in, local Codex authentication, and real model usage"]
async fn authenticated_codex_sol_delegates_to_terra_and_integrates_result() {
    assert!(
        exact_opt_in(),
        "set {RUN_ENV}=1 to authorize this authenticated real-model smoke"
    );

    let workspace = disposable_workspace();
    install_orchestration_preset(&workspace).expect("install disposable orchestration preset");

    let provider = dcc_providers::codex::adapter();
    let session = provider
        .prepare_session(SessionConfig {
            workspace_id: WorkspaceId("codex-multi-agent-smoke".to_string()),
            session_id: SessionId("codex-multi-agent-smoke".to_string()),
            model: Some(PARENT_MODEL.to_string()),
            working_directory: Some(workspace.to_string_lossy().to_string()),
            additional_working_directories: Vec::new(),
            provider_runtime: None,
            mcp_servers: Vec::new(),
        })
        .await;

    let handle = match session {
        Ok(handle) => handle,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&workspace);
            panic!("prepare authenticated Codex session: {error}");
        }
    };
    let mut events = provider.stream_events(&handle);

    let prompt = format!(
        "$dcc-orchestration\n\nRun this DCC orchestration smoke exactly. Spawn exactly one \
         native Codex subagent with model {CHILD_MODEL}. Give it this bounded, read-only task: \
         \"Return exactly {CHILD_SENTINEL} and nothing else. Do not call tools.\" Wait for the \
         child to finish. Only if its result is exactly {CHILD_SENTINEL}, reply exactly \
         {PARENT_SENTINEL} and nothing else. Do not call shell, file, web, MCP, permission, or \
         user-input tools. Do not spawn any other subagent."
    );
    let send_result = provider
        .send_input(
            &handle,
            Input::Turn(ProviderTurnInput {
                prompt,
                tool_instructions: Some(format!(
                    "This is an opt-in DCC multi-agent smoke. You must use the native Codex \
                     spawn_agent tool exactly once with model {CHILD_MODEL}, wait for that child, \
                     and use no other tool."
                )),
                plan_mode: Some(false),
                effort: Some("low".to_string()),
                fast_mode: Some(true),
                approval_policy: None,
            }),
        )
        .await;

    let result = match send_result {
        Ok(()) => timeout(TEST_TIMEOUT, observe_turn(&mut events))
            .await
            .map_err(|_| "timed out waiting for the Sol to Terra workflow".to_string())
            .and_then(|result| result)
            .and_then(|mut observation| {
                add_persisted_child_evidence(&mut observation)?;
                observation.validate()
            }),
        Err(error) => Err(format!("send Sol orchestration turn: {error}")),
    };

    drop(events);
    let cancel_result = provider.cancel(&handle).await;
    let cleanup_result = std::fs::remove_dir_all(&workspace);

    assert!(cancel_result.is_ok(), "cancel Codex smoke session cleanly");
    assert!(cleanup_result.is_ok(), "remove disposable smoke workspace");
    if let Err(error) = result {
        panic!("authenticated Sol to Terra smoke failed: {error}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires explicit opt-in, local Codex authentication, and real model usage"]
async fn authenticated_codex_can_steer_a_running_terra_child_directly() {
    assert!(
        exact_opt_in(),
        "set {RUN_ENV}=1 to authorize this authenticated real-model smoke"
    );

    let workspace = disposable_workspace();
    install_orchestration_preset(&workspace).expect("install disposable orchestration preset");
    let provider = dcc_providers::codex::adapter();
    let handle = provider
        .prepare_session(SessionConfig {
            workspace_id: WorkspaceId("codex-subagent-steering-smoke".to_string()),
            session_id: SessionId("codex-subagent-steering-smoke".to_string()),
            model: Some(PARENT_MODEL.to_string()),
            working_directory: Some(workspace.to_string_lossy().to_string()),
            additional_working_directories: Vec::new(),
            provider_runtime: None,
            mcp_servers: Vec::new(),
        })
        .await
        .expect("prepare authenticated Codex steering session");
    let mut events = provider.stream_events(&handle);
    let mut supervision_events = provider.stream_events(&handle);
    let supervision_provider = provider.clone();
    let supervision_handle = handle.clone();
    let supervision = tokio::spawn(async move {
        loop {
            let event = supervision_events
                .next()
                .await
                .ok_or_else(|| "Codex stream ended before a child could be steered".to_string())?
                .map_err(|error| format!("Codex steering stream failed: {error}"))?;
            let ProviderEvent::NativeSubagentActivity {
                agent_thread_id: Some(agent_thread_id),
                status: NativeSubagentStatus::Running,
                ..
            } = event
            else {
                continue;
            };
            let instruction = format!(
                "Replace the original final output. After the current sleep completes, return \
                 exactly {CHILD_SENTINEL} and nothing else."
            );
            let mut last_error = None;
            for _ in 0..40 {
                match supervision_provider
                    .steer_native_subagent(&supervision_handle, &agent_thread_id, &instruction)
                    .await
                {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        last_error = Some(error.to_string());
                        sleep(Duration::from_millis(50)).await;
                    }
                }
            }
            return Err(format!(
                "could not steer the running Terra child: {}",
                last_error.unwrap_or_else(|| "unknown error".to_string())
            ));
        }
    });

    let prompt = format!(
        "$dcc-orchestration\n\nSpawn exactly one native Codex subagent with model \
         {CHILD_MODEL}. Give it this task: \"Call clock.sleep once for 20 seconds. After that, \
         return exactly {ORIGINAL_CHILD_SENTINEL} and nothing else. If a later user instruction \
         changes the required output, follow the latest instruction. Do not call any other tool.\" \
         Wait for the child. Only if its final result is exactly {CHILD_SENTINEL}, reply exactly \
         {PARENT_SENTINEL} and nothing else. Do not use any non-collaboration tool yourself and \
         do not spawn another subagent."
    );
    let send_result = provider
        .send_input(
            &handle,
            Input::Turn(ProviderTurnInput {
                prompt,
                tool_instructions: Some(format!(
                    "Use native spawn_agent exactly once with model {CHILD_MODEL}; the child may \
                     use clock.sleep, but the parent must use no other tool."
                )),
                plan_mode: Some(false),
                effort: Some("low".to_string()),
                fast_mode: Some(true),
                approval_policy: None,
            }),
        )
        .await;

    let result = match send_result {
        Ok(()) => timeout(TEST_TIMEOUT, observe_turn(&mut events))
            .await
            .map_err(|_| "timed out waiting for the steered Sol-to-Terra workflow".to_string())
            .and_then(|result| result)
            .and_then(|mut observation| {
                add_persisted_child_evidence(&mut observation)?;
                observation.validate()
            }),
        Err(error) => Err(format!("send steered Sol orchestration turn: {error}")),
    };
    let supervision_result = timeout(TEST_TIMEOUT, supervision)
        .await
        .map_err(|_| "timed out steering the Terra child".to_string())
        .and_then(|result| result.map_err(|error| format!("steering task failed: {error}")))
        .and_then(|result| result);

    drop(events);
    let cancel_result = provider.cancel(&handle).await;
    let cleanup_result = std::fs::remove_dir_all(&workspace);

    assert!(
        supervision_result.is_ok(),
        "steer the Terra child directly: {supervision_result:?}"
    );
    assert!(cancel_result.is_ok(), "cancel Codex steering smoke cleanly");
    assert!(
        cleanup_result.is_ok(),
        "remove disposable steering workspace"
    );
    if let Err(error) = result {
        panic!("authenticated native-subagent steering smoke failed: {error}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires explicit opt-in, local Codex authentication, and real model usage"]
async fn authenticated_codex_can_interrupt_terra_without_stopping_sol() {
    assert!(
        exact_opt_in(),
        "set {RUN_ENV}=1 to authorize this authenticated real-model smoke"
    );

    let workspace = disposable_workspace();
    install_orchestration_preset(&workspace).expect("install disposable orchestration preset");
    let provider = dcc_providers::codex::adapter();
    let handle = provider
        .prepare_session(SessionConfig {
            workspace_id: WorkspaceId("codex-subagent-interrupt-smoke".to_string()),
            session_id: SessionId("codex-subagent-interrupt-smoke".to_string()),
            model: Some(PARENT_MODEL.to_string()),
            working_directory: Some(workspace.to_string_lossy().to_string()),
            additional_working_directories: Vec::new(),
            provider_runtime: None,
            mcp_servers: Vec::new(),
        })
        .await
        .expect("prepare authenticated Codex interruption session");
    let mut events = provider.stream_events(&handle);
    let mut supervision_events = provider.stream_events(&handle);
    let supervision_provider = provider.clone();
    let supervision_handle = handle.clone();
    let supervision = tokio::spawn(async move {
        loop {
            let event = supervision_events
                .next()
                .await
                .ok_or_else(|| {
                    "Codex stream ended before a child could be interrupted".to_string()
                })?
                .map_err(|error| format!("Codex interruption stream failed: {error}"))?;
            let ProviderEvent::NativeSubagentActivity {
                agent_thread_id: Some(agent_thread_id),
                status: NativeSubagentStatus::Running,
                ..
            } = event
            else {
                continue;
            };
            let mut last_error = None;
            for _ in 0..40 {
                match supervision_provider
                    .interrupt_native_subagent(&supervision_handle, &agent_thread_id)
                    .await
                {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        last_error = Some(error.to_string());
                        sleep(Duration::from_millis(50)).await;
                    }
                }
            }
            return Err(format!(
                "could not interrupt the running Terra child: {}",
                last_error.unwrap_or_else(|| "unknown error".to_string())
            ));
        }
    });

    let prompt = format!(
        "$dcc-orchestration\n\nSpawn exactly one native Codex subagent with model \
         {CHILD_MODEL}. Give it this task: \"Call clock.sleep once for 60 seconds, then return \
         exactly {ORIGINAL_CHILD_SENTINEL}. Do not call any other tool.\" Wait for that child. If \
         it is interrupted, reply exactly {INTERRUPTED_PARENT_SENTINEL} and nothing else. Do not \
         fail or stop your own turn when the child is interrupted. Do not use a non-collaboration \
         tool yourself and do not spawn another subagent."
    );
    let send_result = provider
        .send_input(
            &handle,
            Input::Turn(ProviderTurnInput {
                prompt,
                tool_instructions: Some(format!(
                    "Use native spawn_agent exactly once with model {CHILD_MODEL}; the child may \
                     use clock.sleep, but the parent must use no other tool."
                )),
                plan_mode: Some(false),
                effort: Some("low".to_string()),
                fast_mode: Some(true),
                approval_policy: None,
            }),
        )
        .await;
    let result = match send_result {
        Ok(()) => timeout(TEST_TIMEOUT, observe_interrupted_turn(&mut events))
            .await
            .map_err(|_| "timed out waiting for the interrupted Sol-to-Terra workflow".to_string())
            .and_then(|result| result),
        Err(error) => Err(format!(
            "send interruptible Sol orchestration turn: {error}"
        )),
    };
    let supervision_result = timeout(TEST_TIMEOUT, supervision)
        .await
        .map_err(|_| "timed out interrupting the Terra child".to_string())
        .and_then(|result| result.map_err(|error| format!("interruption task failed: {error}")))
        .and_then(|result| result);

    drop(events);
    let cancel_result = provider.cancel(&handle).await;
    let cleanup_result = std::fs::remove_dir_all(&workspace);

    assert!(
        supervision_result.is_ok(),
        "interrupt the Terra child through Sol: {supervision_result:?}"
    );
    assert!(cancel_result.is_ok(), "cancel interruption smoke cleanly");
    assert!(
        cleanup_result.is_ok(),
        "remove interruption smoke workspace"
    );
    if let Err(error) = result {
        panic!("authenticated native-subagent interruption smoke failed: {error}");
    }
}
