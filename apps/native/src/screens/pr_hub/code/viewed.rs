//! "Visto" on a pull request's files: the person ticks what they have
//! looked at, the list says how many are left and can leave the seen ones
//! out. A file whose diff changed since it was ticked asks to be looked at
//! again. Kept with the review being written, on this computer only: the
//! forge's own "viewed" marks are neither read nor written.

use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use crate::data::pr_hub::Detail;
use crate::data::pr_review::File;
use crate::data::pr_review_drafts::patch_mark;
use crate::i18n::{Args, Translate};
use crate::screens::pr_hub::PrHub;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Icon, Tooltip};

/// Seen, and its diff is still the one that was seen.
pub(crate) fn seen(viewed: &std::collections::BTreeMap<String, String>, file: &File) -> bool {
    viewed.get(&file.path) == Some(&patch_mark(file.patch.as_deref()))
}

impl PrHub {
    pub(super) fn is_viewed(&self, file: &File) -> bool {
        seen(&self.review.viewed, file)
    }

    /// The file on screen always shows, seen or not.
    pub(super) fn shows_file(&self, file: &File, chosen: Option<&str>) -> bool {
        !self.review.hide_viewed || chosen == Some(file.path.as_str()) || !self.is_viewed(file)
    }

    pub(crate) fn toggle_viewed(&mut self, file: &File, cx: &mut Context<Self>) {
        if self.is_viewed(file) {
            self.review.viewed.remove(&file.path);
        } else {
            self.review
                .viewed
                .insert(file.path.clone(), patch_mark(file.patch.as_deref()));
        }
        self.keep_review(cx);
        cx.notify();
    }

    /// The row's tick.
    pub(super) fn viewed_toggle(
        &self,
        index: usize,
        file: &File,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let on = self.is_viewed(file);
        let toggled = file.clone();
        let key = if on {
            "pullRequests.code.viewed.unmark"
        } else {
            "pullRequests.code.viewed.mark"
        };
        Tooltip::text(
            ("pr-file-viewed-tip", index),
            div()
                .id(("pr-file-viewed", index))
                .debug_selector(move || format!("pr-file-viewed-{index}"))
                .flex()
                .flex_none()
                .size(px(18.0))
                .items_center()
                .justify_center()
                .rounded(px(5.0))
                .border_1()
                .map(|tick| {
                    if on {
                        tick.border_color(tw::EMERALD_500.hsla())
                            .bg(tw::EMERALD_500.alpha(0.15))
                            .text_color(tw::EMERALD_500.hsla())
                    } else {
                        tick.border_color(colors.border)
                            .text_color(gpui::transparent_black())
                    }
                })
                .cursor_pointer()
                .on_click(cx.listener(move |hub, _, _, cx| {
                    // The row under it opens the file: not this click.
                    cx.stop_propagation();
                    hub.toggle_viewed(&toggled, cx);
                }))
                .child(Icon::new("check").size(11.0).stroke(2.6)),
            cx.t(key),
        )
        .into_any_element()
    }

    /// Under the list's title: how many were seen, and leaving them out.
    pub(super) fn viewed_summary(
        &self,
        detail: &Detail,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let seen = detail
            .files
            .iter()
            .filter(|file| self.is_viewed(file))
            .count();
        if seen == 0 {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let hide = self.review.hide_viewed;
        let label: SharedString = cx.t_with(
            "pullRequests.code.viewed.count",
            &Args::default()
                .with("seen", seen.to_string())
                .with("total", detail.files.len().to_string()),
        );
        Some(
            div()
                .debug_selector(|| "pr-files-viewed".into())
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .pb(px(8.0))
                .text_size(px(10.5))
                .text_color(colors.muted_foreground)
                .child(div().flex_1().min_w_0().truncate().child(label))
                .child(
                    div()
                        .id("pr-files-hide-viewed")
                        .debug_selector(|| "pr-files-hide-viewed".into())
                        .flex_none()
                        .rounded(px(5.0))
                        .px(px(5.0))
                        .py(px(1.0))
                        .cursor_pointer()
                        .when(hide, |button| {
                            button.bg(colors.muted).text_color(colors.foreground)
                        })
                        .hover(|button| button.bg(colors.muted.opacity(0.7)))
                        .on_click(cx.listener(|hub, _, _, cx| {
                            hub.review.hide_viewed = !hub.review.hide_viewed;
                            cx.notify();
                        }))
                        .child(cx.t(if hide {
                            "pullRequests.code.viewed.show"
                        } else {
                            "pullRequests.code.viewed.hide"
                        })),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_stays_seen_only_while_its_diff_is_the_one_seen() {
        let file = |patch: &str| File {
            path: "src/a.rs".into(),
            patch: Some(patch.into()),
            ..File::default()
        };
        let mut viewed = std::collections::BTreeMap::new();
        assert!(!seen(&viewed, &file("+a")));
        viewed.insert("src/a.rs".to_string(), patch_mark(Some("+a")));
        assert!(seen(&viewed, &file("+a")));
        // New commits changed it: to look at again.
        assert!(!seen(&viewed, &file("+a\n+b")));
    }
}
