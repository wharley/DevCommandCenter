//! Passing the turn without the person (Tucano's @mention chains), only
//! where the person allowed it: an agent may call some teammates itself
//! (`ResidentAgent::may_call`); when its answer ends with "@Dev …" for one
//! of them, DCC passes it at once with the same message the card would.
//! The Reviewer never joins a chain — it is offered at the end of a task's
//! turn or called by the person. A chain stops after `MAX_CHAIN_STEPS`
//! passes with nobody writing; a message from the person starts it over.

use super::agent::REVIEWER_PRESET;
use super::team_room::{handoff_prompt, HANDOFF_MARK};

pub const MAX_CHAIN_STEPS: u32 = 5;
const STEP_LINE: &str = "Passed on automatically by DCC, chain step ";
const ANSWER_LINE: &str = "Answer passed on: ";

/// Which step of a chain the turn `prompt` started is; 0 for the person's.
pub fn chain_step(prompt: &str) -> u32 {
    if !prompt.starts_with(HANDOFF_MARK) {
        return 0;
    }
    prompt
        .lines()
        .find_map(|line| line.strip_prefix(STEP_LINE))
        .and_then(|rest| rest.split_once(" of "))
        .and_then(|(step, _)| step.trim().parse().ok())
        .unwrap_or(0)
}

/// The step the next pass would be, when `to` may be called without the
/// person: `may_call` holds it, it is not the Reviewer, and the chain has
/// room left after the turn that `prompt` started.
pub fn next_step(
    prompt: &str,
    may_call: &[String],
    to_id: &str,
    to_preset: Option<&str>,
) -> Option<u32> {
    let allowed = may_call.iter().any(|id| id == to_id) && to_preset != Some(REVIEWER_PRESET);
    let next = chain_step(prompt) + 1;
    (allowed && next <= MAX_CHAIN_STEPS).then_some(next)
}

/// The message a pass without the person sends: the card's, with the step
/// and the answer it came from (the room shows it was passed).
pub fn auto_handoff_prompt(
    from: &str,
    to: &str,
    request: &str,
    answer: &str,
    answer_key: &str,
    step: u32,
) -> String {
    format!(
        "{}\n\n{STEP_LINE}{step} of {MAX_CHAIN_STEPS}.\n{ANSWER_LINE}{answer_key}",
        handoff_prompt(from, to, request, answer)
    )
}

/// The answer a pass without the person came from, when `prompt` is one.
pub fn passed_answer(prompt: &str) -> Option<&str> {
    prompt
        .starts_with(HANDOFF_MARK)
        .then(|| {
            prompt
                .lines()
                .rev()
                .find_map(|line| line.strip_prefix(ANSWER_LINE))
        })
        .flatten()
        .map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chain_passes_only_where_allowed_and_stops_after_five() {
        let may = vec!["dev".to_string()];
        assert_eq!(next_step("corrige isso", &may, "dev", None), Some(1));
        assert_eq!(next_step("corrige isso", &may, "qa", None), None);
        assert_eq!(
            next_step("x", &["rev".to_string()], "rev", Some(REVIEWER_PRESET)),
            None,
            "the Reviewer never joins a chain"
        );
        let passed = auto_handoff_prompt("Suporte", "Dev", "@Dev corrigir", "análise", "s:t", 2);
        assert_eq!(chain_step(&passed), 2);
        assert_eq!(passed_answer(&passed), Some("s:t"));
        assert_eq!(next_step(&passed, &may, "dev", None), Some(3));
        let last = auto_handoff_prompt("A", "B", "@B x", "y", "s:t", MAX_CHAIN_STEPS);
        assert_eq!(next_step(&last, &may, "dev", None), None);
        assert_eq!(chain_step("corrige"), 0);
        assert_eq!(passed_answer("corrige"), None);
    }
}
