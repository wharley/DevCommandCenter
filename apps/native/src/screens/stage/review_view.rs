//! How the conversations look on the stage: `ReviewThreadsBar` (one calm
//! line over the diff, the list under it when opened, and the switch to the
//! pull request's diff or back), and `ReviewThreadDrawer` (the thread with
//! its comments and the code it is about, the reply field with Resolver /
//! Responder, then "Abrir revisão", "Composer" and "Enviar ao agente").

use dcc_core::domain::review_thread_detail::review_thread_resolvable;
use dcc_core::domain::review_threads::{
    ReviewThread, ReviewThreadState, review_comment_line, review_thread_root, review_thread_state,
};
use gpui::{
    AnyElement, Context, Focusable, FontWeight, InteractiveElement, KeyBinding, SharedString,
    Window, actions, div, prelude::*, px,
};

use super::Stage;
use super::review::ForgeBusy;
use crate::i18n::{Args, Translate};
use crate::screens::code_column::review::{first_line, state_chip};
use crate::theme::{ActiveTheme, fonts};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tracked, spinner, tabular_nums};

actions!(stage_review, [ReplyToThread]);

const DRAWER_CONTEXT: &str = "ThreadDrawer";

pub fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([KeyBinding::new(
        "cmd-enter",
        ReplyToThread,
        Some(DRAWER_CONTEXT),
    )]);
}

fn author_label(
    thread_comment: &dcc_core::domain::review_threads::WorkspacePrReviewComment,
    cx: &gpui::App,
) -> SharedString {
    thread_comment
        .author
        .as_ref()
        .and_then(|author| author.login.clone())
        .map(|login| SharedString::from(format!("@{login}")))
        .unwrap_or_else(|| cx.t("reviewThread.reviewer"))
}

/// `ReviewThreadsBar`, while the diff shows and the file has conversations.
pub(crate) fn review_bar(stage: &Stage, cx: &mut Context<Stage>) -> Option<AnyElement> {
    let review = stage.review()?;
    if stage.chat_active {
        return None;
    }
    let threads = review.listed();
    if threads.is_empty() {
        return None;
    }
    let colors = cx.theme().colors.clone();
    let showing = stage.showing_pr_diff();
    let committed = stage.diff.as_ref().is_some_and(|diff| {
        diff.request.scope == dcc_core::domain::workspace_git::WorkspaceGitPreviewScope::Committed
    });
    let (mut open, mut outdated, mut resolved) = (0, 0, 0);
    for thread in &threads {
        match review_thread_state(thread) {
            ReviewThreadState::Open => open += 1,
            ReviewThreadState::Outdated => outdated += 1,
            ReviewThreadState::Resolved => resolved += 1,
        }
    }
    let summary = [
        ("reviewThread.bar.open", open),
        ("reviewThread.bar.outdated", outdated),
        ("reviewThread.bar.resolved", resolved),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .map(|(key, count)| cx.t_with(key, &Args::count(count)).to_string())
    .collect::<Vec<_>>()
    .join(" · ");
    let bar_open = review.bar_open;
    let foreground = colors.foreground;
    let primary = colors.primary;
    let toggle = div()
        .id("review-bar-toggle")
        .debug_selector(|| "review-bar-toggle".into())
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap(px(6.0))
        .text_size(px(11.0))
        .text_color(colors.muted_foreground)
        .cursor_pointer()
        .hover(move |style| style.text_color(foreground))
        .on_click(cx.listener(|stage, _, _, cx| stage.toggle_review_bar(cx)))
        .child(
            Icon::new("chevron-right")
                .size(12.0)
                .stroke(2.0)
                .rotate(if bar_open { 90.0 } else { 0.0 }),
        )
        .child(
            div()
                .text_color(primary)
                .child(Icon::new("message-square").size(14.0).stroke(2.0)),
        )
        .child(
            div()
                .flex_shrink_0()
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.foreground)
                .child(cx.t_with("reviewThread.bar.title", &Args::count(threads.len() as i64))),
        )
        .child(div().min_w_0().truncate().child(summary));
    let switch = |id: &'static str, label: SharedString, pr: bool, cx: &mut Context<Stage>| {
        let colors = cx.theme().colors.clone();
        let (text, hover) = if pr {
            (colors.primary, colors.primary.opacity(0.1))
        } else {
            (colors.muted_foreground, colors.muted)
        };
        let foreground = colors.foreground;
        div()
            .id(id)
            .debug_selector(move || id.to_string())
            .flex_shrink_0()
            .rounded(px(4.0))
            .px(px(6.0))
            .py(px(2.0))
            .text_size(px(10.5))
            .when(pr, |button| button.font_weight(FontWeight::MEDIUM))
            .text_color(text)
            .cursor_pointer()
            .hover(move |style| {
                let style = style.bg(hover);
                if pr {
                    style
                } else {
                    style.text_color(foreground)
                }
            })
            .on_click(cx.listener(move |stage, _, _, cx| stage.set_pr_diff(pr, cx)))
            .child(label)
            .into_any_element()
    };
    let show_pr = (!showing && review.pr_base.is_some()).then(|| {
        switch(
            "review-show-pr-diff",
            cx.t("reviewThread.bar.showPullRequestDiff"),
            true,
            cx,
        )
    });
    let show_local = (showing && !committed).then(|| {
        switch(
            "review-show-local-diff",
            cx.t("reviewThread.bar.showLocalDiff"),
            false,
            cx,
        )
    });
    let active = review.active.clone();
    let list = bar_open.then(|| {
        let hint = (!showing).then(|| {
            div()
                .px(px(6.0))
                .pb(px(4.0))
                .text_size(px(10.5))
                .line_height(gpui::relative(1.375))
                .text_color(colors.muted_foreground)
                .child(cx.t("reviewThread.bar.localDiffHint"))
        });
        let rows: Vec<AnyElement> = threads
            .iter()
            .map(|thread| thread_row(thread, active.as_deref(), cx))
            .collect();
        div()
            .id("review-bar-list")
            .max_h(px(176.0))
            .overflow_y_scroll()
            .px(px(6.0))
            .pb(px(6.0))
            .children(hint)
            .children(rows)
    });
    Some(
        div()
            .debug_selector(|| "review-bar".into())
            .flex_shrink_0()
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.muted.opacity(0.2))
            .child(
                div()
                    .flex()
                    .h(px(32.0))
                    .items_center()
                    .gap(px(8.0))
                    .px(px(12.0))
                    .child(toggle)
                    .children(show_pr)
                    .children(show_local),
            )
            .children(list)
            .into_any_element(),
    )
}

