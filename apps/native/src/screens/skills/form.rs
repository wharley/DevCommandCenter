//! The form (`FormState`): name and description side by side, the body in
//! mono, what a GitHub import brings, the agents it compiles to and "only
//! when called"; the footer saves, adds from the catalog or imports.

use dcc_infra::skills::{SkillImportPreview, SkillRecord};
use gpui::{
    AnyElement, AppContext, Context, Entity, FontWeight, SharedString, Window, div, prelude::*, px,
};

use super::{Form, Skills};
use crate::data::skills::TARGETS;
use crate::i18n::{Args, Translate};
use crate::theme::color::mix_srgb;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::text_area::{TextArea, TextAreaEvent};
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Switch, spinner};

/// The record the form starts from.
struct Start {
    editing_existing: bool,
    from_catalog: bool,
    github: Option<SkillImportPreview>,
    skill: SkillRecord,
}

impl Skills {
    fn open_form(&mut self, start: Start, window: &mut Window, cx: &mut Context<Self>) {
        let placeholder = |key: &str, cx: &Context<Self>| Translate::t(&**cx, key);
        let (name_hint, description_hint, body_hint) = (
            placeholder("skills.fields.namePlaceholder", cx),
            placeholder("skills.fields.descriptionPlaceholder", cx),
            placeholder("skills.fields.bodyPlaceholder", cx),
        );
        let skill = start.skill;
        let name = cx.new(|cx| {
            let mut input = TextInput::new(window, cx)
                .with_placeholder(name_hint)
                .with_max_len(64);
            input.set_text(skill.name.clone(), cx);
            input
        });
        let description = cx.new(|cx| {
            let mut input = TextInput::new(window, cx).with_placeholder(description_hint);
            input.set_text(skill.description.clone(), cx);
            input
        });
        let body_text = skill.body.clone();
        let body = cx.new(|cx| {
            let mut area = TextArea::new(window, cx).with_placeholder(body_hint);
            area.set_text(&body_text, cx);
            area
        });
        let events = vec![
            cx.subscribe(&name, |skills, _, event: &TextInputEvent, cx| {
                skills.form_input_event(event, cx)
            }),
            cx.subscribe(&description, |skills, _, event: &TextInputEvent, cx| {
                skills.form_input_event(event, cx)
            }),
            cx.subscribe(
                &body,
                |skills, area, event: &TextAreaEvent, cx| match event {
                    // A text box: Enter is a new line.
                    TextAreaEvent::Submitted => area.update(cx, |area, cx| area.insert("\n", cx)),
                    TextAreaEvent::Cancelled => skills.back_to_library(cx),
                    _ => {}
                },
            ),
        ];
        // `autoFocus`: the name for a new skill, else the description.
        let fixed_name = start.editing_existing || start.from_catalog;
        let focus = if fixed_name {
            gpui::Focusable::focus_handle(description.read(cx), cx)
        } else {
            gpui::Focusable::focus_handle(name.read(cx), cx)
        };
        window.focus(&focus, cx);
        self.form = Some(Form {
            editing_existing: start.editing_existing,
            from_catalog: start.from_catalog,
            source_url: skill.source_url.clone(),
            github: start.github,
            name,
            description,
            body,
            targets: if skill.target_agents.is_empty() {
                vec!["claude".into()]
            } else {
                skill.target_agents
            },
            disable_invocation: skill.disable_model_invocation,
            _events: events,
        });
        cx.notify();
    }

    fn form_input_event(&mut self, event: &TextInputEvent, cx: &mut Context<Self>) {
        match event {
            TextInputEvent::Submitted => self.save(cx),
            TextInputEvent::Cancelled => self.back_to_library(cx),
            _ => cx.notify(),
        }
    }

