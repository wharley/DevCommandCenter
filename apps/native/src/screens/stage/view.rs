//! The stage's strip (`StageChatTab` and the content's tab in a
//! `ScrollableTabStrip`) over the content.

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, Render, SharedString, Window, div, prelude::*, px,
};

use super::{CONTEXT, DiffLoad, ShowChat, Stage};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::tab_strip::TabStrip;
use crate::ui::{Icon, spinner};

/// `h-7 rounded-md px-2.5` with the active `bg-muted/60 shadow-sm`.
fn tab_base(id: &'static str, active: bool, cx: &Context<Stage>) -> gpui::Stateful<gpui::Div> {
    let colors = cx.theme().colors.clone();
    let hover = colors.muted.opacity(0.3);
    let foreground = colors.foreground;
    div()
        .id(id)
        .debug_selector(move || id.to_string())
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

impl Render for Stage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.follow_theme(cx);
        let colors = cx.theme().colors.clone();
        let active = if self.chat_active { 0 } else { 1 };
        self.strip
            .reveal(active, if self.chat_active { "chat" } else { "diff" });
        let tabs: Vec<AnyElement> = [Some(chat_tab(self, cx)), diff_tab(self, cx)]
            .into_iter()
            .flatten()
            .collect();
        let strip = div()
            .flex()
            .h(px(36.0))
            .flex_shrink_0()
            .items_center()
            .border_b_1()
            .border_color(colors.border)
            .child(TabStrip::new("stage-tabs", &mut self.strip).children(tabs));
        let body = self.diff.as_ref().map(|diff| match &diff.load {
            DiffLoad::Ready(view) => view.clone().into_any_element(),
            DiffLoad::Failed(error) => centered(error.clone().into(), colors.destructive),
            DiffLoad::Loading(_) => centered("Loading file...".into(), colors.muted_foreground),
        });
        div()
            .id("stage")
            .debug_selector(|| "stage".into())
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|stage, _: &ShowChat, _, cx| stage.show_chat(cx)))
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
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .bg(colors.background)
                    .when(self.chat_active, |body| body.invisible())
                    .children(body),
            )
    }
}