fn thread_row(thread: &ReviewThread, active: Option<&str>, cx: &mut Context<Stage>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let root = review_thread_root(thread);
    let state = review_thread_state(thread);
    let line = review_comment_line(root)
        .filter(|_| state != ReviewThreadState::Outdated)
        .map_or_else(|| "—".to_string(), |line| format!("L{line}"));
    let replies = thread.comments.len().saturating_sub(1);
    let id = thread.id.clone();
    let selector = format!("review-row-{id}");
    let hover = colors.accent.opacity(0.6);
    div()
        .id(SharedString::from(selector.clone()))
        .debug_selector(move || selector.clone())
        .flex()
        .w_full()
        .items_center()
        .gap(px(8.0))
        .rounded(px(6.0))
        .px(px(6.0))
        .py(px(4.0))
        .text_size(px(11.0))
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .when(active == Some(thread.id.as_str()), |row| {
            row.bg(colors.primary.opacity(0.08))
        })
        .when(state == ReviewThreadState::Resolved, |row| row.opacity(0.6))
        .on_click(cx.listener(move |stage, _, _, cx| stage.open_thread(id.clone(), true, cx)))
        .child(
            div()
                .w(px(36.0))
                .flex_shrink_0()
                .font_family(fonts::MONO)
                .text_size(px(10.0))
                .font_features(tabular_nums())
                .text_color(colors.muted_foreground)
                .child(line),
        )
        .child(
            div()
                .flex_shrink_0()
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.foreground)
                .child(author_label(root, cx)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(colors.muted_foreground)
                .child(first_line(&root.body)),
        )
        .when(replies > 0, |row| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(10.0))
                    .font_features(tabular_nums())
                    .text_color(colors.muted_foreground)
                    .child(cx.t_with("reviewThread.bar.replies", &Args::count(replies as i64))),
            )
        })
        .when(state != ReviewThreadState::Open, |row| {
            row.child(state_chip(state, cx))
        })
        .into_any_element()
}

/// `reviewAuthorInitials`
fn initials(login: Option<&str>) -> String {
    match login.map(str::trim).filter(|login| !login.is_empty()) {
        Some(login) => login.chars().take(2).collect::<String>().to_uppercase(),
        None => "?".into(),
    }
}

