//! `ForgeIdentityChip`: next to the delivery button, who commits and
//! pushes (`@login`, the picture, the forge's dot); amber when that account
//! is not the one the task is bound to.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, img, prelude::*, px};

use super::super::CodeColumn;
use crate::data::avatar::{avatar, initials};
use crate::i18n::{Args, Translate};
use crate::theme::color::hex;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::Tooltip;
use crate::ui::tooltip::Side;

pub(crate) fn identity_chip(
    column: &CodeColumn,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    let forge = column
        .review
        .forge
        .as_ref()
        .filter(|forge| forge.connected())?;
    let login = forge
        .effective_login
        .clone()
        .or_else(|| forge.bound_login.clone())?;
    let colors = cx.theme().colors.clone();
    let dark = cx.theme().appearance == Appearance::Dark;
    let bound = forge
        .bound_login
        .clone()
        .filter(|bound| !bound.trim().is_empty());
    let mismatch = bound.as_ref().is_some_and(|bound| bound != &login);
    let account = column
        .review
        .account
        .as_ref()
        .filter(|account| account.login == login);
    let label = account
        .and_then(|account| account.name.clone())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| login.clone());
    let picture = avatar(
        account.and_then(|account| account.avatar_url.as_deref()),
        cx,
    );
    let handle = format!("@{login}");
    let dot = if forge.provider.as_deref() == Some("gitlab") {
        hex(0xFC6D26)
    } else {
        colors.foreground
    };
    let face = div()
        .flex()
        .size(px(16.0))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .rounded_full()
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.background)
        .text_size(px(8.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.foreground)
        .map(|face| match picture {
            Some(picture) => face.child(img(picture).size_full().rounded_full()),
            None => face.child(initials(&label)),
        });
    let chip = div()
        .id("forge-identity")
        .debug_selector(|| "forge-identity".into())
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(6.0))
        .rounded_full()
        .border_1()
        .px(px(6.0))
        .py(px(2.0))
        .text_size(px(11.0))
        .font_weight(FontWeight::MEDIUM)
        .map(|chip| {
            if mismatch {
                chip.border_color(tw::AMBER_500.alpha(0.4))
                    .bg(tw::AMBER_500.alpha(0.1))
                    .text_color(if dark { tw::AMBER_100 } else { tw::AMBER_950 }.hsla())
            } else {
                chip.border_color(colors.border.opacity(0.6))
                    .bg(colors.background.opacity(0.8))
                    .text_color(colors.muted_foreground)
            }
        })
        .child(face)
        .child(div().max_w(px(112.0)).truncate().child(handle.clone()))
        .when(forge.provider.is_some(), |chip| {
            chip.child(div().size(px(6.0)).flex_shrink_0().rounded_full().bg(dot))
        });
    let tip: SharedString = match bound {
        Some(bound) if mismatch => cx.t_with(
            "inspector.identityChip.mismatch",
            &Args::default().with("handle", handle).with("bound", bound),
        ),
        _ => cx.t_with(
            "inspector.identityChip.commitAs",
            &Args::default().with("handle", handle).with(
                "location",
                forge
                    .host
                    .as_ref()
                    .map(|host| format!(" ({host})"))
                    .unwrap_or_default(),
            ),
        ),
    };
    Some(
        Tooltip::new("forge-identity-tip", chip, move |_, _| {
            div().child(tip.clone()).into_any_element()
        })
        .side(Side::Bottom)
        .into_any_element(),
    )
}
