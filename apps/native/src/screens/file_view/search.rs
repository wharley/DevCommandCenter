//! Find and replace in the editor (`PieceTable.search` and
//! `buildSearchReplacementText`): line by line, case-insensitive unless
//! asked, a whole word bounded by separators, a regex with `$&` / `$1` /
//! `$$` in the replacement. A query spanning lines finds nothing. Also the
//! bracket the caret touches and its pair (`findBracketMatchRanges`).

use std::ops::Range;

use regex::{Regex, RegexBuilder};

/// `MAX_FIND_MATCHES`
const MAX_MATCHES: usize = 100_000;
/// `WORD_SEPARATORS`
const SEPARATORS: &str = "`~!@#$%^&*()-=+[{]}\\|;:'\",.<>/?";
const MAX_BRACKET_LINES: usize = 1_000;
const MAX_BRACKET_CHARS: usize = 50_000;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchParams {
    pub text: String,
    pub replace: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

fn pattern(params: &SearchParams) -> Option<Regex> {
    let body = if params.regex {
        params.text.clone()
    } else {
        regex::escape(&params.text)
    };
    RegexBuilder::new(&body)
        .case_insensitive(!params.case_sensitive)
        .multi_line(params.regex)
        .build()
        .ok()
}

fn is_separator(character: char) -> bool {
    (character as u32) <= 32 || character as u32 == 127 || SEPARATORS.contains(character)
}

fn whole_word(text: &str, range: &Range<usize>) -> bool {
    let before = text[..range.start].chars().next_back();
    let after = text[range.end..].chars().next();
    before.is_none_or(is_separator) && after.is_none_or(is_separator)
}

/// Every match in `text`, in order (byte ranges).
pub fn find_matches(text: &str, params: &SearchParams) -> Vec<Range<usize>> {
    let query = &params.text;
    if query.is_empty()
        || text.is_empty()
        || query.contains('\n')
        || query.contains('\r')
        || params.regex && (query.contains("\\n") || query.contains("\\r"))
    {
        return Vec::new();
    }
    let Some(pattern) = pattern(params) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut start = 0;
    for line in text.split('\n') {
        let body = line.trim_end_matches('\r');
        for found in pattern.find_iter(body) {
            if found.is_empty() {
                continue;
            }
            let range = start + found.start()..start + found.end();
            if !params.whole_word || whole_word(text, &range) {
                out.push(range);
                if out.len() >= MAX_MATCHES {
                    return out;
                }
            }
        }
        start += line.len() + 1;
    }
    out
}

/// What replaces one match: the replacement as typed, or with a regex its
/// `$&`, `$1`… and `$$` filled from that match.
pub fn replacement(text: &str, params: &SearchParams, range: &Range<usize>) -> String {
    if !params.regex {
        return params.replace.clone();
    }
    let Some(pattern) = pattern(params) else {
        return params.replace.clone();
    };
    let line_start = text[..range.start].rfind('\n').map_or(0, |at| at + 1);
    let line_end = text[range.start..]
        .find('\n')
        .map_or(text.len(), |at| range.start + at);
    let line = text[line_start..line_end].trim_end_matches('\r');
    let Some(captures) = pattern.captures_at(line, range.start - line_start) else {
        return params.replace.clone();
    };
    let whole = captures.get(0).expect("the match");
    if whole.start() != range.start - line_start || whole.len() != range.len() {
        return params.replace.clone();
    }
    let mut out = String::new();
    let mut chars = params.replace.char_indices().peekable();
    let source = &params.replace;
    while let Some((at, character)) = chars.next() {
        if character != '$' {
            out.push(character);
            continue;
        }
        match chars.peek().copied() {
            Some((_, '$')) => {
                chars.next();
                out.push('$');
            }
            Some((_, '&')) => {
                chars.next();
                out.push_str(whole.as_str());
            }
            Some((digit_at, digit)) if digit.is_ascii_digit() => {
                let mut end = digit_at;
                while let Some((next_at, next)) = chars.peek().copied() {
                    if !next.is_ascii_digit() {
                        break;
                    }
                    end = next_at + next.len_utf8();
                    chars.next();
                }
                let group: usize = source[digit_at..end].parse().unwrap_or(0);
                if let Some(found) = captures.get(group) {
                    out.push_str(found.as_str());
                }
            }
            _ => out.push_str(&source[at..at + 1]),
        }
    }
    out
}

fn closer(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}

fn opener(close: char) -> Option<char> {
    match close {
        ')' => Some('('),
        ']' => Some('['),
        '}' => Some('{'),
        _ => None,
    }
}

/// The bracket just before the caret (else just after) and its pair, as
/// byte offsets of each.
pub fn bracket_pair(text: &str, caret: usize) -> Option<(usize, usize)> {
    let before = text[..caret].char_indices().next_back().filter(|(at, c)| {
        (closer(*c).is_some() || opener(*c).is_some()) && text[*at..caret].len() == 1
    });
    let after = text[caret..]
        .chars()
        .next()
        .filter(|c| closer(*c).is_some() || opener(*c).is_some())
        .map(|c| (caret, c));
    let (at, bracket) = before.or(after)?;
    if let Some(close) = closer(bracket) {
        let mut depth = 0usize;
        let (mut lines, mut scanned) = (0usize, 0usize);
        for (offset, character) in text[at..].char_indices() {
            scanned += 1;
            if character == '\n' {
                lines += 1;
            }
            if scanned > MAX_BRACKET_CHARS || lines > MAX_BRACKET_LINES {
                return None;
            }
            if character == bracket {
                depth += 1;
            } else if character == close {
                depth -= 1;
                if depth == 0 {
                    return Some((at, at + offset));
                }
            }
        }
        return None;
    }
    let open = opener(bracket)?;
    let mut depth = 0usize;
    let (mut lines, mut scanned) = (0usize, 0usize);
    for (offset, character) in text[..at + 1].char_indices().rev() {
        scanned += 1;
        if character == '\n' {
            lines += 1;
        }
        if scanned > MAX_BRACKET_CHARS || lines > MAX_BRACKET_LINES {
            return None;
        }
        if character == bracket {
            depth += 1;
        } else if character == open {
            depth -= 1;
            if depth == 0 {
                return Some((offset, at));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(text: &str) -> SearchParams {
        SearchParams {
            text: text.into(),
            ..SearchParams::default()
        }
    }

    #[test]
    fn finds_line_by_line_ignoring_case_unless_asked() {
        let text = "let Total = 1;\nlet total = total + 1;\n";
        assert_eq!(find_matches(text, &params("total")).len(), 3);
        let exact = SearchParams {
            case_sensitive: true,
            ..params("total")
        };
        assert_eq!(find_matches(text, &exact), vec![19..24, 27..32]);
        assert!(find_matches(text, &params("1;\nlet")).is_empty());
    }

    #[test]
    fn a_whole_word_is_bounded_by_separators() {
        let text = "total subtotal total_x (total)";
        let word = SearchParams {
            whole_word: true,
            ..params("total")
        };
        let found: Vec<&str> = find_matches(text, &word)
            .into_iter()
            .map(|range| &text[range])
            .collect();
        assert_eq!(found, vec!["total", "total"]);
    }

    #[test]
    fn a_regex_fills_its_groups_in_the_replacement() {
        let text = "fn alpha() {}\nfn beta() {}\n";
        let regex = SearchParams {
            regex: true,
            replace: "fn $1_v2() /* $& $$ */".into(),
            ..params(r"fn (\w+)\(\)")
        };
        let found = find_matches(text, &regex);
        assert_eq!(found.len(), 2);
        assert_eq!(
            replacement(text, &regex, &found[1]),
            "fn beta_v2() /* fn beta() $ */"
        );
        // A broken pattern finds nothing.
        assert!(
            find_matches(
                text,
                &SearchParams {
                    regex: true,
                    ..params("(")
                }
            )
            .is_empty()
        );
    }

    #[test]
    fn the_caret_touches_a_bracket_and_finds_its_pair() {
        let text = "f(a, [b], {c})";
        // Just after the opening paren.
        assert_eq!(bracket_pair(text, 2), Some((1, 13)));
        // Just before a closing bracket.
        assert_eq!(bracket_pair(text, 7), Some((5, 7)));
        assert_eq!(bracket_pair(text, 9), None);
        assert_eq!(bracket_pair("(open", 1), None);
    }
}
