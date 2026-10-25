use super::*;

fn run(text: &str, change: Option<Change>) -> (String, Range<usize>) {
    let change = change.expect("a change");
    (change.apply(text), change.selection)
}

#[test]
fn each_language_has_its_tokens() {
    assert_eq!(comment_tokens("src/lib.rs").line, Some("//"));
    assert_eq!(comment_tokens("db/schema.sql").line, Some("--"));
    assert_eq!(comment_tokens("app/Gemfile").block, ("=begin", "=end"));
    assert_eq!(comment_tokens("Makefile").line, Some("#"));
    assert_eq!(comment_tokens(".env").line, Some("#"));
    assert_eq!(comment_tokens("ci.yml").line, Some("#"));
    let markdown = comment_tokens("README.md");
    assert_eq!((markdown.line, markdown.block), (None, ("<!--", "-->")));
    let css = comment_tokens("style.css");
    assert_eq!((css.line, css.block), (None, ("/*", "*/")));
    assert_eq!(comment_tokens("main.py").block, ("\"\"\"", "\"\"\""));
}

#[test]
fn line_comments_align_to_the_least_indentation_and_come_off() {
    let rust = comment_tokens("a.rs");
    let text = "  a\n\n    b\n";
    let doc = Doc::new(text);
    let (out, selection) = run(text, toggle_comment(&doc, &(0..10), false, rust));
    assert_eq!(out, "  // a\n\n  //   b\n", "the blank line is skipped");
    assert_eq!(selection, 0..16);
    let doc = Doc::new(&out);
    let (back, _) = run(&out, toggle_comment(&doc, &(0..16), false, rust));
    assert_eq!(back, text);
    // The caret alone on a blank line still gets one, and moves past it.
    let doc = Doc::new("x\n\ny");
    assert_eq!(
        run("x\n\ny", toggle_comment(&doc, &(2..2), false, rust)),
        ("x\n// \ny".into(), 5..5)
    );
    // One line commented, one not: both get it.
    let mixed = "// a\nb";
    let doc = Doc::new(mixed);
    assert_eq!(
        run(mixed, toggle_comment(&doc, &(0..6), false, rust)).0,
        "// // a\n// b"
    );
}

#[test]
fn without_line_comments_the_whole_lines_take_a_block() {
    let markdown = comment_tokens("notes.md");
    let text = "  hello\nworld\n";
    let doc = Doc::new(text);
    let (out, _) = run(text, toggle_comment(&doc, &(4..9), false, markdown));
    assert_eq!(out, "  <!-- hello\nworld -->\n");
    // Both lines again: the block comes off.
    let doc = Doc::new(&out);
    assert_eq!(
        run(&out, toggle_comment(&doc, &(8..15), false, markdown)).0,
        text
    );
}

#[test]
fn a_block_comment_wraps_the_selection_and_comes_off() {
    let rust = comment_tokens("a.rs");
    let text = "let x = value;";
    let doc = Doc::new(text);
    let (out, selection) = run(text, toggle_block_comment(&doc, &(8..13), false, rust));
    assert_eq!(out, "let x = /* value */;");
    assert_eq!(&out[selection.clone()], "value", "the text stays selected");
    // The selection just inside the comment: off again.
    let doc = Doc::new(&out);
    let (back, selection) = run(&out, toggle_block_comment(&doc, &selection, false, rust));
    assert_eq!(back, text);
    assert_eq!(&back[selection], "value");
    // The comment itself selected: off as well.
    let doc = Doc::new(&out);
    assert_eq!(
        run(&out, toggle_block_comment(&doc, &(8..19), false, rust)).0,
        text
    );
    // The caret alone: an empty comment with the caret inside.
    let doc = Doc::new("f()");
    let (out, selection) = run("f()", toggle_block_comment(&doc, &(2..2), false, rust));
    assert_eq!(out, "f(/*  */)");
    assert_eq!(selection, 5..5);
}
