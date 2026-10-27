//! The panel: the search, the connected shelf with each one's menu, the
//! catalog shelves, the footer, and the confirmations.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::super::Settings;
use super::Confirm;
use super::glyph::glyph;
use crate::data::connectors::{
    Category, Status, Target, View, integration_for_url, providers_without_connectors,
    sorted_views, visible_presets,
};
use crate::i18n::{Args, Locale, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Dialog, Icon, Menu, MenuAlign, MenuEntry, MenuItem,
    dialog_footer, dialog_header, spinner,
};

fn matches(query: &str, values: &[&str]) -> bool {
    let needle = query.trim().to_lowercase();
    needle.is_empty()
        || values
            .iter()
            .any(|value| value.to_lowercase().contains(&needle))
}

/// `Intl.ListFormat(language, { style: "long", type: "conjunction" })`.
fn joined(items: &[String], locale: Locale) -> String {
    let and = if locale == Locale::En { "and" } else { "e" };
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] if locale == Locale::En && rest.len() > 1 => {
            format!("{}, {and} {last}", rest.join(", "))
        }
        [rest @ .., last] => format!("{} {and} {last}", rest.join(", ")),
    }
}

fn shelf(title: SharedString, rows: Vec<AnyElement>, cx: &gpui::App) -> AnyElement {
    div()
        .child(
            div()
                .mb(px(8.0))
                .px(px(12.0))
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().colors.foreground)
                .child(title),
        )
        .child(
            div()
                .grid()
                .grid_cols(2)
                .gap_x(px(16.0))
                .gap_y(px(4.0))
                .children(rows),
        )
        .into_any_element()
}

fn row_text(title: String, line: AnyElement, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .truncate()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.foreground)
                .child(title),
        )
        .child(
            div()
                .truncate()
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(line),
        )
        .into_any_element()
}

