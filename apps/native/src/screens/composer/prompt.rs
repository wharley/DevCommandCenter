//! The prompt the composer sends, from the field's text and chips
//! (`$extractComposerPrompt`): an attached or mentioned file and an image
//! become `@path` with a space before, a long paste its full text on its own
//! line. Drafts are kept the same way and read back with `promptParts`.

use std::path::Path;

use crate::data::prompt::{PromptPart, is_image_path, prompt_parts};
use crate::ui::text_area::{CHIP, Chip, Part};

pub fn serialize(text: &str, chips: &[(usize, Chip)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous = 0;
    for (offset, chip) in chips {
        out.push_str(&text[previous..*offset]);
        match chip {
            Chip::File(path) | Chip::Image(path) => {
                if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('\n') {
                    out.push(' ');
                }
                out.push('@');
                out.push_str(path);
            }
            Chip::Snippet(body) => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(body);
            }
        }
        previous = offset + CHIP.len_utf8();
    }
    out.push_str(&text[previous..]);
    collapse_blank_lines(&out).trim().to_string()
}

fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut newlines = 0;
    for c in text.chars() {
        if c == '\n' {
            newlines += 1;
            if newlines > 2 {
                continue;
            }
        } else {
            newlines = 0;
        }
        out.push(c);
    }
    out
}

/// A saved draft back into text and chips.
pub fn parts_of(draft: &str) -> Vec<Part> {
    prompt_parts(draft)
        .into_iter()
        .map(|part| match part {
            PromptPart::Text(text) => Part::Text(text),
            PromptPart::Attachment(path) if is_image_path(&path) => Part::Chip(Chip::Image(path)),
            PromptPart::Attachment(path) | PromptPart::Mention(path) => {
                Part::Chip(Chip::File(path))
            }
        })
        .collect()
}

/// `pathRelativeToWorkspace`: inside the workspace the path the agent sees
/// is relative; anywhere else it stays absolute.
pub fn relative_to_workspace(root: Option<&str>, path: &Path) -> String {
    let absolute = path.to_string_lossy().replace('\\', "/");
    let Some(root) = root else {
        return absolute;
    };
    let root = root.replace('\\', "/");
    let root = root.trim_end_matches('/');
    if absolute == root {
        return String::new();
    }
    match absolute.strip_prefix(&format!("{root}/")) {
        Some(relative) => relative.to_string(),
        None => absolute,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chips_become_paths_and_pastes() {
        let text = format!("ver{CHIP}e{CHIP} agora{CHIP}fim");
        let first = 3;
        let second = first + CHIP.len_utf8() + 1;
        let third = second + CHIP.len_utf8() + " agora".len();
        let chips = vec![
            (first, Chip::File("src/app.rs".into())),
            (second, Chip::Image("/tmp/dcc_img_1.png".into())),
            (third, Chip::Snippet("linha 1\nlinha 2".into())),
        ];
        assert_eq!(
            serialize(&text, &chips),
            "ver @src/app.rse @/tmp/dcc_img_1.png agora\nlinha 1\nlinha 2fim"
        );
        assert_eq!(serialize("a\n\n\n\nb", &[]), "a\n\nb");
    }

    #[test]
    fn drafts_come_back_as_chips() {
        let parts = parts_of("olha @/Users/me/shot.png e @src/app.rs");
        assert_eq!(
            parts[1],
            Part::Chip(Chip::Image("/Users/me/shot.png".into()))
        );
        assert_eq!(parts[3], Part::Chip(Chip::File("src/app.rs".into())));
    }

    #[test]
    fn paths_inside_the_workspace_go_relative() {
        let root = Some("/repo/task");
        assert_eq!(
            relative_to_workspace(root, Path::new("/repo/task/src/a.rs")),
            "src/a.rs"
        );
        assert_eq!(
            relative_to_workspace(root, Path::new("/tmp/x.png")),
            "/tmp/x.png"
        );
        assert_eq!(relative_to_workspace(None, Path::new("/a/b")), "/a/b");
    }
}
