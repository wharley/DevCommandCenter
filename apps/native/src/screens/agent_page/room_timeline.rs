//! The team room's order: its lines, the inbox's cards, the check-in's,
//! and DCC's own two cards — the day's recap at the moment it opened and
//! the weekly bulletin at the start of the week — each at its own time, so
//! what the person says after them reads below them. Times are compared as
//! instants, whatever form each source writes them in.

use crate::data::time::parse_ms;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    Line(usize),
    Card(usize),
    CheckIn(usize),
    Recap,
    Bulletin,
}

/// Every slot in time order; one at the same instant keeps its place.
pub(crate) fn order<'a>(
    lines: impl Iterator<Item = &'a str>,
    cards: impl Iterator<Item = &'a str>,
    checks: impl Iterator<Item = &'a str>,
    recap: Option<&str>,
    bulletin: Option<&str>,
) -> Vec<Slot> {
    let at = |value: &str| parse_ms(value).unwrap_or(i64::MAX);
    let mut slots: Vec<(i64, Slot)> = lines
        .enumerate()
        .map(|(index, value)| (at(value), Slot::Line(index)))
        .chain(
            cards
                .enumerate()
                .map(|(index, value)| (at(value), Slot::Card(index))),
        )
        .chain(
            checks
                .enumerate()
                .map(|(index, value)| (at(value), Slot::CheckIn(index))),
        )
        .chain(recap.map(|value| (at(value), Slot::Recap)))
        .chain(bulletin.map(|value| (at(value), Slot::Bulletin)))
        .collect();
    slots.sort_by_key(|(at, _)| *at);
    slots.into_iter().map(|(_, slot)| slot).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recap_and_the_bulletin_sit_at_their_time_among_the_lines() {
        let lines = [
            "2026-11-08T23:00:00.000000+00:00",
            "2026-11-09T11:16:43.098072+00:00",
            "2026-11-09T11:17:29.578168+00:00",
        ];
        let slots = order(
            lines.iter().copied(),
            ["2026-11-09T11:20:00+00:00"].into_iter(),
            std::iter::empty(),
            Some("2026-11-09T10:05:00.000Z"),
            Some("2026-11-09T03:00:00.000Z"),
        );
        assert_eq!(
            slots,
            [
                Slot::Line(0),
                Slot::Bulletin,
                Slot::Recap,
                Slot::Line(1),
                Slot::Line(2),
                Slot::Card(0),
            ]
        );
    }
}
