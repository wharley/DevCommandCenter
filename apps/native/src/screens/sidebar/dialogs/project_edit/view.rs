//! How the project dialog looks: identity on the left, linked folders and
//! the technical facts on the right (one column on narrow windows).

use gpui::{AnyElement, App, Context, Focusable, SharedString, Window, div, prelude::*, px};

use super::{AUTO, ProjectEditDialog};
use crate::i18n::Translate;
use crate::project::{COLOR_IDS, GlyphSize, ProjectGlyph, SYMBOL_IDS, mascot_ids};
use crate::screens::sidebar::Sidebar;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, Icon, Tracked, dialog_footer};

fn glyph_value(value: &str) -> Option<&str> {
    (value != AUTO).then_some(value)
}

fn folder_name(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// `pickTileClass`
fn tile(id: SharedString, selected: bool, cx: &App) -> gpui::Stateful<gpui::Div> {
    let colors = &cx.theme().colors;
    let hover = colors.muted.opacity(0.4);
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .h(px(36.0))
        .rounded(cx.theme().radius.lg)
        .border_1()
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .when(selected, |tile| {
            tile.border_color(colors.foreground.opacity(0.3))
                .bg(colors.muted.opacity(0.45))
                .shadow(vec![gpui::BoxShadow {
                    color: colors.foreground.opacity(0.1),
                    offset: gpui::point(px(0.0), px(0.0)),
                    blur_radius: px(0.0),
                    spread_radius: px(1.0),
                    inset: false,
                }])
        })
        .when(!selected, |tile| {
            tile.border_color(colors.border.opacity(0.55))
        })
}

fn small_note(text: SharedString, cx: &App) -> gpui::Div {
    div()
        .text_size(px(10.5))
        .line_height(px(16.0))
        .text_color(cx.theme().colors.muted_foreground)
        .child(text)
}

fn legend(text: SharedString) -> gpui::Div {
    div()
        .text_size(px(12.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .child(text)
}

fn auto_tile(
    id: &'static str,
    selected: bool,
    cx: &mut Context<Sidebar>,
    pick: fn(&mut ProjectEditDialog),
) -> AnyElement {
    tile(id.into(), selected, cx)
        .text_size(px(10.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .child(cx.t("projectEditor.autoShort"))
        .on_click(cx.listener(move |this, _, _, cx| this.edit_project(cx, pick)))
        .into_any_element()
}

fn name_block(
    dialog: &ProjectEditDialog,
    window: &mut Window,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let repository = &dialog.repository;
    let preview_icon = if dialog.icon == "logo" {
        dialog.logo.clone()
    } else {
        glyph_value(&dialog.icon).map(str::to_string)
    };
    let focused = dialog.name.focus_handle(cx).is_focused(window);
    let same_name = dialog.name.read(cx).text() == repository.name;
    div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .child(
            div().mb(px(2.0)).child(
                ProjectGlyph::new(
                    preview_icon.as_deref(),
                    glyph_value(&dialog.color),
                    Some(&repository.root_path),
                )
                .size(GlyphSize::Large),
            ),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.0))
                        .child(legend(cx.t("projectEditor.nameLabel")))
                        .child(
                            Button::new("project-use-repository-name")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .muted()
                                .disabled(same_name || dialog.saving)
                                .child(Icon::new("rotate-ccw").size(12.0))
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .child(cx.t("projectEditor.useRepositoryName")),
                                )
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.use_repository_name(cx)),
                                ),
                        ),
                )
                .child(
                    // shadcn's Input: `h-8 rounded-lg border-input px-2.5`, ring on focus.
                    div()
                        .flex()
                        .items_center()
                        .h(px(32.0))
                        .px(px(10.0))
                        .rounded(cx.theme().radius.lg)
                        .border_1()
                        .border_color(if focused { colors.ring } else { colors.input })
                        .bg(crate::ui::input_fill(cx.theme()))
                        .when(focused, |field| {
                            field.shadow(vec![gpui::BoxShadow {
                                color: colors.ring.opacity(0.5),
                                offset: gpui::point(px(0.0), px(0.0)),
                                blur_radius: px(0.0),
                                spread_radius: px(3.0),
                                inset: false,
                            }])
                        })
                        .text_size(px(14.0))
                        .when(dialog.saving, |field| field.opacity(0.5))
                        .child(dialog.name.clone()),
                )
                .child(small_note(cx.t("projectEditor.nameHint"), cx)),
        )
        .into_any_element()
}

