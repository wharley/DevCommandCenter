use super::*;

/// Applies a change to `text`, returning the new text and selection.
fn run(text: &str, change: Option<Change>) -> (String, Range<usize>) {
    let change = change.expect("a change");
    let mut edits = change.edits;
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut out = text.to_string();
    for (range, insert) in edits {
        out.replace_range(range, &insert);
    }
    (out, change.selection)
}

fn at_end(text: &str) -> usize {
    text.len()
}

#[test]
fn markers_need_their_space() {
    assert!(marker("- item").is_some());
    assert!(marker("  * item").is_some());
    assert!(marker("12. item").is_some());
    assert!(marker("3) item").is_some());
    assert!(marker("**bold**").is_none());
    assert!(marker("-item").is_none());
    assert!(marker("1.5 apples").is_none());
    assert!(marker("plain").is_none());
}

#[test]
fn enter_starts_the_next_item() {
    let text = "Tarefas:\n- ler o código";
    assert_eq!(
        run(text, enter(text, at_end(text))),
        ("Tarefas:\n- ler o código\n- ".into(), 27..27)
    );
    let text = "1. um";
    assert_eq!(run(text, enter(text, 5)), ("1. um\n2. ".into(), 9..9));
    let text = "  + nested";
    assert_eq!(run(text, enter(text, at_end(text))).0, "  + nested\n  + ");
}

#[test]
fn enter_mid_item_carries_the_rest_to_the_next() {
    let text = "- abcdef";
    assert_eq!(run(text, enter(text, 5)), ("- abc\n- def".into(), 8..8));
}

#[test]
fn enter_on_an_empty_item_leaves_the_list() {
    let text = "- one\n- ";
    assert_eq!(
        run(text, enter(text, at_end(text))),
        ("- one\n".into(), 6..6)
    );
    // Nested, it comes a level out first.
    let text = "- one\n  - ";
    assert_eq!(
        run(text, enter(text, at_end(text))),
        ("- one\n- ".into(), 8..8)
    );
}

#[test]
fn enter_outside_a_list_is_left_to_send() {
    assert!(enter("just a prompt", 5).is_none());
    assert!(!in_list("just a prompt", 5));
    assert!(in_list("a\n- b", 4));
}

#[test]
fn tab_nests_under_the_item_above_and_shift_tab_comes_back() {
    let text = "- one\n- two";
    let (nested, selection) = run(text, level(text, 11..11, true));
    assert_eq!(nested, "- one\n  - two");
    assert_eq!(selection, 13..13);
    let (back, _) = run(&nested, level(&nested, 13..13, false));
    assert_eq!(back, text);
    // The first item has nothing to nest under.
    assert!(level(text, 2..2, true).is_none());
    assert!(level(text, 2..2, false).is_none());
}

#[test]
fn numbers_follow_the_level() {
    let text = "1. one\n2. two";
    let (nested, _) = run(text, level(text, 13..13, true));
    assert_eq!(nested, "1. one\n   1. two");
    let (back, _) = run(&nested, level(&nested, 16..16, false));
    assert_eq!(back, "1. one\n2. two");
}

#[test]
fn emphasis_wraps_unwraps_and_keeps_spaces_out() {
    let text = "make it bold now";
    assert_eq!(
        run(text, emphasis(text, 8..12, "**")),
        ("make it **bold** now".into(), 10..14)
    );
    let wrapped = "make it **bold** now";
    assert_eq!(
        run(wrapped, emphasis(wrapped, 10..14, "**")),
        (text.into(), 8..12)
    );
    assert_eq!(
        run(wrapped, emphasis(wrapped, 8..16, "**")),
        (text.into(), 8..12)
    );
    // A space selected with the word stays outside the marks.
    assert_eq!(
        run(text, emphasis(text, 7..12, "*")).0,
        "make it *bold* now"
    );
    // No selection: the pair, the caret between.
    assert_eq!(
        run("a ", emphasis("a ", 2..2, "**")),
        ("a ****".into(), 4..4)
    );
}

#[test]
fn markers_are_spanned_for_the_faint_color() {
    assert_eq!(
        marker_spans("intro\n- a\n  12. b\n**c**"),
        vec![6..7, 12..15]
    );
}
