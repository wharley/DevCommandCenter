//! `IdeaPanel`: above the composer of an idea's task (its folder in
//! `~/dcc-ideias`), how far the research got — read by DCC from `IDEIA.md`
//! after every turn — and the card that publishes the idea (a GitHub
//! repository, the folder moved, a project the app then opens a task in) or
//! discards it (the Researcher's page opens). The card shows itself once,
//! when the idea first becomes ready. Nothing in any other task.

mod card;

use std::path::PathBuf;

use gpui::{
    AnyElement, AppContext, Context, Entity, FontWeight, Task, Window, div, prelude::*, px,
};

use super::ConversationView;
use crate::data::desktop_api::DesktopApi;
use crate::data::idea::{self, IdeaStatus, MILESTONES, Publish, Visibility};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, TextInput, Toaster};

pub enum IdeaEvent {
    /// The idea is a project now: a task opens in it.
    Published {
        project_id: String,
        root_path: String,
    },
    /// Nothing of it is left: back to the Researcher.
    Discarded,
}

impl gpui::EventEmitter<IdeaEvent> for IdeaPanel {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    Publish,
    Discard,
}

pub struct IdeaPanel {
    root: String,
    project_id: String,
    api: Option<DesktopApi>,
    status: Option<IdeaStatus>,
    /// A turn runs: the file may still change, and nothing leaves.
    busy: bool,
    /// What the file was last read for (the conversation's rows).
    read_for: Option<usize>,
    card_open: bool,
    auto_opened: bool,
    name: Option<Entity<TextInput>>,
    name_touched: bool,
    destination: Option<String>,
    visibility: Visibility,
    keep_local: bool,
    pending: Option<Pending>,
    confirm_discard: bool,
    _read: Option<Task<()>>,
    _action: Option<Task<()>>,
}

impl ConversationView {
    /// The panel of the task on screen, when it is an idea.
    pub(super) fn sync_idea(&mut self, cx: &mut Context<Self>) {
        let Some(data) = self.data.clone() else {
            self.idea = None;
            return;
        };
        let root = data
            .workspace_path
            .clone()
            .filter(|root| idea::is_idea_root(root));
        let Some(root) = root else {
            self.idea = None;
            return;
        };
        let project = data.project_id.clone().unwrap_or_default();
        if self
            .idea
            .as_ref()
            .is_none_or(|(panel, _)| panel.read(cx).root != root)
        {
            let api = if cfg!(test) { None } else { DesktopApi::load() };
            let panel = cx.new(|_| IdeaPanel::new(root, project, api));
            let events = cx.subscribe(&panel, |_, _, event: &IdeaEvent, cx| match event {
                IdeaEvent::Published {
                    project_id,
                    root_path,
                } => cx.emit(super::ConversationEvent::IdeaPublished {
                    project_id: project_id.clone(),
                    root_path: root_path.clone(),
                }),
                IdeaEvent::Discarded => cx.emit(super::ConversationEvent::IdeaDiscarded),
            });
            self.idea = Some((panel, events));
        }
        let busy = data.sessions.iter().any(|session| session.running);
        let rows = data.rows.len();
        if let Some((panel, _)) = &self.idea {
            panel.update(cx, |panel, cx| panel.update_from(busy, rows, cx));
        }
    }
}

impl IdeaPanel {
    fn new(root: String, project_id: String, api: Option<DesktopApi>) -> Self {
        Self {
            root,
            project_id,
            api,
            status: None,
            busy: false,
            read_for: None,
            card_open: false,
            auto_opened: false,
            name: None,
            name_touched: false,
            destination: None,
            visibility: Visibility::Private,
            keep_local: false,
            pending: None,
            confirm_discard: false,
            _read: None,
            _action: None,
        }
    }

