//! A command run in a terminal of the desktop's, drawn in a dialog: the
//! person sees what it prints and answers what it asks (a browser sign-in,
//! a sudo password). It says when the command ends and when the dialog is
//! closed (Esc, ✕). "Conectar GitHub/GitLab" and "Atualização dos agentes"
//! run their commands here.

use std::time::Duration;

use futures::StreamExt;
use gpui::{
    AnyElement, AppContext, Context, ElementId, Entity, EventEmitter, FontWeight, Hsla, Render,
    SharedString, Task, Window, div, prelude::*, px,
};

use super::{ShellContext, TerminalSession};
use crate::data::desktop_api::DesktopApi;
use crate::data::terminal_api::{ShellSpec, TerminalEvent};
use crate::project::ProviderIcon;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, Icon, shortcut_keys};

pub enum CommandDialogEvent {
    /// The command ended, with its exit code.
    Exited(Option<i64>),
    /// The person closed the dialog; the command was stopped.
    Closed,
}

/// What leads the title.
pub enum Leading {
    /// A colored dot (the foreground when `None`).
    Dot(Option<Hsla>),
    /// An agent's icon.
    Provider(String),
}

pub struct CommandDialog {
    id: SharedString,
    title: SharedString,
    detail: Option<SharedString>,
    leading: Leading,
    pub session: Entity<TerminalSession>,
    closed: bool,
    _stream: Option<Task<()>>,
}

impl EventEmitter<CommandDialogEvent> for CommandDialog {}

/// The person's login shell, running `command`.
fn spec(command: String) -> ShellSpec {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.trim().is_empty())
        .unwrap_or_else(|| "/bin/zsh".into());
    ShellSpec {
        cwd: "/".into(),
        command: shell,
        args: vec!["-lc".into(), command],
        cols: 96,
        rows: 24,
    }
}

impl CommandDialog {
    /// `owner` names the desktop's terminal for this command.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<SharedString>,
        owner: String,
        command: String,
        title: impl Into<SharedString>,
        detail: Option<SharedString>,
        leading: Leading,
        api: Option<DesktopApi>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let session_api = api.clone();
        let session = cx.new(|cx| {
            TerminalSession::new(
                owner,
                spec(command),
                ShellContext::default(),
                session_api,
                cx,
            )
            .without_banner()
        });
        window.focus(&session.read(cx).focus_handle(), cx);
        let mut dialog = Self {
            id: id.into(),
            title: title.into(),
            detail,
            leading,
            session,
            closed: false,
            _stream: None,
        };
        dialog.follow(api, cx);
        dialog
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// What the shell prints, from the desktop's stream (ours alone).
    fn follow(&mut self, api: Option<DesktopApi>, cx: &mut Context<Self>) {
        let Some(api) = api else {
            return;
        };
        let (sender, mut receiver) = futures::channel::mpsc::unbounded::<TerminalEvent>();
        std::thread::spawn(move || {
            let mut first = true;
            while !sender.is_closed() {
                if let Ok(events) = api.terminal_events() {
                    if !first && sender.unbounded_send(TerminalEvent::Lagged).is_err() {
                        return;
                    }
                    first = false;
                    for event in events {
                        if sender.unbounded_send(event).is_err() {
                            return;
                        }
                    }
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        });
        self._stream = Some(cx.spawn(async move |this, cx| {
            while let Some(event) = receiver.next().await {
                if this
                    .update(cx, |dialog, cx| dialog.dispatch(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    fn dispatch(&mut self, event: TerminalEvent, cx: &mut Context<Self>) {
        let ours = |pty: &str, cx: &Context<Self>| self.session.read(cx).pty_id() == Some(pty);
        match event {
            TerminalEvent::Output { pty_id, data } if ours(&pty_id, cx) => {
                self.session
                    .update(cx, |session, cx| session.output(&data, cx));
            }
            TerminalEvent::Exit { pty_id, code } if ours(&pty_id, cx) => {
                self.session
                    .update(cx, |session, cx| session.exited(code, cx));
                cx.emit(CommandDialogEvent::Exited(code));
            }
            TerminalEvent::Lagged => self.session.update(cx, |session, cx| session.resync(cx)),
            _ => {}
        }
    }

    /// Stops the command and goes.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.closed = true;
        self._stream = None;
        self.session.update(cx, |session, _| session.kill());
        cx.emit(CommandDialogEvent::Closed);
        cx.notify();
    }

    fn leading(&self, cx: &Context<Self>) -> AnyElement {
        match &self.leading {
            Leading::Dot(color) => div()
                .flex_none()
                .size(px(8.0))
                .rounded_full()
                .bg(color.unwrap_or(cx.theme().colors.foreground))
                .into_any_element(),
            Leading::Provider(id) => ProviderIcon::new(id).size(14.0).into_any_element(),
        }
    }
}

impl Render for CommandDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.closed {
            return div().into_any_element();
        }
        let colors = cx.theme().colors.clone();
        let close = cx.entity().downgrade();
        let focus = self.session.read(cx).focus_handle();
        let id = self.id.clone();
        Dialog::new(ElementId::Name(id.clone()), move |_, cx| {
            let _ = close.update(cx, |dialog, cx| dialog.close(cx));
        })
        .max_width(640.0)
        .padding(0.0)
        .gap(0.0)
        .close_button(false)
        .initial_focus(focus)
        .child(
            div()
                .debug_selector({
                    let id = id.clone();
                    move || id.to_string()
                })
                .w_full()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .h(px(40.0))
                        .px(px(12.0))
                        .border_b_1()
                        .border_color(colors.border.opacity(0.55))
                        .child(
                            div()
                                .flex()
                                .min_w_0()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(12.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors.foreground)
                                .child(self.leading(cx))
                                .child(self.title.clone())
                                .children(self.detail.clone().map(|detail| {
                                    div()
                                        .truncate()
                                        .text_color(colors.muted_foreground.opacity(0.8))
                                        .child(detail)
                                })),
                        )
                        .child(
                            div().ml_auto().child(
                                Button::new(ElementId::Name(format!("{id}-close").into()))
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::Sm)
                                    .padding_x(8.0)
                                    .muted()
                                    .on_click(cx.listener(|dialog, _, _, cx| dialog.close(cx)))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .child(shortcut_keys(&["Esc"], cx))
                                            .child(Icon::new("x").size(14.0).stroke(1.8)),
                                    ),
                            ),
                        ),
                )
                .child(
                    div()
                        .h(px(360.0))
                        .bg(colors.card)
                        .child(self.session.clone()),
                ),
        )
        .into_any_element()
    }
}
