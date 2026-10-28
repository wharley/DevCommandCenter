//! The launch's steps with fakes: the happy path, a failure keeping what
//! is known, a launch another run already holds, and the journal's shape.

use std::cell::RefCell;

use super::*;

#[derive(Default)]
struct Fake {
    calls: RefCell<Vec<String>>,
    fail_at: Option<&'static str>,
    begun: Option<Launch>,
}

impl Fake {
    fn step(&self, name: &str) -> Result<(), String> {
        self.calls.borrow_mut().push(name.to_string());
        if self.fail_at == Some(name) {
            Err(format!("{name} failed"))
        } else {
            Ok(())
        }
    }
}

impl Steps for Fake {
    fn begin(&self, launch: &Launch) -> Result<Launch, String> {
        self.step("begin")?;
        Ok(self.begun.clone().unwrap_or_else(|| launch.clone()))
    }
    fn checkpoint(&self, launch: &Launch) -> Result<Launch, String> {
        self.step(&format!("checkpoint {}", launch.phase))?;
        Ok(launch.clone())
    }
    fn create_task(&self, _: &Launch) -> Result<String, String> {
        self.step("create")?;
        Ok("w-1".into())
    }
    fn start_session(&self, launch: &Launch) -> Result<String, String> {
        assert_eq!(launch.workspace_id.as_deref(), Some("w-1"));
        self.step("start")?;
        Ok("s-1".into())
    }
    fn send(&self, launch: &Launch) -> Result<(), String> {
        assert_eq!(launch.session_id.as_deref(), Some("s-1"));
        self.step("send")
    }
}

fn request() -> Request {
    Request {
        project_id: "p-1".into(),
        root_path: "/repo".into(),
        base_branch: "main".into(),
        isolation_mode: WORKTREE.into(),
        provider_id: "codex".into(),
        model_id: "gpt".into(),
        provider_runtime: None,
        turn: Turn {
            raw_prompt: "  Corrija o login\ncom testes".into(),
            envelope: Envelope {
                effort: Some("high".into()),
                plan_mode: true,
                ..Envelope::default()
            },
        },
    }
}

#[test]
fn a_launch_saves_each_step_before_the_next() {
    let fake = Fake::default();
    let mut seen = Vec::new();
    let done = execute(Launch::new(request()), &fake, &mut |launch| {
        seen.push((launch.phase.clone(), launch.revision))
    });
    assert_eq!(done.phase, "completed");
    assert_eq!(
        fake.calls.borrow().as_slice(),
        [
            "begin",
            "checkpoint creating",
            "create",
            "checkpoint workspaceReady",
            "checkpoint starting",
            "start",
            "checkpoint sessionReady",
            "checkpoint sending",
            "send",
            "checkpoint completed",
        ]
    );
    assert_eq!(seen.last(), Some(&("completed".to_string(), 6)));
    assert_eq!(
        (done.workspace_id.as_deref(), done.session_id.as_deref()),
        (Some("w-1"), Some("s-1"))
    );
}

#[test]
fn a_failure_keeps_what_is_known_and_never_sends_twice() {
    let fake = Fake {
        fail_at: Some("start"),
        ..Fake::default()
    };
    let done = execute(Launch::new(request()), &fake, &mut |_| {});
    assert_eq!(done.phase, "failed");
    assert_eq!(done.error.as_deref(), Some("start failed"));
    assert_eq!(
        done.workspace_id.as_deref(),
        Some("w-1"),
        "the task stays known"
    );
    assert!(!fake.calls.borrow().iter().any(|call| call == "send"));
    assert_eq!(
        fake.calls.borrow().last().map(String::as_str),
        Some("checkpoint failed")
    );
    // The failure's own checkpoint lost: failed anyway, nothing retried.
    let lost = Fake {
        fail_at: Some("checkpoint sending"),
        ..Fake::default()
    };
    let done = execute(Launch::new(request()), &lost, &mut |_| {});
    assert_eq!(done.phase, "failed");
    assert!(!lost.calls.borrow().iter().any(|call| call == "send"));
}

#[test]
fn a_launch_already_begun_elsewhere_is_left_alone() {
    let mut other = Launch::new(request());
    other.phase = "sending".into();
    let fake = Fake {
        begun: Some(other.clone()),
        ..Fake::default()
    };
    let done = execute(Launch::new(request()), &fake, &mut |_| {});
    assert_eq!(done, other);
    assert_eq!(fake.calls.borrow().as_slice(), ["begin"]);
}

#[test]
fn the_request_reads_like_the_desktops() {
    let request = request();
    assert_eq!(request.task_name(), "Corrija o login");
    let options = request.turn_options();
    assert_eq!(
        (
            options.provider_id.as_deref(),
            options.effort.as_deref(),
            options.plan_mode
        ),
        (Some("codex"), Some("high"), true)
    );
    let json = serde_json::to_value(Launch::new(request)).unwrap();
    assert_eq!(
        json["request"]["turn"]["rawPrompt"],
        "  Corrija o login\ncom testes"
    );
    assert_eq!(json["request"]["isolationMode"], "protectedWorktree");
    assert_eq!(json["phase"], "pending");
    assert_eq!(json["workspaceId"], Value::Null);
    let mut selection = Selection::default();
    assert_eq!(selection.mode(), WORKTREE);
    selection.project_id = "p".into();
    selection.modes.insert("p".into(), LOCAL.into());
    assert_eq!(selection.mode(), LOCAL);
}
