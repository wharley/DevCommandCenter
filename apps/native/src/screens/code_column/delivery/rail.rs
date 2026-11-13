//! The delivery rail in the column's header (`ui::delivery_rail`): the six
//! steps settled from the checkout, the pull request and its review, and
//! the line under them saying where things stand.

use dcc_core::domain::commit_mode::CommitMode;
use gpui::{AnyElement, Context, SharedString};

use super::super::CodeColumn;
use crate::data::delivery_rail::{Rail, RailInput, Standing, Step, StepState, rail};
use crate::i18n::{Args, Translate};
use crate::screens::code_column::review::ReviewLoad;
use crate::ui::delivery_rail::{RailStep, RailTone, delivery_rail};

impl CodeColumn {
    pub(crate) fn delivery_rail(&self, mode: CommitMode) -> Rail {
        let status = self.status.as_ref();
        let review = match self.review.state.as_ref() {
            Some(ReviewLoad::Ready(state)) => Some(&**state),
            _ => None,
        };
        rail(
            mode,
            RailInput {
                changes: status.map_or(0, |status| status.staged.len() + status.unstaged.len()),
                ahead: status.map_or(0, |status| status.ahead_of_remote_count),
                branch_files: self.changes.branch_count(),
                pr: self.delivery.pr.as_ref(),
                review,
            },
        )
    }
}

/// "PR #12", or the forge's name for a request before there is one.
fn request_label(column: &CodeColumn) -> String {
    let label = column.delivery.request_label();
    match column.delivery.pr.as_ref().and_then(|pr| pr.number) {
        Some(number) => format!("{label} #{number}"),
        None => label.to_string(),
    }
}

pub(super) fn rail_strip(
    column: &CodeColumn,
    rail: &Rail,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let url = column.delivery.pr.as_ref().and_then(|pr| pr.url.clone());
    let checks = column.offers_checks_list();
    let steps = rail
        .steps
        .iter()
        .map(|(step, state)| {
            let label: SharedString = match step {
                Step::Request => request_label(column).into(),
                step => cx.t(&format!("commit.rail.steps.{}", step.key())),
            };
            let tone = match state {
                StepState::Done => RailTone::Done,
                StepState::Current => RailTone::Current,
                StepState::Failed => RailTone::Failed,
                StepState::Todo => RailTone::Todo,
            };
            let rail_step = RailStep::new(step.key(), label, tone);
            match (step, url.clone()) {
                (Step::Request, Some(url)) => rail_step.on_click(move |_, _, cx| cx.open_url(&url)),
                (Step::Checks, _) if checks => {
                    let column = cx.entity().downgrade();
                    rail_step.on_click(move |_, _, cx| {
                        let _ = column.update(cx, |column, cx| column.toggle_checks(cx));
                    })
                }
                _ => rail_step,
            }
        })
        .collect();
    delivery_rail("delivery-rail", steps, cx)
}

/// Where things stand, and the approvals beside it when the forge counts
/// them and the merge still waits.
pub(super) fn standing_text(
    column: &CodeColumn,
    rail: &Rail,
    cx: &mut Context<CodeColumn>,
) -> (SharedString, Option<SharedString>) {
    let status = column.status.as_ref();
    let key = format!("commit.rail.status.{}", rail.standing.key());
    let label = column.delivery.request_label();
    let args = match rail.standing {
        Standing::ToCommit => Args::count(
            status.map_or(0, |status| status.staged.len() + status.unstaged.len()) as i64,
        ),
        Standing::ToPush => {
            Args::count(status.map_or(0, |status| status.ahead_of_remote_count) as i64)
        }
        _ => Args::default().with("label", label),
    };
    let approvals = rail
        .approvals
        .filter(|_| {
            !matches!(
                rail.standing,
                Standing::Merged | Standing::Closed | Standing::Idle
            )
        })
        .map(|(received, required)| {
            cx.t_with(
                "commit.rail.approvals",
                &Args::count(required as i64).with("received", received.to_string()),
            )
        });
    (cx.t_with(&key, &args), approvals)
}
