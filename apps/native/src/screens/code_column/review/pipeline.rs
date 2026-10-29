//! `GitlabPipelineSection`: the current commit's pipeline (status, failed
//! jobs, duration, link), and open, its jobs — each with its log (bounded,
//! loaded on demand), its page and, when the forge allows, a new run asked
//! first.

use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, Window, div, prelude::*, px};

use super::super::CodeColumn;
use super::PipelineLoad;
use crate::data::forge::{PipelineJob, PipelineTone, format_duration, pipeline_tone};
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance, fonts};
use crate::ui::tooltip::Side;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Dialog, Icon, Toaster, Tooltip, dialog_footer,
    dialog_header, spinner,
};

impl CodeColumn {
    pub(crate) fn toggle_pipeline(&mut self, cx: &mut Context<Self>) {
        self.review.pipeline_open = !self.review.pipeline_open;
        cx.notify();
    }

    /// A job's log, read when opened (`staleTime 30s` aside).
    pub(crate) fn toggle_job_log(&mut self, job: u64, cx: &mut Context<Self>) {
        if self.review.active_job == Some(job) {
            self.review.active_job = None;
            cx.notify();
            return;
        }
        let (Some(root), Some(PipelineLoad::Ready(Some(pipeline)))) =
            (self.root.clone(), self.review.pipeline.clone())
        else {
            return;
        };
        self.review.active_job = Some(job);
        self.review.job_log = None;
        cx.notify();
        let api = self.review_api();
        let login = self.review.login();
        self._job_log = Some(cx.spawn(async move |this, cx| {
            let log = cx
                .background_executor()
                .spawn(
                    async move { api?.pipeline_job_log(&root, pipeline.id, job, login.as_deref()) },
                )
                .await;
            let _ = this.update(cx, |column, cx| {
                if column.review.active_job == Some(job) {
                    column.review.job_log = Some(log.map_err(|error| error.to_string()));
                    cx.notify();
                }
            });
        }));
    }

    /// "Executar novamente", asked first (`window.confirm`).
    pub(crate) fn retry_job(&mut self, job: &PipelineJob, confirmed: bool, cx: &mut Context<Self>) {
        if !confirmed {
            self.review.confirm_job_retry = Some((job.id, job.name.clone()));
            cx.notify();
            return;
        }
        self.review.confirm_job_retry = None;
        let (Some(root), Some(PipelineLoad::Ready(Some(pipeline)))) =
            (self.root.clone(), self.review.pipeline.clone())
        else {
            return;
        };
        let (id, name) = (job.id, job.name.clone());
        self.review.retrying_job = Some(id);
        cx.notify();
        let api = self.review_api();
        let login = self.review.login();
        self._job_retry = Some(cx.spawn(async move |this, cx| {
            let result =
                cx.background_executor()
                    .spawn(async move {
                        api?.pipeline_job_retry(&root, pipeline.id, id, login.as_deref())
                    })
                    .await;
            let _ = this.update(cx, |column, cx| {
                column.review.retrying_job = None;
                match result {
                    Ok(()) => {
                        column.review.active_job = None;
                        Toaster::success(
                            cx.t_with(
                                "inspector.pipeline.retryStarted",
                                &Args::default().with("job", name),
                            ),
                            cx,
                        );
                    }
                    Err(error) => Toaster::error(error.to_string(), cx),
                }
                column.read_review(true, cx);
                cx.notify();
            });
        }));
    }

    pub(crate) fn cancel_job_retry(&mut self, cx: &mut Context<Self>) {
        self.review.confirm_job_retry = None;
        cx.notify();
    }

    pub(crate) fn retry_pipeline_read(&mut self, cx: &mut Context<Self>) {
        self.review.pipeline = Some(PipelineLoad::Pending);
        cx.notify();
        self.read_review(true, cx);
    }
}

/// `StatusIcon`
fn status_icon(
    status: &str,
    id: impl Into<gpui::ElementId>,
    cx: &Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    match pipeline_tone(status) {
        PipelineTone::Success => Icon::new("circle-check")
            .size(14.0)
            .color(tw::EMERALD_500.hsla())
            .into_any_element(),
        PipelineTone::Danger => Icon::new("circle-x")
            .size(14.0)
            .color(colors.destructive)
            .into_any_element(),
        _ if status == "canceled" => Icon::new("ban")
            .size(14.0)
            .color(colors.muted_foreground)
            .into_any_element(),
        PipelineTone::Progress => div()
            .text_color(tw::SKY_500.hsla())
            .child(spinner(id, 14.0))
            .into_any_element(),
        PipelineTone::Waiting => Icon::new("clock-3")
            .size(14.0)
            .color(tw::AMBER_500.hsla())
            .into_any_element(),
        PipelineTone::Muted => Icon::new("circle-dot")
            .size(14.0)
            .color(colors.muted_foreground)
            .into_any_element(),
    }
}

