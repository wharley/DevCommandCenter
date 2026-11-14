//! The agent's part of the resolver: the "Resolver com agente" button, the
//! strip while its run works (with "Cancelar") or after it failed, and the
//! strip of its proposal — what it says it did, block by block, with
//! "Descartar" and "Aceitar todos". The blocks themselves are accepted or
//! refused where they are, in the editor (`proposal`).

use gpui::{AnyElement, Context, FontWeight, div, prelude::*, px};

use super::agent::AgentStatus;
use super::proposal::BlockState;
use super::view::dark;
use super::{Busy, MergeResolver};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, spinner};

/// "Resolver com agente", in the file's bar.
pub(crate) fn resolve_button(
    resolver: &MergeResolver,
    cx: &mut Context<MergeResolver>,
) -> AnyElement {
    let title = cx.t("mergeConflict.agent.resolveFileTitle");
    let working = resolver.busy == Some(Busy::Agent);
    Tooltip::new(
        "merge-resolve-with-agent-tip",
        Button::new("merge-resolve-with-agent")
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::Xs)
            .disabled(resolver.busy.is_some())
            .on_click(cx.listener(|resolver, _, _, cx| resolver.resolve_with_agent(cx)))
            .child(if working {
                spinner("merge-agent-busy", 14.0).into_any_element()
            } else {
                Icon::new("sparkles").size(14.0).into_any_element()
            })
            .child(cx.t("mergeConflict.agent.resolveFile")),
        move |_, _| {
            div()
                .max_w(px(280.0))
                .child(title.clone())
                .into_any_element()
        },
    )
    .into_any_element()
}

/// How much of an unusable answer is shown.
const RAW_SHOWN: usize = 6_000;

/// The run: working (and how to stop it), or what went wrong.
fn run_strip(resolver: &MergeResolver, cx: &mut Context<MergeResolver>) -> Option<AnyElement> {
    let run = resolver.agent.as_ref()?;
    let colors = cx.theme().colors.clone();
    let sky = if dark(cx) {
        tw::SKY_300.hsla()
    } else {
        tw::SKY_700.hsla()
    };
    let strip = div()
        .debug_selector(|| "merge-agent-run".into())
        .flex()
        .items_center()
        .gap(px(10.0))
        .text_size(px(12.0));
    Some(match run.status {
        AgentStatus::Failed => {
            // What it wrote, on request: at most the start of a long answer.
            let answer = run.raw.as_ref().filter(|_| run.raw_open).map(|raw| {
                let shown: String = raw.chars().take(RAW_SHOWN).collect();
                div()
                    .id("merge-agent-answer")
                    .debug_selector(|| "merge-agent-answer".into())
                    .max_h(px(220.0))
                    .overflow_y_scroll()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.7))
                    .bg(colors.muted.opacity(0.3))
                    .p(px(8.0))
                    .font_family(cx.theme().fonts.mono.clone())
                    .text_size(px(11.0))
                    .line_height(px(16.0))
                    .text_color(colors.muted_foreground)
                    .children(
                        shown
                            .lines()
                            .map(|line| div().min_h(px(16.0)).child(line.to_string()))
                            .collect::<Vec<_>>(),
                    )
            });
            let row = strip
                .child(
                    Icon::new("triangle-alert")
                        .size(14.0)
                        .color(colors.destructive),
                )
                .child(
                    div()
                        .debug_selector(|| "merge-agent-error".into())
                        .flex_1()
                        .min_w_0()
                        .text_color(colors.destructive)
                        .child(run.error.clone().unwrap_or_default()),
                )
                .when(run.raw.is_some(), |row| {
                    row.child(
                        div()
                            .debug_selector(|| "merge-agent-show-answer".into())
                            .child(
                                Button::new("merge-agent-show-answer")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::Xs)
                                    .on_click(cx.listener(|resolver, _, _, cx| {
                                        resolver.toggle_agent_answer(cx)
                                    }))
                                    .child(cx.t(if run.raw_open {
                                        "mergeConflict.agent.hideAnswer"
                                    } else {
                                        "mergeConflict.agent.showAnswer"
                                    })),
                            ),
                    )
                })
                .child(
                    Button::new("merge-agent-dismiss")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconXs)
                        .on_click(
                            cx.listener(|resolver, _, _, cx| resolver.dismiss_agent_error(cx)),
                        )
                        .child(Icon::new("x").size(14.0)),
                );
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(row)
                .children(answer)
                .into_any_element()
        }
        status => {
            let stopping = status == AgentStatus::Cancelling;
            strip
                .child(spinner("merge-agent-working", 14.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(sky)
                                .child(cx.t(if stopping {
                                    "mergeConflict.agent.run.cancelling"
                                } else {
                                    "mergeConflict.agent.run.analyzing"
                                })),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(colors.muted_foreground)
                                .child(cx.t("mergeConflict.agent.readOnlyRun")),
                        ),
                )
                .child(
                    Button::new("merge-agent-cancel")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Xs)
                        .disabled(stopping)
                        .on_click(cx.listener(|resolver, _, _, cx| resolver.cancel_agent(cx)))
                        .child(cx.t("mergeConflict.agent.cancel")),
                )
                .into_any_element()
        }
    })
}

