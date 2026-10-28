//! The notes library (`notes-library`, 920px): the eyebrow, title and
//! description; the search and "Nova anotação"; Abertas/Concluídas with
//! their counts and the project and conversation filters; reading, the
//! error, the empty states, or the notes as cards (`card.rs`); the foot
//! ("sobrevivem à limpeza", clearing the completed ones); and "Excluir …?".

use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px, relative};

use super::{NotesWorkspace, StatusFilter, conversations, filtered};
use crate::data::notes::ProjectNote;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Dialog, Icon, Menu, MenuEntry, MenuItem, Tracked, spinner,
};

/// `minmax(230px, 1fr)` with a 15px gap.
const CARD_MIN: f32 = 230.0;
const GAP: f32 = 15.0;

impl NotesWorkspace {
    /// A `<select>` as a menu: the current label, then the choices.
    fn select(
        &self,
        id: &'static str,
        current: String,
        choices: Vec<(Option<String>, String)>,
        pick: fn(&mut Self, Option<String>, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let this = cx.entity().downgrade();
        Menu::dropdown(
            id,
            move |_| {
                div()
                    .debug_selector(move || id.to_string())
                    .flex()
                    .max_w(px(190.0))
                    .min_w_0()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.popover)
                    .px(px(8.0))
                    .py(px(6.0))
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .cursor_pointer()
                    .child(div().min_w_0().truncate().child(current.clone()))
                    .child(Icon::new("chevron-down").size(12.0))
                    .into_any_element()
            },
            move |_, _| {
                choices
                    .iter()
                    .map(|(value, label)| {
                        let (this, value) = (this.clone(), value.clone());
                        MenuEntry::Item(MenuItem::new(label.clone(), move |_, cx| {
                            let _ =
                                this.update(cx, |workspace, cx| pick(workspace, value.clone(), cx));
                        }))
                    })
                    .collect()
            },
        )
        .into_any_element()
    }

