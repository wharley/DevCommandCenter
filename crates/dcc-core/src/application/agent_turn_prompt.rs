//! Per-provider prompt fallbacks for adapters that still need behavioral text.

fn normalized_effort(effort: Option<&str>) -> &'static str {
    match effort.unwrap_or("medium") {
        "minimal" => "minimal",
        "low" => "low",
        "balanced" | "medium" => "medium",
        "high" => "high",
        "xhigh" => "xhigh",
        "max" | "ultrathink" => "max",
        _ => "medium",
    }
}

fn normalized_fast(fast_mode: Option<bool>) -> bool {
    fast_mode.unwrap_or(true)
}

fn normalized_plan(plan_mode: Option<bool>) -> bool {
    plan_mode.unwrap_or(false)
}

fn effort_lines(effort: &str) -> &'static str {
    match effort {
        "minimal" => "Effort minimal: take the most direct path and avoid deep analysis unless needed.",
        "low" => "Effort low: keep reasoning concise; avoid unnecessary tool calls.",
        "medium" => "Effort medium: balance depth vs speed sensibly for the task.",
        "high" => {
            "Effort high: reason thoroughly, verify assumptions, and double-check critical steps."
        }
        "xhigh" => {
            "Effort extra high: reason very thoroughly, inspect edge cases, and verify key assumptions before acting."
        }
        "max" => {
            "Effort max: use maximal care on complex reasoning, prefer correctness over speed, and validate every critical step."
        }
        _ => "Effort medium: balance depth vs speed sensibly for the task.",
    }
}

fn fast_lines(fast: bool) -> &'static str {
    if fast {
        "Fast style: prefer concise assistant replies unless the user needs detail."
    } else {
        "Standard verbosity: explain enough for the user to follow along when useful."
    }
}

fn maybe_push_plan_line(lines: &mut Vec<String>, plan: Option<bool>, on: &str, off: &str) {
    if let Some(plan) = plan {
        lines.push(if plan { on } else { off }.to_string());
    }
}

fn maybe_push_effort_line(lines: &mut Vec<String>, effort: Option<&str>) {
    if let Some(effort) = effort {
        lines.push(effort_lines(effort).to_string());
    }
}

fn maybe_push_fast_line(lines: &mut Vec<String>, fast: Option<bool>) {
    if let Some(fast) = fast {
        lines.push(fast_lines(fast).to_string());
    }
}

fn wire_claude_code_partial(
    body: &str,
    plan: Option<bool>,
    effort: Option<&str>,
    fast: Option<bool>,
) -> String {
    let mut lines = vec![
        "[DCC · Claude Code — composer directives]".to_string(),
        "Runtime: Anthropic Claude agent CLI semantics (tools + workspace).".to_string(),
    ];
    maybe_push_plan_line(
        &mut lines,
        plan,
        "PLAN / permission-style ON: inspect the repository with read-only tools first (list, search, and read files), then return a complete structured plan grounded in real paths. Do NOT write files, apply patches, or run mutating commands. If essential information is missing, ask a specific blocking question and say the plan is not ready for approval.",
        "EXECUTION ON: you may invoke tools, edit files, and run commands as appropriate for the repo.",
    );
    maybe_push_effort_line(&mut lines, effort);
    maybe_push_fast_line(&mut lines, fast);
    lines.push("[End DCC · Claude Code]".to_string());
    lines.push(String::new());
    lines.push(body.to_string());
    lines.join("\n")
}

fn wire_codex_partial(
    body: &str,
    plan: Option<bool>,
    effort: Option<&str>,
    fast: Option<bool>,
) -> String {
    let mut lines = vec![
        "[DCC · OpenAI Codex — composer directives]".to_string(),
        "Runtime: Codex CLI — prefer repo-grounded edits and minimal destructive commands."
            .to_string(),
    ];
    maybe_push_plan_line(
        &mut lines,
        plan,
        "PLAN ON: inspect the repository with read-only tools first (list, search, and read files), then return a complete structured plan grounded in real paths. Do not edit files or run mutating commands. If essential information is missing, ask a specific blocking question and say the plan is not ready for approval.",
        "EXECUTION ON: implement using Codex tools following repository conventions.",
    );
    maybe_push_effort_line(&mut lines, effort);
    maybe_push_fast_line(&mut lines, fast);
    lines.push("[End DCC · Codex]".to_string());
    lines.push(String::new());
    lines.push(body.to_string());
    lines.join("\n")
}

