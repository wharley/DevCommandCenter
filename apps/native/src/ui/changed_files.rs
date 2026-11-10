//! A short list of changed files (Compozy's "Edited N files"): monospace,
//! the folder dimmed, the file name in the foreground, `+N −M` at the end.

use gpui::{AnyElement, App, Hsla, IntoElement, SharedString, div, prelude::*, px};

use crate::theme::{ActiveTheme, fonts};
use crate::ui::tabular_nums;

/// `src/ui/a.rs` → (`src/ui/`, `a.rs`); a root file has no folder.
pub fn split_path(path: &str) -> (&str, &str) {
    let path = path.trim_end_matches('/');
    match path.rfind('/') {
        Some(at) => path.split_at(at + 1),
        None => ("", path),
    }
}

/// One row per file; `id` prefixes each row's selector (`{id}-{n}`).
pub fn changed_files<'a>(
    id: &str,
    files: impl IntoIterator<Item = (&'a str, u64, u64)>,
    (added, removed): (Hsla, Hsla),
    cx: &App,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    div()
        .flex()
        .flex_col()
        .gap(px(1.0))
        .font_family(fonts::MONO)
        .text_size(px(11.0))
        .line_height(px(17.0))
        .children(
            files
                .into_iter()
                .enumerate()
                .map(|(n, (path, additions, deletions))| {
                    let (folder, name) = split_path(path);
                    let selector = format!("{id}-{n}");
                    div()
                        .debug_selector(move || selector.clone())
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .min_w_0()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(
                                    div()
                                        .flex_shrink(1.0)
                                        .min_w_0()
                                        .truncate()
                                        .text_color(colors.muted_foreground.opacity(0.75))
                                        .child(SharedString::from(folder.to_string())),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .text_color(colors.foreground)
                                        .child(SharedString::from(name.to_string())),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_none()
                                .gap(px(4.0))
                                .font_features(tabular_nums())
                                .child(div().text_color(added).child(format!("+{additions}")))
                                .child(div().text_color(removed).child(format!("−{deletions}"))),
                        )
                }),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::split_path;

    #[test]
    fn the_folder_and_the_name() {
        assert_eq!(split_path("src/ui/a.rs"), ("src/ui/", "a.rs"));
        assert_eq!(split_path("README.md"), ("", "README.md"));
        assert_eq!(split_path("a/b/"), ("a/", "b"));
        assert_eq!(split_path(""), ("", ""));
    }
}
