//! Source news in a row ("Fonte · Slack — menções a mim", once per wake-up
//! with nothing to show) read as one line: "4 avisos das fontes · Slack",
//! opened on a click to list them. One alone stays a line of its own.

use std::collections::HashSet;

use crate::data::team_room::RoomLine;
use dcc_core::domain::agent_source::{SOURCE_MARK, source_request};
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use super::room_rows::Row;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::Icon;

fn is_source(row: &Row) -> bool {
    matches!(row, Row::Line { line, .. } if line.text.starts_with(SOURCE_MARK))
}

/// Runs of two or more source lines become one `Row::Sources`, open when
/// its first line's key is in `open`.
pub(crate) fn group(rows: Vec<Row>, open: &HashSet<String>) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::with_capacity(rows.len());
    let mut run: Vec<Row> = Vec::new();
    let flush = |run: &mut Vec<Row>, out: &mut Vec<Row>| {
        if run.len() < 2 {
            out.append(run);
            return;
        }
        let lines: Vec<(usize, RoomLine)> = run
            .drain(..)
            .filter_map(|row| match row {
                Row::Line { index, line, .. } => Some((index, line)),
                _ => None,
            })
            .collect();
        let key = lines[0].1.key.clone();
        out.push(Row::Sources {
            open: open.contains(&key),
            key,
            lines,
        });
    };
    for row in rows {
        if is_source(&row) {
            run.push(row);
        } else {
            flush(&mut run, &mut out);
            out.push(row);
        }
    }
    flush(&mut run, &mut out);
    out
}

impl TeamRoom {
    pub(super) fn sources_row(
        &self,
        key: &str,
        lines: &[(usize, RoomLine)],
        open: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.theme().colors.clone();
        let mut connectors: Vec<String> = Vec::new();
        for (_, line) in lines {
            if let Some((_, connector)) = source_request(&line.text)
                && !connectors.iter().any(|known| known == connector)
            {
                connectors.push(connector.to_string());
            }
        }
        let summary = cx.t_with(
            "agents.sources.grouped",
            &Args::count(lines.len() as i64).with("connectors", connectors.join(", ")),
        );
        let toggled = key.to_string();
        let head = div()
            .id(SharedString::from(format!("room-sources-{key}")))
            .debug_selector({
                let key = key.to_string();
                move || format!("room-sources-{key}")
            })
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(11.5))
            .text_color(colors.muted_foreground)
            .cursor_pointer()
            .on_click(cx.listener(move |room, _, _, cx| {
                if !room.sources_open.remove(&toggled) {
                    room.sources_open.insert(toggled.clone());
                }
                cx.notify();
            }))
            .child(Icon::new("radar").size(12.0).color(colors.muted_foreground))
            .child(div().truncate().child(summary))
            .child(
                Icon::new(if open {
                    "chevron-down"
                } else {
                    "chevron-right"
                })
                .size(12.0)
                .color(colors.muted_foreground),
            )
            .into_any_element();
        let mut rows = vec![head];
        if open {
            rows.extend(lines.iter().filter_map(|(index, line)| {
                self.source_line(*index, &line.text, cx)
                    .map(|row| div().pl(px(18.0)).child(row).into_any_element())
            }));
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::team_room::Speaker;

    fn line(key: &str, text: &str) -> Row {
        Row::Line {
            index: 0,
            line: RoomLine {
                key: key.into(),
                agent_id: "a".into(),
                session_id: "s".into(),
                speaker: Speaker::Person,
                text: text.into(),
                at: "2026-11-12T10:00:00Z".into(),
                receipt: None,
                tasks: Vec::new(),
                steps: Default::default(),
            },
            first: true,
            said: None,
            steps: None,
        }
    }

    #[test]
    fn source_news_in_a_row_read_as_one_line_and_one_alone_stays() {
        let source = format!("{SOURCE_MARK} \"Menções\" (Slack): 1 new item(s).");
        let rows = vec![
            line("a", &source),
            line("b", &source),
            line("c", &source),
            line("d", "oi"),
            line("e", &source),
        ];
        let grouped = group(rows, &HashSet::new());
        assert_eq!(grouped.len(), 3);
        match &grouped[0] {
            Row::Sources { key, lines, open } => {
                assert_eq!(key, "a");
                assert_eq!(lines.len(), 3);
                assert!(!open);
            }
            _ => panic!("the run of three is one row"),
        }
        assert!(matches!(&grouped[2], Row::Line { line, .. } if line.key == "e"));
        let open: HashSet<String> = ["a".to_string()].into();
        let rows = vec![line("a", &source), line("b", &source)];
        assert!(matches!(
            &group(rows, &open)[0],
            Row::Sources { open: true, .. }
        ));
    }
}
