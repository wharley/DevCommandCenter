//! A review comment's text as the app shows it. Reviewers open a comment
//! with a severity marker ("🔴 **Bloqueante**", "⚠️ **Atenção:**",
//! "♻️ **Reuso.**", "🟢 …"): it is read here as a [`Severity`] and its label,
//! so it shows as a small chip instead of an emoji and asterisks. Lists show
//! a comment on one plain line ([`comment_preview`]): the markdown's marks
//! dropped, the words kept.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// 🔴, ⛔, "Bloqueante": red.
    Blocking,
    /// ⚠️, 🟠, 🟡, "Atenção": amber.
    Warning,
    /// ♻️, "Reuso": neutral.
    Reuse,
    /// 🟢, 🔵, 💡, "Sugestão", "Nit": the primary tint.
    Suggestion,
}

/// A comment's opening marker, when it has one, and the text after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentLead {
    pub severity: Option<Severity>,
    /// The marker's own words ("Bloqueante", "Reuso"), without the closing
    /// punctuation; none when only the emoji was there.
    pub label: Option<String>,
    /// The comment from after the marker on (all of it without one).
    pub body: String,
}

/// The one-line preview of a comment for the lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentPreview {
    pub severity: Option<Severity>,
    pub label: Option<String>,
    pub text: String,
}

const EMOJI: &[(&str, Severity)] = &[
    ("🔴", Severity::Blocking),
    ("⛔", Severity::Blocking),
    ("🚫", Severity::Blocking),
    ("🛑", Severity::Blocking),
    ("⚠", Severity::Warning),
    ("🟠", Severity::Warning),
    ("🟡", Severity::Warning),
    ("♻", Severity::Reuse),
    ("🟢", Severity::Suggestion),
    ("🔵", Severity::Suggestion),
    ("💡", Severity::Suggestion),
];

/// A bold opening word that is a marker on its own ("**Bloqueante:** …").
fn severity_of_word(word: &str) -> Option<Severity> {
    match word.to_lowercase().as_str() {
        "bloqueante" | "blocking" | "blocker" | "crítico" | "critico" | "critical" => {
            Some(Severity::Blocking)
        }
        "atenção" | "atencao" | "attention" | "warning" | "aviso" | "cuidado" => {
            Some(Severity::Warning)
        }
        "reuso" | "reuse" | "reutilização" => Some(Severity::Reuse),
        "sugestão" | "sugestao" | "suggestion" | "nit" | "opcional" | "optional" => {
            Some(Severity::Suggestion)
        }
        _ => None,
    }
}

/// The emoji opening `text`, with its variation selector.
fn leading_emoji(text: &str) -> Option<(Severity, &str)> {
    EMOJI.iter().find_map(|(emoji, severity)| {
        let rest = text.strip_prefix(emoji)?;
        let rest = rest.strip_prefix('\u{fe0f}').unwrap_or(rest);
        Some((*severity, rest))
    })
}

/// `**words**` (or `__words__`) opening `text`: the words and what follows.
fn leading_bold(text: &str) -> Option<(&str, &str)> {
    ["**", "__"].iter().find_map(|mark| {
        let inner = text.strip_prefix(mark)?;
        let end = inner.find(mark)?;
        let words = &inner[..end];
        (!words.trim().is_empty() && !words.contains('\n'))
            .then(|| (words, &inner[end + mark.len()..]))
    })
}

fn clean_label(words: &str) -> String {
    words
        .trim()
        .trim_end_matches(['.', ':', '!', '—', '–', '-'])
        .trim()
        .to_string()
}

/// After a marker: its separator (":", ".", "—") and spaces.
fn after_marker(rest: &str) -> &str {
    rest.trim_start_matches([':', '.', '—', '–', '-', ' ', '\t'])
        .trim_start_matches(['\n', '\r'])
}

