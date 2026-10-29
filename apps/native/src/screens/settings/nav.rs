//! `SettingsNavigation`: the brand, the search (accents ignored, every
//! word; Enter picks the first), the groups with their sections, the
//! count while searching and "Mostrar todas", and the open task at the foot.

use gpui::{AnyElement, BoxShadow, Context, FontWeight, Window, div, point, prelude::*, px};

use super::{AVAILABLE, Group, SectionId, Settings};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::color::mix_oklab;
use crate::ui::{Icon, Tracked};

impl Settings {
    /// The sections matching the search, by group.
    pub(super) fn matches(&self, cx: &gpui::App) -> Vec<SectionId> {
        let query = self.search.read(cx).text().to_string();
        Group::ALL
            .iter()
            .flat_map(|group| {
                AVAILABLE
                    .iter()
                    .filter(move |section| section.group() == *group)
            })
            .copied()
            .filter(|section| {
                let key = section.key();
                let haystack = format!(
                    "{} {} {}",
                    cx.t(&format!("settings.sections.{key}.label")),
                    cx.t(&format!("settings.sections.{key}.description")),
                    cx.t(&format!("settings.navigation.keywords.{key}")),
                );
                crate::data::text_match::matches_all_words(&haystack, &query)
            })
            .collect()
    }

