//! One tab's shell (`terminal-store.ts`'s entry): opened in the desktop by
//! its owner key with what it printed so far, then fed from the event
//! stream; keys, the size and the end go to the desktop one at a time, in
//! order, from a thread of its own.

use std::sync::mpsc::{Receiver, Sender, channel};

use gpui::{Context, EventEmitter, FocusHandle, Task};

use super::emulator::Emulator;
use super::palette::TerminalPalette;
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::data::terminal_api::{OpenedTerminal, ShellSpec, TerminalActivity};

/// `TERMINAL_OUTPUT_TRUNCATION`'s neighbour: what a shell's end looks like.
fn exit_line(code: Option<i64>) -> String {
    let code = code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "?".into());
    format!("\r\n\x1b[2m[Terminal exited with code {code}]\x1b[0m\r\n")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionStatus {
    Starting,
    Running,
    Exited(Option<i64>),
    Failed,
}

/// What the dock's banner tells about a new shell (`TerminalContext`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShellContext {
    pub workspace_name: String,
    pub branch: String,
    pub provider_label: Option<String>,
    pub session_state: String,
}

fn banner(context: &ShellContext, spec: &ShellSpec) -> String {
    [
        "\x1b[2mDev Command Center terminal\x1b[0m".to_string(),
        String::new(),
        format!("workspace: {}", context.workspace_name),
        format!("branch: {}", context.branch),
        format!("cwd: {}", spec.cwd),
        format!(
            "provider: {}",
            context.provider_label.as_deref().unwrap_or("none")
        ),
        format!("session: {}", context.session_state),
        format!("shell: {}", spec.command),
        String::new(),
    ]
    .join("\r\n")
        + "\r\n"
}

enum Command {
    Write(String),
    Resize(u16, u16),
    /// Ends the PTY after what was queued; says so when asked.
    Kill(Option<Sender<()>>),
}

/// The calls for one PTY, in the order they were made.
fn commands(api: DesktopApi, pty_id: String) -> Sender<Command> {
    let (sender, receiver) = channel::<Command>();
    std::thread::spawn(move || {
        for command in receiver {
            let result = match command {
                Command::Write(data) => api.write_terminal(&pty_id, &data),
                Command::Resize(cols, rows) => api.resize_terminal(&pty_id, cols, rows),
                Command::Kill(done) => {
                    let result = api.kill_terminal(&pty_id);
                    if let Some(done) = done {
                        let _ = done.send(());
                    }
                    result
                }
            };
            if let Err(error) = result {
                eprintln!("[dcc-native] terminal {pty_id}: {error}");
            }
        }
    });
    sender
}

/// How the shell before this one ends, if there was one.
enum Ending {
    None,
    Queued(Receiver<()>),
    Direct(String),
}

pub enum SessionEvent {
    /// The shell's PTY, once known (the dock routes the stream by it).
    Opened(String),
    /// What the dock's bar shows from it changed (a local server found).
    Changed,
    /// "Enviar seleção ao agente" on its surface.
    SendSelection,
}

pub struct TerminalSession {
    pub owner_key: String,
    spec: ShellSpec,
    context: ShellContext,
    pub emulator: Emulator,
    pub status: SessionStatus,
    pub activity: Option<TerminalActivity>,
    pub(super) pty_id: Option<String>,
    sender: Option<Sender<Command>>,
    /// What was typed for it before its PTY opened (a command run from the
    /// conversation), written once it does.
    queued: Vec<String>,
    api: Option<DesktopApi>,
    /// Output that came before the shell was known (`bootstrappingWrites`).
    early: Vec<String>,
    pub(super) focus: FocusHandle,
    /// The last cell size drawn, for programs asking it.
    pub(super) cell: (f32, f32),
    pub(super) palette: Option<TerminalPalette>,
    /// The text area's size and top-left, as painted last.
    pub(super) text_size: super::view::Measured,
    pub(super) text_origin: super::view::Origin,
    pub(super) caret: Option<(usize, usize)>,
    /// Text the IME is still composing.
    pub(super) marked: Option<String>,
    /// Wheel lines short of a whole one.
    pub(super) wheel: f32,
    pub(super) selecting: bool,
    pub(super) pressed_link: Option<super::select::Link>,
    pub(super) hovered_link: Option<super::select::Link>,
    /// The size the shell was asked to start at.
    opened_at: (u16, u16),
    /// Local servers seen in what it printed.
    pub dev_servers: super::dev_server::DevServers,
    /// The "Dev Command Center terminal" lines a new shell starts with.
    banner: bool,
    _open: Option<Task<()>>,
}

impl EventEmitter<SessionEvent> for TerminalSession {}

