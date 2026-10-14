//! A provider usage limit is a wait, not an error.
//!
//! When an agent runs out of its plan's allowance mid-task, the turn ends,
//! but the task is not broken: it can continue once the window resets. The
//! session records the hit (`SessionEventKind::UsageLimitReached`, with the
//! reset time when the provider reported one) and everything else — the
//! sidebar's "Aguardando limite", "Precisa de você" for a limit with no known
//! reset, the automatic resume — is derived here from the session events, so
//! it survives restarts without a separate store.
//!
//! The desktop sidebar (`apps/desktop/src/features/workspaces/usage-limit.ts`)
//! runs the same `usage_limit_phase` rule; the shared cases live in
//! `packages/contracts/fixtures/attention-vectors.json`.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::session::{visible_session_events, SessionEventKind, SessionEventRecord, TurnId};

/// Automatic resumes allowed per session in `AUTO_RESUME_WINDOW`; past it,
/// the next hit waits for the person instead of burning another window.
pub const MAX_AUTO_RESUMES: u32 = 3;
/// "Per night": the window the automatic resumes are counted in.
pub const AUTO_RESUME_WINDOW_HOURS: i64 = 12;
/// Slack after the reset before resuming, so a resume does not land on the
/// exact second the window reopens (clock skew, provider propagation).
pub const RESUME_SLACK_MIN_SECS: i64 = 60;
pub const RESUME_SLACK_MAX_SECS: i64 = 180;
/// The continuation turn sent on resume. Fixed text: never generated.
pub const CONTINUATION_PROMPT: &str = "Continue de onde parou.";

/// The continuation turn. `last_step` is the last plan step the session
/// knows was finished, when it knows one.
pub fn continuation_prompt(last_step: Option<&str>) -> String {
    match last_step.map(str::trim).filter(|step| !step.is_empty()) {
        Some(step) => {
            let step: String = step.chars().take(200).collect();
            format!("{CONTINUATION_PROMPT} A última etapa concluída foi: {step}.")
        }
        None => CONTINUATION_PROMPT.to_string(),
    }
}

/// What the session knows about the limit that stopped its latest turn.
/// Facts only; the phase depends on the clock (`usage_limit_phase`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsageLimit {
    pub turn_id: TurnId,
    /// When the turn stopped on the limit.
    pub hit_at: String,
    /// When the provider said the window reopens; `None` when it did not say.
    pub resets_at: Option<String>,
    /// Whether DCC continues by itself after the reset: the session's choice
    /// at the hit, or a later change of it.
    pub auto_resume: bool,
    /// Automatic resumes of this session in the `AUTO_RESUME_WINDOW_HOURS`
    /// before the hit.
    pub auto_resumes_used: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum UsageLimitPhase {
    /// "Aguardando limite": nothing to do until the reset.
    Waiting,
    /// "Pausado por limite": needs the person.
    Paused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum UsageLimitPauseReason {
    /// The provider did not say when the limit resets; DCC does not guess.
    UnknownReset,
    /// The window reopened and automatic resume is off.
    ResetPassed,
    /// Automatic resume already ran `MAX_AUTO_RESUMES` times this night.
    AttemptsExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageLimitStatus {
    pub phase: UsageLimitPhase,
    pub pause_reason: Option<UsageLimitPauseReason>,
}

fn parse_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

/// The phase of a limit at `now`. Mirrors `usageLimitPhase` on the desktop.
pub fn usage_limit_phase(limit: &SessionUsageLimit, now: DateTime<Utc>) -> UsageLimitStatus {
    let paused = |reason| UsageLimitStatus {
        phase: UsageLimitPhase::Paused,
        pause_reason: Some(reason),
    };
    let Some(resets_at) = limit.resets_at.as_deref().and_then(parse_time) else {
        return paused(UsageLimitPauseReason::UnknownReset);
    };
    if limit.auto_resume && limit.auto_resumes_used >= MAX_AUTO_RESUMES {
        return paused(UsageLimitPauseReason::AttemptsExhausted);
    }
    if now < resets_at || limit.auto_resume {
        return UsageLimitStatus {
            phase: UsageLimitPhase::Waiting,
            pause_reason: None,
        };
    }
    paused(UsageLimitPauseReason::ResetPassed)
}

/// When an automatic resume is due: the reset plus `slack_secs`, clamped to
/// the slack range. `None` when this limit does not resume by itself.
pub fn auto_resume_due_at(
    limit: &SessionUsageLimit,
    slack_secs: i64,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    if !limit.auto_resume || usage_limit_phase(limit, now).phase != UsageLimitPhase::Waiting {
        return None;
    }
    let resets_at = parse_time(limit.resets_at.as_deref()?)?;
    Some(
        resets_at
            + Duration::seconds(slack_secs.clamp(RESUME_SLACK_MIN_SECS, RESUME_SLACK_MAX_SECS)),
    )
}

/// Whether a limit hit at `at` needs the person right away: no known reset,
/// or automatic resume already used up for the night.
pub fn paused_at_hit(
    events: &[SessionEventRecord],
    resets_at: Option<&str>,
    auto_resume: bool,
    at: DateTime<Utc>,
) -> bool {
    resets_at.and_then(parse_time).is_none()
        || (auto_resume && auto_resumes_before(events, at) >= MAX_AUTO_RESUMES)
}

/// The session's own choice for automatic resume, if it made one.
pub fn session_auto_resume_choice(events: &[SessionEventRecord]) -> Option<bool> {
    events.iter().rev().find_map(|event| match &event.kind {
        SessionEventKind::UsageLimitAutoResumeChanged { enabled } => Some(*enabled),
        _ => None,
    })
}

/// Automatic resumes recorded in the window that ends at `at`.
pub fn auto_resumes_before(events: &[SessionEventRecord], at: DateTime<Utc>) -> u32 {
    let since = at - Duration::hours(AUTO_RESUME_WINDOW_HOURS);
    events
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                SessionEventKind::UsageLimitResumed {
                    automatic: true,
                    ..
                }
            )
        })
        .filter_map(|event| parse_time(&event.occurred_at))
        .filter(|occurred_at| *occurred_at > since && *occurred_at <= at)
        .count() as u32
}

