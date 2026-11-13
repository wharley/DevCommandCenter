//! The code tab's layout (`dcc-pr-code`): reading, the error, no files;
//! else the files (208px, the drafts on each, +/−), the chosen file's
//! header and patch, and the review at the foot (`footer.rs`). Under 620px
//! the files sit over the patch.

use gpui::{
    AnyElement, Context, FontWeight, ScrollWheelEvent, SharedString, Window, div, prelude::*, px,
};

use super::rows::CODE_SIZE;
use crate::data::pr_hub::{Detail, Item};
use crate::data::pr_review::parse_patch;
use crate::i18n::{Args, Translate};
use crate::screens::pr_hub::PrHub;
use crate::theme::color::mix_srgb;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance, fonts};
use crate::ui::{Icon, Tooltip, Tracked, spinner, surface_edge};

/// `w-52`
const FILES: f32 = 208.0;

impl PrHub {
    /// `dcc-pr-detail`'s width, for its 620px container query.
    fn detail_width(&self) -> f32 {
        let width = self.width.get();
        if self.narrow() {
            width
        } else {
            width - (width * 0.3).clamp(285.0, 350.0)
        }
    }

    pub(in crate::screens::pr_hub) fn code_tab(
        &mut self,
        item: &Item,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let detail = match self.code_read(&item.id) {
            None => {
                return div()
                    .debug_selector(|| "pr-code-loading".into())
                    .flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .child(spinner("pr-code-loading", 20.0))
                    .into_any_element();
            }
            Some(Err(error)) => {
                return div()
                    .flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .p(px(32.0))
                    .child(
                        div()
                            .max_w(px(448.0))
                            .rounded(px(12.0))
                            .border_1()
                            .border_color(tw::RED_500.hsla().opacity(0.2))
                            .bg(tw::RED_500.hsla().opacity(0.05))
                            .p(px(16.0))
                            .text_size(px(12.0))
                            .text_color(tw::RED_500.hsla())
                            .child(error.clone()),
                    )
                    .into_any_element();
            }
            Some(Ok(detail)) => detail.clone(),
        };
        if detail.files.is_empty() {
            return div()
                .debug_selector(|| "pr-code-empty".into())
                .flex()
                .flex_col()
                .size_full()
                .items_center()
                .justify_center()
                .child(
                    Icon::new("file-code-corner")
                        .size(28.0)
                        .color(colors.muted_foreground.opacity(0.6)),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t("pullRequests.code.noFiles")),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("pullRequests.code.noFilesHint")),
                )
                .into_any_element();
        }
        let narrow = self.detail_width() < 620.0;
        let diff_width = if narrow {
            self.detail_width()
        } else {
            self.detail_width() - FILES
        };
        let body = div()
            .flex()
            .flex_1()
            .min_h_0()
            .when(narrow, |body| body.flex_col())
            .child(self.files_list(&detail, narrow, cx))
            .child(self.patch(&detail, diff_width, window, cx));
        div()
            .debug_selector(|| "pr-code".into())
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .children(self.since_strip(item, &detail, cx))
            .children(self.agent_section(cx))
            .child(body)
            .child(self.review_footer(item, &detail, narrow, window, cx))
            .into_any_element()
    }

    /// The chosen file, or the first when it is gone.
    pub(crate) fn current_file<'a>(
        &self,
        detail: &'a Detail,
    ) -> Option<&'a crate::data::pr_review::File> {
        self.review
            .path
            .as_deref()
            .and_then(|path| detail.files.iter().find(|file| file.path == path))
            .or_else(|| detail.files.first())
    }

    fn files_list(&self, detail: &Detail, narrow: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let colors = &theme.colors;
        let dark = theme.appearance == Appearance::Dark;
        let current = self.current_file(detail).map(|file| file.path.clone());
        let talks = crate::data::pr_review::conversations_by_file(&detail.inline_comments);
        let rows: Vec<AnyElement> = detail
            .files
            .iter()
            .enumerate()
            .filter(|(_, file)| self.shows_file(file, current.as_deref()))
            .map(|(index, file)| {
                let chosen = current.as_deref() == Some(file.path.as_str());
                let drafts = self
                    .review
                    .drafts
                    .iter()
                    .filter(|draft| draft.path == file.path)
                    .count();
                let path = file.path.clone();
                let selector = format!("pr-file-{index}");
                let pill = (drafts > 0).then(|| {
                    let tip =
                        cx.t_with("pullRequests.code.fileDrafts", &Args::count(drafts as i64));
                    Tooltip::new(
                        SharedString::from(format!("pr-file-drafts-{index}")),
                        div()
                            .flex_none()
                            .rounded_full()
                            .bg(tw::AMBER_500.hsla().opacity(0.15))
                            .px(px(6.0))
                            .py(px(2.0))
                            .text_size(px(10.0))
                            .font_weight(FontWeight::MEDIUM)
                            .font_features(crate::ui::tabular_nums())
                            .text_color(if dark {
                                tw::AMBER_300.hsla()
                            } else {
                                tw::AMBER_700.hsla()
                            })
                            .child(drafts.to_string()),
                        move |_, _| div().child(tip.clone()).into_any_element(),
                    )
                    .into_any_element()
                });
                // Its published conversations: blue while any is open.
                let talk = talks.get(&file.path).copied().map(|talk| {
                    let parts: Vec<String> = [
                        (talk.open, "pullRequests.code.fileConversationsOpen"),
                        (talk.resolved, "pullRequests.code.fileConversationsResolved"),
                    ]
                    .into_iter()
                    .filter(|(count, _)| *count > 0)
                    .map(|(count, key)| cx.t_with(key, &Args::count(count as i64)).to_string())
                    .collect();
                    let tip = parts.join(" · ");
                    let (shown, open) = if talk.open > 0 {
                        (talk.open, true)
                    } else {
                        (talk.resolved, false)
                    };
                    let sky = if dark {
                        tw::SKY_300.hsla()
                    } else {
                        tw::SKY_700.hsla()
                    };
                    let muted = colors.muted_foreground;
                    let talk_selector = format!("pr-file-talks-{index}");
                    Tooltip::new(
                        SharedString::from(talk_selector.clone()),
                        div()
                            .debug_selector(move || talk_selector)
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(px(3.0))
                            .rounded_full()
                            .px(px(6.0))
                            .py(px(2.0))
                            .text_size(px(10.0))
                            .font_weight(FontWeight::MEDIUM)
                            .font_features(crate::ui::tabular_nums())
                            .map(|pill| {
                                if open {
                                    pill.bg(tw::SKY_500.hsla().opacity(0.15)).text_color(sky)
                                } else {
                                    pill.text_color(muted)
                                }
                            })
                            .child(Icon::new("message-square").size(10.0))
                            .child(shown.to_string()),
                        move |_, _| div().child(tip.clone()).into_any_element(),
                    )
                    .into_any_element()
                });
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector)
                    .flex()
                    .w_full()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(8.0))
                    .border_1()
                    .px(px(7.0))
                    .py(px(7.0))
                    .text_size(px(11.0))
                    .cursor_pointer()
                    .map(|row| {
                        if chosen {
                            row.bg(mix_srgb(colors.accent_calm, 0.08, colors.background))
                                .border_color(surface_edge(&theme))
                                .text_color(colors.foreground)
                        } else {
                            let hover = colors.accent.opacity(0.5);
                            let fg = colors.foreground;
                            row.border_color(gpui::transparent_black())
                                .text_color(colors.muted_foreground)
                                .hover(move |row| row.bg(hover).text_color(fg))
                        }
                    })
                    .on_click(cx.listener(move |hub, _, _, cx| hub.choose_file(path.clone(), cx)))
                    .child(
                        Icon::new("file-code-corner")
                            .size(14.0)
                            .when(chosen, |icon| icon.color(colors.accent_calm)),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(file.path.clone()))
                    .children(talk)
                    .children(pill)
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .gap(px(3.0))
                            .text_size(px(9.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(
                                div()
                                    .text_color(tw::EMERALD_500.hsla())
                                    .child(format!("+{}", file.additions)),
                            )
                            .child(
                                div()
                                    .text_color(tw::RED_500.hsla())
                                    .child(format!("−{}", file.deletions)),
                            ),
                    )
                    .child(self.viewed_toggle(index, file, cx))
                    .into_any_element()
            })
            .collect();
        div()
            .id("pr-code-files")
            .debug_selector(|| "pr-code-files".into())
            .flex_none()
            .overflow_y_scroll()
            .bg(mix_srgb(colors.sidebar, 0.35, colors.background))
            .p(px(8.0))
            .map(|files| {
                if narrow {
                    files
                        .w_full()
                        .max_h(px(130.0))
                        .border_b_1()
                        .border_color(surface_edge(&theme))
                } else {
                    files
                        .w(px(FILES))
                        .h_full()
                        .border_r_1()
                        .border_color(colors.border.opacity(0.7))
                }
            })
            .child(
                div()
                    .px(px(8.0))
                    .pb(px(8.0))
                    .pt(px(4.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(colors.muted_foreground)
                    .child(Tracked::new(
                        cx.t_with(
                            "pullRequests.code.changedFiles",
                            &Args::count(detail.files.len() as i64),
                        )
                        .to_uppercase(),
                        10.0,
                        0.12,
                    )),
            )
            .children(self.viewed_summary(detail, cx))
            .child(div().flex().flex_col().gap(px(2.0)).children(rows))
            .into_any_element()
    }

    fn patch(
        &self,
        detail: &Detail,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let Some(file) = self.current_file(detail).cloned() else {
            return div().flex_1().into_any_element();
        };
        let rows = parse_patch(file.patch.as_deref());
        let status_key = format!("pullRequests.code.fileStatuses.{}", file.status);
        let status = {
            let text = cx.t(&status_key);
            if text.as_ref() == status_key {
                file.status.clone()
            } else {
                text.to_string()
            }
        };
        // How far the longest line can move left.
        let system = window.text_system();
        let cell = system
            .advance(
                system.resolve_font(&gpui::font(fonts::MONO)),
                px(CODE_SIZE),
                '0',
            )
            .map(|size| f32::from(size.width))
            .unwrap_or(6.6);
        let longest = rows
            .iter()
            .map(|row| row.content.chars().count())
            .max()
            .unwrap_or(0) as f32
            * cell;
        let room = (width - 28.0 - 84.0 - 16.0 - 2.0 * cell).max(0.0);
        let limit = (longest - room).max(0.0);
        let lines = if rows.is_empty() {
            vec![
                div()
                    .flex()
                    .h(px(192.0))
                    .items_center()
                    .justify_center()
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("pullRequests.code.patchUnavailable"))
                    .into_any_element(),
            ]
        } else {
            self.patch_rows(&file, &rows, detail, width, window, cx)
        };
        let lines: Vec<AnyElement> = self
            .outdated_block(&file, detail, width, window, cx)
            .into_iter()
            .chain(lines)
            .collect();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .bg(colors.background)
            .child(
                div()
                    .debug_selector(|| "pr-code-file-header".into())
                    .flex()
                    .flex_none()
                    .h(px(36.0))
                    .items_center()
                    .border_b_1()
                    .border_color(colors.border)
                    .bg(colors.background.opacity(0.95))
                    .px(px(12.0))
                    .text_size(px(11.0))
                    .child(
                        div().mr(px(8.0)).child(
                            Icon::new("file-code-corner")
                                .size(14.0)
                                .color(colors.muted_foreground),
                        ),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(file.path.clone()),
                    )
                    .child(
                        div()
                            .ml(px(12.0))
                            .flex_none()
                            .text_size(px(10.0))
                            .text_color(colors.muted_foreground)
                            .child(status),
                    ),
            )
            .child(
                div()
                    .id("pr-code-patch")
                    .debug_selector(|| "pr-code-patch".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .on_scroll_wheel(cx.listener(
                        move |hub, event: &ScrollWheelEvent, window, cx| {
                            let delta = event.delta.pixel_delta(window.line_height());
                            let dx = f32::from(delta.x);
                            if dx.abs() > f32::from(delta.y).abs() {
                                hub.review.scroll_x = (hub.review.scroll_x - dx).clamp(0.0, limit);
                                cx.stop_propagation();
                                cx.notify();
                            }
                        },
                    ))
                    .children(lines),
            )
            .into_any_element()
    }
}
