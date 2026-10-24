//! How the composer's goal strip reads the provider's goal
//! (`goal-strip.logic.ts`): which title, what usage, which action, and the
//! turns that resume or drop it.

use dcc_core::domain::goal::{GoalStatus, SessionGoal};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoalTone {
    Active,
    Attention,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoalAction {
    Clear,
    ResumeAndClear,
    Dismiss,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalPresentation {
    pub title_key: &'static str,
    pub tone: GoalTone,
    pub checks: u32,
    pub tokens_used: Option<u64>,
    pub token_budget: Option<u64>,
    pub minutes: Option<u64>,
    /// Why the latest check said it is not done; never on a finished goal.
    pub last_check: Option<String>,
    pub action: GoalAction,
}

/// `presentGoal`
pub fn present(goal: &SessionGoal, working: bool) -> GoalPresentation {
    let tokens_used = goal.tokens_used.filter(|tokens| *tokens > 0);
    let minutes = goal
        .time_used_seconds
        .filter(|seconds| *seconds >= 60)
        .map(|seconds| (seconds as f64 / 60.0).round() as u64);
    let last_check = goal
        .last_check
        .as_deref()
        .map(str::trim)
        .filter(|check| !check.is_empty())
        .map(str::to_string);
    let (title_key, tone, last_check, action) = match goal.status {
        GoalStatus::Complete => (
            "goal.titleComplete",
            GoalTone::Done,
            None,
            GoalAction::Dismiss,
        ),
        GoalStatus::Active => (
            if working {
                "goal.titleWorking"
            } else {
                "goal.titleActive"
            },
            GoalTone::Active,
            last_check,
            GoalAction::Clear,
        ),
        GoalStatus::Paused => (
            "goal.titlePaused",
            GoalTone::Attention,
            last_check,
            GoalAction::ResumeAndClear,
        ),
        GoalStatus::Blocked => (
            "goal.titleBlocked",
            GoalTone::Attention,
            last_check,
            GoalAction::ResumeAndClear,
        ),
        GoalStatus::UsageLimited => (
            "goal.titleUsageLimited",
            GoalTone::Attention,
            last_check,
            GoalAction::ResumeAndClear,
        ),
        GoalStatus::BudgetLimited => (
            "goal.titleBudgetLimited",
            GoalTone::Attention,
            last_check,
            GoalAction::ResumeAndClear,
        ),
    };
    GoalPresentation {
        title_key,
        tone,
        checks: goal.checks,
        tokens_used,
        token_budget: goal.token_budget,
        minutes,
        last_check,
        action,
    }
}

/// `formatGoalTokens`: 950, 12k, 1.2M.
pub fn format_tokens(tokens: u64) -> String {
    let short = |value: f64, unit: &str| {
        if value < 10.0 {
            let text = format!("{value:.1}");
            format!("{}{unit}", text.strip_suffix(".0").unwrap_or(&text))
        } else {
            format!("{}{unit}", value.round() as u64)
        }
    };
    if tokens < 1_000 {
        tokens.to_string()
    } else if tokens < 1_000_000 {
        short(tokens as f64 / 1_000.0, "k")
    } else {
        short(tokens as f64 / 1_000_000.0, "M")
    }
}

/// `goalResumePrompt`: Codex and Grok resume natively; any message continues Claude's.
pub fn resume_prompt(provider: Option<&str>) -> &'static str {
    match provider {
        Some("codex" | "grok") => "/goal resume",
        _ => dcc_core::domain::usage_limit::CONTINUATION_PROMPT,
    }
}

/// `goalClearPrompt`: Cursor has no `/goal clear`; its agent ends a goal when asked.
pub fn clear_prompt(provider: Option<&str>) -> &'static str {
    match provider {
        Some("cursor") => "Encerre o objetivo agora: marque-o como concluído sem fazer mais nada.",
        _ => "/goal clear",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal(status: GoalStatus) -> SessionGoal {
        serde_json::from_value(serde_json::json!({
            "objective": "Fazer X", "status": serde_json::to_value(status).unwrap(), "checks": 2,
            "lastCheck": " falta Y ", "tokensUsed": 12_400, "tokenBudget": 50_000, "timeUsedSeconds": 150,
            "completesWithTurn": false, "updatedAt": "2026-10-24T10:00:00Z"
        }))
        .unwrap()
    }

    #[test]
    fn reads_each_status() {
        let active = present(&goal(GoalStatus::Active), true);
        assert_eq!(
            (active.title_key, active.tone, active.action),
            ("goal.titleWorking", GoalTone::Active, GoalAction::Clear)
        );
        assert_eq!(active.last_check.as_deref(), Some("falta Y"));
        assert_eq!(active.minutes, Some(3));
        let done = present(&goal(GoalStatus::Complete), false);
        assert_eq!(
            (done.tone, done.action, done.last_check),
            (GoalTone::Done, GoalAction::Dismiss, None)
        );
        let blocked = present(&goal(GoalStatus::Blocked), false);
        assert_eq!(blocked.action, GoalAction::ResumeAndClear);
        assert_eq!(format_tokens(950), "950");
        assert_eq!(format_tokens(12_400), "12k");
        assert_eq!(format_tokens(1_200_000), "1.2M");
        assert_eq!(format_tokens(3_000), "3k");
        assert_eq!(resume_prompt(Some("codex")), "/goal resume");
        assert_eq!(
            resume_prompt(Some("claude_code")),
            "Continue de onde parou."
        );
        assert!(clear_prompt(Some("cursor")).starts_with("Encerre"));
    }
}
