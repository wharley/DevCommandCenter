//! The project rooms down the team's column (`docs/PLANO_SALA_POR_PROJETO`):
//! the ones the person made with "Nova sala" first, in order, then any
//! project an agent already talks in; each project once, and only projects
//! that still exist. One order for the desktop's column and the phone's list.

/// The rooms to list, `(project id, name)`: `saved` first, then `talked`,
/// from the person's `projects` as `(id, name)`.
pub fn rooms(
    saved: &[String],
    talked: impl IntoIterator<Item = String>,
    projects: &[(String, String)],
) -> Vec<(String, String)> {
    let mut listed: Vec<(String, String)> = Vec::new();
    for id in saved.iter().cloned().chain(talked) {
        if listed.iter().any(|(kept, _)| *kept == id) {
            continue;
        }
        if let Some((_, name)) = projects.iter().find(|(project, _)| *project == id) {
            listed.push((id, name.clone()));
        }
    }
    listed
}

/// One choice per project (a project can have several checkouts), named
/// after its first one, in the given order: `(project id, display name)`.
pub fn project_names<'a>(
    checkouts: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<(String, String)> {
    let mut names: Vec<(String, String)> = Vec::new();
    for (project, name) in checkouts {
        if !names.iter().any(|(id, _)| id == project) {
            names.push((project.to_string(), name.to_string()));
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn made_rooms_come_first_then_the_ones_talked_in() {
        let projects = [
            ("checkout".to_string(), "Checkout".to_string()),
            ("app".to_string(), "vendeagora-app".to_string()),
        ];
        let listed = rooms(
            &["checkout".into(), "gone".into()],
            ["app".to_string(), "checkout".to_string()],
            &projects,
        );
        assert_eq!(
            listed,
            [
                ("checkout".to_string(), "Checkout".to_string()),
                ("app".to_string(), "vendeagora-app".to_string())
            ]
        );
    }

    #[test]
    fn a_project_is_named_after_its_first_checkout() {
        let names = project_names([("p", "Loja"), ("q", "Site"), ("p", "Loja (2)")]);
        assert_eq!(
            names,
            [
                ("p".to_string(), "Loja".to_string()),
                ("q".to_string(), "Site".to_string())
            ]
        );
    }
}
