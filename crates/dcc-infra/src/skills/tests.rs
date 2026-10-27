//! The skills compiler and its source, against folders of their own.

use super::*;
use crate::skills_context::detect_context_sources;
use tempfile::tempdir;

fn skill(name: &str, targets: &[&str]) -> SkillRecord {
    SkillRecord {
        name: name.to_string(),
        description: format!("desc for {name}"),
        body: format!("Body of {name}."),
        target_agents: targets.iter().map(|s| s.to_string()).collect(),
        disable_model_invocation: false,
        scope: "project".to_string(),
        source_url: None,
    }
}

#[test]
fn validates_skill_names() {
    assert!(is_valid_skill_name("revisar-pr"));
    assert!(is_valid_skill_name("skill1"));
    assert!(!is_valid_skill_name("Revisar"));
    assert!(!is_valid_skill_name("-leading"));
    assert!(!is_valid_skill_name("has space"));
    assert!(!is_valid_skill_name(""));
    assert!(!is_valid_skill_name("../escape"));
}

#[test]
fn renders_and_round_trips_body() {
    let md = render_skill_md("foo", "does foo", "Line one.\nLine two.", false);
    assert!(md.starts_with("---\nname: foo\ndescription: does foo\n---\n"));
    assert_eq!(extract_body(&md), "Line one.\nLine two.");
}

#[test]
fn renders_codex_invocation_policy() {
    assert_eq!(
        render_codex_openai_yaml(false),
        "policy:\n  allow_implicit_invocation: true\n"
    );
    assert_eq!(
        render_codex_openai_yaml(true),
        "policy:\n  allow_implicit_invocation: false\n"
    );
}

#[test]
fn extract_body_without_frontmatter() {
    assert_eq!(extract_body("just a body"), "just a body");
}

#[test]
fn inserts_agents_block_preserving_handwritten() {
    let existing = "# AGENTS\n\nHand written rules.\n";
    let inner = render_agents_inner(&[skill("a", &[TARGET_AGENTS_MD])]);
    let out = upsert_agents_block(existing, &inner);
    assert!(out.contains("Hand written rules."));
    assert!(out.contains(AGENTS_BLOCK_START));
    assert!(out.contains("### a"));
    assert!(out.contains(AGENTS_BLOCK_END));
}

#[test]
fn replaces_block_idempotently() {
    let existing = "Top.\n";
    let first = upsert_agents_block(
        &existing,
        &render_agents_inner(&[skill("a", &[TARGET_AGENTS_MD])]),
    );
    let second = upsert_agents_block(
        &first,
        &render_agents_inner(&[skill("a", &[TARGET_AGENTS_MD])]),
    );
    assert_eq!(first, second, "recompiling the same skills must be stable");
    assert_eq!(first.matches(AGENTS_BLOCK_START).count(), 1);
}

#[test]
fn renders_cursor_mdc_with_frontmatter() {
    let mdc = render_cursor_mdc(&skill("review-pr", &[TARGET_CURSOR]));
    assert!(mdc.starts_with("---\ndescription: desc for review-pr\nalwaysApply: false\n---\n"));
    assert!(mdc.contains("Body of review-pr."));
}

#[test]
fn skills_for_target_excludes_disabled() {
    let mut hidden = skill("hidden", &[TARGET_AGENTS_MD, TARGET_GEMINI]);
    hidden.disable_model_invocation = true;
    let visible = skill("visible", &[TARGET_GEMINI]);
    let all = vec![hidden, visible];
    let gemini = skills_for_target(&all, TARGET_GEMINI);
    assert_eq!(gemini.len(), 1);
    assert_eq!(gemini[0].name, "visible");
}

#[test]
fn empty_inner_removes_block_keeps_handwritten() {
    let existing = "# AGENTS\n\nKeep me.\n";
    let with = upsert_agents_block(
        existing,
        &render_agents_inner(&[skill("a", &[TARGET_AGENTS_MD])]),
    );
    let without = upsert_agents_block(&with, "");
    assert!(!without.contains(AGENTS_BLOCK_START));
    assert!(without.contains("Keep me."));
}

