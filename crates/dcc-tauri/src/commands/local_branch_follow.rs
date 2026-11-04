//! A local-direct conversation follows a branch its own agent switched to.
//! Git's reflog says when the folder moved and from where to where; the
//! conversation's events say when its turns ran. The task follows only when
//! every move from its branch to the current one happened inside one of its
//! own turns, and no other conversation on the same folder had a turn
//! running then. Anything else (the person in the terminal, another task,
//! no reflog) keeps the guard's refusal.

use std::path::Path;

use dcc_core::{
    domain::{session::SessionId, workspace::WorkspaceId},
    ports::WorkspaceRepo,
};
use dcc_infra::db::{SqliteSessionRepo, SqliteWorkspaceRepo, TurnWindow};

use crate::git::run_git_output;

/// Seconds of slack around a turn: the reflog keeps whole seconds.
const SLACK: i64 = 2;
/// How far back the reflog is read.
const MAX_CHECKOUTS: usize = 20;
/// A turn left open longer than this was cut off (the app quit mid-turn),
/// not running: a long run through the night still fits.
const OPEN_TURN_LIMIT: i64 = 24 * 60 * 60;

/// `checkout: moving from <from> to <to>`, at a unix time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Checkout {
    pub at: i64,
    pub from: String,
    pub to: String,
}

/// A turn's start and end in unix seconds; an open turn runs until now.
type Window = (i64, i64);

/// The folder's latest branch moves, newest first.
fn checkouts(root: &str) -> Option<Vec<Checkout>> {
    // `%gd` with `--date=unix` is `HEAD@{<when the move happened>}`; `%ct`
    // would be the date of the commit it landed on, often days older.
    let output = run_git_output(
        root,
        &[
            "reflog",
            "show",
            "--date=unix",
            "--format=%gd%x09%gs",
            "HEAD",
            "--",
        ],
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(parse_checkouts(&String::from_utf8_lossy(&output.stdout)))
}

fn parse_checkouts(reflog: &str) -> Vec<Checkout> {
    reflog
        .lines()
        .filter_map(|line| {
            let (selector, subject) = line.split_once('\t')?;
            let at = selector.rsplit_once("@{")?.1.strip_suffix('}')?;
            let moved = subject.strip_prefix("checkout: moving from ")?;
            let (from, to) = moved.rsplit_once(" to ")?;
            Some(Checkout {
                at: at.trim().parse().ok()?,
                from: from.to_string(),
                to: to.to_string(),
            })
        })
        .take(MAX_CHECKOUTS)
        .collect()
}

fn seconds(at: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|at| at.timestamp())
}

fn inside(windows: &[Window], at: i64) -> bool {
    windows
        .iter()
        .any(|(start, end)| *start - SLACK <= at && at <= *end + SLACK)
}

/// Each move, newest first, lands where the next newer one left from and
/// happened in one of `own`'s turns and none of `others'`, back to the
/// one that left `expected`.
pub(crate) fn followed(
    checkouts: &[Checkout],
    expected: &str,
    current: &str,
    own: &[Window],
    others: &[Window],
) -> bool {
    let mut landing = current;
    for checkout in checkouts {
        if checkout.to != landing || !inside(own, checkout.at) || inside(others, checkout.at) {
            return false;
        }
        if checkout.from == expected {
            return true;
        }
        landing = &checkout.from;
    }
    false
}

/// The windows of these turns; an open one runs until `now` only when it
/// is its conversation's latest and began within `OPEN_TURN_LIMIT` (any
/// other open one was cut off).
fn windows(turns: &[TurnWindow], now: i64) -> Vec<Window> {
    turns
        .iter()
        .enumerate()
        .filter_map(|(index, turn)| {
            let start = seconds(&turn.started_at)?;
            let end = match &turn.ended_at {
                Some(ended) => seconds(ended)?,
                None => {
                    let latest = turns[index + 1..]
                        .iter()
                        .all(|later| later.session_id != turn.session_id);
                    if !latest || now - start > OPEN_TURN_LIMIT {
                        return None;
                    }
                    now
                }
            };
            Some((start, end))
        })
        .collect()
}