fn wire_gemini_partial(
    body: &str,
    plan: Option<bool>,
    effort: Option<&str>,
    fast: Option<bool>,
) -> String {
    let mut lines = vec![
        "[DCC · Google Gemini CLI — composer directives]".to_string(),
        "Runtime: Gemini coding agent — leverage context efficiently.".to_string(),
    ];
    maybe_push_plan_line(
        &mut lines,
        plan,
        "PLAN ON: inspect the repository with read-only tools first, then return a complete plan with milestones and real affected paths. Do not modify artifacts or run mutating commands. If blocked, ask a specific question and say the plan is not ready for approval.",
        "EXECUTION ON: proceed with file and tool actions suitable for the workspace.",
    );
    maybe_push_effort_line(&mut lines, effort);
    maybe_push_fast_line(&mut lines, fast);
    lines.push("[End DCC · Gemini]".to_string());
    lines.push(String::new());
    lines.push(body.to_string());
    lines.join("\n")
}

fn wire_cursor_partial(
    body: &str,
    plan: Option<bool>,
    effort: Option<&str>,
    fast: Option<bool>,
) -> String {
    let mut lines = vec![
        "[DCC · Cursor adapter — composer directives]".to_string(),
        "Runtime: experimental Cursor agent bridge — prefer safe, incremental edits.".to_string(),
    ];
    maybe_push_plan_line(
        &mut lines,
        plan,
        "PLAN ON: inspect the repository with read-only tools first, then return a complete plan grounded in real affected paths. Do not edit files or run mutating commands. If blocked, ask a specific question and say the plan is not ready for approval.",
        "EXECUTION ON: act within Cursor agent capabilities.",
    );
    maybe_push_effort_line(&mut lines, effort);
    maybe_push_fast_line(&mut lines, fast);
    lines.push("[End DCC · Cursor]".to_string());
    lines.push(String::new());
    lines.push(body.to_string());
    lines.join("\n")
}

fn wire_grok_partial(
    body: &str,
    plan: Option<bool>,
    effort: Option<&str>,
    fast: Option<bool>,
) -> String {
    let mut lines = vec![
        "[DCC · Grok Build — composer directives]".to_string(),
        "Runtime: Grok Build ACP agent — use workspace tools deliberately and keep changes reviewable."
            .to_string(),
    ];
    maybe_push_plan_line(
        &mut lines,
        plan,
        "PLAN ON: inspect the repository with read-only tools first, then return a complete concrete plan grounded in real affected paths. Do not edit files or run mutating commands. If blocked, ask a specific question and say the plan is not ready for approval.",
        "EXECUTION ON: use Grok Build tools to implement and verify the requested work.",
    );
    maybe_push_effort_line(&mut lines, effort);
    maybe_push_fast_line(&mut lines, fast);
    lines.push("[End DCC · Grok Build]".to_string());
    lines.push(String::new());
    lines.push(body.to_string());
    lines.join("\n")
}

fn wire_generic_partial(
    body: &str,
    plan: Option<bool>,
    effort: Option<&str>,
    fast: Option<bool>,
) -> String {
    let mut lines = vec!["[DCC composer directives — generic provider]".to_string()];
    maybe_push_plan_line(
        &mut lines,
        plan,
        "Plan mode ON: inspect the repository with read-only tools first, then return a complete structured plan grounded in real affected paths. Do not edit files or run mutating commands. If blocked, ask a specific question and say the plan is not ready for approval.",
        "Plan mode OFF: use tools and edits as appropriate.",
    );
    maybe_push_effort_line(&mut lines, effort);
    maybe_push_fast_line(&mut lines, fast);
    lines.push("[End DCC composer directives]".to_string());
    lines.push(String::new());
    lines.push(body.to_string());
    lines.join("\n")
}