/// `statusClass`: (border, background, text).
fn status_colors(status: &str, cx: &Context<CodeColumn>) -> (Hsla, Hsla, Hsla) {
    let colors = cx.theme().colors.clone();
    let dark = cx.theme().appearance == Appearance::Dark;
    let shade =
        |light: tw::Oklch, dark_shade: tw::Oklch| if dark { dark_shade } else { light }.hsla();
    match pipeline_tone(status) {
        PipelineTone::Success => (
            tw::EMERALD_500.alpha(0.2),
            tw::EMERALD_500.alpha(0.1),
            shade(tw::EMERALD_700, tw::EMERALD_400),
        ),
        PipelineTone::Danger => (
            colors.destructive.opacity(0.2),
            colors.destructive.opacity(0.1),
            colors.destructive,
        ),
        PipelineTone::Progress => (
            tw::SKY_500.alpha(0.2),
            tw::SKY_500.alpha(0.1),
            shade(tw::SKY_700, tw::SKY_400),
        ),
        PipelineTone::Waiting => (
            tw::AMBER_500.alpha(0.2),
            tw::AMBER_500.alpha(0.1),
            shade(tw::AMBER_700, tw::AMBER_400),
        ),
        PipelineTone::Muted => (
            colors.border.opacity(0.6),
            colors.muted.opacity(0.5),
            colors.muted_foreground,
        ),
    }
}

fn status_badge(status: &str, cx: &Context<CodeColumn>) -> AnyElement {
    let (border, background, text) = status_colors(status, cx);
    div()
        .flex()
        .h(px(20.0))
        .flex_shrink_0()
        .items_center()
        .rounded_full()
        .border_1()
        .border_color(border)
        .bg(background)
        .px(px(6.0))
        .text_size(px(9.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(text)
        .child(super::state_label("inspector.pipeline.status", status, cx))
        .into_any_element()
}

fn icon_button(
    id: SharedString,
    icon: AnyElement,
    active: bool,
    cx: &Context<CodeColumn>,
) -> gpui::Stateful<gpui::Div> {
    let colors = cx.theme().colors.clone();
    let hover = colors.accent;
    div()
        .id(id)
        .flex()
        .size(px(24.0))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .text_color(if active {
            colors.foreground
        } else {
            colors.muted_foreground
        })
        .when(active, |button| button.bg(colors.muted))
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .child(icon)
}

fn job_row(
    column: &CodeColumn,
    job: &PipelineJob,
    first: bool,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let active = column.review.active_job == Some(job.id);
    let busy = column.review.retrying_job == Some(job.id);
    let mut detail = job.stage.clone();
    if let Some(duration) = format_duration(job.duration) {
        detail.push_str(&format!(" · {duration}"));
    }
    if job.allow_failure {
        detail.push_str(&format!(" · {}", cx.t("inspector.pipeline.allowedFailure")));
    }
    let id = job.id;
    let log_tip = cx.t("inspector.pipeline.log");
    let log = Tooltip::new(
        SharedString::from(format!("job-log-tip-{id}")),
        icon_button(
            SharedString::from(format!("job-log-{id}")),
            Icon::new("file-terminal").size(14.0).into_any_element(),
            active,
            cx,
        )
        .debug_selector(move || format!("job-log-{id}"))
        .on_click(cx.listener(move |column, _, _, cx| column.toggle_job_log(id, cx))),
        move |_, _| div().child(log_tip.clone()).into_any_element(),
    )
    .side(Side::Top);
    let open = job.web_url.clone().map(|url| {
        let tip = cx.t("inspector.pipeline.openJobShort");
        Tooltip::new(
            SharedString::from(format!("job-open-tip-{id}")),
            icon_button(
                SharedString::from(format!("job-open-{id}")),
                Icon::new("external-link").size(14.0).into_any_element(),
                false,
                cx,
            )
            .on_click(move |_, _, cx| cx.open_url(&url)),
            move |_, _| div().child(tip.clone()).into_any_element(),
        )
    });
    let retry = job.retryable.then(|| {
        let tip = cx.t("inspector.pipeline.retry");
        let job = job.clone();
        let icon = if busy {
            spinner(("job-retrying", id as usize), 14.0).into_any_element()
        } else {
            Icon::new("rotate-ccw").size(14.0).into_any_element()
        };
        Tooltip::new(
            SharedString::from(format!("job-retry-tip-{id}")),
            icon_button(
                SharedString::from(format!("job-retry-{id}")),
                icon,
                false,
                cx,
            )
            .debug_selector(move || format!("job-retry-{id}"))
            .when(!busy, |button| {
                button.on_click(
                    cx.listener(move |column, _, _, cx| column.retry_job(&job, false, cx)),
                )
            }),
            move |_, _| div().child(tip.clone()).into_any_element(),
        )
    });
    let log_body = active.then(|| {
        let content = match &column.review.job_log {
            None => div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .py(px(12.0))
                .text_size(px(10.0))
                .text_color(colors.muted_foreground)
                .child(spinner(("job-log-loading", id as usize), 12.0))
                .child(cx.t("inspector.pipeline.loadingLog"))
                .into_any_element(),
            Some(Err(error)) => div()
                .py(px(8.0))
                .text_size(px(10.0))
                .text_color(colors.destructive)
                .child(SharedString::from(error.clone()))
                .into_any_element(),
            Some(Ok(log)) => {
                let text: SharedString = if log.content.is_empty() {
                    cx.t("inspector.pipeline.emptyLog")
                } else {
                    log.content.clone().into()
                };
                div()
                    .when(log.truncated, |body| {
                        body.child(
                            div()
                                .mb(px(6.0))
                                .text_size(px(9.5))
                                .text_color(tw::AMBER_700.hsla())
                                .child(cx.t("inspector.pipeline.logTruncated")),
                        )
                    })
                    .child(
                        div()
                            .id(("job-log-text", id as usize))
                            .max_h(px(176.0))
                            .overflow_y_scroll()
                            .rounded(px(6.0))
                            .bg(colors.muted.opacity(0.5))
                            .p(px(8.0))
                            .font_family(fonts::MONO)
                            .text_size(px(9.5))
                            .line_height(px(13.8))
                            .text_color(colors.foreground.opacity(0.85))
                            .child(text),
                    )
                    .into_any_element()
            }
        };
        div()
            .border_t_1()
            .border_color(colors.border.opacity(0.3))
            .bg(colors.background.opacity(0.6))
            .px(px(10.0))
            .py(px(8.0))
            .child(content)
    });
    div()
        .child(
            div()
                .when(!first, |row| {
                    row.border_t_1().border_color(colors.border.opacity(0.4))
                })
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(6.0))
                .child(status_icon(&job.status, ("job-status", id as usize), cx))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .truncate()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .child(job.name.clone()),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(9.5))
                                .text_color(colors.muted_foreground)
                                .child(detail),
                        ),
                )
                .child(status_badge(&job.status, cx))
                .child(log)
                .children(open)
                .children(retry),
        )
        .children(log_body)
        .into_any_element()
}

