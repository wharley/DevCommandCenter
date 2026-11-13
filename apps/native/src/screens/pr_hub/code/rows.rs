//! A patch's rows (`DiffRow`): the hunk headers, the lines with their
//! numbers and the + to comment on one, the conversations on the line
//! (reply, resolve), the comment drafted there, and the card that writes
//! it. The code moves sideways under the wheel; the conversations stay put,
//! as the desktop keeps them `sticky` at the left.

use gpui::{AnyElement, BoxShadow, Context, FontWeight, Hsla, div, point, prelude::*, px};

use crate::data::pr_review::{DraftComment, File, PatchRow, RowKind, draft_key};
use crate::i18n::{Args, Translate};
use crate::screens::pr_hub::PrHub;
use crate::theme::color::mix_lab;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance, fonts};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tracked, surface_edge};

/// `leading-6`
pub(super) const LINE: f32 = 24.0;
pub(super) const CODE_SIZE: f32 = 11.0;
const LINE_GROUP: &str = "pr-code-line";

fn dark(cx: &gpui::App) -> bool {
    cx.theme().appearance == Appearance::Dark
}

/// `dcc-code-inline-thread`: 12px in, 14px padding, 520px at most.
pub(super) fn thread_card(width: f32, cx: &gpui::App) -> gpui::Div {
    div()
        .m(px(12.0))
        .w(px(520.0_f32.min(width - 24.0).max(0.0)))
        .p(px(14.0))
        .border_1()
        .border_color(surface_edge(cx.theme()))
        .rounded(px(12.0))
        .bg(cx.theme().colors.card)
}