/// The proposal: what the agent says it did, and keeping or dropping it.
fn proposal_strip(resolver: &MergeResolver, cx: &mut Context<MergeResolver>) -> Option<AnyElement> {
    let proposal = resolver.proposal.as_ref()?;
    let open = resolver.open.as_ref()?;
    if proposal.path != open.entry.path {
        return None;
    }
    let colors = cx.theme().colors.clone();
    let offered = proposal.offered(&open.buffer);
    let whole = proposal.blocks.is_empty();
    let count = if whole {
        cx.t("mergeConflict.proposal.wholeFile")
    } else {
        cx.t_with(
            "mergeConflict.proposal.blocks",
            &Args::count(offered as i64),
        )
    };
    let offered_at: Vec<usize> = proposal
        .lines(&open.buffer)
        .into_iter()
        .map(|(index, _)| index)
        .collect();
    // What it did to the blocks still offered, in the file's order.
    let notes: Vec<AnyElement> = proposal
        .blocks
        .iter()
        .enumerate()
        .filter(|(index, block)| block.state == BlockState::Pending && offered_at.contains(index))
        .map(|(index, block)| {
            let on = proposal.at == index;
            let hover = colors.accent.opacity(0.6);
            // A click takes the editor to that block.
            div()
                .id(("merge-proposal-note", index))
                .debug_selector(move || format!("merge-proposal-note-{index}"))
                .flex()
                .gap(px(6.0))
                .rounded(px(4.0))
                .px(px(4.0))
                .mx(px(-4.0))
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .cursor_pointer()
                .when(on, |row| row.text_color(colors.foreground))
                .hover(move |row| row.bg(hover))
                .on_click(
                    cx.listener(move |resolver, _, _, cx| resolver.reveal_proposed(index, cx)),
                )
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t_with(
                            "mergeConflict.proposal.block",
                            &Args::default().with("number", (index + 1).to_string()),
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(block.note.clone().unwrap_or_default()),
                )
                .into_any_element()
        })
        .collect();
    Some(
        div()
            .debug_selector(|| "merge-proposal".into())
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(12.0))
                    .child(
                        Icon::new("sparkles")
                            .size(14.0)
                            .color(tw::EMERALD_500.hsla()),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(cx.t("mergeConflict.proposal.title")),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(count),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("merge-proposal-discard")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Xs)
                            .on_click(
                                cx.listener(|resolver, _, _, cx| resolver.discard_proposal(cx)),
                            )
                            .child(cx.t("mergeConflict.proposal.discard")),
                    )
                    .child(
                        Button::new("merge-proposal-accept-all")
                            .size(ButtonSize::Xs)
                            .on_click(
                                cx.listener(|resolver, _, _, cx| resolver.accept_proposal(cx)),
                            )
                            .child(cx.t(if whole {
                                "mergeConflict.proposal.keep"
                            } else {
                                "mergeConflict.proposal.acceptAll"
                            })),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .child(proposal.explanation.clone()),
            )
            // Many blocks scroll inside, so the file keeps its room.
            .child(
                div()
                    .id("merge-proposal-notes")
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .max_h(px(96.0))
                    .overflow_y_scroll()
                    .children(notes),
            )
            .when(!whole, |strip| {
                strip.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("mergeConflict.proposal.together")),
                )
            })
            .into_any_element(),
    )
}

/// The run and the proposal, between the bar and the editor.
pub(crate) fn agent_panels(
    resolver: &MergeResolver,
    cx: &mut Context<MergeResolver>,
) -> Vec<AnyElement> {
    [run_strip(resolver, cx), proposal_strip(resolver, cx)]
        .into_iter()
        .flatten()
        .collect()
}