/// The comment's opening marker (`🔴 **Bloqueante**`, `**Reuso.**`) read
/// apart from its text; a comment without one comes back whole.
pub fn comment_lead(body: &str) -> CommentLead {
    let start = body.trim_start();
    if let Some((severity, rest)) = leading_emoji(start) {
        let rest = rest.trim_start_matches([' ', '\t']);
        return match leading_bold(rest) {
            Some((words, after)) => CommentLead {
                severity: Some(severity),
                label: Some(clean_label(words)).filter(|label| !label.is_empty()),
                body: after_marker(after).to_string(),
            },
            None => CommentLead {
                severity: Some(severity),
                label: None,
                body: after_marker(rest).to_string(),
            },
        };
    }
    if let Some((words, after)) = leading_bold(start) {
        let label = clean_label(words);
        if let Some(severity) = severity_of_word(&label) {
            return CommentLead {
                severity: Some(severity),
                label: Some(label),
                body: after_marker(after).to_string(),
            };
        }
    }
    CommentLead {
        severity: None,
        label: None,
        body: body.to_string(),
    }
}

/// A markdown line as plain words: code spans keep their text, links their
/// label, emphasis and HTML tags go, a list or quote marker goes.
pub fn plain_line(line: &str) -> String {
    let mut line = line.trim();
    loop {
        let before = line;
        line = line.trim_start_matches('#').trim_start();
        line = line.trim_start_matches('>').trim_start();
        for marker in ["- [ ] ", "- [x] ", "- ", "* ", "+ "] {
            if let Some(rest) = line.strip_prefix(marker) {
                line = rest.trim_start();
            }
        }
        let digits = line.chars().take_while(char::is_ascii_digit).count();
        if digits > 0
            && let Some(rest) = line[digits..]
                .strip_prefix(". ")
                .or_else(|| line[digits..].strip_prefix(") "))
        {
            line = rest.trim_start();
        }
        if line == before {
            break;
        }
    }
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    while at < chars.len() {
        let char = chars[at];
        match char {
            // A code span: its text as is.
            '`' => {
                let ticks = chars[at..].iter().take_while(|c| **c == '`').count();
                let open = at + ticks;
                let close = (open..chars.len().saturating_sub(ticks - 1))
                    .find(|&index| chars[index..index + ticks].iter().all(|c| *c == '`'));
                match close {
                    Some(close) => {
                        let code: String = chars[open..close].iter().collect();
                        out.push_str(code.trim());
                        at = close + ticks;
                    }
                    None => at = open,
                }
            }
            // `[label](url)` and `![alt](url)`: the label.
            '!' if chars.get(at + 1) == Some(&'[') => at += 1,
            '[' => {
                let close = chars[at..].iter().position(|c| *c == ']').map(|i| at + i);
                match close {
                    Some(close) if chars.get(close + 1) == Some(&'(') => {
                        let end = chars[close..].iter().position(|c| *c == ')');
                        let label: String = chars[at + 1..close].iter().collect();
                        out.push_str(&plain_line(&label));
                        at = end.map_or(chars.len(), |end| close + end + 1);
                    }
                    _ => {
                        out.push(char);
                        at += 1;
                    }
                }
            }
            // An HTML tag (`<details>`, `<br/>`, `<!-- -->`).
            '<' if chars
                .get(at + 1)
                .is_some_and(|next| next.is_ascii_alphabetic() || *next == '/' || *next == '!') =>
            {
                match chars[at..].iter().position(|c| *c == '>') {
                    Some(end) => {
                        out.push(' ');
                        at += end + 1;
                    }
                    None => {
                        out.push(char);
                        at += 1;
                    }
                }
            }
            '*' | '~' => at += 1,
            // `_emphasis_`, not `snake_case`.
            '_' => {
                let before = at.checked_sub(1).map(|i| chars[i]);
                let after = chars.get(at + 1).copied();
                let inside = before.is_some_and(char::is_alphanumeric)
                    && after.is_some_and(char::is_alphanumeric);
                if inside {
                    out.push(char);
                }
                at += 1;
            }
            '\\' if chars
                .get(at + 1)
                .is_some_and(|next| next.is_ascii_punctuation()) =>
            {
                out.push(chars[at + 1]);
                at += 2;
            }
            _ => {
                out.push(char);
                at += 1;
            }
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The comment on one plain line: its marker apart, then its first line
/// with words in it, without the markdown's marks.
pub fn comment_preview(body: &str) -> CommentPreview {
    let lead = comment_lead(body);
    // Prose first; a comment that is only code shows its first line.
    let mut fenced = false;
    let mut code = None;
    let mut text = None;
    for line in lead.body.lines() {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        let plain = if fenced {
            line.split_whitespace().collect::<Vec<_>>().join(" ")
        } else {
            plain_line(line)
        };
        if plain.is_empty() {
            continue;
        }
        if fenced {
            code.get_or_insert(plain);
        } else {
            text = Some(plain);
            break;
        }
    }
    let text = text.or(code).unwrap_or_default();
    CommentPreview {
        severity: lead.severity,
        label: lead.label,
        text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_with_its_words_is_read_apart() {
        let lead = comment_lead("🔴 **Bloqueante** — o `id` pode vir vazio.\n\nMais.");
        assert_eq!(lead.severity, Some(Severity::Blocking));
        assert_eq!(lead.label.as_deref(), Some("Bloqueante"));
        assert_eq!(lead.body, "o `id` pode vir vazio.\n\nMais.");

        let lead = comment_lead("♻️ **Reuso.** Re-implementa `parse_ms`.");
        assert_eq!(lead.severity, Some(Severity::Reuse));
        assert_eq!(lead.label.as_deref(), Some("Reuso"));
        assert_eq!(lead.body, "Re-implementa `parse_ms`.");

        let lead = comment_lead("⚠️ **Atenção:** a ordem muda.");
        assert_eq!(lead.severity, Some(Severity::Warning));
        assert_eq!(lead.label.as_deref(), Some("Atenção"));
        assert_eq!(lead.body, "a ordem muda.");
    }

    #[test]
    fn an_emoji_alone_or_a_word_alone_is_a_marker_too() {
        let lead = comment_lead("🟢 Pode extrair isso.");
        assert_eq!(lead.severity, Some(Severity::Suggestion));
        assert_eq!(lead.label, None);
        assert_eq!(lead.body, "Pode extrair isso.");

        let lead = comment_lead("**Nit:** espaço sobrando");
        assert_eq!(lead.severity, Some(Severity::Suggestion));
        assert_eq!(lead.label.as_deref(), Some("Nit"));
        assert_eq!(lead.body, "espaço sobrando");
    }

    #[test]
    fn a_comment_without_a_marker_comes_back_whole() {
        let body = "**Importante** ler isso\n";
        assert_eq!(
            comment_lead(body),
            CommentLead {
                severity: None,
                label: None,
                body: body.into(),
            }
        );
        assert_eq!(comment_lead("🚀 legal").severity, None);
    }

    #[test]
    fn a_line_loses_its_marks_and_keeps_its_words() {
        assert_eq!(
            plain_line("**Reuso.** Re-implementa `parse_ms` em _outro_ lugar"),
            "Reuso. Re-implementa parse_ms em outro lugar"
        );
        assert_eq!(
            plain_line("- veja [o doc](https://x.dev/a_b) e ~~isso~~ <br/>agora"),
            "veja o doc e isso agora"
        );
        assert_eq!(plain_line("> ## `snake_case` fica"), "snake_case fica");
        assert_eq!(plain_line("1. use ``a`b`` aqui"), "use a`b aqui");
        assert_eq!(plain_line("a \\* b"), "a * b");
    }

    #[test]
    fn the_preview_is_one_plain_line_after_the_marker() {
        let preview = comment_preview(
            "♻️ **Reuso.** Re-implementa `relative_time` que já existe em **data/time.rs**.\n\nOutra linha",
        );
        assert_eq!(preview.severity, Some(Severity::Reuse));
        assert_eq!(preview.label.as_deref(), Some("Reuso"));
        assert_eq!(
            preview.text,
            "Re-implementa relative_time que já existe em data/time.rs."
        );
        let preview = comment_preview("\n```rust\nlet x = 1;\n```\nPor que `x`?");
        assert_eq!(preview.severity, None);
        assert_eq!(preview.text, "Por que x?");
        assert_eq!(comment_preview("```\nlet x = 1;\n```").text, "let x = 1;");
        assert!(!comment_preview("**a** `b`").text.contains(['*', '`']));
    }
}
