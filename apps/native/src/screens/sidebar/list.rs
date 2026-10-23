//! The scrolling task list (`workspace-groups-scroll`), virtualized: only
//! the rows on screen are built, each in its fixed-height slot.

use std::ops::Range;

use gpui::{AnyElement, Context, div, list, prelude::*, px};

use super::state::RailItem;
use super::{Sidebar, row, sections};

/// The span to replace so `old` becomes `new`: unchanged items at both ends
/// keep their measured layout and the scroll position stays put.
pub fn changed_span(old: &[RailItem], new: &[RailItem]) -> (Range<usize>, usize) {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let max_suffix = old.len().min(new.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    (prefix..old.len() - suffix, new.len() - prefix - suffix)
}

pub fn render(sidebar: &Sidebar, cx: &mut Context<Sidebar>) -> impl IntoElement {
    let entity = cx.entity().downgrade();
    let items = sidebar.items.clone();
    div()
        .id("task-list")
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .on_hover(cx.listener(|this, hovered: &bool, _, cx| this.hold_order(*hovered, cx)))
        .child(
            list(sidebar.list.clone(), move |index, _window, cx| {
                let Some(item) = items.get(index).cloned() else {
                    return div().into_any_element();
                };
                let height = item.height();
                let content = entity
                    .update(cx, |sidebar, cx| item_element(sidebar, &item, cx))
                    .unwrap_or_else(|_| div().into_any_element());
                div()
                    .h(px(height))
                    .w_full()
                    .child(content)
                    .into_any_element()
            })
            .size_full(),
        )
}

fn item_element(sidebar: &Sidebar, item: &RailItem, cx: &mut Context<Sidebar>) -> AnyElement {
    match item {
        RailItem::DayLabel(section) => sections::day_label(*section, cx),
        RailItem::Row { task_id, compact } => row::render(sidebar, task_id, *compact, cx),
        RailItem::Header {
            section,
            count,
            open,
        } => sections::hold_header(sidebar, *section, *count, *open, cx),
        RailItem::Gap(_) | RailItem::BottomPadding => div().into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::rail::TaskSectionId;

    fn row(id: &str) -> RailItem {
        RailItem::Row {
            task_id: id.into(),
            compact: false,
        }
    }

    #[test]
    fn only_the_changed_middle_is_spliced() {
        let old = vec![
            RailItem::DayLabel(TaskSectionId::Today),
            row("a"),
            row("b"),
            RailItem::BottomPadding,
        ];
        let new = vec![
            RailItem::DayLabel(TaskSectionId::Today),
            row("a"),
            row("c"),
            row("b"),
            RailItem::BottomPadding,
        ];
        assert_eq!(changed_span(&old, &new), (2..2, 1));
        assert_eq!(changed_span(&new, &old), (2..3, 0));
        assert_eq!(changed_span(&old, &old), (4..4, 0));
        assert_eq!(changed_span(&[], &old), (0..0, 4));
    }
}