/// The limit that stopped the session's latest turn, while it still stands:
/// a later turn (the resume, or anything the person sends) clears it.
pub fn session_usage_limit(events: &[SessionEventRecord]) -> Option<SessionUsageLimit> {
    let visible = visible_session_events(events);
    let mut latest_turn: Option<&TurnId> = None;
    let mut ended_at: Option<&str> = None;
    let mut resets_at: Option<String> = None;
    let mut auto_resume_at_hit: Option<bool> = None;
    let mut hit_seq: Option<u64> = None;
    let mut closed = false;
    for event in &visible {
        match &event.kind {
            SessionEventKind::TurnStarted { turn_id, .. } => {
                latest_turn = Some(turn_id);
                ended_at = None;
                resets_at = None;
                auto_resume_at_hit = None;
                hit_seq = None;
                closed = false;
            }
            SessionEventKind::TurnAborted { turn_id, .. } if latest_turn == Some(turn_id) => {
                ended_at = Some(event.occurred_at.as_str());
            }
            SessionEventKind::TurnCompleted { turn_id } if latest_turn == Some(turn_id) => {
                // A turn that still completed was not stopped by the limit.
                hit_seq = None;
                auto_resume_at_hit = None;
            }
            SessionEventKind::UsageLimitReached {
                turn_id,
                resets_at: reported,
                auto_resume,
                ..
            } if latest_turn == Some(turn_id) => {
                // A later report fills a reset the first one lacked; it never
                // erases one already known.
                if reported.is_some() {
                    resets_at = reported.clone();
                }
                auto_resume_at_hit.get_or_insert(*auto_resume);
                hit_seq.get_or_insert(event.sequence);
            }
            SessionEventKind::SessionCompleted | SessionEventKind::SessionAborted { .. } => {
                closed = true;
            }
            _ => {}
        }
    }
    let turn_id = latest_turn?.clone();
    let hit_at = ended_at?.to_string();
    let hit_seq = hit_seq?;
    if closed {
        return None;
    }
    let auto_resume = visible
        .iter()
        .rev()
        .take_while(|event| event.sequence > hit_seq)
        .find_map(|event| match &event.kind {
            SessionEventKind::UsageLimitAutoResumeChanged { enabled } => Some(*enabled),
            _ => None,
        })
        .or(auto_resume_at_hit)
        .unwrap_or(false);
    let auto_resumes_used = parse_time(&hit_at)
        .map(|at| auto_resumes_before(events, at))
        .unwrap_or(0);
    Some(SessionUsageLimit {
        turn_id,
        hit_at,
        resets_at,
        auto_resume,
        auto_resumes_used,
    })
}

