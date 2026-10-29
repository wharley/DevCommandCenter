//! `ComposerExecutionMenu` + `ModelPickerPanel`: the model chip opens one
//! surface — provider tabs, search, the models (⭐ favorites with their
//! effort, ⌘1–⌘9), then effort on a slider and "Resposta direta".

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, Bounds, BoxShadow, Context, Focusable, FontWeight, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, Pixels, SharedString, Window, canvas, div, point, prelude::*,
    px,
};

use super::Composer;
use crate::data::composer_settings::{DEFAULT_EFFORT, EffortChoice, clamp_effort};
use crate::data::model_picker::{
    self, FAVORITES_TAB, MAX_SHORTCUT_ROWS, ModelFavorite, PickerRow, effort_steps, is_favorited,
};
use crate::i18n::{Args, Translate};
use crate::project::{ProviderIcon, model_label};
use crate::theme::ActiveTheme;
use crate::theme::color::hex;
use crate::theme::tailwind as tw;
use crate::ui::popover::{Popover, PopoverAlign, PopoverSide};
use crate::ui::{Icon, TextInput, TextInputEvent};

/// Brand accent under the open tab; monochrome marks use the foreground.
fn tab_accent(provider: &str) -> Option<u32> {
    match provider {
        "claude_code" => Some(0xD97757),
        "gemini" | "antigravity" => Some(0x4285F4),
        _ => None,
    }
}

/// The picker's moving parts while it is open.
pub(crate) struct PickerState {
    pub open: bool,
    pub tab: String,
    pub highlighted: usize,
    pub search: gpui::Entity<TextInput>,
    pub slider: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub dragging: bool,
    /// The selected provider's plan usage, under the effort.
    pub usage: super::account_usage::AccountUsage,
    /// The account picked per provider, under the tabs.
    pub accounts: super::accounts::Accounts,
    _search_events: gpui::Subscription,
}

impl PickerState {
    pub fn new(window: &mut Window, cx: &mut Context<Composer>) -> Self {
        let placeholder = cx.t("composer.model.search");
        let search = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
        let events = cx.subscribe_in(
            &search,
            window,
            |composer, _, event: &TextInputEvent, window, cx| match event {
                TextInputEvent::Changed => {
                    composer.picker.highlighted = 0;
                    cx.notify();
                }
                TextInputEvent::Submitted => {
                    let rows = composer.picker_rows(cx);
                    if let Some(row) = rows.get(composer.picker.highlighted).cloned() {
                        composer.pick_row(row, window, cx);
                    }
                }
                TextInputEvent::Cancelled => composer.set_picker_open(false, window, cx),
                TextInputEvent::Blurred => {}
            },
        );
        Self {
            open: false,
            tab: FAVORITES_TAB.into(),
            highlighted: 0,
            search,
            slider: Rc::new(Cell::new(None)),
            dragging: false,
            usage: Default::default(),
            accounts: Default::default(),
            _search_events: events,
        }
    }
}

impl Composer {
    fn query(&self, cx: &gpui::App) -> String {
        self.picker.search.read(cx).text().to_string()
    }

    pub(crate) fn picker_rows(&self, cx: &gpui::App) -> Vec<PickerRow> {
        let Some(catalog) = self.catalog.as_ref() else {
            return Vec::new();
        };
        model_picker::rows(
            catalog,
            &self.settings.favorites,
            &self.picker.tab,
            &self.query(cx),
            self.selected_provider_id().as_deref(),
        )
    }

