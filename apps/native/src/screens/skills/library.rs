//! The three lists: "Biblioteca" (the search, "Nova skill", the GitHub
//! import and each skill's card with edit and delete), "Detectado" (the
//! context the agents already read) and "Catálogo" (the bundled presets).

use dcc_infra::skills_context::SkillContextDetection;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px};

use super::Skills;
use crate::data::skills::{catalog, target};
use crate::i18n::{Args, Translate};
use crate::theme::color::mix_srgb;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::{Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Icon, spinner};

/// `.skills-empty`: an icon in the calm accent and a line.
fn empty(icon: &'static str, size: f32, text: SharedString, cx: &Context<Skills>) -> gpui::Div {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(180.0))
        .items_center()
        .justify_center()
        .gap(px(13.0))
        .p(px(30.0))
        .text_center()
        .text_color(colors.muted_foreground)
        .child(
            div()
                .opacity(0.7)
                .child(Icon::new(icon).size(size).color(colors.accent_calm)),
        )
        .child(
            div()
                .max_w(px(340.0))
                .text_size(px(12.0))
                .line_height(px(20.4))
                .child(text),
        )
}

/// `.skills-detection-hint`
fn hint(text: SharedString, cx: &Context<Skills>) -> AnyElement {
    div()
        .py(px(8.0))
        .text_size(px(12.0))
        .line_height(px(19.2))
        .text_color(cx.theme().colors.muted_foreground)
        .child(text)
        .into_any_element()
}

/// `.skills-target-badges`
fn target_badges(targets: &[String], cx: &Context<Skills>) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .mt(px(10.0))
        .flex()
        .flex_wrap()
        .gap(px(5.0))
        .children(targets.iter().map(|id| {
            div()
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(6.0))
                .bg(colors.muted)
                .text_size(px(10.0))
                .text_color(colors.muted_foreground)
                .child(target(id).0)
        }))
        .into_any_element()
}