/// A usage limit recognized in a provider's failure text, for providers or
/// paths that did not report it structurally. Only the product sentences
/// Claude Code and Codex print for a plan limit count: a generic "rate limit"
/// or HTTP 429 elsewhere stays an ordinary error. The outer `Option` says
/// whether it is a limit; the inner one carries the reset when the text
/// encodes it as a timestamp (Claude's `…|<epoch seconds>` form).
pub fn usage_limit_from_failure_message(message: &str) -> Option<Option<String>> {
    let text = message.trim();
    let lower = text.to_lowercase();
    // Claude Code, older: "Claude AI usage limit reached|1760245200".
    if let Some((head, tail)) = text.split_once('|') {
        if head.to_lowercase().contains("usage limit reached") {
            let resets_at = tail
                .trim()
                .parse::<i64>()
                .ok()
                .map(|value| {
                    if value > 10_000_000_000 {
                        value / 1000
                    } else {
                        value
                    }
                })
                .and_then(|seconds| DateTime::<Utc>::from_timestamp(seconds, 0))
                .map(|time| time.to_rfc3339());
            return Some(resets_at);
        }
    }
    // Claude Code builds "You've hit your <window> limit · resets 3am"
    // ("session", "weekly", "Opus", or none); Codex says "You've hit your
    // usage limit. … try again at 3:40 AM.". The fast-mode limit only turns
    // fast mode off, so it is not a stop.
    let hit_your = lower.contains("you've hit your") || lower.contains("you’ve hit your");
    let plan_limit = (hit_your && lower.contains("limit") && !lower.contains("fast limit"))
        || lower.contains("claude ai usage limit reached");
    plan_limit.then_some(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::SessionId;

    fn record(sequence: u64, at: &str, kind: SessionEventKind) -> SessionEventRecord {
        SessionEventRecord {
            event_id: format!("e{sequence}"),
            session_id: SessionId("s".to_string()),
            sequence,
            occurred_at: at.to_string(),
            kind,
        }
    }

    fn started(sequence: u64, at: &str, turn: &str) -> SessionEventRecord {
        record(
            sequence,
            at,
            SessionEventKind::TurnStarted {
                turn_id: TurnId(turn.to_string()),
                prompt: "p".to_string(),
                plan_mode: None,
                model: None,
                evidence: None,
                retry_of_turn_id: None,
                approval_policy: None,
            },
        )
    }

    fn hit(
        sequence: u64,
        at: &str,
        turn: &str,
        resets: Option<&str>,
        auto: bool,
    ) -> SessionEventRecord {
        record(
            sequence,
            at,
            SessionEventKind::UsageLimitReached {
                turn_id: TurnId(turn.to_string()),
                resets_at: resets.map(str::to_string),
                auto_resume: auto,
                paused: false,
            },
        )
    }

    fn aborted(sequence: u64, at: &str, turn: &str) -> SessionEventRecord {
        record(
            sequence,
            at,
            SessionEventKind::TurnAborted {
                turn_id: TurnId(turn.to_string()),
                reason: Some("limit".to_string()),
            },
        )
    }

    fn resumed(sequence: u64, at: &str, automatic: bool) -> SessionEventRecord {
        record(
            sequence,
            at,
            SessionEventKind::UsageLimitResumed {
                automatic,
                turn_id: None,
                account_label: None,
            },
        )
    }

    fn time(value: &str) -> DateTime<Utc> {
        parse_time(value).unwrap()
    }

    #[test]
    fn continuation_prompt_is_fixed_text() {
        assert_eq!(continuation_prompt(None), "Continue de onde parou.");
        assert_eq!(continuation_prompt(Some("  ")), "Continue de onde parou.");
        assert_eq!(
            continuation_prompt(Some("Migrar o banco")),
            "Continue de onde parou. A última etapa concluída foi: Migrar o banco."
        );
    }

    #[test]
    fn a_limited_turn_waits_until_its_reset() {
        let events = vec![
            started(1, "2026-10-12T01:00:00Z", "t1"),
            hit(
                2,
                "2026-10-12T01:30:00Z",
                "t1",
                Some("2026-10-12T03:40:00Z"),
                false,
            ),
            aborted(3, "2026-10-12T01:30:01Z", "t1"),
        ];
        let limit = session_usage_limit(&events).unwrap();
        assert_eq!(limit.hit_at, "2026-10-12T01:30:01Z");
        assert_eq!(limit.resets_at.as_deref(), Some("2026-10-12T03:40:00Z"));
        let before = usage_limit_phase(&limit, time("2026-10-12T02:00:00Z"));
        assert_eq!(before.phase, UsageLimitPhase::Waiting);
        // Without automatic resume, the reopened window is the person's call.
        let after = usage_limit_phase(&limit, time("2026-10-12T03:41:00Z"));
        assert_eq!(after.pause_reason, Some(UsageLimitPauseReason::ResetPassed));
    }

    #[test]
    fn an_unknown_reset_pauses_for_the_person() {
        let events = vec![
            started(1, "2026-10-12T01:00:00Z", "t1"),
            hit(2, "2026-10-12T01:30:00Z", "t1", None, true),
            aborted(3, "2026-10-12T01:30:01Z", "t1"),
        ];
        let limit = session_usage_limit(&events).unwrap();
        let status = usage_limit_phase(&limit, time("2026-10-12T01:31:00Z"));
        assert_eq!(
            status.pause_reason,
            Some(UsageLimitPauseReason::UnknownReset)
        );
        assert_eq!(
            auto_resume_due_at(&limit, 90, time("2026-10-12T01:31:00Z")),
            None
        );
    }

    #[test]
    fn a_late_reset_fills_the_unknown_one_and_never_erases_it() {
        let events = vec![
            started(1, "2026-10-12T01:00:00Z", "t1"),
            hit(2, "2026-10-12T01:30:00Z", "t1", None, true),
            aborted(3, "2026-10-12T01:30:01Z", "t1"),
            hit(
                4,
                "2026-10-12T01:30:02Z",
                "t1",
                Some("2026-10-12T03:40:00Z"),
                true,
            ),
            hit(5, "2026-10-12T01:30:03Z", "t1", None, true),
        ];
        let limit = session_usage_limit(&events).unwrap();
        assert_eq!(limit.resets_at.as_deref(), Some("2026-10-12T03:40:00Z"));
        assert!(limit.auto_resume);
    }

    #[test]
    fn automatic_resume_is_due_after_the_reset_plus_slack() {
        let events = vec![
            started(1, "2026-10-12T01:00:00Z", "t1"),
            hit(
                2,
                "2026-10-12T01:30:00Z",
                "t1",
                Some("2026-10-12T03:40:00Z"),
                true,
            ),
            aborted(3, "2026-10-12T01:30:01Z", "t1"),
        ];
        let limit = session_usage_limit(&events).unwrap();
        let now = time("2026-10-12T02:00:00Z");
        assert_eq!(
            auto_resume_due_at(&limit, 90, now),
            Some(time("2026-10-12T03:41:30Z"))
        );
        // The slack is always 1–3 minutes.
        assert_eq!(
            auto_resume_due_at(&limit, 5, now),
            Some(time("2026-10-12T03:41:00Z"))
        );
        assert_eq!(
            auto_resume_due_at(&limit, 999, now),
            Some(time("2026-10-12T03:43:00Z"))
        );
        // Past the reset (an app restart), it is still waiting and due.
        let later = time("2026-10-12T09:00:00Z");
        assert_eq!(
            usage_limit_phase(&limit, later).phase,
            UsageLimitPhase::Waiting
        );
        assert!(auto_resume_due_at(&limit, 90, later).unwrap() <= later);
    }

    #[test]
    fn three_automatic_resumes_a_night_then_it_waits_for_the_person() {
        let mut events = vec![started(1, "2026-10-11T20:00:00Z", "t0")];
        let mut seq = 2;
        for (index, hour) in ["21", "22", "23"].iter().enumerate() {
            let turn = format!("t{index}");
            events.push(hit(
                seq,
                &format!("2026-10-11T{hour}:00:00Z"),
                &turn,
                Some("2026-10-12T00:00:00Z"),
                true,
            ));
            events.push(aborted(
                seq + 1,
                &format!("2026-10-11T{hour}:00:01Z"),
                &turn,
            ));
            events.push(resumed(seq + 2, &format!("2026-10-11T{hour}:30:00Z"), true));
            events.push(started(
                seq + 3,
                &format!("2026-10-11T{hour}:30:01Z"),
                &format!("t{}", index + 1),
            ));
            seq += 4;
        }
        events.push(hit(
            seq,
            "2026-10-12T00:10:00Z",
            "t3",
            Some("2026-10-12T05:00:00Z"),
            true,
        ));
        events.push(aborted(seq + 1, "2026-10-12T00:10:01Z", "t3"));
        let limit = session_usage_limit(&events).unwrap();
        assert_eq!(limit.auto_resumes_used, 3);
        let status = usage_limit_phase(&limit, time("2026-10-12T00:11:00Z"));
        assert_eq!(
            status.pause_reason,
            Some(UsageLimitPauseReason::AttemptsExhausted)
        );
        // Manual resumes do not count, and the window is 12 hours.
        events.push(resumed(seq + 2, "2026-10-12T00:20:00Z", false));
        assert_eq!(
            auto_resumes_before(&events, time("2026-10-12T10:00:00Z")),
            2
        );
    }

    #[test]
    fn a_later_turn_or_a_completion_clears_the_limit() {
        let mut events = vec![
            started(1, "2026-10-12T01:00:00Z", "t1"),
            hit(
                2,
                "2026-10-12T01:30:00Z",
                "t1",
                Some("2026-10-12T03:40:00Z"),
                true,
            ),
            aborted(3, "2026-10-12T01:30:01Z", "t1"),
        ];
        assert!(session_usage_limit(&events).is_some());
        events.push(started(4, "2026-10-12T03:42:00Z", "t2"));
        assert!(session_usage_limit(&events).is_none());

        // A limit reported on a turn that then completed anyway is no stop.
        let completed = vec![
            started(1, "2026-10-12T01:00:00Z", "t1"),
            hit(2, "2026-10-12T01:30:00Z", "t1", None, false),
            record(
                3,
                "2026-10-12T01:31:00Z",
                SessionEventKind::TurnCompleted {
                    turn_id: TurnId("t1".into()),
                },
            ),
        ];
        assert!(session_usage_limit(&completed).is_none());
    }

    #[test]
    fn the_choice_made_after_the_hit_wins() {
        let events = vec![
            started(1, "2026-10-12T01:00:00Z", "t1"),
            hit(
                2,
                "2026-10-12T01:30:00Z",
                "t1",
                Some("2026-10-12T03:40:00Z"),
                false,
            ),
            aborted(3, "2026-10-12T01:30:01Z", "t1"),
            record(
                4,
                "2026-10-12T01:35:00Z",
                SessionEventKind::UsageLimitAutoResumeChanged { enabled: true },
            ),
        ];
        let limit = session_usage_limit(&events).unwrap();
        assert!(limit.auto_resume);
        assert_eq!(session_auto_resume_choice(&events), Some(true));
    }

    #[test]
    fn failure_text_recognizes_only_plan_limit_sentences() {
        assert_eq!(
            usage_limit_from_failure_message("Claude AI usage limit reached|1760245200"),
            Some(Some("2025-10-12T05:00:00+00:00".to_string()))
        );
        assert_eq!(
            usage_limit_from_failure_message(
                "You've hit your limit · resets 3am (America/Sao_Paulo)"
            ),
            Some(None)
        );
        assert_eq!(
            usage_limit_from_failure_message(
                "You've hit your usage limit. Upgrade to Pro (https://chatgpt.com/explore/pro), or try again at 3:40 AM."
            ),
            Some(None)
        );
        assert_eq!(
            usage_limit_from_failure_message("You've hit your weekly limit · resets Oct 14, 9am"),
            Some(None)
        );
        assert_eq!(
            usage_limit_from_failure_message("You've hit your fast limit"),
            None
        );
        assert_eq!(
            usage_limit_from_failure_message("API Error: 429 rate_limit_error"),
            None
        );
        assert_eq!(
            usage_limit_from_failure_message("codex exited with status 1"),
            None
        );
        assert_eq!(
            usage_limit_from_failure_message("Quota exceeded for quota metric"),
            None
        );
    }
}