    pub(crate) fn set_picker_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if open == self.picker.open || (open && self.turn_settings_disabled()) {
            return;
        }
        if open {
            crate::data::ux_metrics::record("advanced_composer_control_used");
        }
        self.picker.open = open;
        if open {
            self.refresh_catalog(cx);
            self.refresh_accounts(cx);
            self.refresh_account_usage(cx);
            if let Some(catalog) = self.catalog.as_ref() {
                self.picker.tab = model_picker::initial_tab(
                    catalog,
                    &self.settings.favorites,
                    self.selected_provider_id().as_deref(),
                );
            }
            self.picker.highlighted = 0;
            self.picker
                .search
                .update(cx, |search, cx| search.set_text("", cx));
            let handle = self.picker.search.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        } else {
            self.focus(window, cx);
        }
        cx.notify();
    }

    pub(crate) fn set_tab(&mut self, tab: String, window: &mut Window, cx: &mut Context<Self>) {
        self.picker.tab = tab;
        self.picker.highlighted = 0;
        let handle = self.picker.search.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
        cx.notify();
    }

    /// A row picked: its provider and model, a favorite's effort too.
    pub(crate) fn pick_row(&mut self, row: PickerRow, window: &mut Window, cx: &mut Context<Self>) {
        self.select_model(row.provider_id.clone(), Some(row.model_id.clone()), cx);
        if let Some(favorite) = &row.favorite {
            let choice = match favorite.effort.as_deref() {
                Some("ultrathink") => EffortChoice {
                    effort: self.effort_choice().effort,
                    ultrathink: true,
                },
                effort => EffortChoice {
                    effort: effort.unwrap_or(DEFAULT_EFFORT).to_string(),
                    ultrathink: false,
                },
            };
            self.set_effort(choice, cx);
        }
        self.set_picker_open(false, window, cx);
    }

    /// ⭐ on a row: off when starred; else saved with the effort in use.
    pub(crate) fn toggle_favorite(&mut self, row: &PickerRow, cx: &mut Context<Self>) {
        let mut favorites = self.settings.favorites.clone();
        if let Some(favorite) = &row.favorite {
            favorites.retain(|entry| entry != favorite);
        } else if is_favorited(&favorites, &row.provider_id, &row.model_id) {
            favorites.retain(|entry| {
                entry.provider_id != row.provider_id || entry.model_id != row.model_id
            });
        } else {
            let levels = self
                .catalog
                .as_ref()
                .and_then(|catalog| {
                    catalog
                        .iter()
                        .find(|provider| provider.id.0 == row.provider_id)
                })
                .and_then(|provider| {
                    provider
                        .models
                        .iter()
                        .find(|model| model.id == row.model_id)
                })
                .map(|model| model.effort_levels.clone())
                .unwrap_or_default();
            let choice = self.effort_choice();
            let effort = (!levels.is_empty()).then(|| {
                if choice.ultrathink {
                    "ultrathink".to_string()
                } else {
                    clamp_effort(&choice.effort, &levels)
                }
            });
            favorites.push(ModelFavorite {
                provider_id: row.provider_id.clone(),
                model_id: row.model_id.clone(),
                effort,
            });
        }
        self.set_favorites(favorites, cx);
    }

    fn on_picker_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let rows = self.picker_rows(cx);
        // ⌘1…⌘9 picks a visible row.
        if keystroke.modifiers.platform
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.shift
            && let Ok(digit) = keystroke.key.parse::<usize>()
            && (1..=MAX_SHORTCUT_ROWS).contains(&digit)
        {
            if let Some(row) = rows.get(digit - 1).cloned() {
                cx.stop_propagation();
                self.pick_row(row, window, cx);
            }
            return;
        }
        match keystroke.key.as_str() {
            "down" => {
                self.picker.highlighted =
                    (self.picker.highlighted + 1).min(rows.len().saturating_sub(1));
                cx.stop_propagation();
                cx.notify();
            }
            "up" => {
                self.picker.highlighted = self.picker.highlighted.saturating_sub(1);
                cx.stop_propagation();
                cx.notify();
            }
            _ => {}
        }
    }

    /// The slider's stop under the pointer.
    fn slider_step(&self, x: Pixels, steps: usize) -> Option<usize> {
        let bounds = self.picker.slider.get()?;
        let width = f32::from(bounds.size.width);
        if width <= 36.0 || steps < 2 {
            return Some(0);
        }
        let ratio = ((f32::from(x - bounds.left()) - 18.0) / (width - 36.0)).clamp(0.0, 1.0);
        Some((ratio * (steps - 1) as f32).round() as usize)
    }

    pub(crate) fn commit_step(&mut self, step: &str, cx: &mut Context<Self>) {
        let choice = if step == "ultrathink" {
            EffortChoice {
                effort: self.effort_choice().effort,
                ultrathink: true,
            }
        } else {
            EffortChoice {
                effort: step.to_string(),
                ultrathink: false,
            }
        };
        if choice != self.effort_choice() {
            self.set_effort(choice, cx);
        }
    }
}

