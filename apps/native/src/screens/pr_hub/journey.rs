//! Where a pull request is on its way to production, in the hub's detail:
//! the rail under its title (open, checks, review, merge), who reviewed it
//! with their decision and when, and its checks — the ones in its way by
//! name, the ones that passed in one line.

use gpui::{AnyElement, Context, FontWeight, div, prelude::*, px};

use super::PrHub;
use super::list::actor_avatar;
use crate::data::delivery_rail::StepState;
use crate::data::pr_hub::{Detail, Item, relative_date};
use crate::data::pr_rail::{PrStep, Verdict, pr_rail, reviewer_lines, split_checks};
use crate::i18n::{Args, Translate};
use crate::theme::{ActiveTheme, StateTone};
use crate::ui::delivery_rail::{RailStep, RailTone, delivery_rail};
use crate::ui::{Icon, spinner};

type Loaded<'a> = Option<&'a Result<Detail, String>>;

fn read<'a>(detail: Loaded<'a>) -> Option<&'a Detail> {
    detail.and_then(|detail| detail.as_ref().ok())
}

impl PrHub {
    /// The pull request waits on the person's review: asked and not
    /// reviewed, or new since their review — what "Precisa de você" lists.
    pub(super) fn awaits_my_review(&self, item: &Item, cx: &gpui::App) -> bool {
        if item.created_by_viewer || item.is_draft {
            return false;
        }
        let (mine, new) = self.review_of(item, cx);
        // Only what "Revisando" lists: asked of the person, or reviewed.
        if !item.review_requested_for_viewer && !crate::data::pr_news::reviewed(item, cx) {
            return false;
        }
        crate::data::pr_sections::section_of(
            crate::data::pr_hub::Filter::Reviewing,
            item,
            mine.as_deref(),
            new,
        ) == Some(crate::data::pr_sections::Section::NeedsYou)
    }

    /// The pull request's changed files, once its code was read: the
    /// paths a comment's chips can open.
    pub(super) fn changed_paths(&self, item: &Item) -> Vec<String> {
        read(self.merge_detail(item))
            .map(|detail| detail.files.iter().map(|file| file.path.clone()).collect())
            .unwrap_or_default()
    }

    /// A file's chip was clicked: "Código" opens on that file.
    pub(super) fn open_requested_file(&mut self, cx: &mut Context<Self>) {
        let Some(path) = cx
            .try_global::<crate::markdown::files::FileRequest>()
            .and_then(|request| request.0.clone())
        else {
            return;
        };
        cx.set_global(crate::markdown::files::FileRequest(None));
        self.review.path = Some(path);
        self.set_tab(super::code::Tab::Code, cx);
    }

    pub(super) fn pr_rail(
        &self,
        item: &Item,
        detail: Loaded,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let approved = read(detail).is_some_and(|detail| {
            reviewer_lines(&detail.reviews, &item.reviewers)
                .iter()
                .any(|line| line.verdict == Verdict::Approved)
        });
        let steps = pr_rail(item, approved)
            .into_iter()
            .map(|(step, state)| {
                let key = match (step, item.is_draft) {
                    (PrStep::Opened, true) => "draft",
                    (step, _) => step.key(),
                };
                let tone = match state {
                    StepState::Done => RailTone::Done,
                    StepState::Current => RailTone::Current,
                    StepState::Failed => RailTone::Failed,
                    StepState::Todo => RailTone::Todo,
                };
                RailStep::new(step.key(), cx.t(&format!("pullRequests.rail.{key}")), tone)
            })
            .collect();
        div()
            .mt(px(14.0))
            .max_w(px(420.0))
            .child(delivery_rail("pr-rail", steps, cx))
            .into_any_element()
    }

