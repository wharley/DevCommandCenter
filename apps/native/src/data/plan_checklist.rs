//! `parsePlanChecklist`: the agent's own step list, from the input of its plan
//! tool — Claude's TodoWrite (`todos`), Codex's plan update (`plan`) or todo
//! list (`items`). Anything else is not a checklist.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanStepStatus {
    Completed,
    InProgress,
    Pending,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanStep {
    pub text: String,
    pub status: PlanStepStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanChecklist {
    pub steps: Vec<PlanStep>,
    pub completed: usize,
    /// The step being worked on, or the next one when none is marked.
    pub current: Option<usize>,
}

fn step_status(value: Option<&Value>) -> PlanStepStatus {
    match value {
        Some(Value::Bool(true)) => PlanStepStatus::Completed,
        Some(Value::String(value)) => match value.as_str() {
            "completed" | "done" => PlanStepStatus::Completed,
            "in_progress" | "inProgress" | "active" => PlanStepStatus::InProgress,
            _ => PlanStepStatus::Pending,
        },
        _ => PlanStepStatus::Pending,
    }
}

fn read_step(raw: &Value) -> Option<PlanStep> {
    let step = raw.as_object()?;
    let status = step_status(step.get("status").or_else(|| step.get("completed")));
    let text = |key: &str| {
        step.get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
    };
    // Claude shows the present tense ("Running tests") while a step runs.
    let text = (status == PlanStepStatus::InProgress)
        .then(|| text("activeForm"))
        .flatten()
        .or_else(|| text("content"))
        .or_else(|| text("step"))
        .or_else(|| text("text"))?;
    Some(PlanStep {
        text: text.trim().to_string(),
        status,
    })
}

pub fn parse_plan_checklist(input: Option<&str>) -> Option<PlanChecklist> {
    let parsed: Value = serde_json::from_str(input?).ok()?;
    let record = parsed.as_object()?;
    let list = ["todos", "plan", "items"]
        .iter()
        .find_map(|key| record.get(*key).and_then(Value::as_array))?;
    let steps: Vec<PlanStep> = list.iter().filter_map(read_step).collect();
    if steps.is_empty() {
        return None;
    }
    let completed = steps
        .iter()
        .filter(|step| step.status == PlanStepStatus::Completed)
        .count();
    let current = steps
        .iter()
        .position(|step| step.status == PlanStepStatus::InProgress)
        .or_else(|| {
            steps
                .iter()
                .position(|step| step.status == PlanStepStatus::Pending)
        });
    Some(PlanChecklist {
        steps,
        completed,
        current,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_todo_write_and_codex_plans() {
        let todo = r#"{"todos":[{"content":"Ler","status":"completed"},{"content":"Rodar testes","activeForm":"Rodando testes","status":"in_progress"},{"content":"Commit","status":"pending"}]}"#;
        let checklist = parse_plan_checklist(Some(todo)).expect("checklist");
        assert_eq!(checklist.completed, 1);
        assert_eq!(
            checklist.steps[checklist.current.unwrap()].text,
            "Rodando testes"
        );
        let codex = r#"{"plan":[{"step":"A","status":"completed"}]}"#;
        let checklist = parse_plan_checklist(Some(codex)).expect("codex");
        assert_eq!(checklist.current, None);
        assert_eq!(parse_plan_checklist(Some("{\"x\":1}")), None);
        assert_eq!(parse_plan_checklist(Some("not json")), None);
    }
}
