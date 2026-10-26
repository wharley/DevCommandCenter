//! The indicator and the sources menu as the desktop draws them: the
//! `BrainCircuit` button (emerald with the count when the agent received
//! something, amber when the memory is unavailable), 384px with the title,
//! the hint, a search from four sources on, each source with its kind,
//! when, title, excerpt, the person's correction and its actions, and the
//! ignored ones behind "mostrar".

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use crate::data::ai_memory::{SourceHit, matches, partition};
use crate::i18n::{Args, Translate};
use crate::screens::conversation::ConversationView;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::popover::{Popover, PopoverAlign};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, spinner, tabular_nums};

impl ConversationView {
    /// The export still waiting (or being checked), as an icon.
    pub(in crate::screens::conversation::header) fn ai_memory_export(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let memory = &self.header.ai_memory;
        memory.session.as_ref()?;
        if !memory.export_reading && memory.export.is_none() {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let failed = memory
            .export
            .as_ref()
            .is_some_and(|export| export.last_error.is_some());
        // The desktop writes these in Portuguese, without a catalog key.
        let label: SharedString = if memory.export_reading {
            "Verificando sincronização com ai-memory".into()
        } else if let Some(export) = memory.export.as_ref().filter(|_| failed) {
            format!("ai-memory aguardando nova tentativa ({})", export.attempts).into()
        } else {
            "Sessão aguardando sincronização com ai-memory".into()
        };
        let icon: AnyElement = if memory.export_reading {
            spinner("ai-memory-export-reading", 14.0).into_any_element()
        } else {
            Icon::new(if failed { "cloud-off" } else { "cloud" })
                .size(14.0)
                .into_any_element()
        };
        Some(
            Tooltip::text(
                "ai-memory-export-tip",
                div()
                    .id("ai-memory-export")
                    .debug_selector(|| "ai-memory-export".into())
                    .flex()
                    .size(px(28.0))
                    .items_center()
                    .justify_center()
                    .text_color(if failed {
                        tw::AMBER_500.hsla()
                    } else {
                        colors.muted_foreground.opacity(0.7)
                    })
                    .child(icon),
                label,
            )
            .into_any_element(),
        )
    }

    fn ai_memory_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let memory = &self.header.ai_memory;
        let sources = memory.sources.clone().unwrap_or_default();
        let loading = memory.sources.is_none();
        let (visible, ignored) = partition(&sources, &memory.actions);
        let query = memory
            .search
            .as_ref()
            .map(|search| search.read(cx).text().to_string())
            .unwrap_or_default();
        let base: Vec<&SourceHit> = if memory.show_ignored {
            visible.iter().chain(ignored.iter()).copied().collect()
        } else {
            visible.clone()
        };
        let listed: Vec<SourceHit> = base
            .into_iter()
            .filter(|source| matches(source, &query))
            .cloned()
            .collect();
        let project = self
            .data()
            .and_then(|data| data.context.as_ref())
            .and_then(|context| context.project_label.clone())
            .unwrap_or_else(|| "—".into());
        let muted = colors.muted_foreground;
        let note = |text: SharedString| {
            div()
                .px(px(12.0))
                .py(px(24.0))
                .text_center()
                .text_size(px(11.0))
                .text_color(muted)
                .child(text)
        };
        let header = div()
            .border_b_1()
            .border_color(colors.border.opacity(0.5))
            .px(px(12.0))
            .pt(px(12.0))
            .pb(px(10.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(
                                div()
                                    .text_color(tw::EMERALD_500.hsla())
                                    .child(Icon::new("brain-circuit").size(14.0)),
                            )
                            .child(cx.t("workbench.aiMemory.sourcesTitle")),
                    )
                    .when(!visible.is_empty(), |row| {
                        row.child(
                            div()
                                .text_size(px(10.0))
                                .font_features(tabular_nums())
                                .text_color(muted)
                                .child(cx.t_with(
                                    "workbench.aiMemory.sourcesCount",
                                    &Args::count(visible.len() as i64),
                                )),
                        )
                    }),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(11.0))
                    .line_height(px(15.0))
                    .text_color(muted)
                    .child(cx.t_with(
                        "workbench.aiMemory.sourcesHint",
                        &Args::default().with("project", project),
                    )),
            )
            .when_some(
                memory
                    .search
                    .clone()
                    .filter(|_| visible.len() + ignored.len() >= 4),
                |header, search| {
                    header.child(
                        div()
                            .mt(px(10.0))
                            .flex()
                            .h(px(28.0))
                            .items_center()
                            .gap(px(6.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(colors.input)
                            .px(px(8.0))
                            .text_size(px(11.0))
                            .child(
                                div()
                                    .text_color(muted)
                                    .child(Icon::new("search").size(14.0)),
                            )
                            .child(div().flex_1().min_w_0().child(search)),
                    )
                },
            );
        let rows: Vec<AnyElement> = if loading {
            Vec::new()
        } else {
            listed
                .iter()
                .enumerate()
                .map(|(index, source)| self.ai_memory_source(index, source, cx))
                .collect()
        };
        let list = div()
            .id("ai-memory-sources")
            .min_h_0()
            .flex_1()
            .overflow_y_scroll()
            .when(loading, |list| {
                list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(12.0))
                        .py(px(24.0))
                        .text_size(px(11.0))
                        .text_color(muted)
                        .child(spinner("ai-memory-loading", 14.0))
                        .child(cx.t("workbench.aiMemory.sourcesLoading")),
                )
            })
            .when(
                !loading && visible.is_empty() && !memory.show_ignored,
                |list| list.child(note(cx.t("workbench.aiMemory.sourcesEmpty"))),
            )
            .when(
                !loading && listed.is_empty() && !query.trim().is_empty(),
                |list| list.child(note(cx.t("workbench.aiMemory.sourcesNoMatches"))),
            )
            .children(rows);
        let footer = (!ignored.is_empty()).then(|| {
            let label = if memory.show_ignored {
                cx.t("workbench.aiMemory.hideIgnored")
            } else {
                cx.t_with(
                    "workbench.aiMemory.showIgnored",
                    &Args::count(ignored.len() as i64),
                )
            };
            div()
                .border_t_1()
                .border_color(colors.border.opacity(0.5))
                .px(px(12.0))
                .py(px(6.0))
                .child(
                    div().ml(px(-6.0)).child(
                        Button::new("ai-memory-show-ignored")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm)
                            .height(24.0)
                            .padding_x(6.0)
                            .text_size(11.0)
                            .muted()
                            .on_click(cx.listener(|view, _, _, cx| view.toggle_ignored(cx)))
                            .child(Icon::new("eye-off").size(12.0))
                            .child(label),
                    ),
                )
        });
        div()
            .flex()
            .flex_col()
            .max_h(px(576.0))
            .overflow_hidden()
            .child(header)
            .child(list)
            .children(footer)
            .into_any_element()
    }

    /// The `BrainCircuit` menu; hidden with nothing to show and the memory off.
    pub(in crate::screens::conversation::header) fn ai_memory_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let memory = &self.header.ai_memory;
        memory.session.as_ref()?;
        let loading = memory.sources.is_none();
        let sources = memory.sources.clone().unwrap_or_default();
        let (visible, ignored) = partition(&sources, &memory.actions);
        let (active, unavailable) = (memory.active(), memory.unavailable());
        if !loading && visible.is_empty() && ignored.is_empty() && !active && !unavailable {
            return None;
        }
        let label = if loading {
            cx.t("workbench.aiMemory.sourcesLoading")
        } else if active {
            cx.t("workbench.aiMemory.statusActive")
        } else if unavailable {
            cx.t("workbench.aiMemory.statusUnavailable")
        } else {
            cx.t("workbench.aiMemory.sourcesAria")
        };
        let count = visible.len();
        let mut button = Button::new("header-ai-memory")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::IconSm);
        button = if loading {
            button.muted()
        } else if unavailable {
            button.text_color(tw::AMBER_500.hsla())
        } else if active || count > 0 {
            button.text_color(tw::EMERALD_500.hsla())
        } else {
            button.muted()
        };
        let badge = (count > 0).then(|| {
            div()
                .absolute()
                .right(px(-4.0))
                .top(px(-4.0))
                .flex()
                .h(px(16.0))
                .min_w(px(16.0))
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(cx.theme().colors.background)
                .bg(tw::EMERALD_500.hsla())
                .px(px(4.0))
                .text_size(px(9.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(gpui::white())
                .child(count.to_string())
        });
        let trigger = Tooltip::text(
            "header-ai-memory-tip",
            div()
                .relative()
                .child(button.child(Icon::new("brain-circuit").size(14.0)))
                .children(badge),
            label,
        );
        let list = self.ai_memory_list(cx);
        let view = cx.entity().downgrade();
        Some(
            Popover::new(
                "header-ai-memory-menu",
                memory.open,
                trigger,
                move |_, _| list,
                move |open, window, cx| {
                    let _ = view.update(cx, |view, cx| view.set_ai_memory_open(open, window, cx));
                },
            )
            .align(PopoverAlign::End)
            .width(384.0)
            .padding(0.0)
            .into_any_element(),
        )
    }
}
