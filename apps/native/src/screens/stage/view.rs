//! The stage's strip (`StageChatTab` and the content's tab in a
//! `ScrollableTabStrip`) over the content.

use gpui::{
    AnyElement, Context, Focusable, FontWeight, IntoElement, Render, SharedString, Window, div,
    prelude::*, px,
};

use super::files::FileLoad;
use super::{CONTEXT, DiffLoad, Save, ShowChat, Stage};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::tab_strip::TabStrip;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, spinner};

/// `h-7 rounded-md px-2.5` with the active `bg-muted/60 shadow-sm`.
pub(super) fn tab_base(
    id: &'static str,
    active: bool,
    cx: &Context<Stage>,
) -> gpui::Stateful<gpui::Div> {
    tab_base_named(id.into(), active, cx).debug_selector(move || id.to_string())
}

fn tab_base_named(
    id: SharedString,
    active: bool,
    cx: &Context<Stage>,
) -> gpui::Stateful<gpui::Div> {
    let colors = cx.theme().colors.clone();
    let hover = colors.muted.opacity(0.3);
    let foreground = colors.foreground;
    div()
        .id(id)
        .flex()
        .h(px(28.0))
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .text_size(px(11.0))
        .map(|tab| {
            if active {
                tab.bg(colors.muted.opacity(0.6))
                    .text_color(colors.foreground)
                    .shadow_sm()
            } else {
                tab.text_color(colors.muted_foreground)
                    .hover(move |style| style.bg(hover).text_color(foreground))
            }
        })
}

fn chat_tab(stage: &Stage, cx: &mut Context<Stage>) -> AnyElement {
    let status: Option<AnyElement> = if stage.agent_working {
        Some(
            div()
                .debug_selector(|| "stage-chat-working".into())
                .text_color(tw::SKY_500.hsla())
                .child(spinner("stage-chat-spinner", 12.0))
                .into_any_element(),
        )
    } else if stage.chat_unread {
        Some(
            div()
                .debug_selector(|| "stage-chat-unread".into())
                .size(px(6.0))
                .flex_shrink_0()
                .rounded_full()
                .bg(tw::SKY_500.hsla())
                .into_any_element(),
        )
    } else {
        None
    };
    tab_base("stage-chat", stage.chat_active, cx)
        .px(px(10.0))
        .on_click(cx.listener(|stage, _, _, cx| stage.show_chat(cx)))
        .child(Icon::new("message-square").size(14.0).stroke(1.8))
        .child(div().child(cx.t("fileTabs.chat")))
        .children(status)
        .into_any_element()
}

fn diff_tab(stage: &Stage, cx: &mut Context<Stage>) -> Option<AnyElement> {
    let diff = stage.diff.as_ref()?;
    let colors = cx.theme().colors.clone();
    let foreground = colors.foreground;
    let name = SharedString::from(diff.request.name.clone());
    Some(
        tab_base("stage-diff-tab", !stage.chat_active, cx)
            .group("stage-diff-tab")
            .max_w(px(220.0))
            .px(px(8.0))
            .on_click(cx.listener(|stage, _, window, cx| stage.show_content(window, cx)))
            .child(Icon::new("git-compare-arrows").size(14.0).stroke(1.8))
            .child(div().min_w_0().truncate().child(name))
            .child(
                div()
                    .id("stage-diff-close")
                    .debug_selector(|| "stage-diff-close".into())
                    .flex_shrink_0()
                    .rounded(px(4.0))
                    .p(px(2.0))
                    .text_color(colors.muted_foreground)
                    .invisible()
                    .group_hover("stage-diff-tab", |style| style.visible())
                    .hover(move |style| style.text_color(foreground))
                    .on_click(cx.listener(|stage, _, _, cx| {
                        cx.stop_propagation();
                        stage.close(cx)
                    }))
                    .child(Icon::new("x").size(12.0).stroke(2.0)),
            )
            .into_any_element(),
    )
}

fn centered(text: SharedString, color: gpui::Hsla) -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(11.0))
        .text_color(color)
        .child(text)
        .into_any_element()
}

/// A file's tab (`Tab` in `FileTabsSurface`): the unsaved dot, the name
/// (italic while a preview), the X on hover (always when unsaved); a
/// double-click pins it.
fn file_tab(stage: &Stage, at: usize, cx: &mut Context<Stage>) -> Option<AnyElement> {
    let tabs = stage.files.as_ref()?;
    let file = tabs.files.get(at)?;
    let dirty = file.is_dirty(cx);
    let colors = cx.theme().colors.clone();
    let foreground = colors.foreground;
    let active = tabs.active == at && !stage.chat_active;
    let group = SharedString::from(format!("stage-file-tab-{at}"));
    let selector = format!("stage-file-tab-{at}");
    Some(
        tab_base_named(
            SharedString::from(format!("stage-file-tab-{at}")),
            active,
            cx,
        )
        .debug_selector(move || selector.clone())
        .group(group.clone())
        .max_w(px(170.0))
        .px(px(8.0))
        .on_click(
            cx.listener(move |stage, event: &gpui::ClickEvent, window, cx| {
                if event.click_count() == 2 {
                    stage.pin_file(at, cx);
                }
                stage.activate_file(at, window, cx);
            }),
        )
        .when(dirty, |tab| {
            tab.child(
                div()
                    .debug_selector(move || format!("stage-file-unsaved-{at}"))
                    .size(px(6.0))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(colors.primary),
            )
        })
        .child(
            div()
                .min_w_0()
                .truncate()
                .when(file.preview, |name| name.italic())
                .child(file.name.clone()),
        )
        .child(
            div()
                .id(("stage-file-close", at))
                .debug_selector(move || format!("stage-file-close-{at}"))
                .flex_shrink_0()
                .rounded(px(4.0))
                .p(px(2.0))
                .text_color(colors.muted_foreground)
                .when(!dirty, |close| {
                    close
                        .invisible()
                        .group_hover(group, |style| style.visible())
                })
                .hover(move |style| style.text_color(foreground))
                .on_click(cx.listener(move |stage, _, window, cx| {
                    cx.stop_propagation();
                    stage.request_close_file(at, window, cx)
                }))
                .child(Icon::new("x").size(12.0).stroke(2.0)),
        )
        .into_any_element(),
    )
}