impl Settings {
    pub(in super::super) fn connectors_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let panel = &self.connectors;
        let query = panel.search.read(cx).text().to_string();
        let integrations = panel.list().to_vec();
        let connected: Vec<View> = sorted_views(&integrations)
            .into_iter()
            .filter(|view| {
                let catalog = view
                    .preset
                    .map(|preset| {
                        cx.t(&format!("settings.connectors.catalog.{}", preset.id))
                            .to_string()
                    })
                    .unwrap_or_default();
                matches(
                    &query,
                    &[
                        &view.name,
                        view.integration.url().unwrap_or_default(),
                        &catalog,
                    ],
                )
            })
            .collect();
        let shelves: Vec<(Category, Vec<&'static crate::data::connectors::Preset>)> = Category::ALL
            .into_iter()
            .map(|category| {
                let presets = visible_presets()
                    .filter(|preset| preset.category == category)
                    .filter(|preset| integration_for_url(&integrations, preset.url).is_none())
                    .filter(|preset| {
                        let catalog = cx.t(&format!("settings.connectors.catalog.{}", preset.id));
                        matches(&query, &[preset.name, &catalog])
                    })
                    .collect::<Vec<_>>();
                (category, presets)
            })
            .filter(|(_, presets)| !presets.is_empty())
            .collect();
        let state: Option<AnyElement> = match &panel.integrations {
            None => Some(
                div()
                    .flex()
                    .items_center()
                    .text_color(colors.muted_foreground)
                    .child(spinner("settings-connectors-loading", 14.0))
                    .into_any_element(),
            ),
            Some(Err(error)) => Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .text_size(px(12.0))
                    .text_color(colors.destructive)
                    .child(cx.t("settings.connectors.loadError"))
                    .child(
                        div()
                            .text_color(colors.muted_foreground)
                            .child(error.clone()),
                    )
                    .child(
                        Button::new("settings-connectors-retry")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Xs)
                            .text_size(12.0)
                            .on_click(
                                cx.listener(|settings, _, _, cx| settings.load_connectors(cx)),
                            )
                            .child(cx.t("settings.connectors.retry")),
                    )
                    .into_any_element(),
            ),
            Some(Ok(_)) => None,
        };
        let connected_rows: Vec<AnyElement> = connected
            .iter()
            .map(|view| self.connected_row(view, cx))
            .collect();
        let catalog_shelves: Vec<AnyElement> = shelves
            .into_iter()
            .map(|(category, presets)| {
                let rows = presets
                    .into_iter()
                    .map(|preset| {
                        div()
                            .id(SharedString::from(format!(
                                "settings-connector-preset-{}",
                                preset.id
                            )))
                            .debug_selector(move || {
                                format!("settings-connector-preset-{}", preset.id)
                            })
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .min_h(px(56.0))
                            .rounded(px(14.0))
                            .px(px(12.0))
                            .py(px(10.0))
                            .cursor_pointer()
                            .hover(|row| row.bg(colors.muted.opacity(0.5)))
                            .on_click(cx.listener(move |settings, _, window, cx| {
                                settings.open_connect(Target::of_preset(preset), window, cx)
                            }))
                            .child(glyph(preset.logo, None, preset.name, 36.0, cx))
                            .child(row_text(
                                preset.name.into(),
                                cx.t(&format!("settings.connectors.catalog.{}", preset.id))
                                    .into_any_element(),
                                cx,
                            ))
                            .child(Icon::new("plus").size(16.0).color(colors.muted_foreground))
                            .into_any_element()
                    })
                    .collect();
                shelf(
                    cx.t(&format!(
                        "settings.connectors.categories.{}",
                        category.key()
                    )),
                    rows,
                    cx,
                )
            })
            .collect();
        let no_match = (!query.trim().is_empty()
            && connected.is_empty()
            && catalog_shelves.is_empty())
        .then(|| {
            div()
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(cx.t_with(
                    "settings.connectors.noMatch",
                    &Args::default().with("query", query.trim().to_string()),
                ))
        });
        let catalog = self.providers.catalog.clone().unwrap_or_default();
        let unsupported = providers_without_connectors(&catalog);
        let locale = cx.global::<crate::i18n::I18n>().locale();
        let footer = if unsupported.is_empty() {
            cx.t("settings.connectors.footer").to_string()
        } else {
            format!(
                "{} {}",
                cx.t("settings.connectors.footer"),
                cx.t_with(
                    "settings.connectors.footerUnsupported",
                    &Args::default().with("providers", joined(&unsupported, locale)),
                )
            )
        };
        div()
            .debug_selector(|| "settings-connectors".into())
            .flex()
            .flex_col()
            .gap(px(28.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .h(px(32.0))
                    .px(px(12.0))
                    .rounded_full()
                    .border_1()
                    .border_color(colors.input)
                    .text_size(px(12.0))
                    .child(
                        Icon::new("search")
                            .size(14.0)
                            .color(colors.muted_foreground),
                    )
                    .child(panel.search.clone()),
            )
            .when(self.general.api().is_none(), |column| {
                column.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("settings.connectors.desktopOnly")),
                )
            })
            .children(state)
            .when(!connected_rows.is_empty(), |column| {
                column.child(shelf(
                    cx.t("settings.connectors.connectedTitle"),
                    connected_rows,
                    cx,
                ))
            })
            .children(catalog_shelves)
            .children(no_match)
            .child(
                div()
                    .text_size(px(11.0))
                    .line_height(px(17.9))
                    .text_color(colors.muted_foreground)
                    .child(footer),
            )
            .into_any_element()
    }

    fn connected_row(&self, view: &View, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let id = view.id().to_string();
        let busy = self.connectors.busy.is_some();
        let row_busy = self
            .connectors
            .busy
            .as_deref()
            .is_some_and(|busy| busy.ends_with(&format!(":{id}")));
        let line: AnyElement = if view.status == Status::NeedsAuth {
            div()
                .text_color(colors.warning)
                .child(self.connector_status(view, cx))
                .into_any_element()
        } else {
            self.connector_status(view, cx).into_any_element()
        };
        let reconnect_view = view.clone();
        let reconnect = (view.status == Status::NeedsAuth && view.remote).then(|| {
            Button::new(SharedString::from(format!(
                "settings-connector-reconnect-{id}"
            )))
            .variant(ButtonVariant::Outline)
            .size(ButtonSize::Xs)
            .text_size(12.0)
            .on_click(cx.listener(move |settings, _, window, cx| {
                settings.reconnect(&reconnect_view, window, cx)
            }))
            .child(cx.t("settings.connectors.actions.reconnect"))
        });
        let this = cx.entity().downgrade();
        let menu_view = view.clone();
        let labels = (
            cx.t("settings.connectors.actions.test"),
            cx.t("settings.connectors.actions.reconnect"),
            cx.t("settings.connectors.actions.disconnect"),
            cx.t("settings.connectors.actions.remove"),
        );
        let menu = Menu::dropdown(
            SharedString::from(format!("settings-connector-menu-{id}")),
            {
                let colors = colors.clone();
                let id = id.clone();
                move |_| {
                    div()
                        .debug_selector(move || format!("settings-connector-menu-{id}"))
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(28.0))
                        .rounded_full()
                        .when(busy, |button| button.opacity(0.5))
                        .hover(|button| button.bg(colors.muted))
                        .child(Icon::new("ellipsis").size(16.0))
                        .into_any_element()
                }
            },
            move |_, _| {
                let view = menu_view.clone();
                let item = |label: SharedString,
                            action: fn(
                    &mut Settings,
                    &View,
                    &mut gpui::Window,
                    &mut Context<Settings>,
                )| {
                    let (this, view) = (this.clone(), view.clone());
                    MenuItem::new(label, move |window, cx| {
                        let _ = this.update(cx, |settings, cx| action(settings, &view, window, cx));
                    })
                };
                let mut entries = vec![MenuEntry::Item(item(
                    labels.0.clone(),
                    |settings, view, _, cx| settings.test_connector(view, cx),
                ))];
                if view.remote && view.status != Status::NeedsAuth {
                    entries.push(MenuEntry::Item(item(
                        labels.1.clone(),
                        |settings, view, window, cx| settings.reconnect(view, window, cx),
                    )));
                }
                entries.push(MenuEntry::Separator);
                if view.signed_in {
                    entries.push(MenuEntry::Item(
                        item(labels.2.clone(), |settings, view, _, cx| {
                            settings.connectors.confirm = Some(Confirm::Disconnect(view.clone()));
                            cx.notify();
                        })
                        .destructive(),
                    ));
                }
                entries.push(MenuEntry::Item(
                    item(labels.3.clone(), |settings, view, _, cx| {
                        settings.connectors.confirm = Some(Confirm::Remove(view.clone()));
                        cx.notify();
                    })
                    .destructive(),
                ));
                entries
            },
        )
        .align(MenuAlign::End)
        .min_width(176.0);
        div()
            .debug_selector({
                let id = id.clone();
                move || format!("settings-connector-{id}")
            })
            .flex()
            .items_center()
            .gap(px(12.0))
            .min_h(px(56.0))
            .rounded(px(14.0))
            .px(px(12.0))
            .py(px(10.0))
            .child(glyph(
                view.preset.and_then(|preset| preset.logo),
                view.logo_uri(),
                &view.name,
                36.0,
                cx,
            ))
            .child(row_text(view.name.clone(), line, cx))
            .children(row_busy.then(|| {
                spinner(
                    SharedString::from(format!("settings-connector-busy-{id}")),
                    14.0,
                )
            }))
            .children(reconnect)
            .child(menu)
            .into_any_element()
    }

    /// `window.confirm(disconnectConfirm / removeConfirm)`
    pub(in super::super) fn connector_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let confirm = self.connectors.confirm.as_ref()?;
        let (key, action, view) = match confirm {
            Confirm::Disconnect(view) => (
                "settings.connectors.manage.disconnectConfirm",
                "settings.connectors.actions.disconnect",
                view,
            ),
            Confirm::Remove(view) => (
                "settings.connectors.manage.removeConfirm",
                "settings.connectors.actions.remove",
                view,
            ),
        };
        let close = cx.entity().downgrade();
        Some(
            Dialog::new("settings-connector-confirm", move |_, cx| {
                let _ = close.update(cx, |settings, cx| {
                    settings.connectors.confirm = None;
                    cx.notify();
                });
            })
            .close_button(false)
            .child(dialog_header(
                cx.t_with(key, &Args::default().with("name", view.name.clone())),
                None,
                cx,
            ))
            .child(dialog_footer(
                [
                    Button::new("settings-connector-confirm-cancel")
                        .variant(ButtonVariant::Outline)
                        .on_click(cx.listener(|settings, _, _, cx| {
                            settings.connectors.confirm = None;
                            cx.notify();
                        }))
                        .child(cx.t("sidebar.cancel"))
                        .into_any_element(),
                    Button::new("settings-connector-confirm-ok")
                        .variant(ButtonVariant::Destructive)
                        .on_click(cx.listener(|settings, _, _, cx| settings.confirmed(cx)))
                        .child(cx.t(action))
                        .into_any_element(),
                ],
                cx,
            ))
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_read_like_intl() {
        let items = |names: &[&str]| {
            names
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(joined(&items(&["Cursor"]), Locale::PtBr), "Cursor");
        assert_eq!(
            joined(&items(&["Cursor", "Grok"]), Locale::PtBr),
            "Cursor e Grok"
        );
        assert_eq!(joined(&items(&["A", "B", "C"]), Locale::PtBr), "A, B e C");
        assert_eq!(joined(&items(&["A", "B", "C"]), Locale::En), "A, B, and C");
        assert_eq!(joined(&items(&["A", "B"]), Locale::En), "A and B");
    }
}
