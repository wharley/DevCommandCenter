//! "Atualização dos agentes": a card at the top right when an agent's CLI
//! has a newer release (`dcc_providers::cli_updates`), with "Atualizar" on
//! each (and "Atualizar tudo"), as MonoCode shows it. Checked shortly after
//! launch and twice a day; the X hides what it shows until a newer release.
//! "Abrir no terminal" runs the update where the person watches and answers
//! it (a sudo password), as "Conectar GitHub" does: for a CLI DCC cannot
//! tell how it was installed, and after an update that failed.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use dcc_providers::cli_updates::{self, CliUpdate};
use gpui::{
    Anchor, AnyElement, AppContext, BoxShadow, Context, Entity, FontWeight, Hsla, Subscription,
    Task, Window, anchored, deferred, div, point, prelude::*, px,
};

use crate::data::desktop_api::DesktopApi;
use crate::i18n::{Args, Translate};
use crate::project::ProviderIcon;
use crate::screens::terminal::command_dialog::{CommandDialog, CommandDialogEvent, Leading};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, spinner};

const FIRST_CHECK: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_secs(12 * 60 * 60);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Row {
    Updating,
    Updated(String),
    Failed(String),
}

#[derive(Default)]
pub struct CliUpdates {
    updates: Vec<CliUpdate>,
    rows: HashMap<String, Row>,
    /// (provider, release) the person closed.
    closed: HashSet<(String, String)>,
    _checks: Option<Task<()>>,
    runs: HashMap<String, Task<()>>,
    api: Option<DesktopApi>,
    /// The update running where the person watches it.
    terminal: Option<(Entity<CommandDialog>, Subscription)>,
}

/// Runs a provider future on a thread of its own (the CLIs need tokio).
fn blocking<T>(future: impl std::future::Future<Output = T>) -> Option<T> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    Some(runtime.block_on(future))
}