/// `dcc-code-line-plus`
fn line_plus(id: String, cx: &mut Context<PrHub>) -> gpui::Stateful<gpui::Div> {
    let colors = &cx.theme().colors;
    let calm = colors.accent_calm;
    let background = colors.background;
    let selector = id.clone();
    div()
        .id(gpui::SharedString::from(id))
        .debug_selector(move || selector)
        .flex()
        .size(px(20.0))
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .border_1()
        .border_color(mix_lab(calm, 0.35, colors.border))
        .bg(colors.popover)
        .text_color(calm)
        .shadow(vec![BoxShadow {
            color: gpui::black().opacity(0x18 as f32 / 255.0),
            offset: point(px(0.0), px(2.0)),
            blur_radius: px(6.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        .cursor_pointer()
        .opacity(0.0)
        .group_hover(LINE_GROUP, |style| style.opacity(1.0))
        .hover(move |style| style.bg(calm).text_color(background))
        .child(Icon::new("plus").size(14.0).stroke(2.2))
}

impl PrHub {
    /// Every row of `file`'s patch, with what hangs under each line.
    pub(super) fn patch_rows(
        &self,
        file: &File,
        rows: &[PatchRow],
        detail: &crate::data::pr_hub::Detail,
        width: f32,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.theme().colors.clone();
        let dark = dark(cx);
        let mut out = Vec::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            match row.kind {
                RowKind::Hunk => {
                    let sky = tw::SKY_500.hsla();
                    out.push(
                        div()
                            .border_y_1()
                            .border_color(sky.opacity(0.15))
                            .bg(sky.opacity(0.07))
                            .px(px(12.0))
                            .py(px(4.0))
                            .font_family(fonts::MONO)
                            .text_size(px(10.0))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_color(if dark {
                                tw::SKY_400.hsla()
                            } else {
                                tw::SKY_600.hsla()
                            })
                            .child(row.content.clone())
                            .into_any_element(),
                    );
                    continue;
                }
                RowKind::Meta => {
                    out.push(
                        div()
                            .px(px(12.0))
                            .py(px(8.0))
                            .font_family(fonts::MONO)
                            .text_size(px(10.0))
                            .italic()
                            .text_color(colors.muted_foreground)
                            .child(if row.content.is_empty() {
                                " ".to_string()
                            } else {
                                row.content.clone()
                            })
                            .into_any_element(),
                    );
                    continue;
                }
                _ => {}
            }
            out.push(self.code_line(file, row, index, cx));
            let Some((line, side)) = row.review else {
                continue;
            };
            let key = draft_key(&file.path, line, side);
            // An outdated thread's line is where it was: it shows apart.
            for comment in detail.inline_comments.iter().filter(|comment| {
                comment.path == file.path
                    && !comment.outdated
                    && comment.line == Some(line)
                    && comment.side.as_deref() == Some(side)
            }) {
                let mark = super::since::ThreadMark::of(comment, detail);
                out.push(self.thread(comment, mark, width, window, cx));
            }
            if let Some(draft) = self.review.drafts.iter().find(|draft| draft.key() == key) {
                out.push(self.draft_card(draft, width, window, cx));
            }
            if self.review.editing.as_deref() == Some(key.as_str()) {
                out.push(self.line_editor(&file.path, row, width, cx));
            }
        }
        out
    }

    /// `grid-cols-[28px_42px_42px_1fr]`: the +, the old and new numbers, the code.
    fn code_line(
        &self,
        file: &File,
        row: &PatchRow,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let edge = colors.border.opacity(0.3);
        let (green, red) = (tw::EMERALD_500.hsla(), tw::RED_500.hsla());
        let number = |value: Option<u32>| {
            div()
                .w(px(42.0))
                .flex_none()
                .border_r_1()
                .border_color(edge)
                .px(px(8.0))
                .flex()
                .justify_end()
                .text_color(colors.muted_foreground.opacity(0.6))
                .child(value.map(|value| value.to_string()).unwrap_or_default())
        };
        let plus = row.review.map(|(line, side)| {
            let path = file.path.clone();
            line_plus(format!("pr-line-plus-{index}"), cx)
                .on_click(cx.listener(move |hub, _, window, cx| {
                    hub.start_line_comment(&path, line, side, window, cx)
                }))
                .into_any_element()
        });
        let (marker, marker_color): (&str, Option<Hsla>) = match row.kind {
            RowKind::Addition => ("+", Some(green)),
            RowKind::Deletion => ("−", Some(red)),
            _ => (" ", None),
        };
        div()
            .group(LINE_GROUP)
            .flex()
            .min_h(px(LINE))
            .border_b_1()
            .border_color(colors.border.opacity(0.25))
            .font_family(fonts::MONO)
            .text_size(px(CODE_SIZE))
            .line_height(px(LINE))
            .when(row.kind == RowKind::Addition, |line| {
                line.bg(green.opacity(0.08))
            })
            .when(row.kind == RowKind::Deletion, |line| {
                line.bg(red.opacity(0.08))
            })
            .child(
                div()
                    .w(px(28.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_r_1()
                    .border_color(edge)
                    .children(plus),
            )
            .child(number(row.old))
            .child(number(row.new))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .px(px(8.0))
                    .child(
                        div()
                            .flex()
                            .ml(px(-self.review.scroll_x))
                            .whitespace_nowrap()
                            .text_color(colors.foreground.opacity(0.9))
                            .child(
                                div()
                                    .mr(px(8.0))
                                    .when_some(marker_color, |marker, color| {
                                        marker.text_color(color)
                                    })
                                    .child(marker),
                            )
                            .child(row.content.clone()),
                    ),
            )
            .into_any_element()
    }

    /// "Comentário pendente", with its trash.
    fn draft_card(
        &self,
        draft: &DraftComment,
        width: f32,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let idea = colors.accent_idea;
        let amber = if dark(cx) {
            tw::AMBER_400.hsla()
        } else {
            tw::AMBER_600.hsla()
        };
        let key = draft.key();
        let selector = format!("pr-draft-{key}");
        thread_card(width, cx)
            .debug_selector(move || selector)
            .border_color(mix_lab(idea, 0.30, colors.border))
            .bg(mix_lab(idea, 0.04, colors.card))
            .flex()
            .items_start()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(amber)
                            .child(Tracked::new(
                                cx.t("pullRequests.code.pendingComment").to_uppercase(),
                                9.0,
                                0.12,
                            )),
                    )
                    // A comment on several lines replaces them all with its
                    // suggestion: which ones, under the label.
                    .children(draft.start_line.map(|from| {
                        div()
                            .mt(px(2.0))
                            .text_size(px(10.5))
                            .text_color(colors.muted_foreground)
                            .child(
                                cx.t_with(
                                    "pullRequests.code.lines",
                                    &Args::default()
                                        .with("from", from.to_string())
                                        .with("to", draft.line.to_string()),
                                ),
                            )
                    }))
                    // As it will read on GitHub: markdown, a suggestion as code.
                    .child(div().mt(px(4.0)).min_w_0().child(
                        crate::screens::code_column::review::comment_view::comment_body(
                            &draft.body,
                            &format!("pr-draft-body-{key}"),
                            window,
                            cx,
                        ),
                    )),
            )
            .child(
                Button::new(gpui::SharedString::from(format!("pr-draft-remove-{key}")))
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconXs)
                    .on_click(cx.listener(move |hub, _, _, cx| hub.remove_draft(&key, cx)))
                    .child(Icon::new("trash-2").size(14.0)),
            )
            .into_any_element()
    }

    /// `dcc-code-annotation dcc-code-inline-card`: the comment on a line.
    fn line_editor(
        &self,
        path: &str,
        row: &PatchRow,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((line, side)) = row.review else {
            return div().into_any_element();
        };
        let preparing = self
            .agent_busy(cx)
            .is_some_and(|busy| busy.starts_with("inline:"));
        let (prepared_path, prepared_row) = (path.to_string(), row.clone());
        let prepare_tip = cx.t("pullRequests.code.prepareWithAgent");
        let prepare = crate::ui::Tooltip::new(
            "pr-line-prepare-tip",
            div().debug_selector(|| "pr-line-prepare".into()).child(
                Button::new("pr-line-prepare")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconXs)
                    .disabled(preparing)
                    .on_click(cx.listener(move |hub, _, _, cx| {
                        hub.prepare_inline(prepared_path.clone(), prepared_row.clone(), cx)
                    }))
                    .child(if preparing {
                        crate::ui::spinner("pr-line-preparing", 12.0).into_any_element()
                    } else {
                        Icon::new("sparkles").size(12.0).into_any_element()
                    }),
            ),
            move |_, _| div().child(prepare_tip.clone()).into_any_element(),
        )
        .side(crate::ui::tooltip::Side::Top);
        let colors = cx.theme().colors.clone();
        let calm = colors.accent_calm;
        let empty = self.review.draft.read(cx).text().trim().is_empty();
        let shadow = |alpha: f32, y: f32, blur: f32| BoxShadow {
            color: gpui::black().opacity(alpha),
            offset: point(px(0.0), px(y)),
            blur_radius: px(blur),
            spread_radius: px(0.0),
            inset: false,
        };
        let saved = path.to_string();
        div()
            .debug_selector(|| "pr-line-editor".into())
            .m(px(12.0))
            .w(px(396.0_f32.min(width - 24.0).max(0.0)))
            .p(px(14.0))
            .border_1()
            .border_color(surface_edge(cx.theme()))
            .rounded(px(12.0))
            .bg(colors.popover)
            .text_color(colors.popover_foreground)
            .shadow(vec![shadow(0.16, 24.0, 80.0), shadow(0.06, 4.0, 14.0)])
            .child(
                div()
                    .mb(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .size(px(32.0))
                            .items_center()
                            .justify_center()
                            .rounded(px(8.0))
                            .bg(calm.opacity(0.12))
                            .text_color(calm)
                            .child(Icon::new("message-square-plus").size(15.0)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(Tracked::new(
                                cx.t_with(
                                    "pullRequests.code.addInlineComment",
                                    &Args::default().with("line", line.to_string()),
                                ),
                                13.0,
                                -0.015,
                            )))
                            .child(
                                div()
                                    .font_family(fonts::MONO)
                                    .text_size(px(10.0))
                                    .line_height(gpui::relative(1.5))
                                    .text_color(colors.muted_foreground)
                                    .child(path.to_string()),
                            ),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "pr-line-editor-field".into())
                    .min_h(px(88.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(surface_edge(cx.theme()))
                    .bg(colors.background)
                    .px(px(12.0))
                    .py(px(8.0))
                    .text_size(px(12.0))
                    .line_height(gpui::relative(1.6))
                    .child(self.review.draft.clone()),
            )
            .child(self.range_row(path, line, side, cx))
            .child(
                div()
                    .mt(px(12.0))
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .gap(px(8.0))
                    .child(prepare)
                    .child(
                        Button::new("pr-line-cancel")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Xs)
                            .text_size(11.0)
                            .on_click(cx.listener(|hub, _, _, cx| hub.cancel_line_comment(cx)))
                            .child(cx.t("pullRequests.code.cancel")),
                    )
                    .child(
                        Button::new("pr-line-save")
                            .size(ButtonSize::Xs)
                            .text_size(11.0)
                            .disabled(empty)
                            .on_click(cx.listener(move |hub, _, _, cx| {
                                hub.save_line_comment(&saved, line, side, cx)
                            }))
                            .child(cx.t("pullRequests.code.addToReview")),
                    ),
            )
            .into_any_element()
    }
}
