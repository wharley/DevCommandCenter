//! What opens from the composer: the + menu (attach a file) and the
//! `@arquivo` list above it (`MentionPopup`: `rounded-xl border-border/60
//! bg-background shadow-2xl`, 18rem tall at most, items `rounded-lg px-2.5
//! py-2 text-[13px]`, the highlighted one on `bg-muted`).

use gpui::{AnyElement, BoxShadow, Context, FontWeight, SharedString, div, point, prelude::*, px};

use super::Composer;
use super::mentions::MenuItem as Row;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonVariant, Icon, Menu, MenuEntry, MenuItem};

pub(super) fn attach_menu(composer: &Composer, cx: &mut Context<Composer>) -> AnyElement {
    let disabled = !composer.session.usable();
    let muted = cx.theme().colors.muted_foreground;
    let entity = cx.entity().downgrade();
    let label = cx.t("composer.attachments.add");
    Menu::dropdown(
        "composer-attach",
        move |_| {
            Button::new("composer-attach-trigger")
                .variant(ButtonVariant::Ghost)
                .disabled(disabled)
                .text_color(muted)
                .child(
                    div()
                        .flex()
                        .w(px(28.0))
                        .mx(px(-10.0))
                        .items_center()
                        .justify_center()
                        .child(Icon::new("plus").size(15.0).stroke(1.8)),
                )
                .into_any_element()
        },
        move |_, _| {
            let entity = entity.clone();
            vec![MenuEntry::from(
                MenuItem::new(label.clone(), move |_, cx| {
                    let _ = entity.update(cx, |composer, cx| composer.open_picker(cx));
                })
                .icon(Icon::new("paperclip").size(16.0)),
            )]
        },
    )
    .into_any_element()
}

pub(super) fn mention_popup(composer: &Composer, cx: &mut Context<Composer>) -> Option<AnyElement> {
    let menu = composer.mention.as_ref()?;
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let radius = theme.radius;
    let items: Vec<AnyElement> = menu
        .options
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let selected = index == menu.selected;
            let row = div()
                .id(SharedString::from(format!("mention-{index}")))
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(8.0))
                .rounded(radius.lg)
                .px(px(10.0))
                .py(px(8.0))
                .text_size(px(13.0))
                .line_height(px(18.0))
                .cursor_pointer()
                .when(selected, |item| {
                    item.bg(colors.muted).text_color(colors.foreground)
                })
                .on_mouse_move(
                    cx.listener(move |composer, _, _, cx| composer.highlight_mention(index, cx)),
                )
                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                    window.prevent_default()
                })
                .on_click(
                    cx.listener(move |composer, _, _, cx| composer.accept_mention(Some(index), cx)),
                );
            let (icon, name, detail): (&str, SharedString, SharedString) = match item {
                Row::File(path) => {
                    let (directory, name) = match path.rfind('/') {
                        Some(slash) => (&path[..=slash], &path[slash + 1..]),
                        None => ("", path.as_str()),
                    };
                    (
                        "file-text",
                        name.to_string().into(),
                        directory.to_string().into(),
                    )
                }
                Row::Command(command) => (
                    command.icon(),
                    format!("/{}", command.name()).into(),
                    cx.t(command.description_key()),
                ),
            };
            row.child(
                Icon::new(icon)
                    .size(14.0)
                    .stroke(1.8)
                    .color(colors.muted_foreground),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .max_w(px(320.0))
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .child(name),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.0))
                    .text_color(colors.muted_foreground)
                    .child(detail),
            )
            .into_any_element()
        })
        .collect();
    let list = div()
        .rounded(radius.xl)
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.background)
        .text_color(colors.foreground)
        .shadow(vec![
            BoxShadow {
                color: gpui::black().opacity(0.25),
                offset: point(px(0.0), px(25.0)),
                blur_radius: px(50.0),
                spread_radius: px(-12.0),
                inset: false,
            },
            BoxShadow {
                color: gpui::black().opacity(0.05),
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: px(1.0),
                inset: false,
            },
        ])
        .child(
            div()
                .id("mention-list")
                .max_h(px(288.0))
                .overflow_y_scroll()
                .p(px(4.0))
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(6.0))
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.muted_foreground)
                        .child(if menu.commands() {
                            cx.t("composer.commandMenu.title")
                        } else {
                            cx.t("composer.fileMentions")
                        }),
                )
                .children(items),
        );
    // `absolute bottom-full left-0 mb-2 w-[min(640px,calc(100vw-2rem))]`
    Some(
        gpui::deferred(
            div()
                .absolute()
                .left_0()
                .bottom(gpui::relative(1.0))
                .w(px(640.0))
                .max_w_full()
                .pb(px(8.0))
                .child(list),
        )
        .with_priority(1)
        .into_any_element(),
    )
}
