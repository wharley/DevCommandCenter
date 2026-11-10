//! What goes on in each room, for the dot next to its name in the team's
//! column. Only the open room has its feeds loaded, so this reads what the
//! sidebar already knows: where each conversation is and which project each
//! task belongs to (a task shows in its project's room, as in the room).

use std::collections::HashMap;

use super::room_of::room_for;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence {
    Running,
    /// Waits on the person; beats running.
    Waiting,
}

/// Where a piece of work is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Where<'a> {
    /// An agent's conversation in a room (none: Geral).
    Room(Option<&'a str>),
    /// A task of a project, shown in the project's room when it has one.
    Project(&'a str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Work<'a> {
    pub at: Where<'a>,
    pub waiting: bool,
}

/// The presence of each room that has some; `rooms` are the projects with a
/// room.
pub fn presence<'a>(
    work: impl IntoIterator<Item = Work<'a>>,
    rooms: &[String],
) -> HashMap<Option<String>, Presence> {
    let mut found: HashMap<Option<String>, Presence> = HashMap::new();
    for item in work {
        let room = match item.at {
            Where::Room(room) => room.map(str::to_string),
            Where::Project(project) => room_for(Some(project.to_string()), rooms),
        };
        let mark = if item.waiting {
            Presence::Waiting
        } else {
            Presence::Running
        };
        let slot = found.entry(room).or_insert(mark);
        if mark == Presence::Waiting {
            *slot = Presence::Waiting;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(at: Where<'_>, waiting: bool) -> Work<'_> {
        Work { at, waiting }
    }

    #[test]
    fn geral_and_a_projects_room_are_told_apart() {
        let rooms = vec!["checkout".to_string()];
        let found = presence(
            [
                work(Where::Room(None), false),
                work(Where::Project("checkout"), false),
                // No room for it: it shows in Geral.
                work(Where::Project("other"), false),
            ],
            &rooms,
        );
        assert_eq!(found[&None], Presence::Running);
        assert_eq!(found[&Some("checkout".into())], Presence::Running);
        assert_eq!(found.len(), 2);
        let found = presence([work(Where::Room(Some("checkout")), false)], &rooms);
        assert!(!found.contains_key(&None));
    }

    #[test]
    fn waiting_beats_running_whatever_the_order() {
        let rooms = vec!["checkout".to_string()];
        for order in [[false, true], [true, false]] {
            let found = presence(
                order.map(|waiting| work(Where::Project("checkout"), waiting)),
                &rooms,
            );
            assert_eq!(found[&Some("checkout".into())], Presence::Waiting);
        }
    }
}