pub(crate) fn pipeline_section(
    column: &CodeColumn,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    if !column.review.pipeline_enabled() {
        return None;
    }
    let colors = cx.theme().colors.clone();
    let dark = cx.theme().appearance == Appearance::Dark;
    let line = |border: Hsla, background: Hsla, text: Hsla| {
        div()
            .debug_selector(|| "gitlab-pipeline".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(border)
            .bg(background)
            .px(px(10.0))
            .py(px(8.0))
            .text_size(px(11.0))
            .text_color(text)
    };
    match column.review.pipeline.as_ref()? {
        PipelineLoad::Pending => Some(
            line(
                colors.border.opacity(0.5),
                colors.muted.opacity(0.2),
                colors.muted_foreground,
            )
            .child(spinner("pipeline-loading", 14.0))
            .child(cx.t("inspector.pipeline.loading"))
            .into_any_element(),
        ),
        PipelineLoad::Failed => Some(
            line(
                tw::AMBER_500.alpha(0.2),
                tw::AMBER_500.alpha(0.05),
                if dark { tw::AMBER_300 } else { tw::AMBER_700 }.hsla(),
            )
            .child(Icon::new("circle-alert").size(14.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(cx.t("inspector.pipeline.loadFailed")),
            )
            .child(
                div()
                    .id("pipeline-retry-read")
                    .flex()
                    .h(px(24.0))
                    .items_center()
                    .rounded(px(6.0))
                    .px(px(6.0))
                    .text_size(px(10.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(tw::AMBER_500.alpha(0.1)))
                    .on_click(cx.listener(|column, _, _, cx| column.retry_pipeline_read(cx)))
                    .child(cx.t("inspector.pipeline.tryAgain")),
            )
            .into_any_element(),
        ),
        PipelineLoad::Ready(None) => Some(
            line(
                colors.border.opacity(0.5),
                colors.muted.opacity(0.15),
                colors.muted_foreground,
            )
            .child(Icon::new("circle-dot").size(14.0))
            .child(div().truncate().child(cx.t("inspector.pipeline.notFound")))
            .into_any_element(),
        ),
        PipelineLoad::Ready(Some(pipeline)) => {
            let pipeline = pipeline.as_ref().clone();
            let open = column.review.pipeline_open;
            let failed = pipeline
                .jobs
                .iter()
                .filter(|job| job.status == "failed")
                .count();
            let reference = pipeline
                .ref_name
                .clone()
                .unwrap_or_else(|| pipeline.sha.chars().take(8).collect());
            let link = pipeline.web_url.clone().map(|url| {
                icon_button(
                    "pipeline-open".into(),
                    Icon::new("external-link").size(14.0).into_any_element(),
                    false,
                    cx,
                )
                .on_click(move |_, _, cx| cx.open_url(&url))
            });
            let header = div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(8.0))
                .child(
                    div()
                        .id("pipeline-toggle")
                        .debug_selector(|| "pipeline-toggle".into())
                        .flex()
                        .h(px(24.0))
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .gap(px(8.0))
                        .ml(px(-4.0))
                        .px(px(4.0))
                        .cursor_pointer()
                        .on_click(cx.listener(|column, _, _, cx| column.toggle_pipeline(cx)))
                        .child(Icon::new("chevron-right").size(14.0).rotate(if open {
                            90.0
                        } else {
                            0.0
                        }))
                        .child(status_icon(&pipeline.status, "pipeline-status", cx))
                        .child(
                            div()
                                .truncate()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .child(cx.t_with(
                                    "inspector.pipeline.title",
                                    &Args::default().with("id", pipeline.id.to_string()),
                                )),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(10.0))
                                .text_color(colors.muted_foreground)
                                .child(reference),
                        ),
                )
                .when(failed > 0, |row| {
                    row.child(
                        div()
                            .text_size(px(9.5))
                            .text_color(colors.destructive)
                            .child(cx.t_with(
                                "inspector.pipeline.failedJobs",
                                &Args::count(failed as i64),
                            )),
                    )
                })
                .when_some(format_duration(pipeline.duration), |row, duration| {
                    row.child(
                        div()
                            .text_size(px(9.5))
                            .text_color(colors.muted_foreground)
                            .child(duration),
                    )
                })
                .child(status_badge(&pipeline.status, cx))
                .children(link);
            let body = open.then(|| {
                let jobs: Vec<AnyElement> = pipeline
                    .jobs
                    .iter()
                    .enumerate()
                    .map(|(index, job)| job_row(column, job, index == 0, cx))
                    .collect();
                div()
                    .border_t_1()
                    .border_color(colors.border.opacity(0.4))
                    .child(
                        div()
                            .id("pipeline-jobs")
                            .max_h(px(224.0))
                            .overflow_y_scroll()
                            .map(|list| {
                                if jobs.is_empty() {
                                    list.child(
                                        div()
                                            .px(px(10.0))
                                            .py(px(12.0))
                                            .text_size(px(10.0))
                                            .text_color(colors.muted_foreground)
                                            .child(cx.t("inspector.pipeline.noJobs")),
                                    )
                                } else {
                                    list.children(jobs)
                                }
                            }),
                    )
            });
            Some(
                div()
                    .debug_selector(|| "gitlab-pipeline".into())
                    .flex_shrink_0()
                    .overflow_hidden()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.6))
                    .bg(colors.muted.opacity(0.15))
                    .child(header)
                    .children(body)
                    .into_any_element(),
            )
        }
    }
}

