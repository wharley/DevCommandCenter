//! What the palette lists for a query, in the desktop's order: "Ações"
//! (the open task's workbench commands, then the global ones, then the
//! help topics while typing), the tasks, the sessions (this task's, then
//! the others), the checkout's files; separators between them.

use dcc_core::domain::session::SessionSearchResult;
use gpui::{App, SharedString};

use super::CommandPalette;
use crate::data::command_palette::{
    PaletteSession, filter_sessions, group_sessions, matches_action, rank_tasks, recent_sessions,
    session_search,
};
use crate::data::quick_open;
use crate::i18n::{I18n, Translate};
use crate::ui::command_key;

/// `WorkbenchCommand`, the ones the native app has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkbenchCommand {
    FocusComposer,
    Execution,
    TogglePlan,
    TerminalWorktree,
    TerminalProject,
    NewTerminal,
    Changes,
    Files,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PaletteCommand {
    Workbench(WorkbenchCommand),
    /// "Abrir projeto": `openWorkspaceDialog("open")`.
    OpenProject,
    /// "Clonar a partir de URL": `openWorkspaceDialog("clone")`.
    Clone,
    Shortcuts,
    /// The help, on a topic when one was picked.
    Help(Option<&'static str>),
    OpenTask(String),
    OpenSession(PaletteSession),
    OpenFile(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum ItemKind {
    Action {
        shortcut: Option<SharedString>,
    },
    Task {
        branch: String,
        selected: bool,
    },
    Session {
        workspace: String,
        /// Only for an `@` search.
        snippet: Option<String>,
        archived: bool,
    },
    File {
        dir: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Item {
    pub label: SharedString,
    pub kind: ItemKind,
    pub command: PaletteCommand,
}

/// One line of the list, flat so ↑/↓ can scroll any of them into view.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Row {
    Empty,
    Heading(SharedString),
    /// An item, by its index in the items.
    Item(usize),
    /// The group's `p-1` under its last item.
    GroupEnd,
    Separator,
    /// "Buscando conversas…" while an `@` search runs.
    LoadingSessions,
}

/// `getFocusComposerShortcutKeys().join("+")` and the like.
fn keys(keys: &[&str]) -> SharedString {
    keys.join("+").into()
}

struct Action {
    label: SharedString,
    keywords: SharedString,
    shortcut: Option<SharedString>,
    command: PaletteCommand,
}

fn actions(selected: bool, cx: &App) -> Vec<Action> {
    let command = command_key();
    let action = |label: &str, keywords: &str, shortcut: Option<SharedString>, run| Action {
        label: cx.t(label),
        keywords: cx.t(keywords),
        shortcut,
        command: run,
    };
    let workbench = |label: &str, keywords: &str, shortcut, run| {
        action(
            &format!("commandPalette.workbench.{label}"),
            &format!("commandPalette.keywords.{keywords}"),
            shortcut,
            PaletteCommand::Workbench(run),
        )
    };
    let mut list = Vec::new();
    if selected {
        list.extend([
            workbench(
                "focusComposer",
                "focusComposer",
                Some(keys(&[command, "Shift", "L"])),
                WorkbenchCommand::FocusComposer,
            ),
            workbench("execution", "execution", None, WorkbenchCommand::Execution),
            workbench("togglePlan", "plan", None, WorkbenchCommand::TogglePlan),
            workbench(
                "terminalWorktree",
                "worktreeTerminal",
                Some(keys(&[command, "J"])),
                WorkbenchCommand::TerminalWorktree,
            ),
            workbench(
                "terminalProject",
                "projectTerminal",
                None,
                WorkbenchCommand::TerminalProject,
            ),
            workbench(
                "newTerminal",
                "newTerminal",
                None,
                WorkbenchCommand::NewTerminal,
            ),
            workbench(
                "inspectorChanges",
                "inspectorChanges",
                Some(keys(&[command, "Shift", "G"])),
                WorkbenchCommand::Changes,
            ),
            workbench(
                "inspectorFiles",
                "inspectorFiles",
                Some(keys(&[command, "Shift", "E"])),
                WorkbenchCommand::Files,
            ),
        ]);
    }
    list.extend([
        // The desktop shows ⌘O here, though ⌘O opens the editor.
        action(
            "commandPalette.openProject",
            "commandPalette.keywords.openProject",
            Some(keys(&[command, "O"])),
            PaletteCommand::OpenProject,
        ),
        action(
            "commandPalette.cloneFromUrl",
            "commandPalette.keywords.clone",
            None,
            PaletteCommand::Clone,
        ),
        action(
            "commandPalette.keyboardShortcuts",
            "commandPalette.keywords.shortcuts",
            None,
            PaletteCommand::Shortcuts,
        ),
        action(
            "commandPalette.openHelp",
            "commandPalette.keywords.help",
            None,
            PaletteCommand::Help(None),
        ),
    ]);
    list
}

/// A conversation search hit, as the palette lists it.
pub(super) fn from_search(result: SessionSearchResult) -> PaletteSession {
    PaletteSession {
        session_id: result.session_id.0,
        task_id: result.workspace_id.0,
        title: result.thread_title,
        task_name: result.workspace_name,
        branch: result.workspace_branch,
        snippet: result.snippet,
        archived: result.archived_at.is_some(),
        updated_at: result.updated_at,
    }
}

pub(super) fn build(palette: &CommandPalette, cx: &App) -> (Vec<Row>, Vec<Item>) {
    let query = palette.query(cx);
    let deferred = query.trim();
    let search = session_search(&query);
    let context = &palette.context;
    let selected = context.selected_task.as_deref();
    let mut items: Vec<Item> = Vec::new();
    let mut rows: Vec<Row> = Vec::new();
    let mut group = |heading: SharedString, entries: Vec<Item>, rows: &mut Vec<Row>| {
        if entries.is_empty() {
            return false;
        }
        rows.push(Row::Heading(heading));
        for entry in entries {
            rows.push(Row::Item(items.len()));
            items.push(entry);
        }
        rows.push(Row::GroupEnd);
        true
    };

    let mut actions: Vec<Item> = actions(selected.is_some(), cx)
        .into_iter()
        .filter(|action| matches_action(&action.label, &action.keywords, deferred))
        .map(|action| Item {
            label: action.label,
            kind: ItemKind::Action {
                shortcut: action.shortcut,
            },
            command: action.command,
        })
        .collect();
    actions.extend(
        crate::screens::help::palette_topics(deferred, cx.global::<I18n>())
            .into_iter()
            .map(|(topic, label, _)| Item {
                label,
                kind: ItemKind::Action { shortcut: None },
                command: PaletteCommand::Help(Some(topic)),
            }),
    );
    let tasks: Vec<Item> = rank_tasks(&context.tasks, deferred, selected)
        .into_iter()
        .map(|task| Item {
            label: task.name.clone().into(),
            kind: ItemKind::Task {
                branch: task.branch.clone(),
                selected: selected == Some(task.id.as_str()),
            },
            command: PaletteCommand::OpenTask(task.id),
        })
        .collect();

    let had_actions = group(cx.t("commandPalette.actions"), actions, &mut rows);
    if had_actions && !tasks.is_empty() {
        rows.push(Row::Separator);
    }
    if group(
        cx.t("commandPalette.workspacesAndProjects"),
        tasks,
        &mut rows,
    ) {
        rows.push(Row::Separator);
    }

    // The conversations' text once an `@` query settled; nothing while it
    // waits; else the open task's recent ones by their title.
    let searching = search.enabled && palette.settled.as_deref() == Some(search.query.as_str());
    let answered = palette
        .hits
        .as_ref()
        .filter(|hits| searching && hits.query == search.query);
    let sessions = if searching {
        answered
            .map(|hits| hits.sessions.clone())
            .unwrap_or_default()
    } else if search.explicit {
        Vec::new()
    } else {
        filter_sessions(&recent_sessions(context.recent.clone()), deferred)
    };
    if searching && answered.is_none() {
        rows.push(Row::LoadingSessions);
    }
    let (mine, others) = group_sessions(sessions, selected);
    let session_items = |list: Vec<PaletteSession>| -> Vec<Item> {
        list.into_iter()
            .map(|session| Item {
                label: session.title.clone().into(),
                kind: ItemKind::Session {
                    workspace: session.workspace_label(),
                    snippet: (searching && !session.snippet.trim().is_empty())
                        .then(|| session.snippet.clone()),
                    archived: session.archived,
                },
                command: PaletteCommand::OpenSession(session),
            })
            .collect()
    };
    let had_mine = group(
        cx.t("commandPalette.currentWorkspaceSessions"),
        session_items(mine),
        &mut rows,
    );
    let had_others = group(
        cx.t("commandPalette.recentSessions"),
        session_items(others),
        &mut rows,
    );
    if had_mine || had_others {
        rows.push(Row::Separator);
    }

    if context.root.is_some() {
        let files: Vec<Item> = quick_open::rank(&palette.files, deferred)
            .into_iter()
            .map(|path| Item {
                label: quick_open::basename(&path).to_string().into(),
                kind: ItemKind::File {
                    dir: quick_open::dirname(&path).to_string(),
                },
                command: PaletteCommand::OpenFile(path),
            })
            .collect();
        group(cx.t("commandPalette.workspaceFiles"), files, &mut rows);
    }
    if items.is_empty() {
        rows.insert(0, Row::Empty);
    }
    (rows, items)
}
