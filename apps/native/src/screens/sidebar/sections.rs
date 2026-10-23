//! Section marks inside the list: the day labels (Fixadas, Hoje…) and the
//! collapsible "Em espera" / "Concluídos" headers.

use gpui::{AnyElement, ClickEvent, Context, SharedString, Window, div, prelude::*, px};

use super::Sidebar;
use super::state::HoldSection;
use crate::data::disk::format_disk_bytes;
use crate::data::rail::TaskSectionId;
use crate::i18n::I18n;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::tooltip::Side;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, Tracked, tabular_nums};

pub fn day_label(section: TaskSectionId, cx: &mut Context<Sidebar>) -> AnyElement {
    let color = cx.theme().colors.muted_foreground.opacity(0.7);
    let label = cx.t(section.i18n_key()).to_uppercase();
    div()
        .flex()
        .h_full()
        .items_end()
        .gap(px(4.0))
        .px(px(12.0))
        .pb(px(4.0))
        .text_color(color)
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .when(section == TaskSectionId::Pinned, |label| {
            label.child(
                div()
                    .mb(px(3.0))
                    .child(Icon::new("pin").size(10.0).stroke(2.2).rotate(-12.0)),
            )
        })
        .child(Tracked::new(label, 10.0, 0.08))
        .into_any_element()
}

pub fn hold_header(
    sidebar: &Sidebar,
    section: HoldSection,
    count: usize,
    open: bool,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let theme = cx.theme();
    let colors = &theme.colors;
    let (icon, label_key, id) = match section {
        HoldSection::Waiting => ("clock-3", "sidebar.waiting", "hold-waiting"),
        HoldSection::Completed => ("circle-check-big", "sidebar.completed", "hold-completed"),
    };
    let hover_background = colors.accent.opacity(0.5);
    let hover_text = colors.foreground;
    let label: SharedString = cx.t(label_key);
    let completed = section == HoldSection::Completed;
    // `· 1,2 GB` once the completed worktrees were measured.
    let size = completed
        .then(|| sidebar.completed_bytes())
        .flatten()
        .map(|bytes| {
            format!(
                "· {}",
                format_disk_bytes(bytes, cx.global::<I18n>().locale())
            )
        });
    let muted = colors.muted_foreground;
    let cleanup = (completed && count > 0).then(|| {
        Tooltip::text(
            "tip-cleanup",
            Button::new("cleanup-completed")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconXs)
                .muted()
                .child(Icon::new("brush-cleaning").size(14.0))
                .on_click(cx.listener(|this, _, _, cx| this.open_cleanup(cx))),
            cx.t("sidebar.cleanup.action"),
        )
        .side(Side::Top)
    });
    let header = div()
        .id(id)
        .group(id)
        .flex()
        .items_center()
        .gap(px(4.0))
        .pr(px(4.0))
        .rounded(theme.radius.control)
        .cursor_pointer()
        .hover(move |style| style.bg(hover_background))
        .on_click(
            cx.listener(move |this, _: &ClickEvent, _: &mut Window, cx| {
                this.toggle_section(section, cx)
            }),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .justify_between()
                .px(px(8.0))
                .py(px(6.0))
                .rounded(theme.radius.md)
                .text_color(colors.foreground.opacity(0.75))
                .group_hover(id, move |style| style.text_color(hover_text))
                .child(
                    div()
                        .flex()
                        .min_w_0()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            Icon::new("chevron-right")
                                .size(12.0)
                                .stroke(2.2)
                                .color(colors.muted_foreground)
                                .rotate(if open { 90.0 } else { 0.0 }),
                        )
                        .child(
                            Icon::new(icon)
                                .size(13.0)
                                .stroke(1.9)
                                .color(colors.muted_foreground.opacity(0.75)),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(13.0))
                                .line_height(px(20.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(label),
                        )
                        .when_some(size, |row, size| {
                            row.child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(10.0))
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .font_features(tabular_nums())
                                    .text_color(muted.opacity(0.7))
                                    .child(SharedString::from(size)),
                            )
                        }),
                )
                .child(
                    div()
                        .ml(px(6.0))
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
                        .h(px(17.0))
                        .min_w(px(17.0))
                        .px(px(4.0))
                        .rounded_full()
                        .bg(colors.foreground.opacity(0.06))
                        .text_size(px(9.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .font_features(tabular_nums())
                        .text_color(colors.muted_foreground)
                        .child(count.to_string()),
                ),
        )
        .when_some(cleanup, |header, cleanup| {
            header.child(
                div()
                    .flex_shrink_0()
                    .opacity(0.0)
                    .group_hover(id, |style| style.opacity(1.0))
                    .child(cleanup),
            )
        });
    // A task just moved in: the header glows once.
    match sidebar
        .section_flash
        .as_ref()
        .filter(|flash| flash.target == section)
    {
        Some(flash) => super::arrival::mark(
            header,
            format!("section-flash-{id}"),
            flash.turn,
            colors.foreground,
        ),
        None => header.into_any_element(),
    }
}