#[test]
fn detects_passive_project_context_sources() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join(".devcommandcenter/skills/review")).unwrap();
    fs::write(
        root.join(".devcommandcenter/skills/review/SKILL.md"),
        "body",
    )
    .unwrap();
    fs::write(
        root.join("AGENTS.md"),
        format!("{AGENTS_BLOCK_START}\nmanaged\n{AGENTS_BLOCK_END}\nmanual"),
    )
    .unwrap();
    fs::write(root.join("CLAUDE.md"), "manual").unwrap();
    fs::create_dir_all(root.join(".cursor/rules")).unwrap();
    fs::write(root.join(".cursor/rules/manual.mdc"), "manual").unwrap();

    let detections = detect_context_sources(root.to_str().unwrap(), None);
    assert!(detections.iter().any(|item| item.id == "dcc-source"));
    let agents = detections
        .iter()
        .find(|item| item.id == "agents-md")
        .expect("agents detected");
    assert!(agents.has_dcc_block);
    assert_eq!(agents.managed_count, 1);
    let cursor = detections
        .iter()
        .find(|item| item.id == "cursor-rules")
        .expect("cursor rules detected");
    assert_eq!(cursor.count, 1);
    assert_eq!(cursor.external_count, 1);
}

#[test]
fn compiles_codex_skills_to_agents_directory() {
    let source = tempdir().expect("source tempdir");
    let target = tempdir().expect("target tempdir");
    let mut orchestration = skill("dcc-orchestration", &[TARGET_CODEX]);
    orchestration.description = "Delegate suitable work".to_string();
    orchestration.disable_model_invocation = false;

    save_skill(source.path().to_string_lossy().as_ref(), orchestration).unwrap();
    compile_skills(
        &source.path().to_string_lossy(),
        &target.path().to_string_lossy(),
    )
    .unwrap();

    let native_root = target.path().join(".agents/skills/dcc-orchestration");
    let skill_md = fs::read_to_string(native_root.join("SKILL.md")).unwrap();
    let openai_yaml = fs::read_to_string(native_root.join("agents/openai.yaml")).unwrap();
    assert!(skill_md.contains("name: dcc-orchestration"));
    assert!(skill_md.contains("Body of dcc-orchestration."));
    assert!(openai_yaml.contains("allow_implicit_invocation: true"));

    let detections = detect_context_sources(
        &source.path().to_string_lossy(),
        Some(&target.path().to_string_lossy()),
    );
    let codex = detections
        .iter()
        .find(|item| item.id == "codex-skills")
        .expect("Codex native skills detected");
    assert_eq!(codex.relative_path, ".agents/skills");
    assert_eq!(codex.managed_count, 1);
    assert_eq!(codex.external_count, 0);
}

#[test]
fn missing_source_manifest_does_not_remove_managed_targets() {
    let source = tempdir().expect("source tempdir");
    let target = tempdir().expect("target tempdir");
    let native = target.path().join(".agents/skills/existing");
    fs::create_dir_all(&native).unwrap();
    fs::write(native.join("SKILL.md"), "committed skill").unwrap();
    fs::write(
        target.path().join(".agents/skills/.dcc-managed.json"),
        "[\"existing\"]",
    )
    .unwrap();

    compile_skills(
        source.path().to_string_lossy().as_ref(),
        target.path().to_string_lossy().as_ref(),
    )
    .unwrap();

    assert!(native.join("SKILL.md").is_file());
    assert_eq!(
        fs::read_to_string(native.join("SKILL.md")).unwrap(),
        "committed skill"
    );
}

