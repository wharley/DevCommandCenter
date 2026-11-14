//! Passing the turn without the person (Tucano's @mention chains), only
//! where the person allowed it: an agent may call some teammates itself
//! (`ResidentAgent::may_call`); when its answer ends with "@Dev …" for one
//! of them, DCC passes it at once with the same message the card would.
//! The Reviewer never joins a chain — it is offered at the end of a task's
//! turn or called by the person. A chain stops after `MAX_CHAIN_STEPS`
//! passes with nobody writing; a message from the person starts it over.
//!
//! When the chain ends — the last agent answers without passing on — what
//! it answered goes back once to the agent the chain started with, so that
//! one closes it with the person (Tucano's `pedir`, where the result returns
//! to who asked). A return never starts another chain.

use super::agent::REVIEWER_PRESET;
use super::team_room::{handoff_prompt, HANDOFF_MARK};

pub const MAX_CHAIN_STEPS: u32 = 5;
const STEP_LINE: &str = "Passed on automatically by DCC, chain step ";
const ANSWER_LINE: &str = "Answer passed on: ";
/// The line that marks a chain's result going back to who started it.
const RETURN_LINE: &str = "Returned by DCC at the end of the chain.";
/// How much of the last answer goes back.
const RETURN_CHARS: usize = 2_000;

/// Which step of a chain the turn `prompt` started is; 0 for the person's.
pub fn chain_step(prompt: &str) -> u32 {
    if !prompt.starts_with(HANDOFF_MARK) {
        return 0;
    }
    // A result that came back: the chain is over, nothing follows it.
    if is_return(prompt) {
        return MAX_CHAIN_STEPS;
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

/// Whether `prompt` is a chain's result going back to who started it.
pub fn is_return(prompt: &str) -> bool {
    prompt.starts_with(HANDOFF_MARK) && prompt.lines().any(|line| line == RETURN_LINE)
}

/// The message that takes the last answer of a chain back to the agent it
/// started with: the room shows its first line as a pass (`from → to`).
pub fn return_prompt(from: &str, to: &str, answer: &str, answer_key: &str) -> String {
    let said: String = answer.chars().take(RETURN_CHARS).collect();
    format!(
        "{HANDOFF_MARK}{from} → {to}: returned\n\n{RETURN_LINE}\nWhat you passed on reached {from}, who answered:\n{said}\n\nClose it with the person: say where it stands in one or two sentences, in their language. Do not pass it on again. If it adds nothing they need to know, answer exactly {} and nothing else.\n{ANSWER_LINE}{answer_key}",
        super::agent_routine::QUIET_ANSWER
    )
}

/// Whether a turn that `prompt` started and that passed nothing on owes a
/// return: it was itself a pass, and not a return.
pub fn owes_return(prompt: &str) -> bool {
    prompt.starts_with(HANDOFF_MARK) && !is_return(prompt)
}

/// The agent that passed `prompt`, by its name in the first line
/// (`↪ Suporte → Dev: …`).
pub fn passed_by(prompt: &str) -> Option<&str> {
    let heading = prompt.strip_prefix(HANDOFF_MARK)?.lines().next()?;
    heading.split_once(" → ").map(|(from, _)| from.trim())
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

    #[test]
    fn the_last_answer_goes_back_once_and_starts_nothing() {
        let may = vec!["dev".to_string()];
        let passed = auto_handoff_prompt("Suporte", "Dev", "@Dev corrigir", "análise", "s:t", 1);
        assert!(owes_return(&passed));
        assert_eq!(passed_by(&passed), Some("Suporte"));
        // The person's own message owes nothing: there was no pass.
        assert!(!owes_return("corrige isso"));
        assert_eq!(passed_by("corrige isso"), None);

        let back = return_prompt("Dev", "Suporte", "Abri a tarefa e o PR.", "s-dev:t-9");
        // It names the answer it brings, as a pass does (the room groups
        // them by it).
        assert_eq!(passed_answer(&back), Some("s-dev:t-9"));
        assert!(is_return(&back));
        assert!(back.starts_with("↪ Dev → Suporte: returned"));
        assert!(back.contains("Abri a tarefa e o PR."));
        // A return is not a pass to return again, and nothing is passed from it.
        assert!(!owes_return(&back));
        assert_eq!(next_step(&back, &may, "dev", None), None);
        assert!(!is_return(&passed));
        // A long answer goes back cut.
        let long = return_prompt("Dev", "Suporte", &"a".repeat(5_000), "s:t");
        assert!(long.chars().count() < 3_000);
    }
}
