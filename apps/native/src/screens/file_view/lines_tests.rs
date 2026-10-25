use super::*;

fn run(text: &str, change: Option<Change>) -> (String, Range<usize>) {
    let change = change.expect("a change");
    (change.apply(text), change.selection)
}

#[test]
fn lines_shift_in_and_out_as_each_line_starts() {
    let text = "\ta\n\nb\n";
    let doc = Doc::new(text);
    // Both lines selected, the blank one between left alone.
    let (out, selection) = run(text, indent_lines(&doc, &(1..5), false, 2, false));
    assert_eq!(out, "\t\ta\n\n  b\n");
    assert_eq!(selection, 2..8, "each end keeps its column");
    let doc = Doc::new(&out);
    let (back, _) = run(&out, indent_lines(&doc, &(0..8), false, 2, true));
    assert_eq!(back, "\ta\n\nb\n");
    // The caret alone: its line, blank or not, and the caret moves along.
    let doc = Doc::new("x");
    assert_eq!(
        run("x", indent_lines(&doc, &(1..1), false, 4, false)),
        ("    x".into(), 5..5)
    );
    let doc = Doc::new("x");
    assert!(indent_lines(&doc, &(0..0), false, 4, true).is_none());
    // Out from inside the indentation stops at the line's start.
    let doc = Doc::new("    x");
    assert_eq!(
        run("    x", indent_lines(&doc, &(1..1), false, 2, true)),
        ("  x".into(), 0..0)
    );
}

#[test]
fn tab_over_a_selection_on_one_line_replaces_it() {
    let doc = Doc::new("\tlet x = 1;");
    let (out, selection) = run("\tlet x = 1;", Some(tab_over_selection(&doc, &(5..6), 4)));
    assert_eq!(out, "\tlet \t = 1;");
    assert_eq!(selection, 6..6);
    let doc = Doc::new("let x");
    assert_eq!(
        run("let x", Some(tab_over_selection(&doc, &(4..5), 2))).0,
        "let   "
    );
}

#[test]
fn lines_move_past_their_neighbor_and_stop_at_the_edges() {
    let text = "a\nbb\nccc";
    let doc = Doc::new(text);
    // "bb" up, the caret with it.
    assert_eq!(
        run(text, move_lines(&doc, &(3..3), false, true)),
        ("bb\na\nccc".into(), 1..1)
    );
    // "bb" down past the last line (no line break at the end).
    assert_eq!(
        run(text, move_lines(&doc, &(3..3), false, false)),
        ("a\nccc\nbb".into(), 7..7)
    );
    assert!(move_lines(&doc, &(0..0), false, true).is_none());
    assert!(move_lines(&doc, &(6..6), false, false).is_none());
    // A selection ending at a line's start leaves that line in place.
    let (out, selection) = run(text, move_lines(&doc, &(0..2), false, false));
    assert_eq!(out, "bb\na\nccc");
    assert_eq!(selection, 3..5);
    // CRLF stays CRLF.
    let crlf = "a\r\nb\r\n";
    let doc = Doc::new(crlf);
    assert_eq!(
        run(crlf, move_lines(&doc, &(3..3), false, true)).0,
        "b\r\na\r\n"
    );
}

#[test]
fn copies_keep_the_selection_on_the_lower_or_upper_copy() {
    let text = "a\nb";
    let doc = Doc::new(text);
    assert_eq!(
        run(text, Some(copy_lines(&doc, &(0..1), false, true))),
        ("a\na\nb".into(), 2..3)
    );
    assert_eq!(
        run(text, Some(copy_lines(&doc, &(0..1), false, false))),
        ("a\na\nb".into(), 0..1)
    );
    // The last line copied up goes after it, past a new line break.
    assert_eq!(
        run(text, Some(copy_lines(&doc, &(2..3), false, false))),
        ("a\nb\nb".into(), 2..3)
    );
}

#[test]
fn a_blank_line_takes_the_indentation_and_ctrl_k_cuts_to_the_end() {
    let text = "  if x {\n}";
    let doc = Doc::new(text);
    assert_eq!(
        run(text, Some(blank_line(&doc, 3))),
        ("  if x {\n  \n}".into(), 11..11)
    );
    assert_eq!(
        run(text, delete_to_line_end(&doc, &(5..5))),
        ("  if \n}".into(), 5..5)
    );
    // At the line's end the line break goes; at the file's end nothing.
    assert_eq!(run(text, delete_to_line_end(&doc, &(8..8))).0, "  if x {}");
    assert!(delete_to_line_end(&doc, &(10..10)).is_none());
    assert_eq!(run(text, delete_to_line_end(&doc, &(2..4))).0, "   x {\n}");
}

#[test]
fn a_bracket_or_quote_wraps_the_selection() {
    let doc = Doc::new("call(a, b)");
    assert_eq!(
        run("call(a, b)", surround(&doc, &(5..9), "[")),
        ("call([a, b])".into(), 6..10)
    );
    assert_eq!(run("x", surround(&Doc::new("x"), &(0..1), "\"")).0, "\"x\"");
    assert!(surround(&doc, &(5..9), "x").is_none());
    assert!(surround(&doc, &(5..5), "(").is_none());
}
