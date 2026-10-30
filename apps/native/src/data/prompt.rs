//! `promptParts`: a sent prompt split back into its text, the files attached
//! from the composer (`@/absolute/path.ext`) and the workspace files mentioned
//! (`@relative/path.ext`), so the bubble shows chips instead of raw paths.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptPart {
    Text(String),
    Attachment(String),
    Mention(String),
}

/// Fenced blocks and inline code spans, where `@` is literal.
fn code_ranges(content: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    let mut open: Option<(usize, char, usize)> = None;
    for line in content.split_inclusive('\n') {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        let marker = trimmed.chars().next().filter(|c| *c == '`' || *c == '~');
        let run = marker.map_or(0, |c| trimmed.chars().take_while(|x| *x == c).count());
        match open {
            None if indent <= 3 && run >= 3 => open = Some((offset, marker.expect("run"), run)),
            Some((start, c, len))
                if indent <= 3
                    && marker == Some(c)
                    && run >= len
                    && trimmed.trim_end().chars().all(|x| x == c) =>
            {
                ranges.push((start, offset + line.len()));
                open = None;
            }
            _ => {}
        }
        offset += line.len();
    }
    if let Some((start, ..)) = open {
        ranges.push((start, content.len()));
    }
    let bytes = content.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'`'
            || ranges
                .iter()
                .any(|(from, to)| index >= *from && index < *to)
        {
            index += 1;
            continue;
        }
        let ticks = bytes[index..].iter().take_while(|b| **b == b'`').count();
        let fence = &content[index..index + ticks];
        let body = index + ticks;
        match content[body..].find(fence) {
            Some(found) if found > 0 && !content[body..body + 1].starts_with('\n') => {
                let end = body + found + ticks;
                ranges.push((index, end));
                index = end;
            }
            _ => index = body,
        }
    }
    ranges
}

fn is_extension_end(content: &str, dot: usize) -> Option<usize> {
    let ext: usize = content[dot + 1..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .count();
    if !(1..=12).contains(&ext) {
        return None;
    }
    // Any prefix of the run can end the path: the lazy match takes the first
    // one the lookahead accepts.
    (1..=ext).map(|len| dot + 1 + len).find(|&end| {
        let rest = &content[end..];
        let mut chars = rest.chars();
        match chars.next() {
            None => true,
            Some(c) if c.is_whitespace() || c == ')' => true,
            Some(',' | '.' | ';' | ':' | '!' | '?') => chars.next().is_none_or(char::is_whitespace),
            _ => false,
        }
    })
}

/// `@/abs/path.ext`, `@C:\path.ext`: the path may hold spaces, so it runs to
/// the first extension the lookahead accepts.
fn attachment_at(content: &str, at: usize) -> Option<usize> {
    let rest = &content[at + 1..];
    let absolute = rest.starts_with('/')
        || rest.starts_with("\\\\")
        || (rest.len() > 2
            && rest.as_bytes()[0].is_ascii_alphabetic()
            && rest.as_bytes()[1] == b':'
            && matches!(rest.as_bytes()[2], b'/' | b'\\'));
    if !absolute {
        return None;
    }
    for (offset, c) in rest.char_indices() {
        if matches!(c, '\r' | '\n' | '<' | '>') {
            return None;
        }
        if c == '.'
            && offset > 0
            && let Some(end) = is_extension_end(content, at + 1 + offset)
        {
            return Some(end);
        }
    }
    None
}

fn mention_at(content: &str, at: usize) -> Option<usize> {
    let before = content[..at].chars().next_back();
    if !before.is_none_or(|c| c.is_whitespace() || c == '(') {
        return None;
    }
    let rest = &content[at + 1..];
    if rest.starts_with('/') || rest.starts_with('\\') {
        return None;
    }
    let stop = |c: char| c.is_whitespace() || matches!(c, '@' | '<' | '>' | '`' | '"' | '\'');
    let mut previous: Option<char> = None;
    for (offset, c) in rest.char_indices() {
        if stop(c) {
            return None;
        }
        if c == '.'
            && previous.is_some_and(|p| p != '/' && p != '.')
            && let Some(end) = is_extension_end(content, at + 1 + offset)
        {
            return Some(end);
        }
        previous = Some(c);
    }
    None
}

pub fn prompt_parts(content: &str) -> Vec<PromptPart> {
    let code = code_ranges(content);
    let in_code = |index: usize| code.iter().any(|(from, to)| index >= *from && index < *to);
    let mut parts = Vec::new();
    let mut cursor = 0;
    let mut search = 0;
    while let Some(found) = content[search..].find('@') {
        let at = search + found;
        search = at + 1;
        let token = attachment_at(content, at)
            .map(|end| (end, true))
            .or_else(|| {
                (!in_code(at))
                    .then(|| mention_at(content, at))
                    .flatten()
                    .map(|end| (end, false))
            });
        let Some((end, attachment)) = token else {
            continue;
        };
        if at > cursor {
            parts.push(PromptPart::Text(content[cursor..at].to_string()));
        }
        let path = content[at + 1..end].to_string();
        parts.push(if attachment {
            PromptPart::Attachment(path)
        } else {
            PromptPart::Mention(path)
        });
        cursor = end;
        search = end;
    }
    if cursor < content.len() {
        parts.push(PromptPart::Text(content[cursor..].to_string()));
    }
    if parts.is_empty() {
        parts.push(PromptPart::Text(content.to_string()));
    }
    parts
}

/// The file's kind for its chip icon.
pub fn is_image_path(path: &str) -> bool {
    has_extension(
        path,
        &[
            "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "heic", "avif",
        ],
    )
}

/// `isAudioFilePath`: what the person's message plays in place.
pub fn is_audio_path(path: &str) -> bool {
    has_extension(
        path,
        &[
            "mp3", "m4a", "mp4a", "aac", "wav", "ogg", "oga", "opus", "flac", "aif", "aiff", "caf",
        ],
    )
}

fn has_extension(path: &str, extensions: &[&str]) -> bool {
    path.rsplit_once('.').is_some_and(|(_, ext)| {
        extensions
            .iter()
            .any(|candidate| ext.eq_ignore_ascii_case(candidate))
    })
}

pub fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_attachments_and_mentions() {
        let parts = prompt_parts("veja @/Users/me/My Shots/tela 1.png e @src/app.tsx, ok");
        assert_eq!(
            parts,
            vec![
                PromptPart::Text("veja ".into()),
                PromptPart::Attachment("/Users/me/My Shots/tela 1.png".into()),
                PromptPart::Text(" e ".into()),
                PromptPart::Mention("src/app.tsx".into()),
                PromptPart::Text(", ok".into()),
            ]
        );
    }

    #[test]
    fn leaves_handles_and_code_alone() {
        let text = "fala @someone, use @types/node e `@src/a.ts`";
        assert_eq!(prompt_parts(text), vec![PromptPart::Text(text.into())]);
        let fenced = "```\n@src/a.ts\n```\n";
        assert_eq!(prompt_parts(fenced), vec![PromptPart::Text(fenced.into())]);
    }
}
