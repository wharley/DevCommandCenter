//! Per-provider wire prompts (Helmor session semantics + t3code-style routing key),
//! for stdin-only `CliProviderAdapter` binaries.

fn normalized_effort(effort: Option<&str>) -> &'static str {
	match effort.unwrap_or("balanced") {
		"low" => "low",
		"high" => "high",
		_ => "balanced",
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
		"low" => "Effort low: keep reasoning concise; avoid unnecessary tool calls.",
		"high" => {
			"Effort high: reason thoroughly, verify assumptions, and double-check critical steps."
		}
		_ => "Effort balanced: trade depth vs speed sensibly for the task.",
	}
}

fn fast_lines(fast: bool) -> &'static str {
	if fast {
		"Fast style: prefer concise assistant replies unless the user needs detail."
	} else {
		"Standard verbosity: explain enough for the user to follow along when useful."
	}
}

/// Wire text for **Claude Code** — Helmor-style plan / permission framing for agentic CLI.
fn wire_claude_code(
	body: &str,
	plan: bool,
	effort: &str,
	fast: bool,
) -> String {
	let mut lines = vec![
		"[DCC · Claude Code — composer directives]".to_string(),
		"Runtime: Anthropic Claude agent CLI semantics (tools + workspace).".to_string(),
	];
	if plan {
		lines.push(
			"PLAN / permission-style ON: respond with a structured plan first (sections, ordered steps, risks). Do NOT write files, run shell, or apply patches until the user clearly asks to execute or exit planning — treat this like Helmor `permissionMode: plan`.".to_string(),
		);
	} else {
		lines.push(
			"EXECUTION ON: you may invoke tools, edit files, and run commands as appropriate for the repo.".to_string(),
		);
	}
	lines.push(effort_lines(effort).to_string());
	lines.push(fast_lines(fast).to_string());
	lines.push("[End DCC · Claude Code]".to_string());
	lines.push(String::new());
	lines.push(body.to_string());
	lines.join("\n")
}

/// Wire text for **Codex** — OpenAI Codex CLI style (repo tools, patch discipline).
fn wire_codex(body: &str, plan: bool, effort: &str, fast: bool) -> String {
	let mut lines = vec![
		"[DCC · OpenAI Codex — composer directives]".to_string(),
		"Runtime: Codex CLI — prefer repo-grounded edits and minimal destructive commands.".to_string(),
	];
	if plan {
		lines.push(
			"PLAN ON: outline steps and affected paths before changing anything; defer applying code edits or running terminal actions until the user confirms.".to_string(),
		);
	} else {
		lines.push(
			"EXECUTION ON: implement using Codex tools following repository conventions.".to_string(),
		);
	}
	lines.push(effort_lines(effort).to_string());
	lines.push(fast_lines(fast).to_string());
	lines.push("[End DCC · Codex]".to_string());
	lines.push(String::new());
	lines.push(body.to_string());
	lines.join("\n")
}

/// Wire text for **Gemini** CLI — long-context agent defaults.
fn wire_gemini(body: &str, plan: bool, effort: &str, fast: bool) -> String {
	let mut lines = vec![
		"[DCC · Google Gemini CLI — composer directives]".to_string(),
		"Runtime: Gemini coding agent — leverage context efficiently.".to_string(),
	];
	if plan {
		lines.push(
			"PLAN ON: produce a clear plan with milestones before modifying artifacts.".to_string(),
		);
	} else {
		lines.push(
			"EXECUTION ON: proceed with file and tool actions suitable for the workspace.".to_string(),
		);
	}
	lines.push(effort_lines(effort).to_string());
	lines.push(fast_lines(fast).to_string());
	lines.push("[End DCC · Gemini]".to_string());
	lines.push(String::new());
	lines.push(body.to_string());
	lines.join("\n")
}

/// Wire text for **Cursor** experimental adapter — shorter envelope; same knobs.
fn wire_cursor(body: &str, plan: bool, effort: &str, fast: bool) -> String {
	let mut lines = vec![
		"[DCC · Cursor adapter — composer directives]".to_string(),
		"Runtime: experimental Cursor agent bridge — prefer safe, incremental edits.".to_string(),
	];
	if plan {
		lines.push(
			"PLAN ON: describe intent and steps before automated edits or commands.".to_string(),
		);
	} else {
		lines.push("EXECUTION ON: act within Cursor agent capabilities.".to_string());
	}
	lines.push(effort_lines(effort).to_string());
	lines.push(fast_lines(fast).to_string());
	lines.push("[End DCC · Cursor]".to_string());
	lines.push(String::new());
	lines.push(body.to_string());
	lines.join("\n")
}

/// Fallback if an unknown provider id appears in session state.
fn wire_generic(body: &str, plan: bool, effort: &str, fast: bool) -> String {
	let mut lines = vec!["[DCC composer directives — generic provider]".to_string()];
	if plan {
		lines.push(
			"Plan mode ON: plan first; avoid destructive actions until the user confirms execution."
				.to_string(),
		);
	} else {
		lines.push(
			"Plan mode OFF: use tools and edits as appropriate.".to_string(),
		);
	}
	lines.push(effort_lines(effort).to_string());
	lines.push(fast_lines(fast).to_string());
	lines.push("[End DCC composer directives]".to_string());
	lines.push(String::new());
	lines.push(body.to_string());
	lines.join("\n")
}

/// Full stdin payload for the active provider (`claude_code`, `codex`, `gemini`, `cursor`).
pub fn compose_wire_prompt_for_provider(
	provider_id: &str,
	user_prompt: &str,
	plan_mode: Option<bool>,
	effort: Option<&str>,
	fast_mode: Option<bool>,
) -> String {
	let body = user_prompt.trim();
	let plan = normalized_plan(plan_mode);
	let e = normalized_effort(effort);
	let fast = normalized_fast(fast_mode);

	match provider_id {
		"claude_code" => wire_claude_code(body, plan, e, fast),
		"codex" => wire_codex(body, plan, e, fast),
		"gemini" => wire_gemini(body, plan, e, fast),
		"cursor" => wire_cursor(body, plan, e, fast),
		_ => wire_generic(body, plan, e, fast),
	}
}

/// Back-compat: previously single template; now routes as **claude_code** (stable default).
pub fn compose_wire_prompt(
	user_prompt: &str,
	plan_mode: Option<bool>,
	effort: Option<&str>,
	fast_mode: Option<bool>,
) -> String {
	compose_wire_prompt_for_provider(
		"claude_code",
		user_prompt,
		plan_mode,
		effort,
		fast_mode,
	)
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
			Some("balanced"),
			Some(false),
		);
		assert!(out.contains("Claude Code"));
		assert!(out.contains("PLAN / permission-style ON"));
		assert!(out.ends_with("Hi"));
	}

	#[test]
	fn codex_exec_line() {
		let out = compose_wire_prompt_for_provider(
			"codex",
			"x",
			Some(false),
			Some("low"),
			Some(true),
		);
		assert!(out.contains("OpenAI Codex"));
		assert!(out.contains("EXECUTION ON"));
	}

	#[test]
	fn unknown_provider_falls_back() {
		let out =
			compose_wire_prompt_for_provider("unknown", "z", None, None, None);
		assert!(out.contains("generic"));
		assert!(out.ends_with("z"));
	}
}
