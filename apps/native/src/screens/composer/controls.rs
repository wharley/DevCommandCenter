//! The toolbar's turn settings: the approval mode (`ComposerApprovalPolicyMenu`,
//! with the full-access confirmation) and Planejar.

use dcc_core::domain::provider::ProviderApprovalPolicy;
use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, div, prelude::*, px};

use super::Composer;
use crate::data::composer_settings::{menu_policies, policy_key};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::color::hex;
use crate::theme::tailwind as tw;
use crate::ui::popover::{Popover, PopoverSide};
use crate::ui::{Button, Dialog, Icon, Tooltip, dialog_footer, dialog_header};

/// `--plan`
const PLAN: u32 = 0x48968c;

fn policy_icon(policy: Option<ProviderApprovalPolicy>) -> &'static str {
    match policy {
        Some(ProviderApprovalPolicy::FullAccess) => "shield-alert",
        Some(ProviderApprovalPolicy::Auto) => "shield-check",
        _ => "shield",
    }
}

fn policy_text(policy: ProviderApprovalPolicy, part: &str, cx: &gpui::App) -> SharedString {
    cx.t(&format!(
        "composer.approval.options.{}.{part}",
        policy_key(policy)
    ))
}

/// The trigger's text and icon color for the chosen mode.
fn tone(policy: ProviderApprovalPolicy, cx: &gpui::App) -> (Hsla, Option<Hsla>) {
    match policy {
        ProviderApprovalPolicy::FullAccess => {
            (tw::ORANGE_500.hsla(), Some(tw::ORANGE_500.alpha(0.1)))
        }
        ProviderApprovalPolicy::Auto => (tw::EMERALD_500.hsla(), None),
        _ => (cx.theme().colors.muted_foreground, None),
    }
}

