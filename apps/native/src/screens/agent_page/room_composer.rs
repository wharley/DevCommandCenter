//! The team room's field, as a card (the task composer's surface, and
//! Tucano's): the rounded card on the sidebar color with a calm ring while
//! focused, the words on top and one bar under them — attach, `@` to call
//! someone, who would answer now, and the round send button. A pending
//! approval sits right on the card, as wide as it (`room_dock`).

use gpui::{
    AnyElement, BoxShadow, Context, ExternalPaths, Focusable, PathPromptOptions, Window, div,
    point, prelude::*, px,
};

use super::room::TeamRoom;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::color::mix_oklab;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, pulse_spinner};

/// The card's widest, as the task composer's.
const MAX_WIDTH: f32 = 832.0;
/// The card's corner; the approvals' dock on top of it takes the same.
pub(super) const CORNER: f32 = 18.0;

impl TeamRoom {
    /// "Anexar": the system's file picker, several at once.
    fn open_picker(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(cx.t("composer.attachments.pickerTitle")),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let _ = this.update(cx, |room, cx| room.add_paths(&paths, cx));
        })
        .detach();
    }

    /// The `@` button: starts a mention where the person writes, which
    /// opens the team's list as a typed `@` does.
    fn start_mention(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.input.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
        self.input.update(cx, |input, cx| {
            let before = &input.text()[..input.cursor().min(input.text().len())];
            let glued = before
                .chars()
                .last()
                .is_some_and(|last| !last.is_whitespace());
            input.insert(if glued { " @" } else { "@" }, cx);
        });
    }

    /// Who would answer what is written, else how the room picks.
    fn next_hint(&self, text: &str, cx: &gpui::App) -> gpui::SharedString {
        (!text.is_empty())
            .then(|| self.pick(text, cx))
            .flatten()
            .map(|(agent, reason)| {
                cx.t_with(
                    "agents.room.next",
                    &Args::default()
                        .with("agent", agent.name.clone())
                        .with("reason", cx.t(reason.i18n_key()).to_string()),
                )
            })
            .unwrap_or_else(|| cx.t("agents.room.hint"))
    }

    fn bar_button(
        id: &'static str,
        icon: &'static str,
        tip: gpui::SharedString,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        div()
            .debug_selector(move || id.to_string())
            .child(Tooltip::text(
                gpui::SharedString::from(format!("{id}-tip")),
                Button::new(id)
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Icon)
                    .on_click(cx.listener(move |room, _, window, cx| on_click(room, window, cx)))
                    .child(Icon::new(icon).size(15.0).stroke(1.9)),
                tip,
            ))
            .into_any_element()
    }

    pub(super) fn composer(
        &self,
        dock: Option<AnyElement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let drop_border = mix_oklab(colors.accent_calm, 0.7, colors.border);
        let text = self.input.read(cx).text().trim().to_string();
        let blank = text.is_empty();
        let docked = dock.is_some();
        let handle = self.input.read(cx).focus_handle(cx);
        let focused = handle.contains_focused(window, cx);
        let border = if focused {
            mix_oklab(colors.accent_calm, 0.45, colors.border)
        } else {
            colors.border.opacity(0.8)
        };
        let shadow = |alpha: f32, y: f32, blur: f32| BoxShadow {
            color: gpui::black().opacity(alpha),
            offset: point(px(0.0), px(y)),
            blur_radius: px(blur),
            spread_radius: px(0.0),
            inset: false,
        };
        let mut shadows = vec![shadow(0.025, 2.0, 3.0), shadow(0.04, 8.0, 24.0)];
        if focused {
            shadows.push(BoxShadow {
                color: colors.accent_calm.opacity(0.07),
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: px(3.0),
                inset: false,
            });
        }
        let send = Tooltip::text(
            "room-send-tip",
            Button::new("room-send")
                .size(ButtonSize::Icon)
                .disabled(blank || self.sending)
                .on_click(cx.listener(|room, _, _, cx| room.send(cx)))
                .child(if self.sending {
                    pulse_spinner("room-sending", 15.0).into_any_element()
                } else {
                    Icon::new("arrow-up")
                        .size(15.0)
                        .stroke(2.2)
                        .into_any_element()
                }),
            cx.t("agents.room.sendTip"),
        );
        let bar = div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .pl(px(8.0))
            .pr(px(10.0))
            .pb(px(10.0))
            .pt(px(4.0))
            .child(Self::bar_button(
                "room-attach",
                "paperclip",
                cx.t("agents.room.attach"),
                cx,
                |room, _, cx| room.open_picker(cx),
            ))
            .child(Self::bar_button(
                "room-at",
                "at-sign",
                cx.t("agents.room.call"),
                cx,
                |room, window, cx| room.start_mention(window, cx),
            ))
            .child(
                div()
                    .debug_selector(|| "room-next".into())
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .pl(px(6.0))
                    .text_size(px(11.5))
                    .text_color(colors.muted_foreground)
                    .child(self.next_hint(&text, cx)),
            )
            .child(div().flex_none().child(send));
        let card = div()
            .id("room-input")
            .debug_selector(|| "room-input".into())
            .relative()
            .on_drop(
                cx.listener(|room, paths: &ExternalPaths, _, cx| room.add_paths(paths.paths(), cx)),
            )
            .drag_over::<ExternalPaths>(move |style, _, _, _| style.border_color(drop_border))
            .flex()
            .flex_col()
            .rounded(px(CORNER))
            .when(docked, |card| card.rounded_t(px(0.0)))
            .border_1()
            .border_color(border)
            .bg(colors.sidebar)
            .shadow(shadows)
            .child(
                div()
                    .id("room-field")
                    .min_h(px(44.0))
                    .px(px(16.0))
                    .pt(px(13.0))
                    .pb(px(4.0))
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(colors.foreground)
                    .cursor_text()
                    .on_click(move |_, window, cx| window.focus(&handle, cx))
                    .child(self.input.clone()),
            )
            .child(bar)
            .children(self.mention_popup(cx));
        div()
            .flex_none()
            .px(px(24.0))
            .pt(px(6.0))
            .pb(px(14.0))
            .child(
                div()
                    .mx_auto()
                    .w_full()
                    .max_w(px(MAX_WIDTH))
                    .flex()
                    .flex_col()
                    .children(dock)
                    .child(card),
            )
            .into_any_element()
    }
}
