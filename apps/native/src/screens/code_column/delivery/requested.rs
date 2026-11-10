//! A delivery step the room's change card asked for: once this column has
//! read the task it names, its dock's next step runs as if picked in the
//! menu — only when it is still the step the card offered (a commit or the
//! pull request), and those open their dialogs (the commit's message for
//! review, the pull request's form): nothing is committed or pushed until
//! the person confirms there. Anything else, a stale or a mismatched
//! request, is dropped and the task stays on "Mudanças".

use std::time::{Duration, Instant};

use gpui::{Context, Window};

use super::super::CodeColumn;
use crate::data::task_delivery::{DeliveryStep, step_of};

/// How long a request waits for the task to be read.
const WAIT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub(crate) struct RequestedStep {
    pub task_id: String,
    pub step: DeliveryStep,
    /// The checkout shown when it was asked, to tell another task opened
    /// since from the one on its way.
    pub from_root: Option<String>,
    pub at: Instant,
}

/// What to do with the request now.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Follow {
    Wait,
    Drop,
    Run,
}

/// `task`: the task this column has read (none while it reads).
pub(crate) fn follow(
    request: &RequestedStep,
    task: Option<&str>,
    root: Option<&str>,
    offered: Option<DeliveryStep>,
    loading: bool,
    now: Instant,
) -> Follow {
    if now.duration_since(request.at) > WAIT {
        return Follow::Drop;
    }
    let Some(task) = task else {
        return Follow::Wait;
    };
    if task != request.task_id {
        // Still the task shown before, or another one opened since.
        return if root == request.from_root.as_deref() {
            Follow::Wait
        } else {
            Follow::Drop
        };
    }
    if loading {
        return Follow::Wait;
    }
    if offered == Some(request.step) {
        Follow::Run
    } else {
        Follow::Drop
    }
}

impl CodeColumn {
    /// Only a commit or the pull request is asked for: a push runs at once
    /// in the dock, so the person picks it there.
    pub fn request_step(&mut self, task_id: &str, step: DeliveryStep, cx: &mut Context<Self>) {
        if step == DeliveryStep::Push {
            return;
        }
        self.requested = Some(RequestedStep {
            task_id: task_id.to_string(),
            step,
            from_root: self.root.clone(),
            at: Instant::now(),
        });
        cx.notify();
    }

    /// Runs on every change of the column while a request waits.
    pub fn follow_requested(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.requested.as_ref() else {
            return;
        };
        let dock = self.dock();
        let primary = dock.as_ref().and_then(|dock| dock.actions.first().copied());
        let task = self.task_id();
        let decision = follow(
            request,
            task.as_deref(),
            self.root.as_deref(),
            primary.as_ref().and_then(step_of),
            dock.as_ref().is_none_or(|dock| dock.loading),
            Instant::now(),
        );
        match decision {
            Follow::Wait => {}
            Follow::Drop => self.requested = None,
            Follow::Run => {
                self.requested = None;
                if let Some(primary) = primary {
                    self.run_dock_action(primary, window, cx);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> RequestedStep {
        RequestedStep {
            task_id: "w-campo".into(),
            step: DeliveryStep::Commit { push: true },
            from_root: Some("/w/old".into()),
            at: Instant::now(),
        }
    }

    #[test]
    fn it_runs_only_for_its_task_and_the_step_offered() {
        let asked = request();
        let now = asked.at;
        let commit = Some(DeliveryStep::Commit { push: true });
        // The task shown before, then the new one being read.
        assert_eq!(
            follow(&asked, Some("old"), Some("/w/old"), commit, false, now),
            Follow::Wait
        );
        assert_eq!(
            follow(&asked, None, Some("/w/campo"), None, true, now),
            Follow::Wait
        );
        assert_eq!(
            follow(&asked, Some("w-campo"), Some("/w/campo"), commit, true, now),
            Follow::Wait,
            "git still read"
        );
        assert_eq!(
            follow(
                &asked,
                Some("w-campo"),
                Some("/w/campo"),
                commit,
                false,
                now
            ),
            Follow::Run
        );
        // The state moved: another step, or none.
        assert_eq!(
            follow(
                &asked,
                Some("w-campo"),
                Some("/w/campo"),
                Some(DeliveryStep::Push),
                false,
                now
            ),
            Follow::Drop
        );
        assert_eq!(
            follow(&asked, Some("w-campo"), Some("/w/campo"), None, false, now),
            Follow::Drop
        );
        // Another task opened since, or too long.
        assert_eq!(
            follow(&asked, Some("other"), Some("/w/other"), commit, false, now),
            Follow::Drop
        );
        assert_eq!(
            follow(
                &asked,
                Some("w-campo"),
                Some("/w/campo"),
                commit,
                false,
                now + WAIT * 2
            ),
            Follow::Drop
        );
    }
}