fn icon_block(dialog: &ProjectEditDialog, cx: &mut Context<Sidebar>) -> AnyElement {
    let root = dialog.repository.root_path.clone();
    let color = glyph_value(&dialog.color).map(str::to_string);
    let mut fauna = vec![auto_tile(
        "project-icon-auto",
        dialog.icon == AUTO,
        cx,
        |dialog| dialog.icon = AUTO.into(),
    )];
    for option in mascot_ids() {
        fauna.push(
            tile(
                SharedString::from(format!("project-icon-{option}")),
                dialog.icon == option,
                cx,
            )
            .child(ProjectGlyph::new(
                Some(option),
                color.as_deref(),
                Some(&root),
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.edit_project(cx, |dialog| dialog.icon = option.into())
            }))
            .into_any_element(),
        );
    }
    let mut symbols = Vec::new();
    for option in SYMBOL_IDS {
        symbols.push(
            tile(
                SharedString::from(format!("project-icon-{option}")),
                dialog.icon == option,
                cx,
            )
            .child(ProjectGlyph::new(
                Some(option),
                color.as_deref(),
                Some(&root),
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.edit_project(cx, |dialog| dialog.icon = option.into())
            }))
            .into_any_element(),
        );
    }
    let logo_row = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .when_some(dialog.logo.clone(), |row, logo| {
            row.child(
                tile("project-icon-logo".into(), dialog.icon == "logo", cx)
                    .w(px(36.0))
                    .child(ProjectGlyph::new(Some(&logo), color.as_deref(), None))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.edit_project(cx, |dialog| dialog.icon = "logo".into())
                    })),
            )
        })
        .child(
            Button::new("project-upload-logo")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Sm)
                .disabled(dialog.saving)
                .child(div().text_size(px(11.0)).child(if dialog.logo.is_some() {
                    cx.t("projectEditor.replaceLogo")
                } else {
                    cx.t("projectEditor.uploadLogo")
                }))
                .on_click(cx.listener(|this, _, _, cx| this.pick_logo(cx))),
        )
        .when(dialog.logo.is_some(), |row| {
            row.child(
                Button::new("project-remove-logo")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .muted()
                    .disabled(dialog.saving)
                    .child(
                        div()
                            .text_size(px(11.0))
                            .child(cx.t("projectEditor.removeLogo")),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.edit_project(cx, |dialog| {
                            dialog.logo = None;
                            if dialog.icon == "logo" {
                                dialog.icon = AUTO.into();
                            }
                        })
                    })),
            )
        })
        .child(
            small_note(cx.t("projectEditor.logoHint"), cx)
                .min_w_0()
                .text_size(px(10.0)),
        );
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(legend(cx.t("projectEditor.iconLabel")))
        .child(small_note(cx.t("projectEditor.faunaLabel"), cx))
        .child(div().grid().grid_cols(6).gap(px(6.0)).children(fauna))
        .child(small_note(cx.t("projectEditor.logoLabel"), cx).pt(px(4.0)))
        .child(logo_row)
        .child(small_note(cx.t("projectEditor.symbolsLabel"), cx).pt(px(4.0)))
        .child(div().grid().grid_cols(6).gap(px(6.0)).children(symbols))
        .into_any_element()
}