/// Whether `session`'s own agent moved the folder at `physical` from
/// `expected` to `current`. Any read that fails says no.
pub(crate) async fn agent_switched(
    db: &Path,
    session: &SessionId,
    physical: &Path,
    expected: &str,
    current: &str,
) -> bool {
    let Some(checkouts) = checkouts(&physical.to_string_lossy()) else {
        return false;
    };
    let Ok(workspaces) = SqliteWorkspaceRepo::open(db) else {
        return false;
    };
    let Ok(all) = workspaces.list_workspaces().await else {
        return false;
    };
    let sharing: Vec<WorkspaceId> = all
        .into_iter()
        .filter(|workspace| {
            workspace.worktree_path.is_none()
                && std::fs::canonicalize(&workspace.root_path).is_ok_and(|root| root == physical)
        })
        .map(|workspace| workspace.id)
        .collect();
    let Ok(sessions) = SqliteSessionRepo::open_read_only(db) else {
        return false;
    };
    let Ok(mut ids) = sessions.list_session_ids_for_workspace_scope(&sharing) else {
        return false;
    };
    if !ids.contains(session) {
        ids.push(session.clone());
    }
    let Ok(turns) = sessions.turn_windows(&ids) else {
        return false;
    };
    let now = chrono::Utc::now().timestamp();
    let (own, others): (Vec<TurnWindow>, Vec<TurnWindow>) = turns
        .into_iter()
        .partition(|turn| &turn.session_id == session);
    followed(
        &checkouts,
        expected,
        current,
        &windows(&own, now),
        &windows(&others, now),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkout(at: i64, from: &str, to: &str) -> Checkout {
        Checkout {
            at,
            from: from.into(),
            to: to.into(),
        }
    }

    #[test]
    fn follows_only_moves_its_own_turns_made_from_its_branch() {
        let own = [(100, 200)];
        // The agent made a branch in its turn.
        let made = [checkout(150, "main", "dcc/fix")];
        assert!(followed(&made, "main", "dcc/fix", &own, &[]));
        // Twice in the same turn: from where it began to where it ended.
        let twice = [
            checkout(180, "dcc/a", "dcc/b"),
            checkout(150, "main", "dcc/a"),
        ];
        assert!(followed(&twice, "main", "dcc/b", &own, &[]));
        // The person in the terminal, after the turn.
        let later = [checkout(400, "main", "dcc/fix")];
        assert!(!followed(&later, "main", "dcc/fix", &own, &[]));
        // In the turn, then again by hand afterwards.
        let mixed = [
            checkout(400, "dcc/a", "dcc/b"),
            checkout(150, "main", "dcc/a"),
        ];
        assert!(!followed(&mixed, "main", "dcc/b", &own, &[]));
        // Another conversation on the folder was running too: whose was it?
        assert!(!followed(&made, "main", "dcc/fix", &own, &[(140, 160)]));
        // Not from the task's branch, or a move missing from the reflog.
        assert!(!followed(&made, "develop", "dcc/fix", &own, &[]));
        assert!(!followed(&made, "main", "other", &own, &[]));
        assert!(!followed(&[], "main", "dcc/fix", &own, &[]));
    }

    #[test]
    fn a_move_is_dated_when_it_happened_not_by_its_commit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let git = |args: &[&str], date: &str| {
            let status = std::process::Command::new("git")
                .args(["-c", "user.name=T", "-c", "user.email=t@example.com"])
                .args(args)
                .current_dir(root)
                .env("GIT_AUTHOR_DATE", "@1000000000 +0000")
                .env("GIT_COMMITTER_DATE", date)
                .status()
                .unwrap();
            assert!(status.success());
        };
        git(&["init", "-q", "-b", "main"], "@1000000000 +0000");
        // A commit from long ago, then a branch made on it now: the reflog
        // entry takes the committer date git is given at that moment.
        git(
            &["commit", "-q", "--allow-empty", "-m", "old"],
            "@1000000000 +0000",
        );
        git(&["switch", "-q", "-c", "dcc/new"], "@1700000000 +0000");
        assert_eq!(
            checkouts(root).unwrap(),
            [checkout(1_700_000_000, "main", "dcc/new")]
        );
        assert_eq!(
            parse_checkouts("HEAD@{12}\tcommit: x\nHEAD@{5}\tcheckout: moving from a to b\n"),
            [checkout(5, "a", "b")]
        );
    }

    #[test]
    fn an_open_turn_runs_until_now_only_when_it_is_the_latest() {
        let turn = |session: &str, start: &str, end: Option<&str>| TurnWindow {
            session_id: SessionId(session.into()),
            started_at: start.into(),
            ended_at: end.map(Into::into),
        };
        let turns = [
            turn("a", "2026-10-30T20:00:00+00:00", None),
            turn("b", "2026-10-30T20:05:00+00:00", None),
            turn(
                "a",
                "2026-10-30T20:10:00+00:00",
                Some("2026-10-30T20:20:00.5+00:00"),
            ),
        ];
        let base = seconds("2026-10-30T20:00:00+00:00").unwrap();
        assert_eq!(
            windows(&turns, base + 9_999),
            [(base + 300, base + 9_999), (base + 600, base + 1_200)]
        );
        // Open for more than a day: the app quit mid-turn.
        assert_eq!(
            windows(&turns, base + OPEN_TURN_LIMIT + 301),
            [(base + 600, base + 1_200)]
        );
    }
}