impl TerminalSession {
    pub fn new(
        owner_key: String,
        spec: ShellSpec,
        context: ShellContext,
        api: Option<DesktopApi>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut session = Self {
            emulator: Emulator::new(spec.cols, spec.rows),
            owner_key,
            spec,
            context,
            status: SessionStatus::Starting,
            activity: None,
            pty_id: None,
            sender: None,
            queued: Vec::new(),
            api,
            early: Vec::new(),
            focus: cx.focus_handle(),
            cell: (7.2, 15.6),
            palette: None,
            text_size: Default::default(),
            text_origin: Default::default(),
            caret: None,
            marked: None,
            wheel: 0.0,
            selecting: false,
            pressed_link: None,
            hovered_link: None,
            opened_at: (0, 0),
            dev_servers: Default::default(),
            banner: true,
            _open: None,
        };
        session.open(cx);
        session
    }

    /// A shell that only runs one command (a sign-in): no banner.
    pub fn without_banner(mut self) -> Self {
        self.banner = false;
        self
    }

    pub fn pty_id(&self) -> Option<&str> {
        self.pty_id.as_deref()
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    /// `ensureTerminal`: the owner's shell at the emulator's size, with what
    /// it printed so far.
    pub(super) fn open(&mut self, cx: &mut Context<Self>) {
        self.open_after(Ending::None, cx);
    }

    /// Opens the owner's shell, once `ended` (the one restarted) is gone.
    fn open_after(&mut self, ended: Ending, cx: &mut Context<Self>) {
        let (cols, rows) = self.emulator.size();
        self.opened_at = (cols, rows);
        let spec = ShellSpec {
            cols,
            rows,
            ..self.spec.clone()
        };
        let owner_key = self.owner_key.clone();
        let api = self.api.clone();
        self.status = SessionStatus::Starting;
        cx.notify();
        self._open = Some(cx.spawn(async move |this, cx| {
            let opened = cx
                .background_executor()
                .spawn(async move {
                    let api = api.ok_or(ApiError::DesktopClosed)?;
                    match ended {
                        Ending::None => {}
                        // After the keys already queued for it (⌃C first).
                        Ending::Queued(killed) => {
                            let _ = killed.recv();
                        }
                        Ending::Direct(pty_id) => {
                            api.kill_terminal(&pty_id)?;
                        }
                    }
                    api.open_terminal(&owner_key, &spec)
                        .map(|opened| (api, opened))
                })
                .await;
            let _ = this.update(cx, |session, cx| match opened {
                Ok((api, opened)) => session.opened(api, opened, cx),
                Err(error) => session.failed(error, cx),
            });
        }));
    }

    fn opened(&mut self, api: DesktopApi, opened: OpenedTerminal, cx: &mut Context<Self>) {
        let (cols, rows) = self.emulator.size();
        // The screen starts over from what the shell printed.
        self.emulator = Emulator::new(cols, rows);
        self.dev_servers = Default::default();
        let mut found = false;
        let sender = commands(api, opened.pty_id.clone());
        if self.banner && opened.chunks.is_empty() && !opened.existing {
            self.emulator
                .feed(banner(&self.context, &self.spec).as_bytes());
        }
        for chunk in opened.chunks.iter().chain(&self.early) {
            self.emulator.feed(chunk.as_bytes());
            found |= self.dev_servers.feed(chunk);
        }
        self.early.clear();
        // `applyPendingResize`: the size it was drawn at, once it has a PTY.
        if opened.existing || (cols, rows) != self.opened_at {
            let _ = sender.send(Command::Resize(cols, rows));
        }
        self.status = if opened.exited {
            SessionStatus::Exited(opened.exit_code)
        } else {
            SessionStatus::Running
        };
        self.pty_id = Some(opened.pty_id.clone());
        for data in self.queued.drain(..) {
            let _ = sender.send(Command::Write(data));
        }
        self.sender = Some(sender);
        self.answer();
        cx.emit(SessionEvent::Opened(opened.pty_id));
        if found {
            cx.emit(SessionEvent::Changed);
        }
        cx.notify();
    }

    fn failed(&mut self, error: ApiError, cx: &mut Context<Self>) {
        self.status = SessionStatus::Failed;
        self.emulator
            .feed(format!("\r\n\x1b[31mFailed to open terminal: {error}\x1b[0m\r\n").as_bytes());
        cx.notify();
    }

    /// What the shell printed; kept for later while it is still opening.
    pub fn output(&mut self, data: &str, cx: &mut Context<Self>) {
        if self.pty_id.is_none() {
            self.early.push(data.to_string());
            return;
        }
        self.emulator.feed(data.as_bytes());
        if self.dev_servers.feed(data) {
            cx.emit(SessionEvent::Changed);
        }
        self.answer();
        cx.notify();
    }

    pub fn exited(&mut self, code: Option<i64>, cx: &mut Context<Self>) {
        self.pty_id = None;
        self.sender = None;
        self.status = SessionStatus::Exited(code);
        self.activity = None;
        self.emulator.feed(exit_line(code).as_bytes());
        cx.notify();
    }

    /// The stream fell behind: the screen is read again from the desktop.
    pub fn resync(&mut self, cx: &mut Context<Self>) {
        if self.status == SessionStatus::Running {
            self.pty_id = None;
            self.open(cx);
        }
    }

    /// The answers the program asked for.
    fn answer(&mut self) {
        let palette = self
            .palette
            .clone()
            .unwrap_or_else(|| TerminalPalette::new(true, &crate::theme::palette::Palette::dark()));
        for reply in self.emulator.take_replies(&palette, self.cell) {
            self.send(Command::Write(reply));
        }
    }

    fn send(&self, command: Command) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(command);
        }
    }

    /// A command line run as if typed and entered; kept until the PTY opens.
    pub fn run_line(&mut self, command: &str, cx: &mut Context<Self>) {
        let line = format!("{command}\r");
        if self.sender.is_none() {
            self.queued.push(line);
            return;
        }
        self.input(line, cx);
    }

    /// Keys and pastes; the view jumps back to the bottom like xterm.
    pub fn input(&mut self, data: String, cx: &mut Context<Self>) {
        if self.sender.is_none() || data.is_empty() {
            return;
        }
        if self.emulator.display_offset() > 0 {
            self.emulator.scroll_to_bottom();
            cx.notify();
        }
        self.send(Command::Write(data));
    }

    /// The size the view fits; the PTY follows.
    pub fn fit(&mut self, cols: u16, rows: u16, cx: &mut Context<Self>) {
        if self.emulator.resize(cols, rows) {
            self.send(Command::Resize(cols, rows));
            cx.notify();
        }
    }

    /// `clearTerminal`: the screen and scrollback, then `\f` for the prompt.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.emulator.clear();
        if !self.dev_servers.found.is_empty() {
            self.dev_servers = Default::default();
            cx.emit(SessionEvent::Changed);
        }
        self.send(Command::Write("\x0c".into()));
        cx.notify();
    }

    /// `interruptTerminal`: ⌃C to what runs.
    pub fn interrupt(&mut self) {
        self.send(Command::Write("\x03".into()));
    }

    /// `restartTerminal`: this shell ends and a new one opens for the tab.
    /// Returns the PTY that ends, for the dock to stop routing it.
    pub fn restart(&mut self, cx: &mut Context<Self>) -> Option<String> {
        let ended = self.pty_id.take();
        let ending = match (self.sender.take(), ended.clone()) {
            (Some(sender), Some(_)) => {
                let (done, killed) = channel();
                let _ = sender.send(Command::Kill(Some(done)));
                Ending::Queued(killed)
            }
            (_, Some(pty_id)) => Ending::Direct(pty_id),
            (_, None) => Ending::None,
        };
        let (cols, rows) = self.emulator.size();
        self.emulator = Emulator::new(cols, rows);
        self.activity = None;
        self.dev_servers = Default::default();
        self.early.clear();
        self.open_after(ending, cx);
        cx.emit(SessionEvent::Changed);
        ended
    }

    /// Whether a program runs in it (`running` or `waiting`).
    pub fn busy(&self) -> bool {
        use crate::data::terminal_api::ActivityStatus;
        self.activity.as_ref().is_some_and(|activity| {
            matches!(
                activity.status,
                ActivityStatus::Running | ActivityStatus::Waiting
            )
        })
    }

    /// `disposeTerminal`: the shell ends with its tab.
    pub fn kill(&mut self) {
        self.send(Command::Kill(None));
        self.sender = None;
        self.pty_id = None;
    }

    pub fn set_activity(&mut self, activity: Option<TerminalActivity>, cx: &mut Context<Self>) {
        if self.activity != activity {
            self.activity = activity;
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_banner_tells_where_the_shell_runs() {
        let spec = ShellSpec::login("/repo/wt".into(), Some("/bin/zsh".into()), 80, 24);
        let text = banner(
            &ShellContext {
                workspace_name: "feat".into(),
                branch: "feat/x".into(),
                provider_label: None,
                session_state: "idle".into(),
            },
            &spec,
        );
        assert!(text.contains("cwd: /repo/wt\r\n"));
        assert!(text.contains("provider: none\r\n"));
        assert!(text.ends_with("shell: /bin/zsh\r\n\r\n"));
        assert_eq!(
            exit_line(Some(0)),
            "\r\n\x1b[2m[Terminal exited with code 0]\x1b[0m\r\n"
        );
        assert!(exit_line(None).contains("code ?"));
    }
}
