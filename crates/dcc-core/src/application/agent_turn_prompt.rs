//! Composer directives cloned into the wire prompt for stdin-only CLI agents
//! (Claude Code, Codex, Gemini; Cursor uses the same envelope).

/// Builds the full prompt written to the provider stdin (Tauri `send_provider_input`).
/// `user_prompt` is the raw composer text (what we store on the Turn / timeline).
pub fn compose_wire_prompt(
	user_prompt: &str,
	plan_mode: Option<bool>,
	effort: Option<&str>,
	fast_mode: Option<bool>,
) -> String {
	let body = user_prompt.trim();
	let plan_mode = plan_mode.unwrap_or(false);
	let effort = effort.unwrap_or("balanced");
	let fast_mode = fast_mode.unwrap_or(true);

	let mut lines: Vec<String> = Vec::new();
	lines.push("[DCC composer directives — follow before answering]".to_string());

	if plan_mode {
		lines.push(
			"Plan mode is ON: produce a clear step-by-step plan first. Do not modify files, run destructive commands, or apply edits until the user explicitly asks you to execute or leave planning.".to_string(),
		);
	} else {
		lines.push(
			"Plan mode is OFF: you may use tools and edit files as appropriate to satisfy the request.".to_string(),
		);
	}

	match effort {
		"low" => lines.push(
			"Effort low: keep reasoning concise; avoid unnecessary tool calls.".to_string(),
		),
		"high" => lines.push(
			"Effort high: reason thoroughly, verify assumptions, and double-check critical steps."
				.to_string(),
		),
		_ => lines.push(
			"Effort balanced: trade depth vs speed sensibly for the task.".to_string(),
		),
	}

	if fast_mode {
		lines.push(
			"Fast style: prefer concise assistant replies unless the user needs detail.".to_string(),
		);
	} else {
		lines.push(
			"Standard verbosity: explain enough for the user to follow along when useful.".to_string(),
		);
	}

	lines.push("[End DCC composer directives]".to_string());
	lines.push(String::new());
	lines.push(body.to_string());
	lines.join("\n")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn wire_includes_plan_when_enabled() {
		let out = compose_wire_prompt("Hi", Some(true), Some("balanced"), Some(false));
		assert!(out.contains("Plan mode is ON"));
		assert!(out.ends_with("Hi"));
	}

	#[test]
	fn defaults_balanced_and_fast() {
		let out = compose_wire_prompt("x", None, None, None);
		assert!(out.contains("Effort balanced"));
		assert!(out.contains("Fast style"));
	}
}
