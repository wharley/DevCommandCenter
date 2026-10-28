//! `ExecutionContextRail` under the composer: where the task runs (protected
//! worktree or the project's own checkout, explained on click), its project
//! and the branch git reports, and a setup step that failed.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::{Composer, ComposerEvent};
use crate::data::conversation::ExecutionContext;
use crate::i18n::{Args, Translate};
use crate::project::{GlyphSize, ProjectGlyph};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::Icon;
use crate::ui::popover::{Popover, PopoverSide};

/// `workingBranchLabel`: git's "HEAD" (a detached checkout, how a task
/// worktree starts) reads as "no branch" next to its base.
pub fn working_branch(
    current: Option<&str>,
    base: Option<&str>,
    detached: impl Fn(Option<&str>) -> String,
) -> Option<String> {
    let base = base.map(str::trim).filter(|base| !base.is_empty());
    if current == Some("HEAD") {
        return Some(detached(base.filter(|base| *base != "HEAD")));
    }
    current
        .map(str::trim)
        .filter(|current| !current.is_empty())
        .or(base)
        .map(str::to_string)
}

/// The first setup step that warned or failed (mission spec and forge
/// metadata refreshes don't count).
fn setup_problem(context: &ExecutionContext) -> Option<(Option<String>, Option<String>)> {
    if !context.isolated {
        return None;
    }
    let report = context.setup_report.as_ref()?;
    let status = |status: &dcc_core::domain::workspace::WorkspaceSetupStatus| {
        let key = format!("{status:?}").to_lowercase();
        key == "warning" || key == "failed"
    };
    let step = report.steps.iter().find(|step| {
        step.command != "compile_mission_spec_context"
            && step.command != "refresh_repository_forge_metadata"
            && status(&step.status)
    });
    let failed_without_steps =
        format!("{:?}", report.status).to_lowercase() == "failed" && report.steps.is_empty();
    if step.is_none() && !failed_without_steps {
        return None;
    }
    Some((
        step.and_then(|step| step.detail.clone())
            .or_else(|| report.message.clone())
            .or_else(|| step.map(|step| step.label.clone())),
        None,
    ))
}

fn mode_option(
    icon: &'static str,
    title: SharedString,
    description: SharedString,
    chosen: bool,
    highlight: gpui::Hsla,
    protected: bool,
    cx: &gpui::App,
) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .items_start()
        .gap(px(8.0))
        .rounded(px(8.0))
        .p(px(8.0))
        .when(chosen, |option| option.bg(highlight))
        .child(
            div()
                .mt(px(2.0))
                .child(Icon::new(icon).size(16.0).color(if protected {
                    tw::EMERALD_500.hsla()
                } else {
                    colors.foreground
                })),
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
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(title)
                        .when(chosen, |row| row.child(Icon::new("check").size(12.0))),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(colors.muted_foreground)
                        .child(description),
                ),
        )
        .into_any_element()
}

