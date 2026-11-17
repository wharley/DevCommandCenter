//! The projects a resident agent works in. One agent serves the person in
//! every project by default; the person can narrow it to some, and then its
//! call buttons, triggers and the tasks it opens stay in those.

/// More than this many chosen projects is a sign something went wrong.
pub const MAX_AGENT_PROJECTS: usize = 64;

/// The person's choice as stored: trimmed, without repeats, in their order.
/// Empty means every project.
pub fn clean_projects(projects: Vec<String>) -> Result<Vec<String>, String> {
    let mut clean: Vec<String> = Vec::new();
    for project in projects {
        let project = project.trim().to_string();
        if project.is_empty() || clean.contains(&project) {
            continue;
        }
        if project.len() > 512 || project.contains(['\n', '\r']) {
            return Err("agent project is not a project id".to_string());
        }
        clean.push(project);
    }
    if clean.len() > MAX_AGENT_PROJECTS {
        return Err(format!(
            "an agent works in at most {MAX_AGENT_PROJECTS} chosen projects"
        ));
    }
    Ok(clean)
}

/// Whether an agent with `projects` works in `project_id`.
pub fn works_in(projects: &[String], project_id: &str) -> bool {
    projects.is_empty() || projects.iter().any(|project| project == project_id)
}

/// Whether a task an event opened (a source, a trigger, a routine, a pass
/// from a teammate) may start in `project_id` with nobody looking: only
/// where the person already said the work belongs — a project chosen for
/// the agent, or the project of the room the turn runs in. An agent left to
/// every project picks one by its name alone, and a name that matches a word
/// of the request ("painel") is not the project the request is about.
pub fn starts_alone(projects: &[String], room: Option<&str>, project_id: &str) -> bool {
    projects.iter().any(|project| project == project_id) || room == Some(project_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_starts_a_task_only_where_the_person_placed_the_agent() {
        let chosen = vec!["checkout".to_string()];
        assert!(starts_alone(&chosen, None, "checkout"));
        assert!(!starts_alone(&chosen, None, "painel"));
        // Every project (no choice): only the room's own.
        assert!(!starts_alone(&[], None, "painel"));
        assert!(starts_alone(&[], Some("checkout"), "checkout"));
        assert!(!starts_alone(&[], Some("checkout"), "painel"));
    }

    #[test]
    fn no_choice_is_every_project() {
        assert!(works_in(&[], "a"));
        let some = vec!["a".to_string()];
        assert!(works_in(&some, "a"));
        assert!(!works_in(&some, "b"));
        assert_eq!(
            clean_projects(vec![" a ".into(), "a".into(), "".into(), "b".into()]).unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(clean_projects(vec!["a\nb".into()]).is_err());
        assert!(clean_projects((0..65).map(|i| i.to_string()).collect()).is_err());
    }
}