#[test]
fn compiles_multi_provider_skill_in_selected_checkout() {
    let project = tempdir().unwrap();
    let checkout = tempdir().unwrap();
    let source = project.path().to_str().unwrap();
    let target = checkout.path().to_str().unwrap();
    fs::write(
        checkout.path().join("GEMINI.md"),
        "Existing project instructions.\n",
    )
    .unwrap();
    save_skill(
        source,
        skill(
            "team",
            &[
                TARGET_CLAUDE,
                TARGET_CODEX,
                TARGET_GEMINI,
                TARGET_CURSOR,
                TARGET_GROK,
            ],
        ),
    )
    .unwrap();
    compile_skills(source, target).unwrap();

    for path in [
        ".claude/skills/team/SKILL.md",
        ".agents/skills/team/SKILL.md",
        ".grok/skills/team/SKILL.md",
        ".cursor/rules/team.mdc",
        "GEMINI.md",
    ] {
        assert!(fs::read_to_string(checkout.path().join(path))
            .unwrap()
            .contains("Body of team."));
        assert!(
            !project.path().join(path).exists(),
            "must write to the selected checkout: {path}"
        );
    }
    assert!(!checkout.path().join("AGENTS.md").exists());
    assert!(fs::read_to_string(checkout.path().join("GEMINI.md"))
        .unwrap()
        .contains("Existing project instructions."));
    let grok = detect_context_sources(source, Some(target))
        .into_iter()
        .find(|item| item.kind == "grok_skills")
        .unwrap();
    assert_eq!(grok.managed_count, 1);
    assert_eq!(grok.relative_path, ".grok/skills");
}

#[test]
fn native_explicit_invocation_survives_compile_and_can_be_reenabled() {
    let checkout = tempdir().unwrap();
    let root = checkout.path().to_str().unwrap();
    let mut record = skill("team", &[TARGET_CLAUDE, TARGET_CODEX, TARGET_GROK]);
    for explicit_only in [true, false] {
        record.disable_model_invocation = explicit_only;
        save_skill(root, record.clone()).unwrap();
        compile_skills(root, root).unwrap();
        for path in [".claude/skills/team/SKILL.md", ".grok/skills/team/SKILL.md"] {
            let content = fs::read_to_string(checkout.path().join(path)).unwrap();
            assert_eq!(
                content.contains("disable-model-invocation: true"),
                explicit_only
            );
            assert!(content.contains("Body of team."));
        }
        let policy = fs::read_to_string(
            checkout
                .path()
                .join(".agents/skills/team/agents/openai.yaml"),
        )
        .unwrap();
        assert!(policy.contains(&format!("allow_implicit_invocation: {}", !explicit_only)));
    }
}

#[test]
fn removing_grok_target_preserves_external_skills() {
    let checkout = tempdir().unwrap();
    let root = checkout.path().to_str().unwrap();
    let external = checkout.path().join(".grok/skills/external");
    fs::create_dir_all(&external).unwrap();
    fs::write(external.join("SKILL.md"), "External instructions.").unwrap();
    save_skill(root, skill("team", &[TARGET_GROK])).unwrap();
    compile_skills(root, root).unwrap();
    save_skill(root, skill("team", &[TARGET_CLAUDE])).unwrap();
    compile_skills(root, root).unwrap();
    assert!(!checkout.path().join(".grok/skills/team").exists());
    assert_eq!(
        fs::read_to_string(external.join("SKILL.md")).unwrap(),
        "External instructions."
    );
}

#[test]
fn compiles_source_and_targets_in_the_same_checkout() {
    let checkout = tempdir().expect("active checkout");
    save_skill(
        checkout.path().to_string_lossy().as_ref(),
        skill("same-checkout", &[TARGET_CLAUDE]),
    )
    .unwrap();

    let root = checkout.path().to_string_lossy();
    compile_skills(&root, &root).unwrap();

    assert!(checkout
        .path()
        .join(".claude/skills/same-checkout/SKILL.md")
        .is_file());
}
