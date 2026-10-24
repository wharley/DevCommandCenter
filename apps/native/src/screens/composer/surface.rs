//! `.dcc-composer-surface`: the rounded card on the sidebar color (18px,
//! the surface edge, the card shadow, the calm accent ring while focused)
//! with the prompt field and the toolbar under it.

use gpui::{
    AnyElement, BoxShadow, Context, ExternalPaths, Focusable, FontWeight, SharedString, Window,
    div, point, prelude::*, px,
};

use super::{Busy, Composer, popups};
use crate::project::{ProviderIcon, model_label};
use crate::theme::ActiveTheme;
use crate::theme::color::mix_oklab;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, spinner};

/// `--dcc-composer-input-min-height` (4rem).
const INPUT_MIN_HEIGHT: f32 = 64.0;

/// `getCompactComposerModelLabel`: Claude's models drop the "Claude ".
fn compact_model(provider: Option<&str>, model: &str) -> String {
    let label = model_label(model);
    if provider == Some("claude_code") {
        let trimmed = label
            .strip_prefix("Claude ")
            .unwrap_or(&label)
            .trim()
            .to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    label
}

fn model_chip(composer: &Composer, cx: &gpui::App) -> Option<AnyElement> {
    let model = composer.session.model.as_deref()?;
    let provider = composer.session.provider_id.as_deref();
    let colors = &cx.theme().colors;
    // The picker (provider, model, effort) arrives with screen 3c; the chip
    // shows the session's model meanwhile.
    Some(
        div()
            .flex()
            .h(px(28.0))
            .min_w_0()
            .max_w(px(224.0))
            .items_center()
            .gap(px(6.0))
            .px(px(4.0))
            .rounded(px(9.0))
            .text_color(colors.muted_foreground)
            .children(provider.map(|provider| ProviderIcon::new(provider).size(13.0)))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .child(SharedString::from(compact_model(provider, model))),
            )
            .into_any_element(),
    )
}

fn action_button(composer: &Composer, cx: &mut Context<Composer>) -> AnyElement {
    if composer.session.running {
        let stopping = composer.busy == Some(Busy::Stopping);
        return Button::new("composer-stop")
            .variant(ButtonVariant::Destructive)
            .size(ButtonSize::Icon)
            .disabled(stopping)
            .on_click(cx.listener(|composer, _, _, cx| composer.stop(cx)))
            .child(if stopping {
                spinner("composer-stopping", 14.0).into_any_element()
            } else {
                div()
                    .size(px(12.0))
                    .rounded(px(2.0))
                    .bg(cx.theme().colors.destructive)
                    .into_any_element()
            })
            .into_any_element();
    }
    let sending = composer.busy == Some(Busy::Sending);
    Button::new("composer-send")
        .size(ButtonSize::Icon)
        .disabled(!composer.can_send(cx))
        .on_click(cx.listener(|composer, _, window, cx| composer.send(window, cx)))
        .child(if sending {
            spinner("composer-sending", 15.0).into_any_element()
        } else {
            Icon::new("arrow-up")
                .size(15.0)
                .stroke(2.2)
                .into_any_element()
        })
        .into_any_element()
}

pub(super) fn render(
    composer: &mut Composer,
    window: &mut Window,
    cx: &mut Context<Composer>,
) -> AnyElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let focused = composer
        .input
        .read(cx)
        .focus_handle(cx)
        .contains_focused(window, cx);
    let edge = colors.border.opacity(0.8);
    let border = if focused {
        mix_oklab(colors.accent_calm, 0.45, colors.border)
    } else {
        edge
    };
    let mut shadows = vec![
        BoxShadow {
            color: gpui::black().opacity(0.025),
            offset: point(px(0.0), px(2.0)),
            blur_radius: px(3.0),
            spread_radius: px(0.0),
            inset: false,
        },
        BoxShadow {
            color: gpui::black().opacity(0.04),
            offset: point(px(0.0), px(8.0)),
            blur_radius: px(24.0),
            spread_radius: px(0.0),
            inset: false,
        },
    ];
    if focused {
        shadows.push(BoxShadow {
            color: colors.accent_calm.opacity(0.07),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(3.0),
            inset: false,
        });
    }
    let disabled = composer.session.session_id.is_none();
    let input = composer.input.clone();
    let drop_border = mix_oklab(colors.accent_calm, 0.7, colors.border);
    div()
        .flex_shrink_0()
        .px(px(16.0))
        .pb(px(12.0))
        .pt(px(8.0))
        .child(
            div().mx_auto().w_full().max_w(px(832.0)).child(
                div()
                    .id("composer")
                    .relative()
                    .on_drop(cx.listener(|composer, paths: &ExternalPaths, _, cx| {
                        composer.add_paths(paths.paths().to_vec(), cx)
                    }))
                    .drag_over::<ExternalPaths>(move |style, _, _, _| {
                        style.border_color(drop_border)
                    })
                    .children(popups::mention_popup(composer, cx))
                    .flex()
                    .flex_col()
                    .rounded(px(18.0))
                    .border_1()
                    .border_color(border)
                    .bg(colors.sidebar)
                    .shadow(shadows)
                    .px(px(16.0))
                    .py(px(12.0))
                    .when(disabled, |surface| surface.opacity(0.6))
                    .child(
                        div()
                            .id("composer-input")
                            .min_h(px(INPUT_MIN_HEIGHT))
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .text_color(colors.foreground)
                            .cursor_text()
                            .on_click(move |_, window, cx| {
                                let handle = input.read(cx).focus_handle(cx);
                                window.focus(&handle, cx);
                            })
                            .child(composer.input.clone()),
                    )
                    .child(
                        div()
                            .mt(px(10.0))
                            .flex()
                            .items_end()
                            .justify_between()
                            .gap(px(12.0))
                            // Approval mode and plan mode: screen 3c.
                            .child(
                                div()
                                    .flex()
                                    .min_w_0()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(popups::attach_menu(composer, cx)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_shrink_0()
                                    .items_center()
                                    .gap(px(6.0))
                                    .children(model_chip(composer, cx))
                                    .child(div().ml(px(6.0)).child(action_button(composer, cx))),
                            ),
                    ),
            ),
        )
        .into_any_element()
}