pub(super) fn context_rail(composer: &Composer, cx: &mut Context<Composer>) -> Option<AnyElement> {
    let context = composer.session.context.clone()?;
    let colors = cx.theme().colors.clone();
    let dark = cx.theme().appearance == Appearance::Dark;
    let isolated = context.isolated;
    let label = context
        .project_label
        .clone()
        .map(SharedString::from)
        .unwrap_or_else(|| cx.t("composer.executionDock.projectFallback"));
    let detached_label = |base: Option<&str>| match base {
        Some(branch) => cx
            .t_with(
                "workingBranch.detached",
                &Args::default().with("branch", branch.to_string()),
            )
            .to_string(),
        None => cx.t("workingBranch.detachedNoBase").to_string(),
    };
    let branch = working_branch(
        context.current_branch.as_deref(),
        context.base_branch.as_deref(),
        detached_label,
    );
    let banner = setup_problem(&context).map(|(detail, _)| {
        let detail = detail
            .map(SharedString::from)
            .unwrap_or_else(|| cx.t("composer.executionDock.setup.commandFailed"));
        div()
            .flex()
            .min_w_0()
            .items_center()
            .gap(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(tw::AMBER_500.alpha(0.2))
            .bg(tw::AMBER_500.alpha(0.06))
            .px(px(10.0))
            .py(px(8.0))
            .child(
                Icon::new("triangle-alert")
                    .size(14.0)
                    .color(tw::AMBER_500.hsla()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(10.5))
                            .font_weight(FontWeight::MEDIUM)
                            .child(cx.t("composer.executionDock.setup.failed")),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(9.5))
                            .text_color(colors.muted_foreground)
                            .child(detail),
                    ),
            )
            .child(
                // `onOpenTerminal`: the dock on the worktree.
                div()
                    .id("setup-open-terminal")
                    .debug_selector(|| "setup-open-terminal".into())
                    .flex()
                    .h(px(24.0))
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(6.0))
                    .bg(tw::AMBER_500.alpha(0.15))
                    .hover(|style| style.bg(tw::AMBER_500.alpha(0.25)))
                    .cursor_pointer()
                    .px(px(8.0))
                    .text_size(px(10.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if dark {
                        tw::AMBER_200.hsla()
                    } else {
                        tw::AMBER_700.hsla()
                    })
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(ComposerEvent::OpenTerminal)))
                    .child(Icon::new("terminal").size(12.0))
                    .child(cx.t("workbench.terminal.open")),
            )
    });
    let hover = colors.muted.opacity(0.45);
    let trigger = div()
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(6.0))
        .rounded(px(6.0))
        .px(px(6.0))
        .py(px(4.0))
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .child(if isolated {
            Icon::new("shield-check")
                .size(14.0)
                .color(tw::EMERALD_500.hsla())
                .into_any_element()
        } else {
            Icon::new("folder-git-2").size(14.0).into_any_element()
        })
        .child(div().truncate().child(if isolated {
            cx.t("newTask.execution.protected")
        } else {
            cx.t("newTask.execution.local")
        }));
    let options = [
        (
            "shield-check",
            cx.t("newTask.execution.protected"),
            cx.t("newTask.execution.protectedDescription"),
            isolated,
            tw::EMERALD_500.alpha(0.06),
            true,
        ),
        (
            "folder-git-2",
            cx.t("newTask.execution.local"),
            cx.t("newTask.execution.localDescription"),
            !isolated,
            colors.muted.opacity(0.55),
            false,
        ),
    ];
    let fixed = cx.t("newTask.execution.fixedAfterCreation");
    let entity = cx.entity().downgrade();
    let mode = Popover::new(
        "execution-mode",
        composer.mode_open,
        trigger,
        move |_, cx| {
            let muted = cx.theme().colors.muted_foreground;
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .children(options.into_iter().map(
                    |(icon, title, description, chosen, highlight, protected)| {
                        mode_option(icon, title, description, chosen, highlight, protected, cx)
                    },
                ))
                .child(
                    div()
                        .px(px(8.0))
                        .pb(px(4.0))
                        .pt(px(8.0))
                        .text_size(px(9.5))
                        .text_color(muted)
                        .child(fixed),
                )
                .into_any_element()
        },
        move |open, _, cx| {
            let _ = entity.update(cx, |composer, cx| {
                composer.mode_open = open;
                cx.notify();
            });
        },
    )
    .side(PopoverSide::Top)
    .width(320.0)
    .padding(8.0);
    // `NoteCaptureButton`: a note from this task (⌘⇧N does the same).
    let capture = {
        let colors = cx.theme().colors.clone();
        let (fg, popover) = (colors.foreground, colors.popover);
        let shadow = |alpha: u8, y: f32, blur: f32| gpui::BoxShadow {
            color: gpui::black().opacity(alpha as f32 / 255.0),
            offset: gpui::point(px(0.0), px(y)),
            blur_radius: px(blur),
            spread_radius: px(0.0),
            inset: false,
        };
        crate::ui::Tooltip::text(
            "notes-capture-tip",
            div()
                .id("notes-capture")
                .debug_selector(|| "notes-capture".into())
                .flex()
                .flex_none()
                .size(px(32.0))
                .items_center()
                .justify_center()
                .border_1()
                .border_color(colors.border)
                .rounded_tl(px(20.0))
                .rounded_tr(px(20.0))
                .rounded_br(px(6.0))
                .rounded_bl(px(20.0))
                .text_color(colors.muted_foreground)
                .bg(popover)
                .shadow(vec![shadow(0x11, 5.0, 20.0)])
                .cursor_pointer()
                .hover(move |button| button.text_color(fg).shadow(vec![shadow(0x22, 8.0, 26.0)]))
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(crate::screens::shell::CaptureNote), cx)
                })
                .child(Icon::new("lightbulb").size(16.0)),
            cx.t("notes.captureShortcut"),
        )
        .side(crate::ui::tooltip::Side::Top)
        .align(crate::ui::tooltip::Align::End)
        .offset(10.0)
    };
    Some(
        div()
            .mt(px(8.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .px(px(4.0))
            .children(banner)
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .items_center()
                    .justify_between()
                    .gap(px(20.0))
                    .px(px(4.0))
                    .text_size(px(10.5))
                    .text_color(colors.muted_foreground)
                    .child(
                        div()
                            .flex()
                            .min_w_0()
                            .max_w(gpui::relative(0.5))
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(12.0))
                            .child(mode)
                            .child(capture),
                    )
                    .child(
                        div()
                            .ml_auto()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .flex_wrap()
                            .items_center()
                            .justify_end()
                            .gap(px(16.0))
                            .child(
                                div()
                                    .flex()
                                    .min_w_0()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div().pl(px(4.0)).flex_shrink_0().child(
                                            ProjectGlyph::new(
                                                context.project_icon.as_deref(),
                                                context.project_color.as_deref(),
                                                context.project_seed.as_deref(),
                                            )
                                            .size(GlyphSize::Tiny),
                                        ),
                                    )
                                    .child(div().max_w(px(176.0)).truncate().child(label)),
                            )
                            .children(branch.map(|branch| {
                                div()
                                    .flex()
                                    .min_w_0()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(Icon::new("git-branch").size(14.0))
                                    .child(div().min_w_0().child(SharedString::from(branch)))
                            })),
                    ),
            )
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::working_branch;

    #[test]
    fn detached_checkouts_read_as_no_branch() {
        let detached = |base: Option<&str>| match base {
            Some(base) => format!("{base} · sem branch"),
            None => "sem branch".into(),
        };
        assert_eq!(
            working_branch(Some("HEAD"), Some("main"), detached).as_deref(),
            Some("main · sem branch")
        );
        assert_eq!(
            working_branch(Some("HEAD"), Some("HEAD"), detached).as_deref(),
            Some("sem branch")
        );
        assert_eq!(
            working_branch(Some("dcc/x"), Some("main"), detached).as_deref(),
            Some("dcc/x")
        );
        assert_eq!(
            working_branch(None, Some("main"), detached).as_deref(),
            Some("main")
        );
        assert_eq!(working_branch(None, None, detached), None);
    }
}
