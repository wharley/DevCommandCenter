//! A link in the conversation that names a file of the task
//! (`parseWorkspaceFileReference` in `workspace-file-reference.ts`): a
//! relative path, a `file://` URL or an absolute path inside the checkout,
//! with an optional `:line[:column]` or `#L9C3`. Anything else — the web,
//! paths outside the checkout, `..` — is not a reference.

/// A file of the task and where to look in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileReference {
    /// Relative to the checkout, with `/`.
    pub path: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// `decodeURIComponent`, keeping the text when it is not valid.
fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            if let Some(byte) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(byte);
                index += 3;
                continue;
            }
            return value.to_string();
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| value.to_string())
}

/// `stripPosition`: `path#L9C3`, `path:12:3` or `path:12`.
fn strip_position(value: &str) -> (&str, Option<u32>, Option<u32>) {
    if let Some(at) = value.rfind("#L").or_else(|| value.rfind("#l")) {
        let rest = &value[at + 2..];
        let (line, column) = match rest.find(['C', 'c']) {
            Some(split) => (&rest[..split], Some(&rest[split + 1..])),
            None => (rest, None),
        };
        let digits = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
        if digits(line) && column.is_none_or(digits) {
            return (
                &value[..at],
                line.parse().ok(),
                column.and_then(|column| column.parse().ok()),
            );
        }
    }
    let number = |text: &str| -> Option<u32> {
        (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
            .then(|| text.parse().ok())
            .flatten()
    };
    let Some((head, last)) = value.rsplit_once(':') else {
        return (value, None, None);
    };
    let Some(last) = number(last) else {
        return (value, None, None);
    };
    match head
        .rsplit_once(':')
        .and_then(|(path, line)| number(line).map(|line| (path, line)))
    {
        Some((path, line)) => (path, Some(line), Some(last)),
        None => (head, Some(last), None),
    }
}

fn is_windows_absolute(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

fn is_absolute(value: &str) -> bool {
    value.starts_with('/') || is_windows_absolute(value)
}

/// A URL scheme (`https:`), not a Windows drive (`C:\`).
fn has_scheme(value: &str) -> bool {
    let Some(colon) = value.find(':') else {
        return false;
    };
    let scheme = &value[..colon];
    let mut chars = scheme.chars();
    let starts = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
    let rest = chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'));
    let drive = scheme.len() == 1 && matches!(value.as_bytes().get(colon + 1), Some(b'/' | b'\\'));
    starts && rest && !drive
}

/// `parseWorkspaceFileReference`
pub fn parse_file_reference(href: &str, root: &str) -> Option<FileReference> {
    let (root, href) = (root.trim(), href.trim());
    if root.is_empty() || href.is_empty() || href.starts_with('#') {
        return None;
    }
    let lower = href.to_ascii_lowercase();
    if has_scheme(href) && !lower.starts_with("file:") {
        return None;
    }
    let without_scheme = href.strip_prefix("file://").unwrap_or(href);
    let decoded = decode(without_scheme).replace('\\', "/");
    let (path, line, column) = strip_position(&decoded);
    let mut candidate = path.trim().to_string();
    if candidate.is_empty() {
        return None;
    }
    let root = root.replace('\\', "/").trim_end_matches('/').to_string();
    let windows_root = is_windows_absolute(&root);
    if windows_root && candidate.starts_with('/') && is_windows_absolute(&candidate[1..]) {
        candidate.remove(0);
    }
    let relative = if is_absolute(&candidate) {
        let (case_candidate, case_root) = if windows_root {
            (candidate.to_lowercase(), root.to_lowercase())
        } else {
            (candidate.clone(), root.clone())
        };
        let nested =
            case_candidate == case_root || case_candidate.starts_with(&format!("{case_root}/"));
        if !nested {
            return None;
        }
        candidate[root.len()..].trim_start_matches('/').to_string()
    } else {
        candidate
            .strip_prefix("./")
            .unwrap_or(&candidate)
            .to_string()
    };
    if relative.is_empty()
        || relative
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return None;
    }
    Some(FileReference {
        path: relative,
        line,
        column,
    })
}

/// `isLocalFileHref`: a file outside the checkout, opened by the system.
pub fn is_local_file_href(href: &str) -> bool {
    let value = href.trim();
    value.starts_with("file://") || value.starts_with('/') || {
        let bytes = value.as_bytes();
        bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'/' | b'\\')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "/Users/dev/project";

    fn reference(path: &str, line: Option<u32>, column: Option<u32>) -> Option<FileReference> {
        Some(FileReference {
            path: path.into(),
            line,
            column,
        })
    }

    #[test]
    fn parses_relative_paths_with_line_and_column() {
        assert_eq!(
            parse_file_reference("apps/desktop/src/App.tsx:42:8", ROOT),
            reference("apps/desktop/src/App.tsx", Some(42), Some(8))
        );
    }

    #[test]
    fn parses_absolute_file_links_inside_the_workspace() {
        assert_eq!(
            parse_file_reference("file:///Users/dev/project/src/main.rs%3A17", ROOT),
            reference("src/main.rs", Some(17), None)
        );
    }

    #[test]
    fn parses_github_style_line_fragments() {
        assert_eq!(
            parse_file_reference("./src/main.ts#L9C3", ROOT),
            reference("src/main.ts", Some(9), Some(3))
        );
    }

    #[test]
    fn supports_windows_workspace_paths() {
        assert_eq!(
            parse_file_reference("C:\\work\\dcc\\src\\main.ts:11:2", "C:\\work\\dcc"),
            reference("src/main.ts", Some(11), Some(2))
        );
    }

    #[test]
    fn rejects_external_urls_traversal_and_paths_outside_the_workspace() {
        assert_eq!(
            parse_file_reference("https://example.com/file.ts", ROOT),
            None
        );
        assert_eq!(parse_file_reference("../secret.txt", ROOT), None);
        assert_eq!(
            parse_file_reference("/Users/dev/project-other/file.ts", ROOT),
            None
        );
        assert_eq!(parse_file_reference("#anchor", ROOT), None);
    }

    #[test]
    fn a_mention_link_names_its_file() {
        assert_eq!(
            parse_file_reference("./src/app/(group)/page.tsx", ROOT),
            reference("src/app/(group)/page.tsx", None, None)
        );
        assert!(is_local_file_href("/etc/hosts"));
        assert!(is_local_file_href("file:///tmp/a"));
        assert!(!is_local_file_href("src/a.rs"));
    }
}