impl Skills {
    /// The search and "Nova skill", then the GitHub import.
    pub(super) fn library_tools(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let edge = crate::ui::surface_edge(cx.theme());
        let search_focused =
            gpui::Focusable::focus_handle(self.search.read(cx), cx).is_focused(window);
        let url_focused =
            gpui::Focusable::focus_handle(self.github_url.read(cx), cx).is_focused(window);
        let blocked = self.busy || self.loading || !self.can_write();
        let has_url = !self.github_url.read(cx).text().trim().is_empty();
        div()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .px(px(28.0))
                    .pt(px(18.0))
                    .pb(px(12.0))
                    .child(
                        div()
                            .debug_selector(|| "skills-search".into())
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap(px(9.0))
                            .px(px(11.0))
                            .py(px(8.0))
                            .rounded(px(11.0))
                            .border_1()
                            .border_color(if search_focused {
                                colors.accent_calm
                            } else {
                                edge
                            })
                            .bg(colors.background)
                            .text_color(colors.muted_foreground)
                            .when(search_focused, |field| {
                                field.shadow(vec![gpui::BoxShadow {
                                    color: colors.accent_calm.opacity(0.08),
                                    offset: gpui::point(px(0.0), px(0.0)),
                                    blur_radius: px(0.0),
                                    spread_radius: px(3.0),
                                    inset: false,
                                }])
                            })
                            .child(Icon::new("search").size(15.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(12.0))
                                    .text_color(colors.foreground)
                                    .child(self.search.clone()),
                            ),
                    )
                    .child(
                        Button::new("skills-new")
                            .size(ButtonSize::Sm)
                            .disabled(blocked)
                            .on_click(
                                cx.listener(|skills, _, window, cx| skills.open_new(window, cx)),
                            )
                            .child(Icon::new("plus").size(15.0))
                            .child(cx.t("skills.newSkill")),
                    ),
            )
            .child(
                div()
                    .px(px(28.0))
                    .pb(px(14.0))
                    .child(
                        div()
                            .mb(px(6.0))
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(cx.t("skills.github.urlLabel")),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .h(px(32.0))
                                    .flex()
                                    .items_center()
                                    .px(px(10.0))
                                    .rounded(cx.theme().radius.lg)
                                    .border_1()
                                    .border_color(if url_focused {
                                        colors.ring
                                    } else {
                                        colors.input
                                    })
                                    .text_size(px(14.0))
                                    .child(self.github_url.clone()),
                            )
                            .child(
                                Button::new("skills-github-preview")
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Sm)
                                    .disabled(blocked || !has_url)
                                    .on_click(
                                        cx.listener(|skills, _, _, cx| skills.preview_import(cx)),
                                    )
                                    .child(if self.busy {
                                        spinner("skills-previewing", 14.0).into_any_element()
                                    } else {
                                        Icon::new("download").size(14.0).into_any_element()
                                    })
                                    .child(cx.t("skills.github.previewImport")),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(6.0))
                            .text_size(px(10.0))
                            .line_height(px(15.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("skills.github.urlHint")),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn library(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let theme = cx.theme();
        let edge = crate::ui::surface_edge(theme);
        let surface = theme.radius.surface;
        let searching = !self.search.read(cx).text().trim().is_empty();
        let filtered: Vec<_> = self.filtered(cx).into_iter().cloned().collect();
        if filtered.is_empty() {
            return empty(
                "file-text",
                28.0,
                cx.t(if searching {
                    "skills.design.noResults"
                } else {
                    "skills.emptyList"
                }),
                cx,
            )
            .when(searching, |empty| {
                empty.child(
                    Button::new("skills-clear-search")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .on_click(cx.listener(|skills, _, _, cx| {
                            skills.search.update(cx, |input, cx| input.set_text("", cx));
                            cx.notify();
                        }))
                        .child(cx.t("skills.design.clearSearch")),
                )
            })
            .into_any_element();
        }
        let blocked = self.busy || !self.can_write();
        let cards = filtered.into_iter().enumerate().map(|(index, skill)| {
            let edit = skill.clone();
            let name = skill.name.clone();
            let source = skill.source_url.clone();
            div()
                .id(("skills-card", index))
                .debug_selector(move || format!("skills-card-{index}"))
                .flex()
                .gap(px(12.0))
                .min_w_0()
                .p(px(17.0))
                .rounded(surface)
                .border_1()
                .border_color(edge)
                .bg(colors.background)
                .shadow(crate::ui::card_shadow())
                .hover(|card| card.border_color(mix_srgb(colors.accent_calm, 0.3, colors.border)))
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .size(px(34.0))
                        .items_center()
                        .justify_center()
                        .rounded(px(11.0))
                        .text_color(colors.accent_calm)
                        .bg(colors.accent_calm.opacity(0.1))
                        .child(Icon::new("file-text").size(17.0)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap(px(7.0))
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(skill.name.clone()),
                                )
                                .when(skill.disable_model_invocation, |heading| {
                                    heading.child(
                                        Badge::new(
                                            cx.t("skills.slashOnlyBadge"),
                                            BadgeVariant::Outline,
                                        )
                                        .text_size(9.0),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .mt(px(5.0))
                                .text_size(px(12.0))
                                .line_height(px(19.8))
                                .text_color(colors.muted_foreground)
                                .child(skill.description.clone()),
                        )
                        .children(source.map(|url| {
                            div()
                                .id(("skills-origin", index))
                                .mt(px(5.0))
                                .text_size(px(10.0))
                                .text_color(colors.accent_calm)
                                .cursor_pointer()
                                .on_click(move |_, _, cx| cx.open_url(&url))
                                .child(cx.t("skills.github.importedFrom"))
                        }))
                        .child(target_badges(&skill.target_agents, cx)),
                )
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .gap(px(2.0))
                        .child(
                            Button::new(("skills-edit", index))
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::IconSm)
                                .disabled(blocked)
                                .on_click(cx.listener(move |skills, _, window, cx| {
                                    skills.open_edit(edit.clone(), window, cx)
                                }))
                                .child(Icon::new("pencil").size(14.0)),
                        )
                        .child(
                            Button::new(("skills-delete", index))
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::IconSm)
                                .muted()
                                .hover_text_color(colors.destructive)
                                .disabled(blocked)
                                .on_click(cx.listener(move |skills, _, _, cx| {
                                    skills.delete(name.clone(), cx)
                                }))
                                .child(Icon::new("trash-2").size(14.0)),
                        ),
                )
                .into_any_element()
        });
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .children(cards)
            .into_any_element()
    }

    fn detection(
        &self,
        index: usize,
        item: &SkillContextDetection,
        cx: &Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let edge = crate::ui::surface_edge(cx.theme());
        let behavior = match item.kind.as_str() {
            "dcc_source" => "dccSource",
            "instructions_file" => "instructionsFile",
            "claude_skills" => "claudeSkills",
            "cursor_rules" => "cursorRules",
            "codex_skills" => "codexSkills",
            "grok_skills" => "grokSkills",
            _ => "generic",
        };
        div()
            .debug_selector(move || format!("skills-detection-{index}"))
            .flex()
            .items_start()
            .gap(px(12.0))
            .p(px(17.0))
            .rounded(cx.theme().radius.surface)
            .border_1()
            .border_color(edge)
            .bg(mix_srgb(colors.muted, 0.2, colors.background))
            .child(
                div()
                    .mt(px(3.0))
                    .flex_none()
                    .child(Icon::new("file-text").size(17.0).color(colors.accent_calm)),
            )
            .child(
                div()
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(item.title.clone()),
                            )
                            .child(
                                Badge::new(
                                    cx.t_with(
                                        "skills.detected.itemCount",
                                        &Args::count(item.count as i64),
                                    ),
                                    BadgeVariant::Outline,
                                )
                                .text_size(9.0),
                            )
                            .when(item.managed_count > 0, |row| {
                                row.child(
                                    Badge::new(
                                        cx.t_with(
                                            "skills.detected.managedBadge",
                                            &Args::count(item.managed_count as i64),
                                        ),
                                        BadgeVariant::Secondary,
                                    )
                                    .text_size(9.0),
                                )
                            })
                            .when(item.external_count > 0, |row| {
                                row.child(
                                    Badge::new(
                                        cx.t_with(
                                            "skills.detected.externalBadge",
                                            &Args::count(item.external_count as i64),
                                        ),
                                        BadgeVariant::Outline,
                                    )
                                    .text_size(9.0),
                                )
                            }),
                    )
                    .child(
                        div()
                            .mt(px(8.0))
                            .font_family(fonts::MONO)
                            .text_size(px(11.0))
                            .child(item.relative_path.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t(if item.root_kind == "project_root" {
                                "skills.detected.projectRoot"
                            } else {
                                "skills.detected.worktreeRoot"
                            })),
                    )
                    .child(
                        div()
                            .mt(px(8.0))
                            .text_size(px(11.0))
                            .line_height(px(17.6))
                            .text_color(colors.muted_foreground)
                            .child(cx.t(&format!("skills.detected.behavior.{behavior}"))),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn detected(&self, cx: &mut Context<Self>) -> AnyElement {
        let items: Vec<AnyElement> = if self.detections.is_empty() {
            vec![empty("folder-search", 28.0, cx.t("skills.detected.empty"), cx).into_any_element()]
        } else {
            self.detections
                .iter()
                .enumerate()
                .map(|(index, item)| self.detection(index, item, cx))
                .collect()
        };
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(hint(cx.t("skills.detected.hint"), cx))
            .children(items)
            .into_any_element()
    }

    pub(super) fn catalog(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let surface = cx.theme().radius.surface;
        let presets =
            catalog()
                .into_iter()
                .enumerate()
                .map(|(index, (skill, title, description))| {
                    let installed = self.skills.iter().any(|item| item.name == skill.name);
                    let preset = skill.clone();
                    div()
                        .debug_selector(move || format!("skills-preset-{index}"))
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .p(px(15.0))
                        .rounded(surface)
                        .border_1()
                        .border_color(mix_srgb(colors.accent_idea, 0.2, colors.border))
                        .bg(mix_srgb(colors.accent_idea, 0.05, colors.popover))
                        .child(
                            div()
                                .flex()
                                .flex_none()
                                .size(px(36.0))
                                .items_center()
                                .justify_center()
                                .rounded(px(13.0))
                                .text_color(colors.accent_idea)
                                .bg(colors.accent_idea.opacity(0.1))
                                .child(Icon::new("workflow").size(19.0)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(cx.t(title)),
                                )
                                .child(
                                    div()
                                        .mt(px(3.0))
                                        .text_size(px(11.0))
                                        .line_height(px(16.5))
                                        .text_color(colors.muted_foreground)
                                        .child(cx.t(description)),
                                )
                                .child(target_badges(&skill.target_agents, cx)),
                        )
                        .child(
                            Button::new(("skills-preset-use", index))
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Sm)
                                .disabled(self.busy || !self.can_write())
                                .on_click(cx.listener(move |skills, _, window, cx| {
                                    skills.open_catalog(preset.clone(), window, cx)
                                }))
                                .child(cx.t(if installed {
                                    "skills.catalog.update"
                                } else {
                                    "skills.design.usePreset"
                                }))
                                .when(!installed, |button| {
                                    button.child(Icon::new("chevron-right").size(13.0))
                                }),
                        )
                        .into_any_element()
                });
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(hint(cx.t("skills.catalog.hint"), cx))
            .children(presets)
            .into_any_element()
    }

    pub(super) fn loading_state(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        div()
            .flex()
            .flex_col()
            .min_h(px(180.0))
            .items_center()
            .justify_center()
            .gap(px(13.0))
            .p(px(30.0))
            .text_color(colors.muted_foreground)
            .child(spinner("skills-loading", 24.0))
            .child(div().text_size(px(12.0)).child(cx.t("skills.loading")))
            .into_any_element()
    }
}
