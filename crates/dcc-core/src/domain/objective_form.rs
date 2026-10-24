//! The person's side of the objective (`session-objective.logic.ts`): the
//! form with its numbers still as typed, the reason a draft is refused
//! before it reaches the backend, the transitions each status offers and
//! the counters as the panel shows them.

use super::objective::{
    ObjectiveStatus, ObjectiveTransition, SessionObjective, SessionObjectiveDraft,
    DEFAULT_OBJECTIVE_MAX_CONSECUTIVE_FAILURES, MAX_OBJECTIVE_MAX_CONSECUTIVE_FAILURES,
    MAX_OBJECTIVE_MAX_TURNS, MAX_OBJECTIVE_TEXT_CHARS,
};

/// What the fields hold; the limits stay text until saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectiveForm {
    pub intent: String,
    pub done_when: String,
    pub max_consecutive_failures: String,
    pub max_turns: String,
}

impl Default for ObjectiveForm {
    /// `EMPTY_OBJECTIVE_FORM`: three failures, no turn budget.
    fn default() -> Self {
        Self {
            intent: String::new(),
            done_when: String::new(),
            max_consecutive_failures: DEFAULT_OBJECTIVE_MAX_CONSECUTIVE_FAILURES.to_string(),
            max_turns: String::new(),
        }
    }
}

/// Why the form can't be saved; `key()` is the catalog's
/// `composer.objective.errors.*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectiveFormError {
    IntentRequired,
    TextTooLong,
    FailuresOutOfRange,
    TurnsOutOfRange,
}

impl ObjectiveFormError {
    pub fn key(self) -> &'static str {
        match self {
            Self::IntentRequired => "intent_required",
            Self::TextTooLong => "text_too_long",
            Self::FailuresOutOfRange => "failures_out_of_range",
            Self::TurnsOutOfRange => "turns_out_of_range",
        }
    }
}

/// Empty is "not set"; otherwise digits only, inside `min..=max`.
fn bounded(value: &str, min: u32, max: u32) -> Result<Option<u32>, ()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(());
    }
    match trimmed.parse::<u32>() {
        Ok(parsed) if (min..=max).contains(&parsed) => Ok(Some(parsed)),
        _ => Err(()),
    }
}

impl ObjectiveForm {
    /// `objectiveFormFromRecord`.
    pub fn from_record(objective: Option<&SessionObjective>) -> Self {
        let Some(objective) = objective else {
            return Self::default();
        };
        Self {
            intent: objective.intent.clone(),
            done_when: objective.done_when.clone(),
            max_consecutive_failures: objective.max_consecutive_failures.to_string(),
            max_turns: objective
                .max_turns
                .map(|turns| turns.to_string())
                .unwrap_or_default(),
        }
    }

    /// `objectiveDraftFromForm`: the backend's own rules, checked first so
    /// the form says which field is wrong.
    pub fn to_draft(&self) -> Result<SessionObjectiveDraft, ObjectiveFormError> {
        let intent = self.intent.trim();
        let done_when = self.done_when.trim();
        if intent.is_empty() {
            return Err(ObjectiveFormError::IntentRequired);
        }
        if intent.chars().count() > MAX_OBJECTIVE_TEXT_CHARS
            || done_when.chars().count() > MAX_OBJECTIVE_TEXT_CHARS
        {
            return Err(ObjectiveFormError::TextTooLong);
        }
        let max_consecutive_failures = bounded(
            &self.max_consecutive_failures,
            1,
            MAX_OBJECTIVE_MAX_CONSECUTIVE_FAILURES,
        )
        .map_err(|()| ObjectiveFormError::FailuresOutOfRange)?;
        let max_turns = bounded(&self.max_turns, 1, MAX_OBJECTIVE_MAX_TURNS)
            .map_err(|()| ObjectiveFormError::TurnsOutOfRange)?;
        Ok(SessionObjectiveDraft {
            intent: intent.to_string(),
            done_when: done_when.to_string(),
            max_consecutive_failures,
            max_turns,
        })
    }
}