/// `ReviewThreadDrawer`, over the diff's top right.
pub(crate) fn thread_drawer(
    stage: &Stage,
    window: &mut Window,
    cx: &mut Context<Stage>,
) -> Option<AnyElement> {
    let review = stage.review()?;
    let thread = review.active_thread()?;
    let request = &stage.diff.as_ref()?.request;
    let colors = cx.theme().colors.clone();
    let radius = cx.theme().radius.lg;
    let root = review_thread_root(&thread).clone();
    let line = review_comment_line(&root);
    let state = review_thread_state(&thread);
    let multiple = thread.comments.len() > 1;
    let snippet = stage.thread_snippet(&thread, cx);
    let review_url = root.html_url.clone().or_else(|| {
        thread
            .comments
            .iter()
            .find_map(|comment| comment.html_url.clone())
    });
    let english = cx.global::<crate::i18n::I18n>().locale() == crate::i18n::Locale::En;
    let now = crate::data::time::now_ms();

    let header = div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .border_b_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.muted.opacity(0.3))
        .px(px(12.0))
        .py(px(10.0))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .size(px(28.0))
                .items_center()
                .justify_center()
                .rounded(radius)
                .bg(colors.primary.opacity(0.1))
                .border_1()
                .border_color(colors.primary.opacity(0.15))
                .text_color(colors.primary)
                .child(Icon::new("message-square").size(14.0).stroke(2.2)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .truncate()
                                .text_size(px(12.0))
                                .line_height(gpui::relative(1.25))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(cx.t("reviewThread.title")),
                        )
                        .when(multiple, |row| {
                            row.child(
                                div()
                                    .flex_shrink_0()
                                    .rounded_full()
                                    .bg(colors.muted)
                                    .px(px(6.0))
                                    .text_size(px(9.5))
                                    .line_height(px(15.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .font_features(tabular_nums())
                                    .text_color(colors.muted_foreground)
                                    .child(thread.comments.len().to_string()),
                            )
                        })
                        .child(state_chip(state, cx)),
                )
                .child(
                    div()
                        .debug_selector(|| "thread-path".into())
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis_start()
                        .font_family(fonts::MONO)
                        .text_size(px(10.0))
                        .line_height(gpui::relative(1.25))
                        .text_color(colors.muted_foreground)
                        .child(match line {
                            Some(line) => format!("{}:{line}", request.path),
                            None => request.path.clone(),
                        }),
                ),
        )
        .child(
            Button::new("thread-close")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconXs)
                .muted()
                .on_click(cx.listener(|stage, _, _, cx| stage.close_thread(cx)))
                .child(Icon::new("x").size(14.0).stroke(2.0)),
        );

    let comments = thread.comments.iter().map(|comment| {
        let login = comment
            .author
            .as_ref()
            .and_then(|author| author.login.as_deref());
        let time = comment
            .created_at
            .as_deref()
            .and_then(crate::data::time::parse_ms)
            .map(|at| crate::data::time::relative_time(at, now, english));
        div()
            .relative()
            .pl(px(36.0))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .flex()
                    .size(px(24.0))
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(colors.muted)
                    .border_2()
                    .border_color(colors.popover)
                    .text_size(px(9.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(colors.muted_foreground)
                    .child(initials(login)),
            )
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(6.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.foreground)
                            .child(author_label(comment, cx)),
                    )
                    .children(time.map(|time| {
                        div()
                            .flex_shrink_0()
                            .text_size(px(10.0))
                            .text_color(colors.muted_foreground)
                            .child(time)
                    })),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(12.0))
                    .line_height(gpui::relative(1.55))
                    .text_color(colors.foreground.opacity(0.9))
                    .child(SharedString::from(comment.body.clone())),
            )
    });
    let figure = (!snippet.trim().is_empty()).then(|| {
        let caption = cx.t("reviewThread.codeContext").to_uppercase();
        div()
            .mt(px(12.0))
            .overflow_hidden()
            .rounded(radius)
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.muted.opacity(0.3))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .border_b_1()
                    .border_color(colors.border.opacity(0.5))
                    .px(px(10.0))
                    .py(px(4.0))
                    .text_size(px(9.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.muted_foreground)
                    .child(
                        div()
                            .size(px(4.0))
                            .rounded_full()
                            .bg(colors.primary.opacity(0.6)),
                    )
                    .child(Tracked::new(caption, 9.5, 0.025))
                    .children(line.map(|line| {
                        div()
                            .ml_auto()
                            .font_family(fonts::MONO)
                            .text_color(colors.foreground.opacity(0.7))
                            .child(format!("L{line}"))
                    })),
            )
            .child(
                div()
                    .id("thread-snippet")
                    .max_h(px(144.0))
                    .overflow_scroll()
                    .px(px(10.0))
                    .py(px(8.0))
                    .font_family(fonts::MONO)
                    .text_size(px(10.5))
                    .line_height(gpui::relative(1.5))
                    .text_color(colors.foreground.opacity(0.8))
                    .whitespace_nowrap()
                    .children(
                        snippet
                            .replace('\t', "  ")
                            .split('\n')
                            .map(|line| div().child(SharedString::from(line.to_string())))
                            .collect::<Vec<_>>(),
                    ),
            )
    });
    let body = div()
        .id("thread-body")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px(px(12.0))
        .py(px(12.0))
        .child(
            div()
                .relative()
                .flex()
                .flex_col()
                .gap(px(14.0))
                .when(multiple, |list| {
                    list.child(
                        div()
                            .absolute()
                            .left(px(11.0))
                            .top(px(8.0))
                            .bottom(px(8.0))
                            .w(px(1.0))
                            .bg(colors.border.opacity(0.7)),
                    )
                })
                .children(comments),
        )
        .children(figure);

    // Reply and resolve run on the forge, once the PR's number is known.
    let forge = review.number.is_some().then(|| {
        let busy = review.busy;
        let empty = review.reply.read(cx).text().trim().is_empty();
        let focused = review.reply.read(cx).focus_handle(cx).is_focused(window);
        let resolve = review_thread_resolvable(&thread).then(|| {
            Button::new("thread-resolve")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Xs)
                .height(28.0)
                .padding_x(8.0)
                .text_size(12.0)
                .muted()
                .disabled(busy.is_some())
                .on_click(cx.listener(|stage, _, _, cx| stage.toggle_thread_resolved(cx)))
                .child(if busy == Some(ForgeBusy::Resolve) {
                    spinner("thread-busy", 14.0).into_any_element()
                } else {
                    Icon::new("check").size(14.0).stroke(2.0).into_any_element()
                })
                .child(cx.t(if state == ReviewThreadState::Resolved {
                    "reviewThread.reopen"
                } else {
                    "reviewThread.resolve"
                }))
        });
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .border_t_1()
            .border_color(colors.border.opacity(0.6))
            .px(px(12.0))
            .py(px(10.0))
            .child(
                div()
                    .key_context(DRAWER_CONTEXT)
                    .on_action(
                        cx.listener(|stage, _: &ReplyToThread, _, cx| stage.reply_to_thread(cx)),
                    )
                    .debug_selector(|| "thread-reply".into())
                    .min_h(px(52.0))
                    .w_full()
                    .rounded(radius)
                    .border_1()
                    .border_color(if focused { colors.ring } else { colors.input })
                    .px(px(10.0))
                    .py(px(8.0))
                    .text_size(px(12.0))
                    .when(busy.is_some(), |field| field.opacity(0.5))
                    .child(review.reply.clone()),
            )
            .children(review.error.clone().map(|error| {
                div()
                    .text_size(px(10.5))
                    .line_height(gpui::relative(1.375))
                    .text_color(colors.destructive)
                    .child(error)
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .children(resolve)
                    .child(
                        div().ml_auto().child(
                            Button::new("thread-reply-send")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Xs)
                                .height(28.0)
                                .text_size(12.0)
                                .disabled(busy.is_some() || empty)
                                .on_click(cx.listener(|stage, _, _, cx| stage.reply_to_thread(cx)))
                                .children(
                                    (busy == Some(ForgeBusy::Reply))
                                        .then(|| spinner("thread-busy", 14.0).into_any_element()),
                                )
                                .child(cx.t("reviewThread.reply")),
                        ),
                    ),
            )
    });

    let footer = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .border_t_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.muted.opacity(0.2))
        .px(px(12.0))
        .py(px(10.0))
        .child(match review_url {
            Some(url) => div()
                .mr_auto()
                .child(
                    Button::new("thread-open-on-forge")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Xs)
                        .height(28.0)
                        .padding_x(8.0)
                        .text_size(12.0)
                        .muted()
                        .on_click(move |_, _, cx| cx.open_url(&url))
                        .child(Icon::new("external-link").size(14.0).stroke(2.0))
                        .child(cx.t("reviewThread.openOnForge")),
                )
                .into_any_element(),
            None => div().mr_auto().into_any_element(),
        })
        .child(
            Button::new("thread-composer")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Xs)
                .height(28.0)
                .text_size(12.0)
                .on_click(cx.listener(|stage, _, _, cx| stage.thread_to_agent(false, cx)))
                .child(cx.t("reviewThread.composer")),
        )
        .child(
            Button::new("thread-send-to-agent")
                .variant(ButtonVariant::Default)
                .size(ButtonSize::Xs)
                .height(28.0)
                .text_size(12.0)
                .on_click(cx.listener(|stage, _, _, cx| stage.thread_to_agent(true, cx)))
                .child(Icon::new("send").size(14.0).stroke(2.0))
                .child(cx.t("reviewThread.sendToAgent")),
        );

    let viewport = window.viewport_size();
    let max_height = (f32::from(viewport.height) * 0.72).min(580.0);
    Some(
        div()
            .id("thread-drawer")
            .debug_selector(|| "thread-drawer".into())
            .absolute()
            .right(px(12.0))
            .top(px(48.0))
            .flex()
            .flex_col()
            .max_h(px(max_height))
            .w(px(368.0))
            .overflow_hidden()
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.border.opacity(0.8))
            .bg(colors.popover)
            .text_color(colors.popover_foreground)
            .shadow_xl()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(header)
            .child(body)
            .children(forge)
            .child(footer)
            .into_any_element(),
    )
}