pub(super) fn approval_menu(composer: &Composer, cx: &mut Context<Composer>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let disabled = composer.session.session_id.is_none();
    let policies = menu_policies(&composer.supported_policies());
    let selected = composer.approval();
    let Some(selected) = selected.filter(|_| !policies.is_empty()) else {
        // The provider manages permissions itself.
        let provider = composer
            .provider()
            .map(|provider| provider.label.clone())
            .unwrap_or_else(|| "Provider".into());
        return Tooltip::text(
            "composer-approval-managed",
            div()
                .flex()
                .h(px(28.0))
                .flex_shrink_0()
                .items_center()
                .gap(px(4.0))
                .px(px(6.0))
                .text_color(colors.muted_foreground.opacity(0.55))
                .when(disabled, |label| label.opacity(0.45))
                .child(Icon::new("shield").size(13.0).stroke(1.8))
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t("composer.approval.managed")),
                ),
            cx.t_with(
                "composer.approval.managedHint",
                &Args::default().with("provider", provider),
            ),
        )
        .into_any_element();
    };
    let (text, background) = tone(selected, cx);
    let hover = colors.accent.opacity(0.6);
    let trigger = div()
        .flex()
        .h(px(28.0))
        .flex_shrink_0()
        .items_center()
        .gap(px(4.0))
        .px(px(6.0))
        .rounded(px(9.0))
        .cursor_pointer()
        .text_color(text)
        .when_some(background, |trigger, background| trigger.bg(background))
        .hover(move |style| {
            style.bg(if background.is_some() {
                tw::ORANGE_500.alpha(0.15)
            } else {
                hover
            })
        })
        .when(disabled, |trigger| trigger.opacity(0.45))
        .child(
            Icon::new(policy_icon(Some(selected)))
                .size(13.0)
                .stroke(1.8),
        )
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .font_weight(FontWeight::MEDIUM)
                .whitespace_nowrap()
                .child(policy_text(selected, "label", cx)),
        )
        .child(
            div()
                .opacity(0.4)
                .child(Icon::new("chevron-down").size(12.0)),
        );
    let trigger: AnyElement = if composer.plan_mode() {
        Tooltip::text(
            "composer-approval-plan",
            trigger,
            cx.t("composer.approval.planOverride"),
        )
        .into_any_element()
    } else {
        trigger.into_any_element()
    };
    let open = composer.approval_menu_open && !disabled;
    let entity = cx.entity().downgrade();
    let toggle = entity.clone();
    let items: Vec<(ProviderApprovalPolicy, SharedString, SharedString)> = policies
        .iter()
        .map(|policy| {
            (
                *policy,
                policy_text(*policy, "label", cx),
                policy_text(*policy, "description", cx),
            )
        })
        .collect();
    Popover::new(
        "composer-approval",
        open,
        trigger,
        move |_, cx| {
            let colors = cx.theme().colors.clone();
            let accent = colors.accent;
            div()
                .flex()
                .flex_col()
                .children(items.into_iter().map(|(policy, label, description)| {
                    let full = policy == ProviderApprovalPolicy::FullAccess;
                    let pick = entity.clone();
                    div()
                        .id(SharedString::from(format!(
                            "approval-{}",
                            policy_key(policy)
                        )))
                        .flex()
                        .items_start()
                        .gap(px(10.0))
                        .px(px(8.0))
                        .py(px(10.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .hover(move |style| style.bg(accent))
                        .on_click(move |_, _, cx| {
                            let _ =
                                pick.update(cx, |composer, cx| composer.pick_approval(policy, cx));
                        })
                        .child(
                            div()
                                .mt(px(2.0))
                                .when(full, |icon| icon.text_color(tw::ORANGE_500.hsla()))
                                .child(Icon::new(policy_icon(Some(policy))).size(13.0).stroke(1.8)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .when(full, |label| label.text_color(tw::ORANGE_500.hsla()))
                                        .child(label),
                                )
                                .child(
                                    div()
                                        .mt(px(2.0))
                                        .text_size(px(12.0))
                                        .line_height(px(16.0))
                                        .text_color(colors.muted_foreground)
                                        .child(description),
                                ),
                        )
                        .when(policy == selected, |item| {
                            item.child(div().mt(px(2.0)).child(Icon::new("check").size(16.0)))
                        })
                }))
                .into_any_element()
        },
        move |open, _, cx| {
            let _ = toggle.update(cx, |composer, cx| {
                composer.approval_menu_open = open;
                cx.notify();
            });
        },
    )
    .side(PopoverSide::Top)
    .width(320.0)
    .padding(4.0)
    .into_any_element()
}

impl Composer {
    /// Full access asks first; the other modes apply at once.
    pub(crate) fn pick_approval(&mut self, policy: ProviderApprovalPolicy, cx: &mut Context<Self>) {
        self.approval_menu_open = false;
        if policy == ProviderApprovalPolicy::FullAccess && self.approval() != Some(policy) {
            self.confirm_full_access = true;
            cx.notify();
            return;
        }
        self.set_approval(policy, cx);
    }
}

pub(super) fn full_access_dialog(
    composer: &Composer,
    cx: &mut Context<Composer>,
) -> Option<AnyElement> {
    if !composer.confirm_full_access {
        return None;
    }
    let colors = cx.theme().colors.clone();
    let close = cx.entity().downgrade();
    let cancel = close.clone();
    let enable = close.clone();
    let header = div()
        .flex()
        .flex_col()
        .child(
            div()
                .mb(px(4.0))
                .flex()
                .size(px(36.0))
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(tw::ORANGE_500.alpha(0.1))
                .text_color(tw::ORANGE_500.hsla())
                .child(Icon::new("shield-alert").size(20.0)),
        )
        .child(dialog_header(
            cx.t("composer.approval.confirm.title"),
            Some(cx.t("composer.approval.confirm.description")),
            cx,
        ));
    let warning = div()
        .rounded(cx.theme().radius.lg)
        .border_1()
        .border_color(tw::ORANGE_500.alpha(0.2))
        .bg(tw::ORANGE_500.alpha(0.05))
        .px(px(12.0))
        .py(px(8.0))
        .text_size(px(12.0))
        .line_height(px(20.0))
        .text_color(colors.muted_foreground)
        .child(cx.t("composer.approval.confirm.worktreeWarning"));
    let footer = dialog_footer(
        [
            Button::new("full-access-cancel")
                .variant(crate::ui::ButtonVariant::Ghost)
                .size(crate::ui::ButtonSize::Default)
                .on_click(move |_, _, cx| {
                    let _ = cancel.update(cx, |composer, cx| {
                        composer.confirm_full_access = false;
                        cx.notify();
                    });
                })
                .child(cx.t("composer.approval.confirm.cancel"))
                .into_any_element(),
            div()
                .id("full-access-enable")
                .flex()
                .h(px(32.0))
                .items_center()
                .px(px(12.0))
                .rounded(cx.theme().radius.control)
                .bg(tw::ORANGE_600.hsla())
                .hover(|style| style.bg(tw::ORANGE_700.hsla()))
                .text_color(gpui::white())
                .text_size(px(14.0))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    let _ = enable.update(cx, |composer, cx| {
                        composer.confirm_full_access = false;
                        composer.set_approval(ProviderApprovalPolicy::FullAccess, cx);
                    });
                })
                .child(cx.t("composer.approval.confirm.enable"))
                .into_any_element(),
        ],
        cx,
    );
    Some(
        Dialog::new("full-access-dialog", move |_, cx| {
            let _ = close.update(cx, |composer, cx| {
                composer.confirm_full_access = false;
                cx.notify();
            });
        })
        .close_button(false)
        .max_width(512.0)
        .child(header)
        .child(warning)
        .child(footer)
        .into_any_element(),
    )
}

pub(super) fn plan_toggle(composer: &Composer, cx: &mut Context<Composer>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let on = composer.plan_mode();
    let disabled = composer.session.session_id.is_none();
    let color: Hsla = if on {
        hex(PLAN)
    } else {
        colors.muted_foreground.opacity(0.7)
    };
    let hover = colors.accent.opacity(0.6);
    let mut hint = if on {
        cx.t("composer.controls.planModeActiveHint").to_string()
    } else {
        cx.t("composer.controls.planModeHint").to_string()
    };
    if composer.plan_is_prompt_fallback() {
        hint.push(' ');
        hint.push_str(&cx.t("composer.controls.planModePromptFallback"));
    }
    let trigger = div()
        .id("composer-plan")
        .flex()
        .h(px(28.0))
        .flex_shrink_0()
        .items_center()
        .gap(px(4.0))
        .px(px(6.0))
        .rounded(px(9.0))
        .text_color(color)
        .when(!disabled, |button| {
            button
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_click(cx.listener(|composer, _, _, cx| composer.toggle_plan(cx)))
        })
        .when(disabled, |button| button.opacity(0.45))
        .child(Icon::new("clipboard-list").size(13.0).stroke(1.8))
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .font_weight(FontWeight::MEDIUM)
                .child(cx.t("composer.controls.plan")),
        );
    Tooltip::text("composer-plan-tip", trigger, hint).into_any_element()
}
