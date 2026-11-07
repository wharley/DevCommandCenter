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

#[cfg(test)]
mod tests {
    use super::*;

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