/// "Salvar" (with something unsaved) and "Fechar editor".
fn file_actions(stage: &Stage, cx: &mut Context<Stage>) -> AnyElement {
    let active = stage.files.as_ref().and_then(|tabs| tabs.active());
    let saving = active.is_some_and(|file| file.saving);
    let dirty = active.is_some_and(|file| file.is_dirty(cx));
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(4.0))
        .pr(px(8.0))
        .child(
            div().debug_selector(|| "stage-file-save".into()).child(
                Button::new("stage-file-save")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .muted()
                    .disabled(!dirty || saving)
                    .on_click(cx.listener(|stage, _, _, cx| stage.save(cx)))
                    .child(if saving {
                        spinner("stage-file-saving", 14.0).into_any_element()
                    } else {
                        Icon::new("save").size(14.0).stroke(1.8).into_any_element()
                    })
                    .child(cx.t("fileSurface.save")),
            ),
        )
        .child(
            div().debug_selector(|| "stage-files-close".into()).child(
                Button::new("stage-files-close")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .muted()
                    .on_click(cx.listener(|stage, _, _, cx| stage.request_close_files(cx)))
                    .child(Icon::new("x").size(14.0).stroke(1.8)),
            ),
        )
        .into_any_element()
}

fn file_body(stage: &Stage, cx: &mut Context<Stage>) -> Option<AnyElement> {
    let colors = cx.theme().colors.clone();
    let file = stage.files.as_ref()?.active()?;
    Some(match &file.load {
        Some(FileLoad::Text(view)) => view.clone().into_any_element(),
        Some(FileLoad::Image(view)) => view.clone().into_any_element(),
        Some(FileLoad::Failed(error)) => centered(error.clone().into(), colors.destructive),
        Some(FileLoad::Loading(_)) | None => {
            centered(cx.t("fileSurface.loading"), colors.muted_foreground)
        }
    })
}

impl Render for Stage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A file just read takes the focus (`reveal`).
        if self.focus_editor
            && let Some(Some(FileLoad::Text(view))) = self
                .files
                .as_ref()
                .and_then(|tabs| tabs.active())
                .map(|file| &file.load)
        {
            self.focus_editor = false;
            if !self.chat_active {
                window.focus(&view.focus_handle(cx), cx);
            }
        }
        self.follow_theme(cx);
        self.follow_files_theme(cx);
        let colors = cx.theme().colors.clone();
        // The tab to keep in view: the Chat, the diff, or the active file.
        let (active, key) = match (&self.files, self.chat_active) {
            (_, true) => (0, SharedString::from("\u{0}chat")),
            (Some(tabs), false) => (
                tabs.active + 1,
                tabs.active()
                    .map_or_else(SharedString::default, |file| file.path.clone().into()),
            ),
            (None, false) if self.session.is_some() => (1, SharedString::from("session")),
            (None, false) => (1, SharedString::from("diff")),
        };
        self.strip.reveal(active, key);
        let file_count = self.files.as_ref().map_or(0, |tabs| tabs.files.len());
        let tabs: Vec<AnyElement> = [
            Some(chat_tab(self, cx)),
            diff_tab(self, cx),
            super::session::session_tab(self, cx),
        ]
        .into_iter()
        .flatten()
        .chain((0..file_count).filter_map(|at| file_tab(self, at, cx)))
        .collect();
        let actions = self.files.is_some().then(|| file_actions(self, cx));
        let strip = div()
            .flex()
            .h(px(36.0))
            .flex_shrink_0()
            .items_center()
            .border_b_1()
            .border_color(colors.border)
            .child(TabStrip::new("stage-tabs", &mut self.strip).children(tabs))
            .children(actions);
        let drawer = (!self.chat_active)
            .then(|| super::review_view::thread_drawer(self, window, cx))
            .flatten();
        let body = self
            .diff
            .as_ref()
            .map(|diff| match &diff.load {
                DiffLoad::Ready(view) => view.clone().into_any_element(),
                DiffLoad::Failed(error) => centered(error.clone().into(), colors.destructive),
                DiffLoad::Loading(_) => centered("Loading file...".into(), colors.muted_foreground),
            })
            .or_else(|| file_body(self, cx))
            .or_else(|| super::session::session_body(self));
        div()
            .id("stage")
            .debug_selector(|| "stage".into())
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            // Esc closes the open conversation first.
            .on_action(cx.listener(|stage, _: &ShowChat, _, cx| {
                if stage.review().is_some_and(|review| review.active.is_some()) {
                    stage.close_thread(cx)
                } else {
                    stage.show_chat(cx)
                }
            }))
            // Behind the Chat tab the files keep quiet.
            .on_action(cx.listener(|stage, _: &Save, _, cx| {
                if !stage.chat_active {
                    stage.save(cx)
                }
            }))
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .bg(colors.background)
            .text_color(colors.foreground)
            .font_weight(FontWeight::NORMAL)
            .child(strip)
            .children(super::review_view::review_bar(self, cx))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .bg(colors.background)
                    .when(self.chat_active, |body| body.invisible())
                    .children(body)
                    .children(drawer),
            )
            .children(super::dialogs::dialogs(self, window, cx))
    }
}