    pub(super) fn navigation(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let edge = crate::ui::surface_edge(theme);
        let matches = self.matches(cx);
        let query = self.search.read(cx).text().trim().to_string();
        let focused = gpui::Focusable::focus_handle(self.search.read(cx), cx).is_focused(window);
        let groups = Group::ALL.iter().filter_map(|group| {
            let members: Vec<SectionId> = matches
                .iter()
                .copied()
                .filter(|section| section.group() == *group)
                .collect();
            (!members.is_empty()).then(|| {
                let links = members.into_iter().map(|section| {
                    let current = self.active == section;
                    let hover = colors.accent.opacity(0.6);
                    let foreground = colors.foreground;
                    div()
                        .id(("settings-section", section as usize))
                        .debug_selector(move || format!("settings-section-{}", section.key()))
                        .relative()
                        .flex()
                        .w_full()
                        .items_center()
                        .gap(px(11.0))
                        .min_h(px(36.0))
                        .mt(px(3.0))
                        .px(px(10.0))
                        .py(px(8.0))
                        .rounded(px(9.0))
                        .text_size(px(12.0))
                        .cursor_pointer()
                        .map(|link| {
                            if current {
                                link.text_color(colors.foreground)
                                    .bg(colors.popover)
                                    .shadow(
                                        crate::ui::card_shadow()
                                            .into_iter()
                                            .chain([BoxShadow {
                                                color: edge,
                                                offset: point(px(0.0), px(0.0)),
                                                blur_radius: px(0.0),
                                                spread_radius: px(1.0),
                                                inset: true,
                                            }])
                                            .collect(),
                                    )
                            } else {
                                link.text_color(colors.muted_foreground)
                                    .hover(move |style| style.text_color(foreground).bg(hover))
                            }
                        })
                        .on_click(
                            cx.listener(move |settings, _, _, cx| settings.select(section, cx)),
                        )
                        .child({
                            let icon = Icon::new(section.icon()).size(16.0).stroke(1.8);
                            if current {
                                icon.color(colors.accent_calm)
                            } else {
                                icon
                            }
                        })
                        .child(cx.t(&format!("settings.sections.{}.label", section.key())))
                        .when(current, |link| {
                            link.child(
                                div()
                                    .ml_auto()
                                    .size(px(4.0))
                                    .rounded_full()
                                    .bg(colors.accent_calm),
                            )
                        })
                });
                div()
                    .mt(px(23.0))
                    .child(
                        div()
                            .mx(px(10.0))
                            .mb(px(7.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.muted_foreground)
                            .child(Tracked::new(
                                cx.t(&format!("settings.navigation.groups.{}", group.key())),
                                10.0,
                                0.08,
                            )),
                    )
                    .children(links)
            })
        });
        let groups: Vec<_> = groups.collect();
        let results = (!query.is_empty()).then(|| {
            div()
                .mx(px(10.0))
                .mt(px(18.0))
                .mb(px(8.0))
                .text_size(px(11.0))
                .line_height(px(17.6))
                .text_color(colors.muted_foreground)
                .child(cx.t_with(
                    "settings.navigation.results",
                    &Args::count(matches.len() as i64),
                ))
        });
        let show_all = matches.is_empty().then(|| {
            div()
                .id("settings-show-all")
                .debug_selector(|| "settings-show-all".into())
                .mx(px(10.0))
                .text_size(px(11.0))
                .text_color(colors.foreground)
                .underline()
                .cursor_pointer()
                .on_click(cx.listener(|settings, _, _, cx| {
                    settings
                        .search
                        .update(cx, |input, cx| input.set_text("", cx));
                    cx.notify();
                }))
                .child(cx.t("settings.navigation.showAll"))
        });
        let (name, root) = &self.workspace;
        let workspace = name
            .clone()
            .filter(|name| !name.is_empty())
            .or_else(|| root.clone())
            .unwrap_or_else(|| cx.t("settings.git.noWorkspace").to_string());
        let ring = colors.ring;
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(222.0))
            .h_full()
            .pt(px(24.0))
            .px(px(14.0))
            .pb(px(16.0))
            .rounded_l(crate::ui::inner_radius(theme.radius.dialog, true))
            .bg(mix_oklab(colors.sidebar, 0.97, colors.accent_calm))
            .border_r_1()
            .border_color(edge)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(8.0))
                    .pb(px(23.0))
                    .text_size(px(14.0))
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .size(px(30.0))
                            .items_center()
                            .justify_center()
                            .rounded_tl(px(10.0))
                            .rounded_tr(px(10.0))
                            .rounded_br(px(4.0))
                            .rounded_bl(px(10.0))
                            .bg(colors.accent_calm.opacity(0.12))
                            .text_color(colors.accent_calm)
                            .child(Icon::new("settings-2").size(17.0)),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight(550.0))
                            .child(cx.t("settings.title")),
                    ),
            )
            .child(
                div()
                    .id("settings-nav")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .debug_selector(|| "settings-search".into())
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .h(px(32.0))
                            .px(px(10.0))
                            .rounded(px(10.0))
                            .border_1()
                            // Like the help's search: the ring is the border's
                            // colour (a drawn outline was clipped by the list).
                            .border_color(if focused { ring } else { colors.input })
                            .bg(colors.background)
                            .text_color(colors.muted_foreground)
                            .child(Icon::new("search").size(14.0).stroke(1.9))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(13.0))
                                    .text_color(colors.foreground)
                                    .child(self.search.clone()),
                            )
                            .when(!query.is_empty(), |field| {
                                field.child(
                                    div()
                                        .id("settings-clear-search")
                                        .flex()
                                        .flex_none()
                                        .w(px(22.0))
                                        .h(px(26.0))
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .on_click(cx.listener(|settings, _, window, cx| {
                                            settings.search.update(cx, |input, cx| {
                                                input.set_text("", cx);
                                            });
                                            let focus = gpui::Focusable::focus_handle(
                                                settings.search.read(cx),
                                                cx,
                                            );
                                            window.focus(&focus, cx);
                                            cx.notify();
                                        }))
                                        .child(Icon::new("x").size(13.0)),
                                )
                            }),
                    )
                    .children(groups)
                    .children(results)
                    .children(show_all),
            )
            .child(
                div()
                    .mt_auto()
                    .pt(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .text_color(colors.muted_foreground)
                    .child(
                        div()
                            .flex_none()
                            .child(Icon::new("folder-git-2").size(16.0)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .child(cx.t("settings.currentWorkspace")),
                            )
                            .child(
                                div()
                                    .mt(px(3.0))
                                    .truncate()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(workspace),
                            ),
                    ),
            )
            .into_any_element()
    }
}