fn tabs(composer: &Composer, cx: &mut Context<Composer>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let catalog = composer.catalog.clone().unwrap_or_default();
    let selected_provider = composer.selected_provider_id();
    let visible = model_picker::picker_providers(&catalog, selected_provider.as_deref());
    let mut ids: Vec<(String, Option<String>, Option<String>)> =
        vec![(FAVORITES_TAB.into(), None, None)];
    ids.extend(visible.iter().map(|provider| {
        (
            provider.id.0.clone(),
            Some(provider.label.clone()),
            model_picker::setup_reason(provider),
        )
    }));
    let (muted, foreground, hover) = (
        colors.muted_foreground,
        colors.foreground,
        colors.accent.opacity(0.7),
    );
    let buttons =
        ids.into_iter().map(|(id, label, reason)| {
            let selected = id == composer.picker.tab;
            let title: SharedString = match (&label, &reason) {
                (Some(label), Some(_)) => cx.t_with(
                    "composer.picker.providerUnavailable",
                    &Args::default().with("provider", label.clone()),
                ),
                (Some(label), None) => label.clone().into(),
                (None, _) => cx.t("composer.favorites.title"),
            };
            let icon: AnyElement = if id == FAVORITES_TAB {
                Icon::new(if selected { "star-filled" } else { "star" })
                    .size(16.0)
                    .stroke(1.75)
                    .into_any_element()
            } else {
                ProviderIcon::new(&id)
                    .size(17.0)
                    .opacity(if reason.is_some() { 0.4 } else { 1.0 })
                    .into_any_element()
            };
            let underline = selected.then(|| {
                div()
                    .absolute()
                    .left(px(8.0))
                    .right(px(8.0))
                    .bottom_0()
                    .h(px(2.0))
                    .rounded_full()
                    .bg(tab_accent(&id).map_or(foreground, hex))
            });
            let tab = id.clone();
            div()
                .id(SharedString::from(format!("picker-tab-{id}")))
                .relative()
                .flex()
                .flex_shrink_0()
                .h(px(40.0))
                .min_w(px(36.0))
                .items_center()
                .justify_center()
                .px(px(8.0))
                .rounded(px(8.0))
                .cursor_pointer()
                .text_color(if selected { foreground } else { muted })
                .hover(move |style| style.bg(hover).text_color(foreground))
                .on_click(cx.listener(move |composer, _, window, cx| {
                    composer.set_tab(tab.clone(), window, cx)
                }))
                .child(icon)
                .when(selected, |tab| {
                    tab.child(
                        div()
                            .ml(px(6.0))
                            .max_w(px(144.0))
                            .truncate()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(title.clone()),
                    )
                })
                .children(underline)
        });
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(2.0))
        .px(px(6.0))
        .border_b_1()
        .border_color(colors.border.opacity(0.6))
        .child(
            div()
                .id("picker-tabs")
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(2.0))
                .overflow_x_scroll()
                .children(buttons),
        )
        .into_any_element()
}