    /// Reads the file again when the conversation moved and no turn runs.
    fn update_from(&mut self, busy: bool, rows: usize, cx: &mut Context<Self>) {
        self.busy = busy;
        if busy || self.read_for == Some(rows) {
            cx.notify();
            return;
        }
        self.read_for = Some(rows);
        let path = PathBuf::from(&self.root).join(idea::IDEA_FILE);
        self._read = Some(cx.spawn(async move |this, cx| {
            let text = cx
                .background_executor()
                .spawn(async move { std::fs::read_to_string(path).ok() })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.status = text.as_deref().map(idea::parse);
                // The favourite changed and the person kept the suggestion.
                if let Some(input) = panel.name.clone().filter(|_| !panel.name_touched) {
                    let favorite = panel.favorite();
                    input.update(cx, |input, cx| input.set_text(favorite, cx));
                }
                if panel.status.as_ref().is_some_and(IdeaStatus::ready) && !panel.auto_opened {
                    panel.auto_opened = true;
                    panel.card_open = true;
                }
                cx.notify();
            });
        }));
    }

    fn toggle_card(&mut self, cx: &mut Context<Self>) {
        self.card_open = !self.card_open;
        cx.notify();
    }

    /// The favourite name as a repository name, or nothing yet.
    fn favorite(&self) -> String {
        self.status
            .as_ref()
            .and_then(|status| status.favorite_name.clone())
            .map(|name| idea::repository_name(&name))
            .unwrap_or_default()
    }

    /// The name field, made the first time the card shows.
    fn ensure_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.name.is_some() {
            return;
        }
        let favorite = self.favorite();
        let input = cx.new(|cx| {
            let mut input = TextInput::new(window, cx);
            input.set_text(favorite, cx);
            input
        });
        cx.subscribe(&input, |panel, _, event, cx| {
            if matches!(event, crate::ui::TextInputEvent::Changed) {
                panel.name_touched = true;
                cx.notify();
            }
        })
        .detach();
        self.name = Some(input);
    }

    fn name(&self, cx: &gpui::App) -> String {
        self.name
            .as_ref()
            .map(|input| input.read(cx).text().trim().to_string())
            .unwrap_or_default()
    }

    /// `parentFolder(parentFolder(rootPath))`: the home folder, next to `~/dcc-ideias`.
    fn folder(&self) -> String {
        self.destination.clone().unwrap_or_else(|| {
            let root = PathBuf::from(&self.root);
            root.parent()
                .and_then(|ideas| ideas.parent())
                .map(|home| home.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
    }

    fn publish(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        let name = self.name(cx);
        let publish = Publish {
            root_path: self.root.clone(),
            name: name.clone(),
            destination: self.folder(),
            visibility: self.visibility,
            keep_idea_local: self.keep_local,
        };
        self.pending = Some(Pending::Publish);
        cx.notify();
        let project = self.project_id.clone();
        self._action = Some(cx.spawn(async move |this, cx| {
            let published = cx
                .background_executor()
                .spawn(async move { api.publish_idea(&publish) })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.pending = None;
                match published {
                    Ok(published) => {
                        let url = published
                            .repository_url
                            .clone()
                            .unwrap_or_else(|| published.root_path.clone());
                        Toaster::success(
                            cx.t_with(
                                "agents.research.published",
                                &Args::default().with("name", name).with("url", url),
                            ),
                            cx,
                        );
                        cx.emit(IdeaEvent::Published {
                            project_id: project,
                            root_path: published.root_path,
                        });
                    }
                    Err(error) => Toaster::error(error.text(cx), cx),
                }
                cx.notify();
            });
        }));
    }

    fn discard(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        let root = self.root.clone();
        self.pending = Some(Pending::Discard);
        cx.notify();
        self._action = Some(cx.spawn(async move |this, cx| {
            let discarded = cx
                .background_executor()
                .spawn(async move { api.discard_idea(&root) })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.pending = None;
                match discarded {
                    Ok(()) => {
                        panel.confirm_discard = false;
                        cx.emit(IdeaEvent::Discarded);
                    }
                    Err(error) => Toaster::error(error.text(cx), cx),
                }
                cx.notify();
            });
        }));
    }

    fn progress(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let filled = self.status.as_ref().map(|status| status.milestones);
        let count = self.status.as_ref().map_or(0, IdeaStatus::filled_count);
        let ready = self.status.as_ref().is_some_and(IdeaStatus::ready);
        let milestones = MILESTONES.iter().enumerate().map(|(index, key)| {
            let done = filled.is_some_and(|filled| filled[index]);
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .text_size(px(11.0))
                .text_color(if done {
                    tw::EMERALD_600.hsla()
                } else {
                    colors.muted_foreground
                })
                .child(
                    Icon::new(if done { "check" } else { "circle" }).size(if done {
                        12.0
                    } else {
                        10.0
                    }),
                )
                .child(cx.t(&format!("agents.research.milestones.{key}")))
        });
        div()
            .debug_selector(|| "idea-progress".into())
            .flex()
            .items_center()
            .gap(px(12.0))
            .rounded(px(14.0))
            .border_1()
            .border_color(colors.border.opacity(0.7))
            .bg(colors.card)
            .px(px(12.0))
            .py(px(8.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .gap(px(8.0))
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(cx.t("agents.research.progress")),
                            )
                            .child(
                                cx.t_with(
                                    "agents.research.progressCount",
                                    &Args::default()
                                        .with_count(count as i64)
                                        .with("total", MILESTONES.len().to_string()),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .flex()
                            .flex_wrap()
                            .gap_x(px(12.0))
                            .gap_y(px(2.0))
                            .children(milestones),
                    ),
            )
            .child(
                Button::new("idea-card-toggle")
                    .variant(if ready {
                        ButtonVariant::Default
                    } else {
                        ButtonVariant::Outline
                    })
                    .size(ButtonSize::Sm)
                    .on_click(cx.listener(|panel, _, _, cx| panel.toggle_card(cx)))
                    .child(cx.t("agents.research.card")),
            )
            .into_any_element()
    }
}

impl Render for IdeaPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.card_open {
            self.ensure_name(window, cx);
        }
        div()
            .mb(px(8.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(self.progress(cx))
            .children(self.card_open.then(|| self.card(cx)))
            .children(self.discard_dialog(cx))
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