fn compose_partial_prompt_for_provider(
    provider_id: &str,
    body: &str,
    plan: Option<bool>,
    effort: Option<&str>,
    fast: Option<bool>,
) -> String {
    match provider_id {
        "claude_code" => wire_claude_code_partial(body, plan, effort, fast),
        "codex" => wire_codex_partial(body, plan, effort, fast),
        "gemini" => wire_gemini_partial(body, plan, effort, fast),
        "cursor" => wire_cursor_partial(body, plan, effort, fast),
        "grok" => wire_grok_partial(body, plan, effort, fast),
        _ => wire_generic_partial(body, plan, effort, fast),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PromptInjectionOptions {
    pub plan: bool,
    pub effort: bool,
    pub fast: bool,
}

/// Full prompt fallback for providers that only accept a plain text turn payload.
pub fn compose_wire_prompt_for_provider(
    provider_id: &str,
    user_prompt: &str,
    plan_mode: Option<bool>,
    effort: Option<&str>,
    fast_mode: Option<bool>,
) -> String {
    compose_partial_prompt_for_provider(
        provider_id,
        user_prompt.trim(),
        Some(normalized_plan(plan_mode)),
        Some(normalized_effort(effort)),
        Some(normalized_fast(fast_mode)),
    )
}

/// Prompt fallback when effort already travels as a native provider parameter.
pub fn compose_behavior_prompt_for_provider(
    provider_id: &str,
    user_prompt: &str,
    plan_mode: Option<bool>,
    fast_mode: Option<bool>,
) -> String {
    compose_partial_prompt_for_provider(
        provider_id,
        user_prompt.trim(),
        Some(normalized_plan(plan_mode)),
        None,
        Some(normalized_fast(fast_mode)),
    )
}

/// Prompt fallback for adapters that only need a subset of behavior text.
pub fn compose_fallback_prompt_for_provider(
    provider_id: &str,
    user_prompt: &str,
    plan_mode: Option<bool>,
    effort: Option<&str>,
    fast_mode: Option<bool>,
    options: PromptInjectionOptions,
) -> String {
    compose_partial_prompt_for_provider(
        provider_id,
        user_prompt.trim(),
        options.plan.then(|| normalized_plan(plan_mode)),
        options.effort.then(|| normalized_effort(effort)),
        options.fast.then(|| normalized_fast(fast_mode)),
    )
}

/// Back-compat: previously single template; now routes as **claude_code** (stable default).
pub fn compose_wire_prompt(
    user_prompt: &str,
    plan_mode: Option<bool>,
    effort: Option<&str>,
    fast_mode: Option<bool>,
) -> String {
    compose_wire_prompt_for_provider("claude_code", user_prompt, plan_mode, effort, fast_mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_plan_distinct() {
        let out = compose_wire_prompt_for_provider(
            "claude_code",
            "Hi",
            Some(true),
            Some("medium"),
            Some(false),
        );
        assert!(out.contains("Claude Code"));
        assert!(out.contains("PLAN / permission-style ON"));
        assert!(out.contains("read-only tools first"));
        assert!(out.ends_with("Hi"));
    }

    #[test]
    fn codex_exec_line() {
        let out =
            compose_wire_prompt_for_provider("codex", "x", Some(false), Some("low"), Some(true));
        assert!(out.contains("OpenAI Codex"));
        assert!(out.contains("EXECUTION ON"));
    }

    #[test]
    fn unknown_provider_falls_back() {
        let out = compose_wire_prompt_for_provider("unknown", "z", None, None, None);
        assert!(out.contains("generic"));
        assert!(out.ends_with("z"));
    }

    #[test]
    fn grok_prompt_includes_plan_and_effort_directives() {
        let out = compose_wire_prompt_for_provider(
            "grok",
            "inspect the repo",
            Some(true),
            Some("high"),
            Some(true),
        );
        assert!(out.contains("Grok Build"));
        assert!(out.contains("PLAN ON"));
        assert!(out.contains("Effort high"));
    }

    #[test]
    fn normalizes_balanced_to_medium() {
        let out = compose_wire_prompt_for_provider(
            "codex",
            "x",
            Some(false),
            Some("balanced"),
            Some(true),
        );
        assert!(out.contains("Effort medium"));
    }

    #[test]
    fn supports_max_effort_prompt_lines() {
        let out =
            compose_wire_prompt_for_provider("codex", "x", Some(false), Some("max"), Some(true));
        assert!(out.contains("Effort max"));
    }

    #[test]
    fn behavior_prompt_omits_effort_lines() {
        let out = compose_behavior_prompt_for_provider("codex", "x", Some(true), Some(true));
        assert!(out.contains("PLAN ON"));
        assert!(out.contains("Fast style"));
        assert!(!out.contains("Effort "));
    }

    #[test]
    fn fallback_prompt_can_omit_plan_lines() {
        let out = compose_fallback_prompt_for_provider(
            "gemini",
            "x",
            Some(true),
            Some("high"),
            Some(true),
            PromptInjectionOptions {
                plan: false,
                effort: true,
                fast: true,
            },
        );
        assert!(!out.contains("PLAN ON"));
        assert!(!out.contains("EXECUTION ON"));
        assert!(out.contains("Effort high"));
        assert!(out.contains("Fast style"));
    }

    #[test]
    fn fallback_prompt_can_omit_fast_lines() {
        let out = compose_fallback_prompt_for_provider(
            "codex",
            "x",
            Some(true),
            Some("high"),
            Some(true),
            PromptInjectionOptions {
                plan: true,
                effort: false,
                fast: false,
            },
        );
        assert!(out.contains("PLAN ON"));
        assert!(!out.contains("Effort "));
        assert!(!out.contains("Fast style"));
    }
}