    pub(super) fn library(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let store = self.store.read(cx);
        let (loading, error) = (store.loading, store.error.is_some());
        let notes = store.notes.clone();
        let search = self.search.read(cx).text().to_string();
        let project = self.project_filter.clone();
        let shown: Vec<ProjectNote> = filtered(
            &notes,
            self.status,
            project.as_deref(),
            self.conversation_filter.as_deref(),
            &search,
        )
        .into_iter()
        .cloned()
        .collect();
        let count = |status: StatusFilter| {
            notes
                .iter()
                .filter(|note| {
                    note.status == status.key()
                        && project.as_deref().is_none_or(|id| note.project_id == id)
                })
                .count()
        };
        let tabs: Vec<AnyElement> = [StatusFilter::Open, StatusFilter::Completed]
            .into_iter()
            .map(|status| {
                let chosen = self.status == status;
                let fg = colors.foreground;
                div()
                    .id(SharedString::from(format!("notes-status-{}", status.key())))
                    .debug_selector(move || format!("notes-status-{}", status.key()))
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(px(7.0))
                    .text_size(px(11.0))
                    .cursor_pointer()
                    .map(|tab| {
                        if chosen {
                            tab.bg(colors.popover).text_color(fg).shadow_sm()
                        } else {
                            tab.text_color(colors.muted_foreground)
                        }
                    })
                    .on_click(
                        cx.listener(move |workspace, _, _, cx| workspace.set_status(status, cx)),
                    )
                    .child(match status {
                        StatusFilter::Open => Icon::new("circle").size(12.0),
                        StatusFilter::Completed => Icon::new("check-check").size(14.0),
                    })
                    .child(cx.t(&format!("notes.{}", status.key())))
                    .child(
                        div()
                            .text_size(px(9.0))
                            .opacity(0.5)
                            .child(count(status).to_string()),
                    )
                    .into_any_element()
            })
            .collect();
        let all_projects = self.all_projects(cx);
        let project_label = project
            .as_ref()
            .and_then(|id| all_projects.iter().find(|known| &known.id == id))
            .map(|known| known.name.clone())
            .unwrap_or_else(|| cx.t("notes.allProjects").to_string());
        let mut project_choices = vec![(None, cx.t("notes.allProjects").to_string())];
        project_choices.extend(
            all_projects
                .into_iter()
                .map(|known| (Some(known.id), known.name)),
        );
        let project_select = self.select(
            "notes-project-filter",
            project_label,
            project_choices,
            Self::set_project_filter,
            cx,
        );
        let mut conversation_choices = vec![(None, cx.t("notes.allConversations").to_string())];
        conversation_choices.extend(
            conversations(&notes, project.as_deref())
                .into_iter()
                .map(|title| (Some(title.clone()), title)),
        );
        let conversation_label = self
            .conversation_filter
            .clone()
            .unwrap_or_else(|| cx.t("notes.allConversations").to_string());
        let conversation_select = self.select(
            "notes-conversation-filter",
            conversation_label,
            conversation_choices,
            Self::set_conversation_filter,
            cx,
        );
        // `sm:max-w-[920px]` less the 30px padding; the cards fill the row.
        let viewport = window.viewport_size();
        let narrow = f32::from(viewport.width) < 640.0;
        let pad = if narrow { 20.0 } else { 30.0 };
        let inner =
            (920.0_f32.min(f32::from(viewport.width) - 32.0) - 2.0 * pad - 6.0).max(CARD_MIN);
        let columns = if narrow {
            1.0
        } else {
            ((inner + GAP) / (CARD_MIN + GAP)).floor().max(1.0)
        };
        let card_width = (inner - GAP * (columns - 1.0)) / columns;
        let body: AnyElement = if loading {
            div()
                .flex()
                .flex_col()
                .min_h(px(235.0))
                .items_center()
                .justify_center()
                .gap(px(12.0))
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(spinner("notes-loading", 20.0))
                .child(cx.t("notes.loading"))
                .into_any_element()
        } else if error {
            div()
                .debug_selector(|| "notes-error".into())
                .flex()
                .flex_col()
                .min_h(px(235.0))
                .items_center()
                .justify_center()
                .gap(px(12.0))
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("notes.loadError"))
                .child(
                    Button::new("notes-retry")
                        .size(ButtonSize::Default)
                        .on_click(cx.listener(|workspace, _, _, cx| {
                            workspace.store.update(cx, |store, cx| store.load(cx))
                        }))
                        .child(cx.t("notes.retry")),
                )
                .into_any_element()
        } else if shown.is_empty() {
            self.empty(!search.is_empty(), cx)
        } else {
            let cards: Vec<AnyElement> = shown
                .iter()
                .enumerate()
                .map(|(index, note)| self.card(note, index, card_width, cx))
                .collect();
            div()
                .debug_selector(|| "notes-grid".into())
                .flex()
                .flex_wrap()
                .items_start()
                .gap(px(GAP))
                .children(cards)
                .into_any_element()
        };
        let clear = (self.status == StatusFilter::Completed && !shown.is_empty()).then(|| {
            let ids: Vec<String> = shown.iter().map(|note| note.id.clone()).collect();
            let destructive = colors.destructive;
            div()
                .id("notes-clear-completed")
                .debug_selector(|| "notes-clear-completed".into())
                .flex()
                .items_center()
                .gap(px(5.0))
                .cursor_pointer()
                .hover(move |button| button.text_color(destructive))
                .on_click(
                    cx.listener(move |workspace, _, _, cx| workspace.ask_delete(ids.clone(), cx)),
                )
                .child(Icon::new("trash-2").size(13.0))
                .child(cx.t("notes.clearCompleted"))
        });
        let creating = self.creating || loading;
        let close = cx.entity().downgrade();
        Dialog::new("notes-library", move |_, cx| {
            let _ = close.update(cx, |workspace, cx| workspace.close_library(cx));
        })
        .max_width(920.0)
        .padding(pad)
        .gap(if narrow { 15.0 } else { 20.0 })
        .radius(26.0)
        .child(
            div()
                .debug_selector(|| "notes-library".into())
                .flex()
                .flex_col()
                .gap(px(8.0))
                .pr(px(28.0))
                .child(
                    div()
                        .mb(px(6.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_color(colors.muted_foreground)
                        .child(Icon::new("sticky-note").size(14.0))
                        .child(Tracked::new(
                            cx.t("notes.eyebrow").to_uppercase(),
                            10.0,
                            0.15,
                        )),
                )
                .child(
                    div()
                        .text_size(px(if narrow { 24.0 } else { 29.0 }))
                        .line_height(relative(1.2))
                        .font_weight(FontWeight(550.0))
                        .child(Tracked::new(
                            cx.t("notes.heading"),
                            if narrow { 24.0 } else { 29.0 },
                            -0.055,
                        )),
                )
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("notes.description")),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .debug_selector(|| "notes-search".into())
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .gap(px(9.0))
                        .rounded(px(12.0))
                        .border_1()
                        .border_color(colors.border)
                        .bg(colors.background)
                        .px(px(13.0))
                        .py(px(10.0))
                        .text_color(colors.muted_foreground)
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
                    Button::new("notes-new")
                        .size(ButtonSize::Default)
                        .disabled(creating)
                        .radius(12.0)
                        .on_click(
                            cx.listener(|workspace, _, _, cx| workspace.create(String::new(), cx)),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(Icon::new("plus").size(14.0))
                                .child(cx.t("notes.new")),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .mr_auto()
                        .flex()
                        .gap(px(2.0))
                        .p(px(3.0))
                        .rounded(px(10.0))
                        .bg(colors.muted)
                        .children(tabs),
                )
                .child(project_select)
                .child(conversation_select),
        )
        .child(
            div()
                .id("notes-scroll")
                .min_h(px(240.0))
                .max_h(px(f32::from(viewport.height) * 0.49))
                .overflow_y_scroll()
                .pt(px(3.0))
                .px(px(3.0))
                .pb(px(14.0))
                .child(body),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap(px(10.0))
                .border_t_1()
                .border_color(colors.border)
                .pt(px(14.0))
                .text_size(px(if narrow { 9.0 } else { 10.0 }))
                .text_color(colors.muted_foreground)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .child(Icon::new("check-check").size(13.0))
                        .child(cx.t("notes.survivesCleanup")),
                )
                .children(clear),
        )
        .into_any_element()
    }

    /// "Excluir esta anotação?"
    pub(super) fn delete_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.delete_ids.is_empty() {
            return None;
        }
        let close = cx.entity().downgrade();
        let busy = self.busy;
        Some(
            Dialog::new("notes-delete", move |_, cx| {
                let _ = close.update(cx, |workspace, cx| workspace.cancel_delete(cx));
            })
            .child(
                div()
                    .debug_selector(|| "notes-delete".into())
                    .child(crate::ui::dialog_header(
                        cx.t_with(
                            "notes.deleteTitle",
                            &Args::count(self.delete_ids.len() as i64),
                        ),
                        Some(cx.t("notes.deleteDescription")),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        Button::new("notes-delete-cancel")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Default)
                            .disabled(busy)
                            .on_click(
                                cx.listener(|workspace, _, _, cx| workspace.cancel_delete(cx)),
                            )
                            .child(cx.t("notes.cancel")),
                    )
                    .child(
                        Button::new("notes-delete-confirm")
                            .variant(ButtonVariant::Destructive)
                            .size(ButtonSize::Default)
                            .disabled(busy)
                            .on_click(
                                cx.listener(|workspace, _, _, cx| workspace.confirm_delete(cx)),
                            )
                            .child(cx.t("notes.delete")),
                    ),
            )
            .into_any_element(),
        )
    }
}