    pub(super) fn open_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let skill = SkillRecord {
            name: String::new(),
            description: String::new(),
            body: String::new(),
            target_agents: vec!["claude".into()],
            disable_model_invocation: false,
            scope: "project".into(),
            source_url: None,
        };
        self.open_form(
            Start {
                editing_existing: false,
                from_catalog: false,
                github: None,
                skill,
            },
            window,
            cx,
        );
    }

    pub(super) fn open_edit(
        &mut self,
        skill: SkillRecord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_form(
            Start {
                editing_existing: true,
                from_catalog: false,
                github: None,
                skill,
            },
            window,
            cx,
        );
    }

    /// `catalogForm`: the preset, keeping an installed copy's choices.
    pub(super) fn open_catalog(
        &mut self,
        preset: SkillRecord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let installed = self
            .skills
            .iter()
            .find(|skill| skill.name == preset.name)
            .cloned();
        let skill = SkillRecord {
            target_agents: installed.as_ref().map_or_else(
                || preset.target_agents.clone(),
                |skill| skill.target_agents.clone(),
            ),
            disable_model_invocation: installed
                .as_ref()
                .map_or(preset.disable_model_invocation, |skill| {
                    skill.disable_model_invocation
                }),
            source_url: None,
            ..preset
        };
        self.open_form(
            Start {
                editing_existing: installed.is_some(),
                from_catalog: true,
                github: None,
                skill,
            },
            window,
            cx,
        );
    }

    pub(super) fn open_import(
        &mut self,
        preview: SkillImportPreview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let skill = SkillRecord {
            target_agents: vec!["claude".into()],
            disable_model_invocation: false,
            source_url: Some(preview.source_url.clone()),
            ..preview.skill.clone()
        };
        self.open_form(
            Start {
                editing_existing: false,
                from_catalog: false,
                github: Some(preview),
                skill,
            },
            window,
            cx,
        );
    }

    pub(super) fn back_to_library(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.form = None;
        cx.notify();
    }

    fn toggle_target(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(form) = &mut self.form {
            if let Some(index) = form.targets.iter().position(|target| target == id) {
                form.targets.remove(index);
            } else {
                form.targets.push(id.to_string());
            }
            cx.notify();
        }
    }

    pub(super) fn form_heading(&self, form: &Form, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let edge = crate::ui::surface_edge(cx.theme());
        let title = if form.editing_existing {
            "skills.design.edit"
        } else if form.github.is_some() {
            "skills.github.reviewTitle"
        } else if form.from_catalog {
            "skills.catalog.review"
        } else {
            "skills.newSkill"
        };
        let hint = if form.github.is_some() {
            "skills.github.reviewHint"
        } else if form.from_catalog {
            if form.editing_existing {
                "skills.catalog.updateHint"
            } else {
                "skills.catalog.reviewHint"
            }
        } else {
            "skills.design.formHint"
        };
        div()
            .flex()
            .items_center()
            .gap(px(9.0))
            .px(px(24.0))
            .py(px(12.0))
            .border_t_1()
            .border_color(edge)
            .child(
                Button::new("skills-back")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconSm)
                    .disabled(self.busy)
                    .on_click(cx.listener(|skills, _, _, cx| skills.back_to_library(cx)))
                    .child(Icon::new("arrow-left").size(15.0)),
            )
            .child(
                div()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(cx.t(title)),
                    )
                    .child(
                        div()
                            .mt(px(3.0))
                            .text_size(px(11.0))
                            .line_height(px(16.5))
                            .text_color(colors.muted_foreground)
                            .child(cx.t(hint)),
                    ),
            )
            .into_any_element()
    }

    fn input_box(
        &self,
        input: &Entity<TextInput>,
        fixed: bool,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let colors = &cx.theme().colors;
        let edge = crate::ui::surface_edge(cx.theme());
        let focused = gpui::Focusable::focus_handle(input.read(cx), cx).is_focused(window);
        let content: AnyElement = if fixed {
            // `disabled`: the name a skill already has.
            div()
                .opacity(0.5)
                .child(SharedString::from(input.read(cx).text().to_string()))
                .into_any_element()
        } else {
            input.clone().into_any_element()
        };
        div()
            .h(px(32.0))
            .flex()
            .items_center()
            .px(px(10.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(if focused { colors.ring } else { edge })
            .bg(colors.popover)
            .text_size(px(12.0))
            .child(content)
            .into_any_element()
    }

    pub(super) fn form_body(
        &self,
        form: &Form,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let edge = crate::ui::surface_edge(theme);
        let surface = theme.radius.surface;
        let label = |text: SharedString| {
            div()
                .mb(px(8.0))
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(text)
        };
        let section = || {
            div()
                .p(px(18.0))
                .rounded(surface)
                .border_1()
                .border_color(edge)
                .bg(colors.background)
        };
        let fixed_name = form.editing_existing || form.from_catalog;
        let body_focus = gpui::Focusable::focus_handle(form.body.read(cx), cx).is_focused(window);
        let fields = section()
            .child(
                div()
                    .flex()
                    .gap(px(14.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(label(cx.t("skills.fields.name")))
                            .child(self.input_box(&form.name, fixed_name, window, cx)),
                    )
                    .child({
                        let mut wide = div()
                            .min_w_0()
                            .flex_basis(px(0.0))
                            .child(label(cx.t("skills.fields.description")))
                            .child(self.input_box(&form.description, false, window, cx));
                        wide.style().flex_grow = Some(2.0);
                        wide
                    }),
            )
            .child(
                div()
                    .mt(px(18.0))
                    .child(label(cx.t("skills.fields.body")))
                    .child(
                        div()
                            .min_h(px(220.0))
                            .px(px(12.0))
                            .py(px(8.0))
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(if body_focus { colors.ring } else { edge })
                            .bg(colors.popover)
                            .font_family(fonts::MONO)
                            .text_size(px(12.0))
                            .line_height(px(20.4))
                            .child(form.body.clone()),
                    ),
            );
        let review = form
            .github
            .as_ref()
            .map(|preview| self.import_review(form, preview, cx));
        let targets = TARGETS.iter().map(|(id, name, path)| {
            let pressed = form.targets.iter().any(|target| target == id);
            let id = *id;
            div()
                .id(SharedString::from(format!("skills-target-{id}")))
                .debug_selector(move || format!("skills-target-{id}"))
                .flex()
                .flex_1()
                .flex_basis(px(0.0))
                .min_w_0()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(11.0))
                .rounded(px(11.0))
                .border_1()
                .cursor_pointer()
                .map(|target| {
                    if pressed {
                        target
                            .border_color(mix_srgb(colors.accent_calm, 0.45, colors.border))
                            .bg(mix_srgb(colors.accent_calm, 0.06, colors.popover))
                    } else {
                        target.border_color(edge).bg(colors.popover)
                    }
                })
                .on_click(cx.listener(move |skills, _, _, cx| skills.toggle_target(id, cx)))
                .child(Icon::new("bot").size(15.0).color(if pressed {
                    colors.accent_calm
                } else {
                    colors.muted_foreground
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .child(*name),
                        )
                        .child(
                            div()
                                .mt(px(3.0))
                                .font_family(fonts::MONO)
                                .text_size(px(9.0))
                                .text_color(colors.muted_foreground)
                                .child(*path),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .size(px(15.0))
                        .items_center()
                        .justify_center()
                        .rounded(px(5.0))
                        .border_1()
                        .map(|check| {
                            if pressed {
                                check
                                    .border_color(gpui::transparent_black())
                                    .bg(colors.accent_calm)
                                    .text_color(colors.background)
                                    .child(Icon::new("check").size(11.0))
                            } else {
                                check.border_color(edge)
                            }
                        }),
                )
                .into_any_element()
        });
        let mut targets: Vec<AnyElement> = targets.collect();
        let mut rows = Vec::new();
        while !targets.is_empty() {
            let rest = targets.split_off(targets.len().min(3));
            let mut row = div()
                .flex()
                .gap(px(8.0))
                .children(std::mem::replace(&mut targets, rest));
            // The last row keeps the grid's thirds.
            for _ in row_len_padding(&rows, 5) {
                row = row.child(div().flex_1().flex_basis(px(0.0)));
            }
            rows.push(row);
        }
        let legacy = form
            .targets
            .iter()
            .any(|target| target == "agents")
            .then(|| {
                div()
                    .mt(px(8.0))
                    .text_size(px(12.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("skills.legacyTargetHint"))
                    .child(
                        div()
                            .id("skills-remove-legacy")
                            .mt(px(4.0))
                            .underline()
                            .cursor_pointer()
                            .on_click(
                                cx.listener(|skills, _, _, cx| skills.toggle_target("agents", cx)),
                            )
                            .child(cx.t("skills.removeLegacyTarget")),
                    )
            });
        let entity = cx.entity().downgrade();
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(px(18.0))
            .child(fields)
            .children(review)
            .child(
                section()
                    .child(
                        div()
                            .mb(px(12.0))
                            .text_size(px(12.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(cx.t("skills.fields.targetAgents")),
                    )
                    .child(div().flex().flex_col().gap(px(8.0)).children(rows))
                    .children(legacy)
                    .child(
                        div()
                            .mt(px(12.0))
                            .text_size(px(11.0))
                            .line_height(px(17.6))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("skills.targetsHint")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(18.0))
                    .px(px(18.0))
                    .py(px(16.0))
                    .rounded(surface)
                    .border_1()
                    .border_color(edge)
                    .child(
                        div()
                            .flex_1()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(cx.t("skills.disableInvocation.title")),
                            )
                            .child(
                                div()
                                    .mt(px(5.0))
                                    .text_size(px(11.0))
                                    .line_height(px(17.6))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("skills.disableInvocation.body")),
                            ),
                    )
                    .child(
                        Switch::new("skills-invocation", form.disable_invocation)
                            .disabled(self.busy)
                            .on_toggle(move |checked, _, cx| {
                                let _ = entity.update(cx, |skills, cx| {
                                    if let Some(form) = &mut skills.form {
                                        form.disable_invocation = checked;
                                        cx.notify();
                                    }
                                });
                            }),
                    ),
            )
            .into_any_element()
    }

    fn import_review(
        &self,
        form: &Form,
        preview: &SkillImportPreview,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let edge = crate::ui::surface_edge(cx.theme());
        let note = |text: String| {
            div()
                .text_size(px(10.0))
                .line_height(px(15.0))
                .text_color(colors.muted_foreground)
                .child(text)
        };
        let name = form.name.read(cx).text().trim().to_string();
        let duplicate = self.skills.iter().any(|skill| skill.name == name);
        let total = preview.files.len();
        let files = (total > 0).then(|| {
            div()
                .id("skills-import-files")
                .max_h(px(90.0))
                .overflow_y_scroll()
                .pl(px(18.0))
                .text_size(px(10.0))
                .text_color(colors.muted_foreground)
                .flex()
                .flex_col()
                .gap(px(3.0))
                .children(preview.files.iter().take(8).map(|file| {
                    div()
                        .font_family(fonts::MONO)
                        .text_size(px(9.0))
                        .child(format!("• {}", file.path))
                }))
                .when(total > 8, |list| {
                    list.child(cx.t_with("skills.github.moreFiles", &Args::count(total as i64 - 8)))
                })
        });
        let url = preview.source_url.clone();
        div()
            .debug_selector(|| "skills-import-review".into())
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(12.0))
            .py(px(11.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(edge)
            .bg(mix_srgb(colors.accent_calm, 0.05, colors.background))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .text_size(px(11.0))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .child(cx.t("skills.github.recognized")),
                    )
                    .child(div().text_color(colors.muted_foreground).child(
                        cx.t_with("skills.github.resourceCount", &Args::count(total as i64)),
                    )),
            )
            .child(
                div()
                    .id("skills-import-source")
                    .text_size(px(10.0))
                    .text_color(colors.accent_calm)
                    .cursor_pointer()
                    .on_click(move |_, _, cx| cx.open_url(&url))
                    .child(preview.source_url.clone()),
            )
            .children(preview.warnings.iter().map(|warning| note(warning.clone())))
            .when(duplicate, |review| {
                review.child(note(cx.t("skills.github.duplicateName").to_string()))
            })
            .children(files)
            .child(note(cx.t("skills.github.trustHint").to_string()))
            .into_any_element()
    }

    pub(super) fn form_footer(&self, form: &Form, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let edge = crate::ui::surface_edge(cx.theme());
        let action = if form.from_catalog && !form.editing_existing {
            "skills.catalog.add"
        } else if form.github.is_some() {
            "skills.github.confirmImport"
        } else {
            "skills.save"
        };
        div()
            .flex()
            .flex_wrap()
            .flex_none()
            .items_center()
            .gap(px(8.0))
            .px(px(28.0))
            .py(px(14.0))
            .border_t_1()
            .border_color(edge)
            .bg(mix_srgb(colors.muted, 0.25, colors.popover))
            .child(
                div()
                    .flex_1()
                    .min_w(px(120.0))
                    .text_size(px(10.0))
                    .line_height(px(15.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("skills.compileNote")),
            )
            .child(
                Button::new("skills-cancel")
                    .variant(ButtonVariant::Ghost)
                    .disabled(self.busy)
                    .on_click(cx.listener(|skills, _, _, cx| skills.back_to_library(cx)))
                    .child(cx.t("skills.cancel")),
            )
            .child(
                Button::new("skills-save")
                    .disabled(self.busy)
                    .on_click(cx.listener(|skills, _, _, cx| skills.save(cx)))
                    .children(self.busy.then(|| spinner("skills-saving", 14.0)))
                    .child(cx.t(action)),
            )
            .into_any_element()
    }
}

/// Empty cells that keep a last, shorter row of three on the grid.
fn row_len_padding(rows: &[gpui::Div], total: usize) -> std::ops::Range<usize> {
    let done = rows.len() * 3;
    let left = total.saturating_sub(done);
    if left > 0 && left < 3 {
        0..(3 - left)
    } else {
        0..0
    }
}
