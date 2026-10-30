//! HTML inside GitHub markdown (bot comments, PR bodies), the way Streamdown
//! with `rehype-harden` shows it: text and links stay, tags and comments go.
//! `<a href>` links its text, `<img alt>` shows the alt, `<br>` and block tags
//! break the line, `<b>`/`<strong>`, `<i>`/`<em>`, `<s>`/`<del>` and `<code>`
//! style. Anything else (`<sup>`, `<picture>`, `<relative-time>`…) keeps
//! only its text.

use std::sync::LazyLock;

use regex::Regex;

use super::parse::InlineBuilder;

/// What carries over between the pieces of HTML the parser hands over.
#[derive(Default)]
pub(super) struct Html {
    /// Inside `<!-- … -->`, which may span several pieces.
    comment: bool,
    code: bool,
    /// A block tag ended: the next text starts a new line.
    pending_break: bool,
}

impl Html {
    /// An HTML block ended: styles and links do not leak into what follows.
    pub(super) fn end_block(&mut self, inline: &mut InlineBuilder) {
        self.code = false;
        self.pending_break = false;
        inline.link = None;
        inline.style = Default::default();
    }
}

const BREAKS: &[&str] = &[
    "p",
    "div",
    "details",
    "summary",
    "li",
    "tr",
    "table",
    "ul",
    "ol",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "blockquote",
    "pre",
];

pub(super) fn push(html: &str, state: &mut Html, inline: &mut InlineBuilder) {
    let mut rest = html;
    while !rest.is_empty() {
        if state.comment {
            match rest.find("-->") {
                Some(end) => {
                    state.comment = false;
                    rest = &rest[end + 3..];
                    continue;
                }
                None => return,
            }
        }
        if let Some(after) = rest.strip_prefix("<!--") {
            state.comment = true;
            rest = after;
            continue;
        }
        if rest.starts_with('<')
            && let Some(end) = tag_end(rest)
        {
            tag(&rest[1..end], state, inline);
            rest = &rest[end + 1..];
            continue;
        }
        let next = rest[1..].find('<').map_or(rest.len(), |at| at + 1);
        text(&rest[..next], state, inline);
        rest = &rest[next..];
    }
}

/// The `>` closing a tag that starts `rest`, past quoted attribute values.
fn tag_end(rest: &str) -> Option<usize> {
    let mut quote = None;
    for (at, c) in rest.char_indices().skip(1) {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(open), c) if c == open => quote = None,
            (None, '>') => return Some(at),
            (None, '<') => return None,
            _ => {}
        }
    }
    None
}

fn attribute(inner: &str, name: &str) -> Option<String> {
    static ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i)([a-z-]+)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+))"#)
            .expect("attribute pattern")
    });
    ATTRIBUTE.captures_iter(inner).find_map(|found| {
        found[1].eq_ignore_ascii_case(name).then(|| {
            let value = found.get(2).or(found.get(3)).or(found.get(4));
            decode(value.map_or("", |value| value.as_str()))
        })
    })
}

fn tag(inner: &str, state: &mut Html, inline: &mut InlineBuilder) {
    let closing = inner.starts_with('/');
    let body = inner.trim_start_matches('/');
    let name: String = body
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect::<String>()
        .to_ascii_lowercase();
    match name.as_str() {
        "a" if closing => inline.link = None,
        "a" => {
            inline.link = attribute(body, "href").filter(|href| {
                ["http://", "https://", "mailto:"]
                    .iter()
                    .any(|scheme| href.starts_with(scheme))
            })
        }
        "img" => {
            if let Some(alt) = attribute(body, "alt").filter(|alt| !alt.trim().is_empty()) {
                text(&alt, state, inline);
            }
        }
        "br" => inline.push("\n", false),
        "b" | "strong" => inline.style.bold = !closing,
        "i" | "em" => inline.style.italic = !closing,
        "s" | "del" | "strike" => inline.style.strike = !closing,
        "code" => state.code = !closing,
        name if BREAKS.contains(&name) => state.pending_break = true,
        _ => {}
    }
}

