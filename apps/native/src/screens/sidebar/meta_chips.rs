//! `WorkspaceRailMetaChips`: the PR number and the uncommitted +/−,
//! right-aligned on a row's state line.

use gpui::{AnyElement, App, Hsla, SharedString, div, prelude::*, px};

use crate::data::recap::{PrState, RailMeta};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Icon, tabular_nums};

fn by_theme(cx: &App, light: Hsla, dark: Hsla) -> Hsla {
    match cx.theme().appearance {
        Appearance::Light => light,
        Appearance::Dark => dark,
    }
}

/// `PR_STATE_CLASS`
fn pr_color(state: Option<PrState>, cx: &App) -> Hsla {
    let muted = cx.theme().colors.muted_foreground;
    match state {
        Some(PrState::Open) => by_theme(cx, tw::EMERALD_700.hsla(), tw::EMERALD_300.alpha(0.9)),
        Some(PrState::Merged) => by_theme(cx, tw::VIOLET_700.hsla(), tw::VIOLET_300.alpha(0.9)),
        Some(PrState::Closed) => muted.opacity(0.7),
        Some(PrState::Draft) | None => muted,
    }
}

pub fn render(meta: &RailMeta, cx: &App) -> AnyElement {
    let has_diff = meta.additions > 0 || meta.deletions > 0;
    let pr = meta.pr_number.map(|number| {
        div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .text_color(pr_color(meta.pr_state, cx))
            .when(meta.pr_state == Some(PrState::Closed), |chip| {
                chip.line_through()
            })
            .child(Icon::new("git-pull-request").size(10.0))
            .child(SharedString::from(format!("#{number}")))
    });
    let additions = by_theme(cx, tw::EMERALD_700.hsla(), tw::EMERALD_400.hsla());
    let deletions = by_theme(cx, tw::RED_600.hsla(), tw::RED_400.hsla());
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .text_size(px(10.0))
        .line_height(px(16.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .font_features(tabular_nums())
        .children(pr)
        .when(has_diff, |chips| {
            chips.child(
                div()
                    .flex()
                    .gap(px(3.0))
                    .child(
                        div()
                            .text_color(additions)
                            .child(format!("+{}", meta.additions)),
                    )
                    .child(
                        div()
                            .text_color(deletions)
                            .child(format!("\u{2212}{}", meta.deletions)),
                    ),
            )
        })
        .into_any_element()
}
