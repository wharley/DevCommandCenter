//! The pull request's checks under the delivery rail, opened from its
//! "Checks" step: the failed and the running ones by name, the ones that
//! passed in one line. Read from the forge when opened, not on a timer.

use gpui::{AnyElement, Context, div, prelude::*, px};

use super::super::CodeColumn;
use crate::data::pr_hub::{Check, Item};
use crate::i18n::{Args, Translate};
use crate::theme::{ActiveTheme, StateTone};
use crate::ui::{Icon, spinner};

#[derive(Default)]
pub(crate) struct ChecksList {
    pub open: bool,
    /// `None` while the forge is read.
    pub read: Option<Result<Vec<Check>, ()>>,
    /// The pull request the checks are of.
    of: Option<u32>,
}

/// The checks that did not pass (failed first, then running), and how many
/// passed.
pub(crate) fn split(checks: &[Check]) -> (Vec<&Check>, usize) {
    let passed = checks
        .iter()
        .filter(|check| check.state == "success")
        .count();
    let mut open: Vec<&Check> = checks
        .iter()
        .filter(|check| check.state != "success")
        .collect();
    open.sort_by_key(|check| check.state != "failure");
    (open, passed)
}

impl CodeColumn {
    /// Whether the rail's "Checks" step opens a list: GitHub, with a pull
    /// request (GitLab has its pipeline's own section).
    pub(crate) fn offers_checks_list(&self) -> bool {
        let github =
            self.review.forge.as_ref().is_some_and(|forge| {
                forge.connected() && forge.provider.as_deref() == Some("github")
            });
        github
            && self
                .delivery
                .pr
                .as_ref()
                .is_some_and(|pr| pr.number.is_some())
    }

    pub(crate) fn toggle_checks(&mut self, cx: &mut Context<Self>) {
        let open = !self.review.checks.open;
        self.review.checks.open = open;
        cx.notify();
        if open {
            self.read_checks(cx);
        }
    }

    fn read_checks(&mut self, cx: &mut Context<Self>) {
        let number = self.delivery.pr.as_ref().and_then(|pr| pr.number);
        let (Some(number), Some(root), Ok(api)) = (number, self.root.clone(), self.review_api())
        else {
            self.review.checks.read = Some(Err(()));
            return;
        };
        if self.review.checks.of != Some(number) {
            self.review.checks.read = None;
            self.review.checks.of = Some(number);
        }
        let item = Item {
            repository_root: root,
            forge_login: self.review.login(),
            number,
            ..Item::default()
        };
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.pr_hub_detail(&item, false) })
                .await;
            let _ = this.update(cx, |column, cx| {
                if column.review.checks.of == Some(number) {
                    column.review.checks.read =
                        Some(read.map(|detail| detail.checks).map_err(|_| ()));
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

pub(crate) fn checks_list(column: &CodeColumn, cx: &mut Context<CodeColumn>) -> Option<AnyElement> {
    let list = &column.review.checks;
    if !list.open || !column.offers_checks_list() {
        return None;
    }
    let colors = cx.theme().colors.clone();
    let quiet = |text| {
        div()
            .text_size(px(11.0))
            .text_color(colors.muted_foreground)
            .child(text)
            .into_any_element()
    };
    let body: Vec<AnyElement> = match &list.read {
        None => vec![spinner("delivery-checks-loading", 14.0).into_any_element()],
        Some(Err(())) => vec![quiet(cx.t("commit.rail.checks.failed"))],
        Some(Ok(checks)) if checks.is_empty() => vec![quiet(cx.t("pullRequests.noChecks"))],
        Some(Ok(checks)) => {
            let (open, passed) = split(checks);
            let mut rows: Vec<AnyElement> = open
                .into_iter()
                .map(|check| {
                    let failed = check.state == "failure";
                    let (icon, tone) = if failed {
                        ("x", StateTone::Failed)
                    } else {
                        ("clock-3", StateTone::NeedsYou)
                    };
                    let tone = tone.text(cx.theme());
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(12.0))
                        .child(Icon::new(icon).size(12.0).stroke(2.4).color(tone))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(check.name.clone()),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(11.0))
                                .text_color(tone)
                                .child(cx.t(if failed {
                                    "commit.rail.checks.failure"
                                } else {
                                    "commit.rail.checks.pending"
                                })),
                        )
                        .into_any_element()
                })
                .collect();
            if passed > 0 {
                rows.push(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(12.0))
                        .text_color(colors.muted_foreground)
                        .child(
                            Icon::new("check")
                                .size(12.0)
                                .stroke(2.4)
                                .color(StateTone::Done.text(cx.theme())),
                        )
                        .child(cx.t_with("commit.rail.checks.passed", &Args::count(passed as i64)))
                        .into_any_element(),
                );
            }
            rows
        }
    };
    Some(
        div()
            .id("delivery-checks")
            .debug_selector(|| "delivery-checks".into())
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(12.0))
            .pb(px(10.0))
            .children(body)
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_checks_come_first_and_the_passed_ones_are_counted() {
        let check = |name: &str, state: &str| Check {
            name: name.into(),
            state: state.into(),
        };
        let checks = [
            check("lint", "success"),
            check("e2e", "pending"),
            check("unit", "failure"),
            check("build", "success"),
        ];
        let (open, passed) = split(&checks);
        assert_eq!(passed, 2);
        let names: Vec<&str> = open.iter().map(|check| check.name.as_str()).collect();
        assert_eq!(names, ["unit", "e2e"]);
    }
}