impl CliUpdates {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut this = Self::default();
        if !cfg!(test) {
            this.api = DesktopApi::load();
            this._checks = Some(cx.spawn(async move |this, cx| {
                let mut wait = FIRST_CHECK;
                loop {
                    cx.background_executor().timer(wait).await;
                    wait = EVERY;
                    let found = cx
                        .background_executor()
                        .spawn(async { blocking(cli_updates::check_all()).unwrap_or_default() })
                        .await;
                    if this.update(cx, |this, cx| this.found(found, cx)).is_err() {
                        return;
                    }
                }
            }));
        }
        this
    }

    /// A check's answer; a row being updated or just updated stays.
    fn found(&mut self, found: Vec<CliUpdate>, cx: &mut Context<Self>) {
        let keep: Vec<CliUpdate> = self
            .updates
            .drain(..)
            .filter(|update| self.rows.contains_key(&update.provider_id))
            .collect();
        let fresh: Vec<CliUpdate> = found
            .into_iter()
            .filter(|update| {
                !self
                    .closed
                    .contains(&(update.provider_id.clone(), update.latest.clone()))
                    && !keep
                        .iter()
                        .any(|kept| kept.provider_id == update.provider_id)
            })
            .collect();
        self.updates = keep.into_iter().chain(fresh).collect();
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        for update in self.updates.drain(..) {
            self.closed.insert((update.provider_id, update.latest));
        }
        self.rows.clear();
        cx.notify();
    }

    fn pending(&self) -> Vec<CliUpdate> {
        self.updates
            .iter()
            .filter(|update| {
                update.action.is_some()
                    && matches!(
                        self.rows.get(&update.provider_id),
                        None | Some(Row::Failed(_))
                    )
            })
            .cloned()
            .collect()
    }

    fn start(&mut self, update: CliUpdate, cx: &mut Context<Self>) {
        self.follow(update, true, cx);
    }

    /// Runs the update here (`run`), or checks one the terminal ran (`verify`).
    fn follow(&mut self, update: CliUpdate, run: bool, cx: &mut Context<Self>) {
        let id = update.provider_id.clone();
        let key = id.clone();
        self.rows.insert(id.clone(), Row::Updating);
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let done = if run {
                        blocking(cli_updates::run(&update))
                    } else {
                        blocking(cli_updates::verify(&update))
                    };
                    done.unwrap_or_else(|| Err(dcc_core::CoreError::Provider("no runtime".into())))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let row = match result {
                    Ok(version) => Row::Updated(version),
                    Err(error) => Row::Failed(error.to_string()),
                };
                this.rows.insert(id.clone(), row);
                this.runs.remove(&id);
                cx.notify();
            });
        });
        self.runs.insert(key, task);
        cx.notify();
    }

    /// The update in a terminal the person watches; when it ends well its
    /// version is checked, when it fails the terminal stays to be read.
    fn open_terminal(&mut self, update: CliUpdate, window: &mut Window, cx: &mut Context<Self>) {
        let title = cx.t_with(
            "cliUpdates.terminalTitle",
            &Args::default().with("agent", Self::label(&update.provider_id)),
        );
        let (owner, command, api) = (
            format!("cli-update:{}", update.provider_id),
            update.terminal_command.clone(),
            self.api.clone(),
        );
        let leading = Leading::Provider(update.provider_id.clone());
        let terminal = cx.new(|cx| {
            CommandDialog::new(
                "cli-update-terminal",
                owner,
                command,
                title,
                None,
                leading,
                api,
                window,
                cx,
            )
        });
        let events = cx.subscribe(&terminal, move |this, terminal, event, cx| match event {
            CommandDialogEvent::Exited(Some(0)) => {
                terminal.update(cx, |terminal, cx| terminal.close(cx));
                this.follow(update.clone(), false, cx);
            }
            CommandDialogEvent::Exited(_) => {
                let failed = cx.t("cliUpdates.terminalFailed").to_string();
                this.rows
                    .insert(update.provider_id.clone(), Row::Failed(failed));
                cx.notify();
            }
            CommandDialogEvent::Closed => {
                this.terminal = None;
                cx.notify();
            }
        });
        self.terminal = Some((terminal, events));
        cx.notify();
    }

    fn terminal_button(&self, update: &CliUpdate, cx: &mut Context<Self>) -> AnyElement {
        let target = update.clone();
        Button::new(("cli-update-terminal-open", hash(&update.provider_id)))
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::Xs)
            .text_size(11.0)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_terminal(target.clone(), window, cx)
            }))
            .child(cx.t("cliUpdates.openTerminal"))
            .into_any_element()
    }

    fn label(id: &str) -> String {
        dcc_providers::offline_catalog()
            .providers
            .into_iter()
            .find(|provider| provider.id.0 == id)
            .map_or_else(|| id.to_string(), |provider| provider.label)
    }

    fn row(&self, update: &CliUpdate, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let id = update.provider_id.clone();
        let state = self.rows.get(&id).cloned();
        let trailing: AnyElement = match (&state, &update.action) {
            (Some(Row::Updated(version)), _) => div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .text_size(px(11.0))
                .text_color(crate::theme::tailwind::EMERALD_500.hsla())
                .child(Icon::new("check").size(12.0))
                .child(cx.t_with(
                    "cliUpdates.updated",
                    &Args::default().with("version", cli_updates::short_version(version)),
                ))
                .into_any_element(),
            (Some(Row::Updating), _) => div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(spinner(("cli-update-spinner", hash(&id)), 12.0))
                .child(cx.t("cliUpdates.updating"))
                .into_any_element(),
            (_, None) => self.terminal_button(update, cx),
            (_, Some(_)) => {
                let target = update.clone();
                let label = if matches!(state, Some(Row::Failed(_))) {
                    cx.t("cliUpdates.retry")
                } else {
                    cx.t("cliUpdates.update")
                };
                Button::new(("cli-update", hash(&id)))
                    .variant(ButtonVariant::Secondary)
                    .size(ButtonSize::Xs)
                    .text_size(11.0)
                    .on_click(cx.listener(move |this, _, _, cx| this.start(target.clone(), cx)))
                    .child(label)
                    .into_any_element()
            }
        };
        let versions = (!matches!(state, Some(Row::Updated(_)))).then(|| {
            div()
                .truncate()
                .font_family("Menlo")
                .text_size(px(11.0))
                .line_height(px(16.0))
                .text_color(colors.muted_foreground)
                .child(format!(
                    "{} → {}",
                    cli_updates::short_version(&update.installed),
                    cli_updates::short_version(&update.latest)
                ))
        });
        div()
            .debug_selector(move || format!("cli-update-{id}"))
            .px(px(12.0))
            .py(px(10.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(ProviderIcon::new(&update.provider_id).size(16.0))
                    // The name over its versions: a long one never pushes the button out.
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.0))
                                    .line_height(px(18.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(Self::label(&update.provider_id)),
                            )
                            .children(versions),
                    )
                    .child(div().flex_none().child(trailing)),
            )
            .when_some(
                match &state {
                    Some(Row::Failed(error)) => Some(error.clone()),
                    _ => None,
                },
                |row, error| {
                    // Failed: the reason, and the same update where it can be watched.
                    row.child(
                        div()
                            .mt(px(4.0))
                            .flex()
                            .items_start()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(11.0))
                                    .text_color(colors.destructive)
                                    .child(error),
                            )
                            .when(update.action.is_some(), |line| {
                                line.child(self.terminal_button(update, cx))
                            }),
                    )
                },
            )
            .into_any_element()
    }
}

