//! "Acesso" in the agent's editor (`dcc_core::domain::agent_access`): each
//! group of DCC's tools and each connector the person installed at "Sem
//! acesso", "Ler" or "Ler e escrever" — read and write by default, as
//! before. DCC applies it where the tools are served, from the next call.

use dcc_core::domain::agent_access::{AccessLevel, DCC_TOOL_GROUPS};
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::editor::{AgentEditor, field};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::ToggleGroup;

const LEVELS: [AccessLevel; 3] = [AccessLevel::None, AccessLevel::Read, AccessLevel::Write];
/// Opening tasks has no read-only half: it does or it does not.
const TASK_LEVELS: [AccessLevel; 2] = [AccessLevel::None, AccessLevel::Write];

/// Where a level is set.
#[derive(Clone)]
enum Target {
    Group(&'static str),
    Connector(String),
}

impl AgentEditor {
    /// The person's connectors, by id and name, read when the editor opens.
    pub(super) fn load_connectors(&self, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            return;
        };
        cx.spawn(async move |editor, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.connectors() })
                .await;
            if let Ok(list) = read {
                let _ = editor.update(cx, |editor, cx| {
                    editor.connector_names = list
                        .iter()
                        .map(|item| {
                            (
                                item.definition.id.clone(),
                                item.definition.display_name.clone(),
                            )
                        })
                        .collect();
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn access_row(
        &self,
        id: String,
        label: SharedString,
        target: Target,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let current = match &target {
            Target::Group(group) => self.tool_access.group(group),
            Target::Connector(id) => self.tool_access.connector(id),
        };
        let tasks = matches!(target, Target::Group("tasks"));
        let levels: &'static [AccessLevel] = if tasks { &TASK_LEVELS } else { &LEVELS };
        let labels: Vec<SharedString> = levels
            .iter()
            .map(|level| match (level, tasks) {
                (AccessLevel::None, true) => cx.t("agents.access.tasksOff"),
                (_, true) => cx.t("agents.access.tasksOn"),
                (AccessLevel::None, false) => cx.t("agents.access.none"),
                (AccessLevel::Read, false) => cx.t("agents.access.read"),
                (AccessLevel::Write, false) => cx.t("agents.access.write"),
            })
            .collect();
        let picked = levels.iter().position(|level| *level == current);
        let editor = cx.entity().downgrade();
        let group = ToggleGroup::new(SharedString::from(id), labels, picked).on_select(
            move |index, _, cx| {
                let level = levels[index];
                let target = target.clone();
                let _ = editor.update(cx, |editor, cx| {
                    let map = match &target {
                        Target::Group(group) => (&mut editor.tool_access.groups, group.to_string()),
                        Target::Connector(id) => (&mut editor.tool_access.connectors, id.clone()),
                    };
                    if level == AccessLevel::Write {
                        map.0.remove(&map.1);
                    } else {
                        map.0.insert(map.1, level);
                    }
                    cx.notify();
                });
            },
        );
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.5))
                    .text_color(colors.foreground)
                    .child(label),
            )
            .child(div().flex_none().child(group))
            .into_any_element()
    }

    pub(super) fn access_field(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut rows: Vec<AnyElement> = DCC_TOOL_GROUPS
            .iter()
            .map(|group| {
                self.access_row(
                    format!("agent-editor-access-{group}"),
                    cx.t(&format!("agents.access.groups.{group}")),
                    Target::Group(group),
                    cx,
                )
            })
            .collect();
        // Under "Abrir tarefas": what its tasks may do without asking.
        rows.extend(self.task_mode_row(cx));
        let connectors = self.connector_names.clone();
        rows.extend(
            connectors
                .into_iter()
                .enumerate()
                .map(|(index, (id, name))| {
                    self.access_row(
                        format!("agent-editor-access-connector-{index}"),
                        name.into(),
                        Target::Connector(id),
                        cx,
                    )
                }),
        );
        field(
            cx.t("agents.access.title"),
            Some(cx.t("agents.access.hint")),
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .children(rows)
                .into_any_element(),
            cx,
        )
    }
}