fn color_block(dialog: &ProjectEditDialog, cx: &mut Context<Sidebar>) -> AnyElement {
    let root = dialog.repository.root_path.clone();
    let icon = glyph_value(&dialog.icon).map(str::to_string);
    let mut tiles = vec![auto_tile(
        "project-color-auto",
        dialog.color == AUTO,
        cx,
        |dialog| dialog.color = AUTO.into(),
    )];
    for option in COLOR_IDS {
        tiles.push(
            tile(
                SharedString::from(format!("project-color-{option}")),
                dialog.color == option,
                cx,
            )
            .child(ProjectGlyph::new(
                icon.as_deref(),
                Some(option),
                Some(&root),
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.edit_project(cx, |dialog| dialog.color = option.into())
            }))
            .into_any_element(),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(legend(cx.t("projectEditor.colorLabel")))
        .child(div().grid().grid_cols(7).gap(px(6.0)).children(tiles))
        .into_any_element()
}

fn folders_block(dialog: &ProjectEditDialog, cx: &mut Context<Sidebar>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let radius = cx.theme().radius;
    let mono = cx.theme().fonts.mono.clone();
    let mut items = Vec::new();
    for (index, folder) in dialog.folders.iter().enumerate() {
        let name = folder_name(folder);
        let context = dialog.context_folders.contains(folder);
        let remove = folder.clone();
        let hover = colors.muted;
        let hover_text = colors.foreground;
        items.push(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(radius.lg)
                .border_1()
                .border_color(colors.border.opacity(0.55))
                .text_size(px(11.0))
                .child(
                    div()
                        .text_color(colors.muted_foreground)
                        .child(Icon::new("folder").size(14.0)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .flex()
                                .min_w_0()
                                .items_center()
                                .gap(px(6.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(colors.foreground)
                                .child(div().truncate().child(SharedString::from(name.clone())))
                                .when(context, |line| {
                                    line.child(
                                        div()
                                            .flex_shrink_0()
                                            .px(px(6.0))
                                            .rounded_full()
                                            .bg(colors.muted)
                                            .text_size(px(9.5))
                                            .text_color(colors.muted_foreground)
                                            .child(cx.t("projectEditor.contextOnly")),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .truncate()
                                .font_family(mono.clone())
                                .text_size(px(10.0))
                                .text_color(colors.muted_foreground)
                                .child(SharedString::from(folder.clone())),
                        ),
                )
                .child(
                    div()
                        .id(("linked-folder-remove", index))
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
                        .size(px(24.0))
                        .rounded(radius.md)
                        .cursor_pointer()
                        .text_color(colors.muted_foreground)
                        .hover(move |style| style.bg(hover).text_color(hover_text))
                        .child(Icon::new("x").size(14.0))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_project(cx, |dialog| {
                                dialog.folders.retain(|candidate| *candidate != remove)
                            })
                        })),
                ),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(legend(cx.t("projectEditor.linkedFoldersLabel")))
        .child(small_note(cx.t("projectEditor.linkedFoldersHint"), cx).text_size(px(11.0)))
        .when(!items.is_empty(), |block| {
            block.child(div().flex().flex_col().gap(px(4.0)).children(items))
        })
        .child(
            div().flex().child(
                Button::new("project-add-linked-folder")
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Sm)
                    .disabled(dialog.saving)
                    .child(Icon::new("folder-plus").size(14.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .child(cx.t("projectEditor.addLinkedFolder")),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.add_linked_folder(cx))),
            ),
        )
        .into_any_element()
}

fn technical_block(dialog: &ProjectEditDialog, cx: &App) -> AnyElement {
    let colors = &cx.theme().colors;
    let mono = cx.theme().fonts.mono.clone();
    let repository = &dialog.repository;
    let icon = |name: &'static str| {
        div()
            .text_color(colors.muted_foreground)
            .child(Icon::new(name).size(14.0))
    };
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(12.0))
        .rounded(cx.theme().radius.xl)
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.muted.opacity(0.15))
        .child(
            div()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(colors.muted_foreground)
                .child(Tracked::new(
                    cx.t("projectEditor.technicalIdentity").to_uppercase(),
                    10.0,
                    0.08,
                )),
        )
        .child(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(10.0))
                .text_size(px(11.0))
                .child(icon("folder"))
                .child(
                    div()
                        .min_w_0()
                        .child(
                            div()
                                .truncate()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(colors.foreground)
                                .child(SharedString::from(repository.name.clone())),
                        )
                        .child(
                            div()
                                .truncate()
                                .font_family(mono)
                                .text_size(px(10.0))
                                .text_color(colors.muted_foreground)
                                .child(SharedString::from(repository.root_path.clone())),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(10.0))
                .text_size(px(11.0))
                .child(icon("git-branch"))
                .child(
                    div()
                        .truncate()
                        .child(SharedString::from(repository.base_branch.clone())),
                ),
        )
        .into_any_element()
}

pub fn render(
    _sidebar: &Sidebar,
    dialog: &ProjectEditDialog,
    window: &mut Window,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let viewport = window.viewport_size();
    let width = (f32::from(viewport.width) - 32.0).min(832.0);
    let two_columns = f32::from(viewport.width) >= 768.0;
    let max_height = (f32::from(viewport.height) * 0.88).min(736.0);
    let saving = dialog.saving;
    let dirty = dialog.is_dirty(cx);
    let header = div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .pr(px(28.0))
        .child(
            div()
                .text_size(px(15.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(cx.t("projectEditor.title")),
        )
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(20.0))
                .text_color(cx.theme().colors.muted_foreground)
                .child(cx.t("projectEditor.description")),
        );
    let left = div()
        .flex()
        .min_w_0()
        .flex_col()
        .gap(px(20.0))
        .child(name_block(dialog, window, cx))
        .child(icon_block(dialog, cx))
        .child(color_block(dialog, cx));
    let right = div()
        .flex()
        .min_w_0()
        .flex_col()
        .gap(px(20.0))
        .child(folders_block(dialog, cx))
        .child(technical_block(dialog, cx));
    let body = div()
        .id("project-edit-body")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .mr(px(-8.0))
        .pr(px(8.0))
        .grid()
        .grid_cols(if two_columns { 2 } else { 1 })
        .gap(px(24.0))
        .child(left)
        .child(right);
    let footer = dialog_footer(
        [
            Button::new("project-edit-cancel")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Default)
                .disabled(saving)
                .child(cx.t("projectEditor.cancel"))
                .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx)))
                .into_any_element(),
            Button::new("project-edit-save")
                .size(ButtonSize::Default)
                .disabled(!dirty || saving)
                .child(if saving {
                    cx.t("projectEditor.saving")
                } else {
                    cx.t("projectEditor.save")
                })
                .on_click(cx.listener(|this, _, _, cx| this.save_project(cx)))
                .into_any_element(),
        ],
        cx,
    );
    let close = cx.entity().downgrade();
    Dialog::new("project-edit-dialog", move |_: &mut Window, cx| {
        let _ = close.update(cx, |sidebar, cx| sidebar.close_dialog(cx));
    })
    .max_width(width)
    .max_height(max_height)
    .padding(20.0)
    .gap(20.0)
    .close_button(!saving)
    .child(header)
    .child(body)
    .child(footer)
    .into_any_element()
}