/// A stable small number for an element id.
fn hash(text: &str) -> usize {
    text.bytes().fold(0usize, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as usize)
    })
}

impl Render for CliUpdates {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.updates.is_empty() {
            return div().into_any_element();
        }
        let colors = cx.theme().colors.clone();
        let pending = self.pending();
        let busy = self.rows.values().any(|row| *row == Row::Updating);
        let any_updated = self.rows.values().any(|row| matches!(row, Row::Updated(_)));
        let rows: Vec<AnyElement> = self
            .updates
            .clone()
            .iter()
            .map(|update| self.row(update, cx))
            .collect();
        let title = cx.t_with("cliUpdates.title", &Args::count(self.updates.len() as i64));
        let hint = if any_updated {
            cx.t("cliUpdates.hintUpdated")
        } else {
            cx.t("cliUpdates.hint")
        };
        let viewport = window.viewport_size();
        let card = div()
            .id("cli-updates")
            .debug_selector(|| "cli-updates".into())
            .occlude()
            .w(px(340.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_color(colors.foreground)
            .shadow(vec![BoxShadow {
                color: Hsla::black().opacity(0.12),
                offset: point(px(0.0), px(6.0)),
                blur_radius: px(18.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .border_b_1()
                    .border_color(colors.border)
                    .px(px(12.0))
                    .py(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .when(pending.len() > 1, |header| {
                        header.child(
                            Button::new("cli-update-all")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Xs)
                                .text_size(11.0)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    for update in this.pending() {
                                        this.start(update, cx);
                                    }
                                }))
                                .child(cx.t("cliUpdates.updateAll")),
                        )
                    })
                    .child(
                        Button::new("cli-update-close")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::IconXs)
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| this.close(cx)))
                            .child(Icon::new("x").size(12.0)),
                    ),
            )
            .children(rows)
            .child(
                div()
                    .border_t_1()
                    .border_color(colors.border)
                    .px(px(12.0))
                    .py(px(8.0))
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(hint),
            );
        div()
            .child(
                deferred(
                    anchored()
                        .position_mode(gpui::AnchoredPositionMode::Window)
                        .position(point(viewport.width - px(16.0), px(52.0)))
                        .anchor(Anchor::TopRight)
                        .child(card),
                )
                .priority(crate::ui::layer::TOAST),
            )
            .children(self.terminal.as_ref().map(|(terminal, _)| terminal.clone()))
            .into_any_element()
    }
}