fn list(composer: &Composer, rows: &[PickerRow], cx: &mut Context<Composer>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let catalog = composer.catalog.clone().unwrap_or_default();
    let query = composer.query(cx);
    let selected_provider = composer.selected_provider_id();
    let selected_model = composer.selected_model_id();
    let effort_id = composer.effort_label_id();
    let active_provider = catalog
        .iter()
        .find(|provider| provider.id.0 == composer.picker.tab);
    let setup = active_provider
        .and_then(model_picker::setup_reason)
        .filter(|_| query.is_empty());
    let mut children: Vec<AnyElement> = Vec::new();
    if let (Some(reason), Some(provider)) = (setup, active_provider) {
        children.push(
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(12.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t_with(
                            "composer.picker.providerUnavailable",
                            &Args::default().with("provider", provider.label.clone()),
                        )),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .line_height(px(18.0))
                        .text_color(colors.muted_foreground)
                        .child(reason),
                )
                .into_any_element(),
        );
    } else if rows.is_empty() {
        let empty = if composer.picker.tab == FAVORITES_TAB && query.is_empty() {
            cx.t("composer.picker.favoritesEmpty")
        } else {
            cx.t("composer.model.empty")
        };
        children.push(
            div()
                .px(px(10.0))
                .py(px(16.0))
                .text_center()
                .text_size(px(12.0))
                .line_height(px(19.0))
                .text_color(colors.muted_foreground)
                .child(empty)
                .into_any_element(),
        );
    } else {
        let first_other = rows.iter().position(|row| row.from_other_provider);
        for (index, row) in rows.iter().enumerate() {
            if Some(index) == first_other {
                children.push(
                    div()
                        .px(px(10.0))
                        .pb(px(4.0))
                        .pt(px(8.0))
                        .text_size(px(10.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.muted_foreground.opacity(0.7))
                        .child(crate::ui::Tracked::new(
                            cx.t("composer.picker.otherProviders").to_uppercase(),
                            10.0,
                            0.06,
                        ))
                        .into_any_element(),
                );
            }
            let active = Some(&row.provider_id) == selected_provider.as_ref()
                && Some(&row.model_id) == selected_model.as_ref()
                && row.favorite.as_ref().is_none_or(|favorite| {
                    favorite
                        .effort
                        .as_ref()
                        .is_none_or(|effort| *effort == effort_id)
                });
            let starred = row.favorite.is_some()
                || is_favorited(
                    &composer.settings.favorites,
                    &row.provider_id,
                    &row.model_id,
                );
            let highlighted = index == composer.picker.highlighted;
            let picked = row.clone();
            let starred_row = row.clone();
            let effort_note = row
                .favorite
                .as_ref()
                .map(|favorite| match &favorite.effort {
                    None => cx.t("composer.favorites.managed"),
                    Some(effort) => cx.t(&format!("composer.effort.{effort}")),
                });
            let star_color = if starred {
                tw::AMBER_500.hsla()
            } else {
                colors.muted_foreground.opacity(0.35)
            };
            let star_hover = colors.background.opacity(0.7);
            children.push(
                div()
                    .id(SharedString::from(format!("picker-row-{index}")))
                    .flex()
                    .h(px(32.0))
                    .items_center()
                    .gap(px(8.0))
                    .pl(px(10.0))
                    .pr(px(4.0))
                    .rounded(px(8.0))
                    .text_size(px(13.0))
                    .when(highlighted, |item| item.bg(colors.accent))
                    .on_mouse_move(cx.listener(move |composer, _, _, cx| {
                        if composer.picker.highlighted != index {
                            composer.picker.highlighted = index;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |composer, _, window, cx| {
                        composer.pick_row(picked.clone(), window, cx)
                    }))
                    .when(row.favorite.is_some() || row.from_other_provider, |item| {
                        item.child(ProviderIcon::new(&row.provider_id).size(14.0))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .when(active, |label| label.font_weight(FontWeight::MEDIUM))
                            .child(SharedString::from(row.label.clone())),
                    )
                    .children(effort_note.map(|note| {
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(note)
                    }))
                    .when(active, |item| {
                        item.child(Icon::new("check").size(14.0).stroke(2.25))
                    })
                    .when(index < MAX_SHORTCUT_ROWS, |item| {
                        item.child(
                            div()
                                .flex_shrink_0()
                                .rounded(px(6.0))
                                .bg(colors.muted)
                                .px(px(6.0))
                                .text_size(px(10.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors.muted_foreground)
                                .child(format!("⌘{}", index + 1)),
                        )
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!("picker-star-{index}")))
                            .flex()
                            .flex_shrink_0()
                            .size(px(24.0))
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            .text_color(star_color)
                            .hover(move |style| style.bg(star_hover))
                            .on_click(cx.listener(move |composer, _, _, cx| {
                                cx.stop_propagation();
                                composer.toggle_favorite(&starred_row, cx);
                            }))
                            .child(
                                Icon::new(if starred { "star-filled" } else { "star" }).size(14.0),
                            ),
                    )
                    .into_any_element(),
            );
        }
    }
    div()
        .id("picker-list")
        .h(px(240.0))
        .overflow_y_scroll()
        .p(px(4.0))
        .children(children)
        .into_any_element()
}

fn slider(composer: &Composer, steps: &[String], cx: &mut Context<Composer>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let value = composer.effort_label_id();
    let index = steps.iter().position(|step| *step == value).unwrap_or(0);
    let last = steps.len().saturating_sub(1).max(1) as f32;
    let bounds = composer.picker.slider.clone();
    let width = bounds
        .get()
        .map_or(280.0, |bounds| f32::from(bounds.size.width));
    // Keep the 28px thumb inside the track at both ends.
    let position = |step: usize| 18.0 + (width - 36.0) * (step as f32 / last);
    let track_steps = steps.to_vec();
    let move_steps = steps.to_vec();
    let stops = steps.iter().enumerate().map(|(stop, step)| {
        let reached = stop <= index;
        let x = position(stop);
        if step == "ultrathink" {
            div()
                .absolute()
                .left(px(x - 6.0))
                .top(px(12.0))
                .child(Icon::new("sparkles").size(12.0).color(if reached {
                    gpui::white().opacity(0.85)
                } else {
                    colors.muted_foreground.opacity(0.6)
                }))
                .into_any_element()
        } else {
            div()
                .absolute()
                .left(px(x - 2.0))
                .top(px(16.0))
                .size(px(4.0))
                .rounded_full()
                .bg(if reached {
                    gpui::white().opacity(0.75)
                } else {
                    colors.muted_foreground.opacity(0.45)
                })
                .into_any_element()
        }
    });
    let thumb = position(index);
    div()
        .id("effort-slider")
        .relative()
        .mt(px(8.0))
        .h(px(36.0))
        .rounded_full()
        .bg(colors.muted)
        .cursor_pointer()
        .child(
            canvas(move |area, _, _| bounds.set(Some(area)), |_, _, _, _| {})
                .absolute()
                .size_full(),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w(px(thumb + 14.0))
                .rounded_full()
                .bg(colors.info),
        )
        .children(stops)
        .child(
            div()
                .absolute()
                .left(px(thumb - 14.0))
                .top(px(4.0))
                .size(px(28.0))
                .rounded_full()
                .bg(gpui::white())
                .shadow(vec![
                    BoxShadow {
                        color: gpui::black().opacity(0.25),
                        offset: point(px(0.0), px(1.0)),
                        blur_radius: px(3.0),
                        spread_radius: px(0.0),
                        inset: false,
                    },
                    BoxShadow {
                        color: gpui::black().opacity(0.08),
                        offset: point(px(0.0), px(0.0)),
                        blur_radius: px(0.0),
                        spread_radius: px(0.5),
                        inset: false,
                    },
                ]),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |composer, event: &MouseDownEvent, _, cx| {
                composer.picker.dragging = true;
                if let Some(step) = composer.slider_step(event.position.x, track_steps.len()) {
                    composer.commit_step(&track_steps[step].clone(), cx);
                }
            }),
        )
        .on_mouse_move(cx.listener(move |composer, event: &MouseMoveEvent, _, cx| {
            if composer.picker.dragging
                && event.pressed_button == Some(MouseButton::Left)
                && let Some(step) = composer.slider_step(event.position.x, move_steps.len())
            {
                composer.commit_step(&move_steps[step].clone(), cx);
            }
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|composer, _, _, _| composer.picker.dragging = false),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|composer, _, _, _| composer.picker.dragging = false),
        )
        .into_any_element()
}

fn effort_section(composer: &Composer, cx: &mut Context<Composer>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let steps = effort_steps(&composer.effort_levels());
    let fast = composer.settings.fast;
    let label = if steps.is_empty() {
        cx.t("composer.picker.managedEffort")
    } else {
        cx.t(&format!("composer.effort.{}", composer.effort_label_id()))
    };
    let direct = if fast {
        cx.t("composer.picker.directOn")
    } else {
        cx.t("composer.picker.directOff")
    };
    let hint = format!(
        "{direct} — {}",
        cx.t("composer.execution.response.directHint")
    );
    let hover = colors.accent;
    let levels = composer.effort_levels();
    let reset = (!steps.is_empty()).then(|| {
        crate::ui::Tooltip::text(
            "effort-reset-tip",
            div()
                .id("effort-reset")
                .flex()
                .size(px(28.0))
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .cursor_pointer()
                .text_color(colors.muted_foreground.opacity(0.7))
                .hover(move |style| style.bg(hover))
                .on_click(cx.listener(move |composer, _, _, cx| {
                    composer.set_effort(
                        EffortChoice {
                            effort: clamp_effort(DEFAULT_EFFORT, &levels),
                            ultrathink: false,
                        },
                        cx,
                    )
                }))
                .child(Icon::new("rotate-ccw").size(14.0)),
            cx.t("composer.execution.resetEffort"),
        )
        .into_any_element()
    });
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .border_t_1()
        .border_color(colors.border.opacity(0.6))
        .px(px(12.0))
        .pb(px(12.0))
        .pt(px(8.0))
        .child(
            div()
                .grid()
                .grid_cols(3)
                .items_center()
                .child(crate::ui::Tooltip::text(
                    "direct-tip",
                    div()
                        .id("direct-toggle")
                        .flex()
                        .size(px(28.0))
                        .items_center()
                        .justify_center()
                        .rounded(px(6.0))
                        .cursor_pointer()
                        .text_color(if fast {
                            tw::AMBER_500.hsla()
                        } else {
                            colors.muted_foreground.opacity(0.7)
                        })
                        .hover(move |style| style.bg(hover))
                        .on_click(
                            cx.listener(move |composer, _, _, cx| composer.set_fast(!fast, cx)),
                        )
                        .child(Icon::new(if fast { "zap-filled" } else { "zap" }).size(14.0)),
                    hint,
                ))
                .child(
                    div()
                        .truncate()
                        .text_center()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(if steps.is_empty() {
                            colors.muted_foreground
                        } else {
                            colors.info
                        })
                        .child(label),
                )
                .child(div().flex().justify_end().children(reset)),
        )
        .when(!steps.is_empty(), |section| {
            section.child(slider(composer, &steps, cx))
        })
        .into_any_element()
}

fn panel(composer: &Composer, cx: &mut Context<Composer>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let rows = composer.picker_rows(cx);
    let catalog = composer.catalog.clone().unwrap_or_default();
    let hidden =
        model_picker::providers_needing_setup(&catalog, composer.selected_provider_id().as_deref());
    let hidden_line = (!hidden.is_empty()).then(|| {
        div()
            .flex_shrink_0()
            .border_t_1()
            .border_color(colors.border.opacity(0.6))
            .px(px(12.0))
            .py(px(6.0))
            .text_size(px(11.0))
            .text_color(colors.muted_foreground)
            .child(cx.t_with(
                "composer.picker.hiddenProviders",
                &Args::count(hidden.len() as i64),
            ))
    });
    div()
        .id("model-picker")
        .flex()
        .flex_col()
        .min_h_0()
        .on_key_down(cx.listener(Composer::on_picker_key))
        .child(tabs(composer, cx))
        .children(super::accounts::account_strip(composer, cx))
        .child(
            div()
                .flex()
                .h(px(40.0))
                .flex_shrink_0()
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .border_b_1()
                .border_color(colors.border.opacity(0.6))
                .text_size(px(13.0))
                .child(
                    Icon::new("search")
                        .size(14.0)
                        .color(colors.muted_foreground.opacity(0.6)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(composer.picker.search.clone()),
                ),
        )
        .child(list(composer, &rows, cx))
        .children(hidden_line)
        .child(effort_section(composer, cx))
        .children(super::account_usage::account_usage_strip(composer, cx))
        .into_any_element()
}

/// The model chip, opening the picker above it.
pub(super) fn model_menu(
    composer: &Composer,
    trigger: AnyElement,
    cx: &mut Context<Composer>,
) -> AnyElement {
    let entity = cx.entity().downgrade();
    let open = composer.picker.open && composer.catalog.is_some();
    let content = open.then(|| panel(composer, cx));
    Popover::new(
        "model-picker-popover",
        open,
        trigger,
        move |_, _| content.unwrap_or_else(|| div().into_any_element()),
        move |open, window, cx| {
            let _ = entity.update(cx, |composer, cx| {
                composer.set_picker_open(open, window, cx)
            });
        },
    )
    .side(PopoverSide::Top)
    .align(PopoverAlign::End)
    .side_offset(6.0)
    .width(320.0)
    .padding(0.0)
    .into_any_element()
}

/// `getCompactComposerModelLabel` for the chip: Claude drops "Claude ".
pub(super) fn compact_model(provider: Option<&str>, model: &str) -> String {
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
