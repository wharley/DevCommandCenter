//! What an agent did in tasks, for the next turn of its own conversation:
//! a conversation in a task (a trigger woke it there, the person called it
//! from the task) is another conversation than the room's, so the room's
//! one hears it here — the way Tucano posts a project's events in the
//! channel the agent reads. Short, the newest few, and only what came after
//! the conversation's last turn.

use super::agent_routine::is_quiet;

/// At most this many tasks, the newest.
const MAX_ENTRIES: usize = 3;
const ASKED_CHARS: usize = 220;
const ANSWER_CHARS: usize = 700;

/// One task conversation's last turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskWork {
    /// The task's name.
    pub task: String,
    /// What started the turn (a trigger's wake, the person's words).
    pub asked: String,
    /// The agent's final answer.
    pub answer: String,
    /// When it ended (RFC 3339).
    pub at: String,
}

fn clip(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let cut: String = flat.chars().take(max - 1).collect();
    format!("{}…", cut.trim_end())
}

/// The first line that says something, for what started a turn.
fn first_line(text: &str) -> &str {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
}

/// The block for the conversation's next turn, or none when nothing new.
pub fn recent_work_block(mut work: Vec<TaskWork>) -> Option<String> {
    work.retain(|entry| !entry.answer.trim().is_empty() && !is_quiet(&entry.answer));
    if work.is_empty() {
        return None;
    }
    work.sort_by(|left, right| right.at.cmp(&left.at));
    work.truncate(MAX_ENTRIES);
    work.reverse();
    let entries: Vec<String> = work
        .iter()
        .map(|entry| {
            format!(
                "- In the task \"{}\" ({}), started by: {}\n  You answered: {}",
                entry.task,
                entry.at,
                clip(first_line(&entry.asked), ASKED_CHARS),
                clip(&entry.answer, ANSWER_CHARS)
            )
        })
        .collect();
    Some(format!(
        "<dcc_recent_work>\nWhat you did in tasks since your last turn here, each in that task's own conversation (this one did not see it). When the person refers to it, this is it; open the task for more.\n{}\n</dcc_recent_work>",
        entries.join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(task: &str, at: &str, answer: &str) -> TaskWork {
        TaskWork {
            task: task.into(),
            asked: "[DCC] New review activity on this task's pull request acme/app#1049 from @keppelen.\n\nRead what they asked".into(),
            answer: answer.into(),
            at: at.into(),
        }
    }

    #[test]
    fn the_newest_work_goes_in_short_and_in_order() {
        let block = recent_work_block(vec![
            work("A", "2026-11-11T10:00:00Z", "first"),
            work(
                "B",
                "2026-11-11T10:55:00Z",
                "O comentário do @keppelen procede.",
            ),
            work("C", "2026-11-11T09:00:00Z", "oldest"),
            work("D", "2026-11-11T10:30:00Z", "NOTHING_TO_REPORT"),
            work("E", "2026-11-11T10:40:00Z", &"x".repeat(900)),
        ])
        .unwrap();
        // The quiet one is left out; the three newest, oldest first.
        assert!(!block.contains("\"C\"") && !block.contains("\"D\""));
        let a = block.find("\"A\"").unwrap();
        let e = block.find("\"E\"").unwrap();
        let b = block.find("\"B\"").unwrap();
        assert!(a < e && e < b);
        assert!(block.contains("started by: [DCC] New review activity on this task's pull request acme/app#1049 from @keppelen."));
        assert!(block.contains(&format!("{}…", "x".repeat(ANSWER_CHARS - 1))));
        assert!(recent_work_block(vec![work("D", "t", "NOTHING_TO_REPORT")]).is_none());
        assert!(recent_work_block(Vec::new()).is_none());
    }
}
