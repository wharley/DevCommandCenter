use super::*;

fn input<'a>(result: &'a str, language: &'a str) -> AgentResolutionInput<'a> {
    AgentResolutionInput {
        path: "src/example.ts",
        kind: WorkspaceGitConflictKind::BothModified,
        current_ref: "feature",
        incoming_ref: "main",
        response_language: language,
        base_text: Some("const value = 0;"),
        current_text: Some("const value = 1;"),
        incoming_text: Some("const value = 2;"),
        result_text: result,
    }
}

#[test]
fn the_request_fences_every_side_as_untrusted_and_asks_for_each_block() {
    let prompt =
        build_agent_resolution_prompt(&input("<<<<<<< HEAD", "Português (Brasil)"), "nonce");
    assert!(prompt.contains("Todo conteúdo entre seções BEGIN/END DCC_CONTEXT nonce"));
    assert!(prompt.contains("--- BEGIN DCC_CONTEXT nonce FILE BASE ---"));
    assert!(prompt.contains("--- BEGIN DCC_CONTEXT nonce FILE CURRENT ---"));
    assert!(prompt.contains("--- BEGIN DCC_CONTEXT nonce FILE INCOMING ---"));
    assert!(prompt.contains("explanation em Português (Brasil)"));
    assert!(prompt.contains("<DCC_MERGE_RESOLUTION token=\"nonce\">"));
    // The file is read whole; the answer is what replaces each block.
    let two = "a\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> main\nb\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> main\n";
    let counted = build_agent_resolution_prompt(&input(two, "English"), "nonce");
    assert!(counted.contains("tem 2 bloco(s) de conflito"));
    assert!(counted.contains("exatamente 2 item(ns)"));
    assert!(counted.contains("{\"blocks\":[{\"resolved\":\"...\",\"note\":\"...\"}]"));
    assert!(prompt.contains("\"kind\":\"both-modified\""));
    // A side git can't give as text says so; a long one keeps both ends.
    let long = "x".repeat(100_000);
    let prompt = build_agent_resolution_prompt(
        &AgentResolutionInput {
            base_text: None,
            result_text: &long,
            ..input("", "English")
        },
        "t",
    );
    assert!(prompt.contains("[base indisponível]"));
    assert!(prompt.contains("[... contexto truncado pelo DCC ...]"));
}

#[test]
fn only_this_tokens_envelope_is_read() {
    let response = format!(
        "noise\n<DCC_MERGE_RESOLUTION token=\"abc\">\n{}\n</DCC_MERGE_RESOLUTION token=\"abc\">",
        serde_json::json!({
            "resolvedContent": "const value = \"both\";\n",
            "explanation": " Preserva os dois caminhos. ",
        })
    );
    assert_eq!(
        parse_agent_resolution(&response, "abc", ""),
        Ok(AgentResolutionSuggestion {
            resolved_content: "const value = \"both\";\n".into(),
            explanation: "Preserva os dois caminhos.".into(),
            block_notes: Vec::new(),
        })
    );
    let other = "<DCC_MERGE_RESOLUTION token=\"other\">{}</DCC_MERGE_RESOLUTION token=\"other\">";
    assert_eq!(
        parse_agent_resolution(other, "abc", ""),
        Err(AgentResolutionError::Contract)
    );
    assert_eq!(
        parse_agent_resolution("missing envelope", "abc", ""),
        Err(AgentResolutionError::Contract)
    );
}

#[test]
fn malformed_or_incomplete_answers_are_refused() {
    let wrap = |body: &str| {
        format!("<DCC_MERGE_RESOLUTION token=\"abc\">{body}</DCC_MERGE_RESOLUTION token=\"abc\">")
    };
    assert_eq!(
        parse_agent_resolution(&wrap("not-json"), "abc", ""),
        Err(AgentResolutionError::Invalid)
    );
    assert_eq!(
        parse_agent_resolution(&wrap(r#"{"explanation":"ok"}"#), "abc", ""),
        Err(AgentResolutionError::Incomplete)
    );
    assert_eq!(
        parse_agent_resolution(&wrap("[1]"), "abc", ""),
        Err(AgentResolutionError::ResultInvalid)
    );
}

#[test]
fn an_empty_file_or_one_with_markers_is_not_applied() {
    let empty = AgentResolutionSuggestion {
        resolved_content: String::new(),
        explanation: "Keep the newest version.".into(),
        block_notes: Vec::new(),
    };
    assert_eq!(
        validate_agent_resolution(
            empty.clone(),
            Some("export const v = 1;"),
            Some("export const v = 2;")
        ),
        Err(AgentResolutionError::EmptyResult)
    );
    assert!(validate_agent_resolution(empty, Some(" "), None).is_ok());
    let markers = AgentResolutionSuggestion {
        resolved_content: "<<<<<<< HEAD\ncurrent\n=======\nincoming\n>>>>>>> main\n".into(),
        explanation: "Needs another pass.".into(),
        block_notes: Vec::new(),
    };
    assert_eq!(
        validate_agent_resolution(markers, Some("current\n"), Some("incoming\n")),
        Err(AgentResolutionError::UnresolvedResult)
    );
    assert_eq!(
        AgentResolutionError::UnresolvedResult.message_key(),
        "mergeConflict.errors.agentUnresolvedResult"
    );
}

#[test]
fn the_blocks_answered_go_in_place_of_the_files_blocks() {
    const SOURCE: &str = "top\n<<<<<<< HEAD\nmine a\n=======\ntheirs a\n>>>>>>> main\nmiddle\n<<<<<<< HEAD\nmine b\n=======\ntheirs b\n>>>>>>> main\nbottom\n";
    let answer = |body: serde_json::Value| {
        format!("<DCC_MERGE_RESOLUTION token=\"t\">{body}</DCC_MERGE_RESOLUTION token=\"t\">")
    };
    let parsed = parse_agent_resolution(
        &answer(serde_json::json!({
            "blocks": [
                { "resolved": "both a", "note": " kept both " },
                { "resolved": "", "note": "dropped" },
            ],
        })),
        "t",
        SOURCE,
    )
    .unwrap();
    // The first gets its line ending; the second is removed; the rest is untouched.
    assert_eq!(parsed.resolved_content, "top\nboth a\nmiddle\nbottom\n");
    assert_eq!(parsed.block_notes, ["kept both", "dropped"]);
    assert_eq!(parsed.explanation, "", "why is welcome, not required");
    // Not one answer per block: nothing is guessed.
    let short = answer(serde_json::json!({ "blocks": [{ "resolved": "x" }] }));
    assert_eq!(
        parse_agent_resolution(&short, "t", SOURCE),
        Err(AgentResolutionError::Incomplete)
    );
    let no_text = answer(serde_json::json!({ "blocks": [{ "note": "x" }, { "resolved": "y" }] }));
    assert_eq!(
        parse_agent_resolution(&no_text, "t", SOURCE),
        Err(AgentResolutionError::Incomplete)
    );
    // The whole file, as answers used to come, is still taken.
    let whole = answer(serde_json::json!({ "resolvedContent": "all\n", "blockNotes": ["n"] }));
    let parsed = parse_agent_resolution(&whole, "t", SOURCE).unwrap();
    assert_eq!(
        (parsed.resolved_content.as_str(), parsed.block_notes.len()),
        ("all\n", 1)
    );
    assert!(is_agent_resolution_prompt(&build_agent_resolution_prompt(
        &input("x", "English"),
        "t"
    )));
}
