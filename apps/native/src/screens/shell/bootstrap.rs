//! `WorkspaceBootstrapState`: no task at all yet. Open a project or clone
//! one, or read how it works; the provider new work would start on is
//! named underneath.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::Shell;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon};

/// "Provedor atual": the global pick, else a ready stable provider.
pub(super) fn current_provider() -> Option<(String, Option<String>)> {
    let providers = crate::data::provider_catalog::offline(&crate::data::loader::database_path());
    let settings = crate::screens::composer::load_settings();
    let pick = crate::data::provider_selection::new_task_pick(
        &providers,
        None,
        settings.global_pick.as_ref(),
    )?;
    let provider = providers
        .iter()
        .find(|provider| provider.id.0 == pick.provider_id)?;
    let model = provider
        .models
        .iter()
        .find(|model| model.id == pick.model_id)
        .map(|model| model.label.clone());
    Some((provider.label.clone(), model))
}

pub(super) fn render(shell: &Shell, cx: &mut Context<Shell>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let provider = shell.bootstrap_provider.clone().map(|(provider, model)| {
        cx.t_with(
            "bootstrap.currentProvider",
            &Args::default().with("provider", provider).with(
                "model",
                model.map(|model| format!(" · {model}")).unwrap_or_default(),
            ),
        )
    });
    let button =
        |id: &'static str, variant: ButtonVariant, icon: &'static str, label: SharedString| {
            Button::new(id).variant(variant).size(ButtonSize::Sm).child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(Icon::new(icon).size(14.0).stroke(2.0))
                    .child(label),
            )
        };
    div()
        .debug_selector(|| "bootstrap".into())
        .flex()
        .flex_1()
        .size_full()
        .min_h_0()
        .items_center()
        .justify_center()
        .px(px(24.0))
        .py(px(40.0))
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .w_full()
                .max_w(px(672.0))
                .child(
                    div()
                        .mb(px(16.0))
                        .flex()
                        .size(px(48.0))
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .border_1()
                        .border_color(colors.border.opacity(0.7))
                        .bg(colors.muted.opacity(0.2))
                        .text_color(colors.muted_foreground)
                        .child(Icon::new("folder-open").size(20.0)),
                )
                .child(
                    div()
                        .text_size(px(15.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .child(cx.t("bootstrap.title")),
                )
                .child(
                    div()
                        .mt(px(8.0))
                        .max_w(px(512.0))
                        .text_center()
                        .text_size(px(13.0))
                        .line_height(px(24.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("bootstrap.description")),
                )
                .child(
                    div()
                        .mt(px(24.0))
                        .flex()
                        .flex_wrap()
                        .justify_center()
                        .gap(px(8.0))
                        .child(
                            button(
                                "bootstrap-open",
                                ButtonVariant::Default,
                                "folder-open",
                                cx.t("bootstrap.openProject"),
                            )
                            .on_click(cx.listener(
                                |shell, _, window, cx| shell.open_from_bootstrap(false, window, cx),
                            )),
                        )
                        .child(
                            button(
                                "bootstrap-clone",
                                ButtonVariant::Outline,
                                "link-2",
                                cx.t("bootstrap.cloneFromUrl"),
                            )
                            .on_click(cx.listener(
                                |shell, _, window, cx| shell.open_from_bootstrap(true, window, cx),
                            )),
                        ),
                )
                // "Como funciona": the help on its first topic.
                .child(
                    div().mt(px(8.0)).child(
                        button(
                            "bootstrap-how-it-works",
                            ButtonVariant::Ghost,
                            "circle-question-mark",
                            cx.t("bootstrap.howItWorks"),
                        )
                        .muted()
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.open_help(Some(crate::screens::help::DEFAULT_TOPIC), window, cx)
                        })),
                    ),
                )
                .children(provider.map(|line| {
                    div()
                        .mt(px(16.0))
                        .text_size(px(12.0))
                        .text_color(colors.muted_foreground)
                        .child(line)
                })),
        )
        .into_any_element()
}
