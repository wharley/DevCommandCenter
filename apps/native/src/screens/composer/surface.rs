//! `.dcc-composer-surface`: the rounded card on the sidebar color (18px,
//! the surface edge, the card shadow, the calm accent ring while focused)
//! with the prompt field and the toolbar under it.

use gpui::{
    AnyElement, BoxShadow, Context, ExternalPaths, Focusable, FontWeight, SharedString, Window,
    div, point, prelude::*, px,
};

use super::{Busy, Composer, context_rail, controls, objective, picker, popups, queue, strips};
use crate::i18n::Translate;
use crate::project::ProviderIcon;
use crate::theme::ActiveTheme;
use crate::theme::color::mix_oklab;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, spinner};

fn model_chip(composer: &Composer, cx: &mut Context<Composer>) -> Option<AnyElement> {
    let model_id = composer.selected_model_id()?;
    let provider = composer.selected_provider_id();
    let colors = cx.theme().colors.clone();
    let label = composer
        .model()
        .map(|model| model.label.clone())
        .unwrap_or_else(|| model_id.clone());
    let disabled = !composer.session.usable();
    let hover = colors.accent.opacity(0.6);
    let trigger = div()
        .flex()
        .h(px(28.0))
        .min_w_0()
        .max_w(px(224.0))
        .items_center()
        .gap(px(6.0))
        .px(px(4.0))
        .py(px(2.0))
        .rounded(px(9.0))
        .text_color(colors.muted_foreground)
        .when(!disabled, |chip| {
            chip.cursor_pointer().hover(move |style| style.bg(hover))
        })
        .when(disabled, |chip| chip.opacity(0.45))
        .children(
            provider
                .as_deref()
                .map(|provider| ProviderIcon::new(provider).size(13.0)),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.foreground)
                .child(SharedString::from(picker::compact_model(
                    provider.as_deref(),
                    &label,
                ))),
        )
        .when(!composer.effort_levels().is_empty(), |chip| {
            chip.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(colors.muted_foreground)
                    .child(format!(
                        "· {}",
                        cx.t(&format!("composer.effort.{}", composer.effort_label_id()))
                    )),
            )
        })
        .child(
            div()
                .flex_shrink_0()
                .opacity(0.4)
                .child(Icon::new("chevron-down").size(12.0)),
        )
        .into_any_element();
    Some(picker::model_menu(composer, trigger, cx))
}

/// While the agent works: stop, and with something written, queue it for
/// later or (when the provider takes it) steer the running turn.
fn running_buttons(composer: &Composer, cx: &mut Context<Composer>) -> AnyElement {
    let stopping = composer.busy == Some(Busy::Stopping);
    let stop = Button::new("composer-stop")
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
        });
    let written = !composer.prompt(cx).trim().is_empty();
    let disabled = !composer.can_send(cx);
    let queue =
        written.then(|| {
            let queueing = composer.busy == Some(Busy::Queueing);
            Tooltip::text(
                "composer-queue-tip",
                Button::new("composer-queue")
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Icon)
                    .disabled(disabled)
                    .on_click(cx.listener(|composer, _, _, cx| {
                        composer.follow_up(queue::FollowUp::Queue, cx)
                    }))
                    .child(if queueing {
                        spinner("composer-queueing", 14.0).into_any_element()
                    } else {
                        Icon::new("list-plus")
                            .size(15.0)
                            .stroke(2.2)
                            .into_any_element()
                    }),
                cx.t("composer.followUp.queue"),
            )
        });
    let steer =
        (written && composer.can_steer()).then(|| {
            let steering = composer.busy == Some(Busy::Steering);
            Tooltip::text(
                "composer-steer-tip",
                Button::new("composer-steer")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Icon)
                    .disabled(disabled)
                    .on_click(cx.listener(|composer, _, _, cx| {
                        composer.follow_up(queue::FollowUp::Steer, cx)
                    }))
                    .child(if steering {
                        spinner("composer-steering", 14.0).into_any_element()
                    } else {
                        Icon::new("corner-up-right")
                            .size(15.0)
                            .stroke(2.1)
                            .into_any_element()
                    }),
                cx.t("composer.followUp.steerNow"),
            )
        });
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(stop)
        .children(queue)
        .children(steer)
        .into_any_element()
}

fn action_button(composer: &Composer, cx: &mut Context<Composer>) -> AnyElement {
    if composer.session.running {
        return running_buttons(composer, cx);
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
    let disabled = !composer.session.usable();
    let input = composer.input.clone();
    let density = crate::theme::Density::current(cx);
    let (padding_x, padding_y) = density.composer_padding();
    let drop_border = mix_oklab(colors.accent_calm, 0.7, colors.border);
    div()
        .flex_shrink_0()
        .px(px(16.0))
        .pb(px(12.0))
        .pt(px(8.0))
        .child(
            div()
                .mx_auto()
                .w_full()
                .max_w(px(832.0))
                .child(
                    div()
                        .id("composer")
                        .relative()
                        .on_drop(cx.listener(
                            |composer,
                             file: &crate::screens::code_column::DraggedFile,
                             window,
                             cx| {
                                composer.mention_file(&file.0, window, cx)
                            },
                        ))
                        .drag_over::<crate::screens::code_column::DraggedFile>(
                            move |style, _, _, _| style.border_color(drop_border),
                        )
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
                        .px(px(padding_x))
                        .py(px(padding_y))
                        .when(disabled, |surface| surface.opacity(0.6))
                        .children(strips::usage_limit_strip(composer, cx))
                        .children(strips::goal_strip(composer, cx))
                        .children(queue::queue_tray(composer, cx))
                        .child(
                            div()
                                .id("composer-input")
                                .min_h(px(density.composer_input_min_height()))
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
                                .child(
                                    div()
                                        .flex()
                                        .min_w_0()
                                        .items_center()
                                        .gap(px(6.0))
                                        .child(popups::attach_menu(composer, cx))
                                        .child(controls::approval_menu(composer, cx))
                                        .child(controls::plan_toggle(composer, cx))
                                        .children(objective::objective_control(
                                            composer, window, cx,
                                        ))
                                        .children(strips::auto_resume_toggle(composer, cx)),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_shrink_0()
                                        .items_center()
                                        .gap(px(6.0))
                                        .children(model_chip(composer, cx))
                                        .child(
                                            div().ml(px(6.0)).child(action_button(composer, cx)),
                                        ),
                                ),
                        ),
                )
                .children(context_rail::context_rail(composer, cx)),
        )
        .children(controls::full_access_dialog(composer, cx))
        .into_any_element()
}
