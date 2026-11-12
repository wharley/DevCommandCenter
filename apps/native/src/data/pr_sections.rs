//! The hub's list in sections (the room's day separators, for pull
//! requests), nearest to production first: "Revisando" opens on what the
//! person approved (about to be merged), then what waits on the author,
//! then what needs their review; "Criados por você" on what is ready to
//! merge. Each section folds on a click.

use super::pr_hub::{Filter, Item};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Section {
    /// They approved it; nothing new since.
    Approved,
    /// They commented or asked for changes; nothing new since.
    WaitingOnAuthor,
    /// Asked for their review and not given yet, or new since it.
    NeedsYou,
    /// Theirs: approved, checks passing, not a draft.
    ReadyToMerge,
    /// Theirs: changes asked for, or checks failing.
    NeedsAction,
    /// Theirs: waiting on reviewers.
    AwaitingReview,
    Drafts,
}

impl Section {
    pub fn key(self) -> &'static str {
        match self {
            Section::NeedsYou => "needsYou",
            Section::WaitingOnAuthor => "waitingOnAuthor",
            Section::Approved => "approved",
            Section::ReadyToMerge => "readyToMerge",
            Section::NeedsAction => "needsAction",
            Section::AwaitingReview => "awaitingReview",
            Section::Drafts => "drafts",
        }
    }

    /// The header's color.
    pub fn tone(self) -> Tone {
        match self {
            Section::Approved | Section::ReadyToMerge => Tone::Done,
            Section::WaitingOnAuthor | Section::AwaitingReview => Tone::Waiting,
            Section::NeedsYou | Section::NeedsAction => Tone::NeedsYou,
            Section::Drafts => Tone::Quiet,
        }
    }
}

/// Green: about to be merged; blue: on someone else; amber: on the person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Done,
    Waiting,
    NeedsYou,
    Quiet,
}

/// Where a pull request goes; none on "Todos", which keeps one list.
/// `my_review` is the person's latest review, `new` whether something came
/// since it.
pub fn section_of(
    filter: Filter,
    item: &Item,
    my_review: Option<&str>,
    new: bool,
) -> Option<Section> {
    match filter {
        Filter::All => None,
        Filter::Reviewing => Some(match my_review {
            None => Section::NeedsYou,
            Some(_) if new => Section::NeedsYou,
            Some("APPROVED") => Section::Approved,
            Some(_) => Section::WaitingOnAuthor,
        }),
        Filter::Mine => {
            let decision = item.review_decision.as_deref().unwrap_or_default();
            let checks = item.checks_state.as_str();
            Some(if item.is_draft {
                Section::Drafts
            } else if decision == "changes_requested" || checks == "failure" {
                Section::NeedsAction
            } else if decision == "approved" && checks != "pending" {
                Section::ReadyToMerge
            } else {
                Section::AwaitingReview
            })
        }
    }
}

/// The items in their sections, in the sections' order; an empty section
/// does not show.
pub fn grouped<'a>(
    filter: Filter,
    items: &[&'a Item],
    review: impl Fn(&Item) -> (Option<String>, bool),
) -> Vec<(Option<Section>, Vec<&'a Item>)> {
    let mut groups: Vec<(Option<Section>, Vec<&'a Item>)> = Vec::new();
    for &item in items {
        let (mine, new) = review(item);
        let section = section_of(filter, item, mine.as_deref(), new);
        match groups.iter_mut().find(|(key, _)| *key == section) {
            Some((_, rows)) => rows.push(item),
            None => groups.push((section, vec![item])),
        }
    }
    groups.sort_by_key(|(section, _)| *section);
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(number: u32, decision: Option<&str>, checks: &str, draft: bool) -> Item {
        Item {
            id: number.to_string(),
            number,
            review_decision: decision.map(str::to_string),
            checks_state: checks.into(),
            is_draft: draft,
            ..Item::default()
        }
    }

    #[test]
    fn reviewing_puts_what_is_nearest_to_production_first() {
        let a = item(1, None, "success", false);
        let b = item(2, None, "success", false);
        let c = item(3, None, "success", false);
        let d = item(4, None, "success", false);
        let reviews = |item: &Item| match item.number {
            1 => (Some("APPROVED".to_string()), false),
            2 => (None, false),
            3 => (Some("COMMENTED".to_string()), false),
            _ => (Some("APPROVED".to_string()), true),
        };
        let groups = grouped(Filter::Reviewing, &[&a, &b, &c, &d], reviews);
        let shape: Vec<(Option<Section>, Vec<u32>)> = groups
            .iter()
            .map(|(section, rows)| (*section, rows.iter().map(|item| item.number).collect()))
            .collect();
        assert_eq!(
            shape,
            vec![
                (Some(Section::Approved), vec![1]),
                (Some(Section::WaitingOnAuthor), vec![3]),
                (Some(Section::NeedsYou), vec![2, 4]),
            ]
        );
        assert_eq!(Section::Approved.tone(), Tone::Done);
        assert_eq!(Section::WaitingOnAuthor.tone(), Tone::Waiting);
    }

    #[test]
    fn mine_puts_ready_to_merge_first() {
        let mine = |item: &Item| section_of(Filter::Mine, item, None, false);
        assert_eq!(
            mine(&item(1, Some("approved"), "success", false)),
            Some(Section::ReadyToMerge)
        );
        assert_eq!(
            mine(&item(2, Some("approved"), "pending", false)),
            Some(Section::AwaitingReview)
        );
        assert_eq!(
            mine(&item(3, Some("approved"), "failure", false)),
            Some(Section::NeedsAction)
        );
        assert_eq!(
            mine(&item(4, Some("changes_requested"), "success", false)),
            Some(Section::NeedsAction)
        );
        assert_eq!(mine(&item(5, None, "success", true)), Some(Section::Drafts));
        assert_eq!(
            section_of(Filter::All, &item(6, None, "", false), None, false),
            None
        );
    }
}