fn text(raw: &str, state: &mut Html, inline: &mut InlineBuilder) {
    let decoded = decode(raw);
    let mut collapsed = String::with_capacity(decoded.len());
    for word in decoded.split_whitespace() {
        if !collapsed.is_empty() {
            collapsed.push(' ');
        }
        collapsed.push_str(word);
    }
    let current = &inline.inline.text;
    let at_start = current.is_empty() || current.ends_with(char::is_whitespace);
    if decoded.starts_with(char::is_whitespace) && !at_start && !state.pending_break {
        inline.push(" ", false);
    }
    if collapsed.is_empty() {
        return;
    }
    if state.pending_break && !inline.inline.text.trim().is_empty() {
        let trimmed = inline.inline.text.trim_end().len();
        inline.inline.text.truncate(trimmed);
        inline.push("\n", false);
    }
    state.pending_break = false;
    inline.push(&collapsed, state.code);
    if decoded.ends_with(char::is_whitespace) {
        inline.push(" ", false);
    }
}

/// The entities GitHub's HTML uses: named basics and numeric ones.
fn decode(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    static ENTITY: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"&(#[0-9]+|#[xX][0-9a-fA-F]+|[a-zA-Z]+);").expect("entity"));
    ENTITY
        .replace_all(text, |found: &regex::Captures| {
            let entity = &found[1];
            let number = if let Some(hex) = entity.strip_prefix("#x").or(entity.strip_prefix("#X"))
            {
                u32::from_str_radix(hex, 16).ok()
            } else {
                entity.strip_prefix('#').and_then(|dec| dec.parse().ok())
            };
            let named = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => None,
            };
            number
                .and_then(char::from_u32)
                .or(named)
                .map_or_else(|| found[0].to_string(), String::from)
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::super::parse::{Block, parse};

    fn paragraph(blocks: &[Block], index: usize) -> &super::super::parse::Inline {
        match &blocks[index] {
            Block::Paragraph(inline) => inline,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_bot_comment_keeps_its_text_and_links() {
        let blocks = parse(concat!(
            "<!-- vercel-comment -->\n",
            "The latest updates.\n\n",
            "| Project | Updated |\n|---|---|\n",
            "| <a href=\"https://vercel.com/z\"><sup><img src=\"x\" alt=\"\" /></sup></a> [zedy](https://z.dev) ",
            "| <relative-time datetime=\"2026\">Oct 29, 2026 3:48pm UTC</relative-time> |\n\n",
            "<a href=\"https://vercel.com/review?a=1&amp;b=2\" rel=\"noreferrer\"><picture>",
            "<source media=\"(prefers-color-scheme: dark)\" srcset=\"d.svg\">",
            "<img src=\"l.svg\" alt=\"Request Review\"></picture></a>\n",
        ));
        assert_eq!(paragraph(&blocks, 0).text, "The latest updates.");
        let Block::Table { rows, .. } = &blocks[1] else {
            panic!("{blocks:?}")
        };
        assert_eq!(rows[0][0].text.trim(), "zedy");
        assert_eq!(rows[0][1].text.trim(), "Oct 29, 2026 3:48pm UTC");
        let review = paragraph(&blocks, 2);
        assert_eq!(review.text, "Request Review");
        assert_eq!(
            review.spans[0].link.as_deref(),
            Some("https://vercel.com/review?a=1&b=2")
        );
    }

    #[test]
    fn breaks_styles_and_unsafe_links() {
        let blocks = parse(
            "<details><summary>Ver mais</summary>\n\nlinha<br>outra <b>forte</b> &lt;ok&gt;\n\n</details>\n\n<a href=\"javascript:alert(1)\">x</a>\n",
        );
        let text: Vec<String> = blocks
            .iter()
            .filter_map(|block| match block {
                Block::Paragraph(inline) => Some(inline.text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(text[0], "Ver mais");
        assert_eq!(text[1], "linha\noutra forte <ok>");
        let last = paragraph(&blocks, blocks.len() - 1);
        assert_eq!(last.text, "x");
        assert!(last.spans.iter().all(|span| span.link.is_none()));
    }
}
