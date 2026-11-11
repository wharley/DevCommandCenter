//! What a task answered, under its row of the room's report: a task that
//! changed nothing (a survey, an analysis) is its answer, so the room shows
//! it in place — a few lines, "Ler tudo" opening the rest — instead of a
//! card of zeros that sends the person into the task to read it. A task
//! with changes shows its answer's first lines above its files.

use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use crate::data::task_reports::TaskReport;
use crate::i18n::Translate;
use crate::markdown::{MarkdownStyle, render_markdown};
use crate::theme::ActiveTheme;

/// Lines shown before "Ler tudo": more when the answer is the whole result.
const LINES_ALONE: usize = 8;
const LINES_WITH_CHANGES: usize = 3;
/// And never more than this many characters folded.
const FOLDED_CHARS: usize = 900;

/// `text` cut to `lines` lines and `FOLDED_CHARS`, and whether it was cut.
pub(crate) fn folded(text: &str, lines: usize) -> (String, bool) {
    let text = text.trim();
    let mut kept: Vec<&str> = text.lines().take(lines).collect();
    while kept.last().is_some_and(|line| line.trim().is_empty()) {
        kept.pop();
    }
    let mut short = kept.join("\n");
    if short.chars().count() > FOLDED_CHARS {
        short = short.chars().take(FOLDED_CHARS).collect::<String>();
        short = format!("{}…", short.trim_end());
    }
    let cut = short.trim_end_matches('…').len() < text.len();
    (short, cut)
}

impl TeamRoom {
    pub(super) fn task_answer(
        &self,
        index: usize,
        row: usize,
        task: &TaskReport,
        changed: bool,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let answer = task.answer.as_deref()?.trim();
        if answer.is_empty() {
            return None;
        }
        let key = format!(
            "task-answer-{}",
            task.task_id.as_deref().unwrap_or(&task.title)
        );
        let open = self.opened_answers.borrow().contains(&key);
        let (shown, cut) = if open {
            (answer.to_string(), false)
        } else {
            folded(
                answer,
                if changed {
                    LINES_WITH_CHANGES
                } else {
                    LINES_ALONE
                },
            )
        };
        let colors = cx.theme().colors.clone();
        let mentions = self.list.model().mentions.clone();
        let blocks = self
            .markdown
            .borrow_mut()
            .get(&format!("{key}-{open}"), &shown, &mentions);
        let toggle = (cut || open).then(|| {
            let key = key.clone();
            div()
                .id(SharedString::from(format!(
                    "room-answer-more-{index}-{row}"
                )))
                .debug_selector(move || format!("room-answer-more-{index}-{row}"))
                .mt(px(4.0))
                .cursor_pointer()
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .hover(|toggle| toggle.text_color(colors.foreground))
                .on_click(cx.listener(move |room, _, _, cx| {
                    let mut opened = room.opened_answers.borrow_mut();
                    if !opened.remove(&key) {
                        opened.insert(key.clone());
                    }
                    drop(opened);
                    room.list.remeasure_row(index);
                    cx.notify();
                }))
                .child(if open {
                    cx.t("agents.report.readLess")
                } else {
                    cx.t("agents.report.readAll")
                })
        });
        Some(
            div()
                .debug_selector(move || format!("room-answer-text-{index}-{row}"))
                .mx(px(8.0))
                .mt(px(2.0))
                .mb(px(4.0))
                .pl(px(10.0))
                .border_l_2()
                .border_color(colors.border)
                .text_size(px(12.5))
                .text_color(colors.foreground)
                .child(render_markdown(
                    &blocks,
                    MarkdownStyle {
                        size: 12.5,
                        ..MarkdownStyle::reply()
                    },
                    &format!("room-{key}"),
                    window,
                    cx,
                ))
                .children(toggle)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::folded;

    #[test]
    fn an_answer_folds_to_its_first_lines() {
        let answer =
            "Confirmados 19 PRs.\n\n- #1001\n- #1002\n- #1003\n- #1004\n- #1005\n- #1006\n- #1007";
        let (short, cut) = folded(answer, 3);
        assert_eq!(short, "Confirmados 19 PRs.\n\n- #1001");
        assert!(cut);
        let (whole, cut) = folded("Feito.", 8);
        assert_eq!((whole.as_str(), cut), ("Feito.", false));
        let (long, cut) = folded(&"x".repeat(2000), 8);
        assert!(cut && long.ends_with('…') && long.chars().count() == 901);
    }
}