    /// Who reviewed, with their decision and when; then who was asked.
    pub(super) fn reviewers_card(
        &self,
        item: &Item,
        detail: Loaded,
        english: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let colors = &theme.colors;
        let now = crate::data::time::now_ms();
        let reviews = read(detail).map(|detail| detail.reviews.as_slice());
        if reviews.is_none() && detail.is_none() {
            return spinner("pr-reviewers-loading", 16.0).into_any_element();
        }
        let lines = reviewer_lines(reviews.unwrap_or_default(), &item.reviewers);
        if lines.is_empty() {
            return div()
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("pullRequests.noReviewers"))
                .into_any_element();
        }
        let viewer = item.forge_login.clone();
        let rows: Vec<AnyElement> = lines
            .into_iter()
            .map(|line| {
                let tone = match line.verdict {
                    Verdict::Approved => StateTone::Done,
                    Verdict::ChangesRequested => StateTone::NeedsYou,
                    Verdict::Commented | Verdict::Dismissed | Verdict::Requested => {
                        StateTone::Quiet
                    }
                }
                .text(&theme);
                let name = if viewer.as_deref() == Some(line.actor.login.as_str()) {
                    cx.t("pullRequests.you").to_string()
                } else {
                    line.actor.login.clone()
                };
                let login = line.actor.login.clone();
                div()
                    .debug_selector(move || format!("pr-reviewer-{login}"))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(12.0))
                    .child(actor_avatar(Some(&line.actor), cx))
                    .child(div().flex_1().min_w_0().truncate().child(name))
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(px(4.0))
                            .text_color(tone)
                            .when(line.verdict == Verdict::Approved, |verdict| {
                                verdict.child(Icon::new("check").size(12.0).stroke(2.4).color(tone))
                            })
                            .child(cx.t(&format!("pullRequests.verdicts.{}", line.verdict.key()))),
                    )
                    .child(
                        div()
                            .w(px(72.0))
                            .flex_none()
                            .text_right()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(relative_date(line.at.as_deref(), now, english)),
                    )
                    .into_any_element()
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .children(rows)
            .into_any_element()
    }

    /// The checks in the way one by one, the ones that passed in one line.
    pub(super) fn checks_card(&self, detail: Loaded, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let colors = &theme.colors;
        let quiet = |text| {
            div()
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(text)
                .into_any_element()
        };
        let checks = match detail {
            None => return spinner("pr-checks-loading", 16.0).into_any_element(),
            Some(Err(_)) => return quiet(cx.t("pullRequests.noChecks")),
            Some(Ok(detail)) if detail.checks.is_empty() => {
                return quiet(cx.t("pullRequests.noChecks"));
            }
            Some(Ok(detail)) => &detail.checks,
        };
        let (open, passed) = split_checks(checks);
        let mut rows: Vec<AnyElement> = open
            .into_iter()
            .enumerate()
            .map(|(index, check)| {
                let failed = check.state == "failure";
                let (icon, tone, key) = if failed {
                    ("x", StateTone::Failed, "commit.rail.checks.failure")
                } else {
                    ("clock-3", StateTone::NeedsYou, "commit.rail.checks.pending")
                };
                let tone = tone.text(&theme);
                let row = div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(12.0))
                    .child(Icon::new(icon).size(13.0).stroke(2.4).color(tone))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(check.name.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(11.0))
                            .text_color(tone)
                            .child(cx.t(key)),
                    )
                    .children(self.check_buttons(index, check, cx));
                // The failing step's log opens under its check.
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(row)
                    .children(self.check_log(check, cx))
                    .into_any_element()
            })
            .collect();
        if !passed.is_empty() {
            let names = passed
                .iter()
                .map(|check| check.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            rows.push(
                div()
                    .debug_selector(|| "pr-checks-passed".into())
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(12.0))
                    .child(
                        Icon::new("check")
                            .size(13.0)
                            .stroke(2.4)
                            .color(StateTone::Done.text(&theme)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::MEDIUM)
                            .child(cx.t_with(
                                "commit.rail.checks.passed",
                                &Args::count(passed.len() as i64),
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(names),
                    )
                    .into_any_element(),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .children(rows)
            .into_any_element()
    }
}
