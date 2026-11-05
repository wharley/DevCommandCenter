//! Which task each simulator belongs to: the task that opened it, by the
//! person or by its agent. A task holds one simulator, two tasks never
//! drive the same one, and the panel follows the task on screen to its own
//! (each task's app and Metro keep running on their simulator).

use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owner {
    pub task_id: String,
    /// The task's name, for "in use by …".
    pub title: String,
}

#[derive(Debug, Default)]
pub(crate) struct Owners {
    by_udid: HashMap<String, Owner>,
}

impl Owners {
    /// `udid` becomes `owner`'s. The simulator the task held before is let
    /// go and returned.
    pub fn claim(&mut self, udid: &str, owner: Owner) -> Option<String> {
        let previous = self
            .of_task(&owner.task_id)
            .filter(|previous| *previous != udid)
            .map(str::to_string);
        if let Some(previous) = &previous {
            self.by_udid.remove(previous);
        }
        self.by_udid.insert(udid.to_string(), owner);
        previous
    }

    pub fn owner(&self, udid: &str) -> Option<&Owner> {
        self.by_udid.get(udid)
    }

    /// The simulator `task_id` holds.
    pub fn of_task(&self, task_id: &str) -> Option<&str> {
        self.by_udid
            .iter()
            .find(|(_, owner)| owner.task_id == task_id)
            .map(|(udid, _)| udid.as_str())
    }

    /// The other task that holds `udid`, if it is not `task_id`'s.
    pub fn taken(&self, udid: &str, task_id: Option<&str>) -> Option<&Owner> {
        self.owner(udid)
            .filter(|owner| Some(owner.task_id.as_str()) != task_id)
    }

    pub fn release(&mut self, udid: &str) {
        self.by_udid.remove(udid);
    }

    /// The tasks holding a simulator.
    pub fn tasks(&self) -> Vec<String> {
        let mut tasks: Vec<String> = self
            .by_udid
            .values()
            .map(|owner| owner.task_id.clone())
            .collect();
        tasks.sort();
        tasks
    }

    /// Lets go of `task_id`'s simulator and returns it.
    pub fn release_task(&mut self, task_id: &str) -> Option<String> {
        let udid = self.of_task(task_id)?.to_string();
        self.by_udid.remove(&udid);
        Some(udid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str) -> Owner {
        Owner {
            task_id: id.into(),
            title: format!("Task {id}"),
        }
    }

    #[test]
    fn a_task_holds_one_simulator_and_another_task_sees_it_taken() {
        let mut owners = Owners::default();
        assert_eq!(owners.claim("sim-1", task("a")), None);
        assert_eq!(owners.of_task("a"), Some("sim-1"));
        assert_eq!(owners.taken("sim-1", Some("a")), None, "its own");
        assert_eq!(owners.taken("sim-1", Some("b")), Some(&task("a")));
        assert_eq!(owners.taken("sim-1", None), Some(&task("a")));
        assert_eq!(owners.taken("sim-2", Some("b")), None, "a free one");
        // Another simulator for the same task: the first is let go.
        assert_eq!(owners.claim("sim-2", task("a")), Some("sim-1".into()));
        assert_eq!(owners.owner("sim-1"), None);
        assert_eq!(owners.claim("sim-2", task("a")), None, "the same again");
    }

    #[test]
    fn a_finished_task_lets_its_simulator_go() {
        let mut owners = Owners::default();
        owners.claim("sim-1", task("a"));
        owners.claim("sim-2", task("b"));
        assert_eq!(owners.tasks(), ["a", "b"]);
        assert_eq!(owners.release_task("a"), Some("sim-1".into()));
        assert_eq!(owners.release_task("a"), None);
        owners.release("sim-2");
        assert!(owners.tasks().is_empty());
    }
}
