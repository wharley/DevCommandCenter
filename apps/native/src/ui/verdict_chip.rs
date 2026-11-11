//! The reviewer's verdict as a small colored chip — green "Pode aprovar",
//! red "Pedir mudanças", amber "Falta decidir" — in the pull request's
//! review footer and on the team room's review card.

use gpui::{AnyElement, App, FontWeight, SharedString, div, prelude::*, px};

use crate::data::pr_review_verdict::Verdict;
use crate::i18n::Translate;
use crate::theme::{ActiveTheme, Appearance, tailwind as tw};
use crate::ui::Icon;

pub fn verdict_chip(id: impl Into<SharedString>, verdict: Verdict, cx: &App) -> AnyElement {
    let dark = cx.theme().appearance == Appearance::Dark;
    let (base, ink, icon) = match verdict {
        Verdict::Approve => (
            tw::EMERALD_500.hsla(),
            if dark {
                tw::EMERALD_400
            } else {
                tw::EMERALD_700
            }
            .hsla(),
            "check",
        ),
        Verdict::RequestChanges => (
            tw::RED_500.hsla(),
            if dark { tw::RED_400 } else { tw::RED_600 }.hsla(),
            "x",
        ),
        Verdict::Undecided => (
            tw::AMBER_500.hsla(),
            if dark { tw::AMBER_400 } else { tw::AMBER_600 }.hsla(),
            "circle-question-mark",
        ),
    };
    let id: SharedString = id.into();
    div()
        .debug_selector(move || id.to_string())
        .flex_none()
        .flex()
        .items_center()
        .gap(px(4.0))
        .px(px(7.0))
        .py(px(2.0))
        .rounded_full()
        .bg(base.opacity(0.12))
        .text_size(px(10.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(ink)
        .child(Icon::new(icon).size(11.0).stroke(2.4).color(ink))
        .child(cx.t(verdict.key()))
        .into_any_element()
}