/// `availableObjectiveTransitions`.
pub fn available_transitions(status: ObjectiveStatus) -> &'static [ObjectiveTransition] {
    match status {
        ObjectiveStatus::Active => &[ObjectiveTransition::Pause, ObjectiveTransition::Complete],
        ObjectiveStatus::Paused => &[ObjectiveTransition::Resume, ObjectiveTransition::Complete],
        ObjectiveStatus::Done => &[ObjectiveTransition::Resume],
    }
}

/// `summarizeObjective`: the counters as the panel prints them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectiveSummary {
    /// "4" or "4/10" with a turn budget.
    pub turns: String,
    /// "1/3".
    pub failures: String,
    pub retries: u32,
    /// Queued follow-ups wait until the person acts.
    pub blocks_automatic_dispatch: bool,
}

pub fn summarize(objective: &SessionObjective) -> ObjectiveSummary {
    ObjectiveSummary {
        turns: match objective.max_turns {
            Some(max) => format!("{}/{max}", objective.turns_used),
            None => objective.turns_used.to_string(),
        },
        failures: format!(
            "{}/{}",
            objective.consecutive_failures, objective.max_consecutive_failures
        ),
        retries: objective.retries,
        blocks_automatic_dispatch: !objective.allows_automatic_dispatch(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::SessionId;

    fn form(intent: &str, failures: &str, turns: &str) -> ObjectiveForm {
        ObjectiveForm {
            intent: intent.into(),
            done_when: " testes passam ".into(),
            max_consecutive_failures: failures.into(),
            max_turns: turns.into(),
        }
    }

    #[test]
    fn the_form_fails_on_the_field_that_is_wrong() {
        assert_eq!(
            form("  ", "3", "").to_draft(),
            Err(ObjectiveFormError::IntentRequired)
        );
        assert_eq!(
            form(&"x".repeat(2_001), "3", "").to_draft(),
            Err(ObjectiveFormError::TextTooLong)
        );
        for failures in ["0", "21", "2.5", "-1", "três"] {
            assert_eq!(
                form("Checkout", failures, "").to_draft(),
                Err(ObjectiveFormError::FailuresOutOfRange),
                "{failures}"
            );
        }
        assert_eq!(
            form("Checkout", "3", "10001").to_draft(),
            Err(ObjectiveFormError::TurnsOutOfRange)
        );
        let draft = form(" Checkout ", "", " 12 ").to_draft().unwrap();
        assert_eq!(draft.intent, "Checkout");
        assert_eq!(draft.done_when, "testes passam");
        assert_eq!(draft.max_consecutive_failures, None);
        assert_eq!(draft.max_turns, Some(12));
        assert!(draft.validate().is_ok());
    }

    #[test]
    fn records_fill_the_form_and_summarize_their_counters() {
        assert_eq!(
            ObjectiveForm::from_record(None).max_consecutive_failures,
            "3"
        );
        let mut objective = SessionObjective::new(
            SessionId("s".into()),
            form("Checkout", "2", "10")
                .to_draft()
                .unwrap()
                .validate()
                .unwrap(),
            "2026-10-24T00:00:00Z",
        );
        objective.turns_used = 4;
        objective.consecutive_failures = 1;
        let filled = ObjectiveForm::from_record(Some(&objective));
        assert_eq!(
            (
                filled.max_consecutive_failures.as_str(),
                filled.max_turns.as_str()
            ),
            ("2", "10")
        );
        let summary = summarize(&objective);
        assert_eq!(
            (summary.turns.as_str(), summary.failures.as_str()),
            ("4/10", "1/2")
        );
        assert!(!summary.blocks_automatic_dispatch);
        objective.transition(ObjectiveTransition::Pause, "2026-10-24T00:00:01Z");
        assert!(summarize(&objective).blocks_automatic_dispatch);
        assert_eq!(
            available_transitions(objective.status),
            &[ObjectiveTransition::Resume, ObjectiveTransition::Complete]
        );
        assert_eq!(
            available_transitions(ObjectiveStatus::Done),
            &[ObjectiveTransition::Resume]
        );
    }
}