#[cfg(test)]
impl CliUpdates {
    pub(crate) fn show_for_tests(&mut self, found: Vec<CliUpdate>, cx: &mut Context<Self>) {
        self.found(found, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_providers::cli_updates::UpdateAction;
    use gpui::{TestAppContext, WindowAppearance, size};

    fn update(id: &str, latest: &str, program: Option<&str>) -> CliUpdate {
        CliUpdate {
            provider_id: id.into(),
            installed: "1.0.0".into(),
            latest: latest.into(),
            terminal_command: format!("{id} update"),
            action: program.map(|program| UpdateAction {
                program: program.into(),
                args: vec!["update".into()],
                path_prepend: None,
            }),
        }
    }

    #[gpui::test]
    fn the_card_offers_updates_and_closing_waits_for_a_newer_one(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(crate::theme::Theme::new(
                crate::theme::ThemePreference::Light,
                WindowAppearance::Light,
            ));
            crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        });
        let (card, cx) = cx.add_window_view(|_, cx| CliUpdates::new(cx));
        cx.simulate_resize(size(px(1200.0), px(800.0)));
        card.update(cx, |card, cx| {
            card.show_for_tests(
                vec![
                    update("codex", "2.0.0", Some("/nonexistent/dcc-cli-update")),
                    update("cursor", "2026.10.22", None),
                ],
                cx,
            )
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("cli-updates").is_some());
        assert!(cx.debug_bounds("cli-update-codex").is_some());
        assert!(cx.debug_bounds("cli-update-cursor").is_some());
        // Only the one DCC knows how to update can be started.
        let pending = card.read_with(cx, |card, _| card.pending().len());
        assert_eq!(pending, 1);
        // An update that cannot run says why and offers to retry.
        card.update(cx, |card, cx| {
            let codex = card.updates[0].clone();
            card.start(codex, cx);
        });
        cx.run_until_parked();
        let row = card.read_with(cx, |card, _| card.rows.get("codex").cloned());
        assert!(matches!(row, Some(Row::Failed(_))), "{row:?}");
        // The one DCC cannot update opens in a terminal the person watches;
        // a command that fails keeps it open and says so.
        let cursor = card.read_with(cx, |card, _| card.updates[1].clone());
        cx.update(|window, cx| card.update(cx, |card, cx| card.open_terminal(cursor, window, cx)));
        cx.run_until_parked();
        assert!(cx.debug_bounds("cli-update-terminal").is_some());
        let terminal = card.read_with(cx, |card, _| card.terminal.as_ref().unwrap().0.clone());
        terminal.update(cx, |_, cx| cx.emit(CommandDialogEvent::Exited(Some(1))));
        cx.run_until_parked();
        let row = card.read_with(cx, |card, _| card.rows.get("cursor").cloned());
        assert_eq!(
            row,
            Some(Row::Failed(
                "O comando terminou com erro; veja no terminal.".into()
            ))
        );
        assert!(cx.debug_bounds("cli-update-terminal").is_some());
        terminal.update(cx, |terminal, cx| terminal.close(cx));
        cx.run_until_parked();
        assert!(cx.debug_bounds("cli-update-terminal").is_none());
        assert!(card.read_with(cx, |card, _| card.terminal.is_none()));
        // Closed: the same releases stay hidden, a newer one shows again.
        card.update(cx, |card, cx| card.close(cx));
        cx.run_until_parked();
        assert!(cx.debug_bounds("cli-updates").is_none());
        card.update(cx, |card, cx| {
            card.show_for_tests(vec![update("codex", "2.0.0", None)], cx)
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("cli-updates").is_none());
        card.update(cx, |card, cx| {
            card.show_for_tests(vec![update("codex", "2.1.0", None)], cx)
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("cli-update-codex").is_some());
    }
}
