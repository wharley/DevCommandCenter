//! Whether a task may be opened, and started, from the turn that asks for
//! it: the person's own turn opens and starts tasks; an event that woke an
//! agent in its own conversation (a trigger, a routine) opens one in a
//! project, started only when the person lets that agent start tasks
//! itself; a task's [DCC] status never opens one — that is how review →
//! fix → review chains would form with nobody asking.

use dcc_core::domain::timeline::parse_delegation_hand_back;

const DCC_TURN_PREFIX: &str = "[DCC] ";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskGate {
    Refused,
    /// It waits as a card the person starts.
    CardOnly,
    MayStart,
}

/// `prompt`: the open turn's prompt, none outside a turn; `in_project`: a
/// task of its own in a project (`dcc_open_task`); `starts_itself`: the
/// agent's "Pode começar tarefas sozinho".
pub fn task_gate(prompt: Option<&str>, in_project: bool, starts_itself: bool) -> TaskGate {
    let Some(prompt) = prompt else {
        return TaskGate::CardOnly;
    };
    if !prompt.starts_with(DCC_TURN_PREFIX) {
        return TaskGate::MayStart;
    }
    let event = in_project && parse_delegation_hand_back(prompt).is_none();
    match (event, starts_itself) {
        (false, _) => TaskGate::Refused,
        (true, false) => TaskGate::CardOnly,
        (true, true) => TaskGate::MayStart,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_person_starts_tasks_an_event_proposes_them_and_a_status_never() {
        let person = Some("corrige o campo da Shopify");
        let issue = Some("[DCC] An issue was assigned to the person: acme/app #4 — \"x\" (url).");
        let routine = Some("[DCC] Routine (09:00): veja o Slack");
        let status = Some("[DCC] Task \"Fase 2\" ended a turn — Claude Code (task t-1).");
        assert_eq!(task_gate(person, true, false), TaskGate::MayStart);
        assert_eq!(task_gate(issue, true, false), TaskGate::CardOnly);
        assert_eq!(task_gate(routine, true, true), TaskGate::MayStart);
        assert_eq!(task_gate(status, true, true), TaskGate::Refused);
        // In a task (not its own conversation), an event opens nothing.
        assert_eq!(task_gate(issue, false, true), TaskGate::Refused);
        assert_eq!(task_gate(None, true, true), TaskGate::CardOnly);
    }
}