/// `window.confirm` before a job runs again.
pub(crate) fn pipeline_retry_confirm(
    column: &CodeColumn,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    let (id, name) = column.review.confirm_job_retry.clone()?;
    let job = match &column.review.pipeline {
        Some(PipelineLoad::Ready(Some(pipeline))) => {
            pipeline.jobs.iter().find(|job| job.id == id)?.clone()
        }
        _ => return None,
    };
    let header = dialog_header(
        cx.t_with(
            "inspector.pipeline.retryConfirm",
            &Args::default().with("job", name),
        ),
        None,
        cx,
    );
    let footer = dialog_footer(
        [
            Button::new("job-retry-cancel")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Default)
                .child(cx.t("inspector.gitConfirmation.cancel"))
                .on_click(cx.listener(|column, _, _, cx| column.cancel_job_retry(cx)))
                .into_any_element(),
            Button::new("job-retry-run")
                .size(ButtonSize::Default)
                .child("OK")
                .on_click(cx.listener(move |column, _, _, cx| column.retry_job(&job, true, cx)))
                .into_any_element(),
        ],
        cx,
    );
    let close = cx.entity().downgrade();
    Some(
        Dialog::new("job-retry-confirm", move |_: &mut Window, cx| {
            let _ = close.update(cx, |column, cx| column.cancel_job_retry(cx));
        })
        .close_button(false)
        .child(header)
        .child(footer)
        .into_any_element(),
    )
}
