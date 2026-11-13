//! Reads a real database end to end. Ignored by default; run it against a
//! copy (never the live file):
//! `DCC_APP_DATA_DIR=/path/to/copy cargo test --test real_database -- --ignored --nocapture`

use std::time::Instant;

use dcc_native::data::{loader, rail, seen::SeenResults, time};

#[test]
#[ignore]
fn loads_the_sidebar_from_a_database_copy() {
    let started = Instant::now();
    let data = loader::load(&loader::database_path(), "Nova tarefa").expect("load");
    let elapsed = started.elapsed();
    let now = time::now_ms();
    let split = rail::split(&data.tasks, now);
    let sections = rail::task_list(&split.active, &data.activities, now, None);
    let seen = SeenResults::default();
    let needs = rail::needs_you(
        &data.tasks,
        &data.activities,
        &[],
        &|id, at| seen.is_unread(id, at),
        now,
    );
    println!(
        "loaded in {elapsed:?}: {} tasks, {} projects, {} with activity",
        data.tasks.len(),
        data.projects.len(),
        data.activities.len()
    );
    for section in &sections {
        println!("[{:?}]", section.id);
        for task in &section.tasks {
            let activity = data
                .activities
                .get(&task.id)
                .map(|a| format!("{:?}", a.activity.state));
            println!(
                "  {} · {} · {:?}",
                task.display_title(),
                data.provider_ids.get(&task.id).map_or("-", String::as_str),
                activity
            );
        }
    }
    println!(
        "on hold: {}, completed: {}, needs you: {}",
        split.waiting.len(),
        split.completed.len(),
        needs.len()
    );
    assert!(!data.tasks.is_empty());
}

#[test]
#[ignore]
fn reads_git_and_pull_requests_for_open_tasks() {
    use dcc_native::data::git_watch;
    use dcc_native::screens::sidebar::state::SidebarState;

    let path = loader::database_path();
    let data = loader::load(&path, "Nova tarefa").expect("load");
    let mut state = SidebarState {
        data,
        now_ms: time::now_ms(),
        ..SidebarState::default()
    };
    let started = Instant::now();
    let cache = git_watch::refresh(&path, &state.watched(), Default::default(), state.now_ms);
    println!(
        "git + PR for {} tasks in {:?}",
        cache.len(),
        started.elapsed()
    );
    let open: Vec<String> = state.watched().into_iter().map(|task| task.id).collect();
    state.blockers = git_watch::blockers(
        &open,
        &cache,
        &state.data.review_pending,
        &mut state.first_seen,
        "2026-10-23T12:00:00Z",
    );
    state.git = cache;
    for task in &state.data.tasks {
        let git = state.row_git(task);
        println!(
            "{} | branch {} | recap {:?} | meta {:?}",
            task.display_title(),
            git.current_branch,
            git.recap.map(|recap| recap.message_key),
            git.meta.map(|meta| (
                meta.pr_number,
                meta.pr_state,
                meta.additions,
                meta.deletions
            )),
        );
    }
    println!("blockers: {:?}", state.blockers);
}

/// Every row action against a copy: each write shows up when the sidebar
/// reads again, and the task ends as it started.
#[test]
#[ignore]
fn row_actions_write_through_the_shared_backend() {
    use dcc_native::data::actions::{self, TaskAction};
    use dcc_native::data::tasks::TaskStatus;

    let path = loader::database_path();
    let read = |id: &str| {
        loader::load(&path, "Nova tarefa")
            .expect("load")
            .tasks
            .into_iter()
            .find(|task| task.id == id)
            .expect("task")
    };
    let data = loader::load(&path, "Nova tarefa").expect("load");
    let task = data
        .tasks
        .iter()
        .find(|task| {
            task.status == TaskStatus::Ready && task.bundle_id.is_none() && task.pinned_at.is_none()
        })
        .expect("a ready single-project task")
        .clone();
    let id = task.id.as_str();
    let run = |action: TaskAction| actions::run(&path, id, None, &action).expect("action");
    println!("task {id} «{}»", task.display_title());

    run(TaskAction::SetPinned(true));
    assert!(read(id).pinned_at.is_some());
    run(TaskAction::SetPinned(false));
    assert!(read(id).pinned_at.is_none());

    let until = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    run(TaskAction::Snooze(Some(until)));
    assert!(read(id).snoozed_until.is_some());
    run(TaskAction::Snooze(None));
    assert!(read(id).snoozed_until.is_none());

    run(TaskAction::Rename("  Renomeada   pelo nativo ".into()));
    assert_eq!(read(id).name, "Renomeada pelo nativo");
    let empty = actions::run(&path, id, None, &TaskAction::Rename("   ".into()));
    assert!(empty.is_err());
    run(TaskAction::Rename(task.name.clone()));

    run(TaskAction::Archive);
    assert_eq!(read(id).status, TaskStatus::Archived);
    run(TaskAction::Restore);
    assert_eq!(read(id).status, TaskStatus::Ready);
    run(TaskAction::Complete);
    assert_eq!(read(id).status, TaskStatus::Completed);
    run(TaskAction::Restore);
    let after = read(id);
    assert_eq!(after.status, TaskStatus::Ready);
    assert_eq!(after.name, task.name);

    if let Some(bundle) = data
        .tasks
        .iter()
        .find(|task| task.bundle_id.is_some() && task.status == TaskStatus::Ready)
    {
        let bundle_id = bundle.bundle_id.as_deref();
        actions::run(&path, &bundle.id, bundle_id, &TaskAction::Archive).expect("archive bundle");
        assert_eq!(read(&bundle.id).status, TaskStatus::Archived);
        actions::run(&path, &bundle.id, bundle_id, &TaskAction::Restore).expect("restore bundle");
        assert_eq!(read(&bundle.id).status, TaskStatus::Ready);
        println!(
            "bundle {} «{}» archived and restored",
            bundle.id,
            bundle.display_title()
        );
    }
}

/// What the completed tasks take on disk, read only (git status and a walk).
#[test]
#[ignore]
fn measures_completed_tasks_on_a_database_copy() {
    use dcc_native::data::{deletion, disk};
    let path = loader::database_path();
    let data = loader::load(&path, "Nova tarefa").expect("load");
    let completed = rail::split(&data.tasks, time::now_ms()).completed;
    let ids = disk::disk_usage_ids(&completed.iter().collect::<Vec<_>>());
    let started = Instant::now();
    let scan = deletion::cleanup_scan(&path, ids).expect("scan");
    let rows = disk::cleanup_rows(&completed.iter().collect::<Vec<_>>(), &scan.workspaces);
    println!(
        "{} completed, {} in {:?}",
        rows.len(),
        disk::format_disk_bytes(scan.total_bytes, dcc_native::i18n::Locale::PtBr),
        started.elapsed()
    );
    for row in rows.iter().take(8) {
        println!(
            "  {} · {} · {:?} · preselected {}",
            row.task_id,
            disk::format_disk_bytes(row.bytes, dcc_native::i18n::Locale::PtBr),
            row.safety,
            disk::preselected(row)
        );
    }
}

/// Resident agents and child tasks as the sidebar reads them (read only).
#[test]
#[ignore]
fn reads_agents_and_child_tasks_on_a_database_copy() {
    let data = loader::load(&loader::database_path(), "Nova tarefa").expect("load");
    for agent in &data.agents {
        let states: Vec<_> = agent.sessions.iter().map(|session| session.state).collect();
        println!(
            "agent {} ({:?}) {:?} sessions {:?}",
            agent.name, agent.preset, agent.avatar, states
        );
    }
    for (task, children) in &data.child_tasks {
        for child in children {
            println!("  child of {task}: {} · {:?}", child.title, child.state);
        }
    }
}

/// Every conversation of the open tasks folded into rows (read only).
#[test]
#[ignore]
fn projects_conversations_on_a_database_copy() {
    use dcc_core::domain::timeline::{Annotation, Role, project_session};
    use dcc_core::domain::workspace::WorkspaceId;
    use dcc_core::ports::SessionEventRepo;
    let path = loader::database_path();
    let data = loader::load(&path, "Nova tarefa").expect("load");
    let sessions = dcc_infra::db::SqliteSessionRepo::open_read_only(&path).expect("open");
    for task in data.tasks.iter().take(4) {
        for summary in sessions
            .list_workspace_sessions(&WorkspaceId(task.id.clone()))
            .expect("sessions")
            .iter()
            .take(1)
        {
            let started = Instant::now();
            let events =
                futures::executor::block_on(sessions.list_events_by_session(&summary.session.id))
                    .expect("events");
            let rows = project_session(&events, &summary.session.id.0);
            let tools: usize = rows
                .iter()
                .flat_map(|row| &row.annotations)
                .filter(|annotation| matches!(annotation, Annotation::ToolCall(_)))
                .count();
            println!(
                "{} · {} events → {} rows ({} user, {} assistant, {} system), {tools} tools, {:?}",
                task.display_title(),
                events.len(),
                rows.len(),
                rows.iter().filter(|row| row.role == Role::User).count(),
                rows.iter()
                    .filter(|row| row.role == Role::Assistant)
                    .count(),
                rows.iter().filter(|row| row.role == Role::System).count(),
                started.elapsed()
            );
            if let Some(last) = rows.iter().rev().find(|row| row.role == Role::Assistant) {
                let preview: String = last.content.chars().take(100).collect();
                println!(
                    "    last answer: {preview:?} settled={} model={:?}",
                    last.turn_settled, last.model
                );
            }
        }
    }
}

#[test]
#[ignore = "reads the desktop's database; point DCC_APP_DATA_DIR at a copy"]
fn loads_task_conversations_on_a_database_copy() {
    use dcc_native::data::conversation::{self, EventCache};
    let path = loader::database_path();
    let data = loader::load(&path, "Nova tarefa").expect("load");
    for task in data.tasks.iter().take(6) {
        let mut cache = EventCache::default();
        let started = Instant::now();
        let first = conversation::load(&path, &task.id, None, &mut cache).expect("conversation");
        let cold = started.elapsed();
        let started = Instant::now();
        let again = conversation::load(&path, &task.id, None, &mut cache).expect("again");
        println!(
            "{} · session {:?} · {} rows · {} delegations · {} turns with tokens · model {:?} → {} · cold {cold:?}, warm {:?} · running={}",
            task.display_title(),
            first.session_id,
            first.rows.len(),
            first.delegations.len(),
            first.turn_tokens.len(),
            first.model,
            first
                .model
                .as_deref()
                .map(dcc_native::project::model_label)
                .unwrap_or_default(),
            started.elapsed(),
            first.running,
        );
        println!(
            "    context {:?} · branch {:?} · limit {} · goal {:?} · auto-resume {}",
            first.context.as_ref().map(|context| (
                context.project_label.as_deref(),
                context.current_branch.as_deref(),
                context.base_branch.as_deref(),
                context.isolated
            )),
            first.branch,
            first.usage_limit.is_some(),
            first.goal.as_ref().map(|goal| goal.status),
            first.auto_resume,
        );
        assert!(first.same_as(&again) || first.running);
    }
}

#[test]
#[ignore = "reads a real checkout; point DCC_TREE_ROOT at one"]
fn builds_the_code_tree_of_a_real_checkout() {
    use std::collections::HashSet;
    let root = std::env::var("DCC_TREE_ROOT").expect("DCC_TREE_ROOT");
    let started = Instant::now();
    let mut paths = dcc_infra::workspace_git::git_tracked_files(&root).expect("tracked");
    let ignored = dcc_infra::workspace_git::git_ignored_files(&root).expect("ignored");
    let listed = started.elapsed();
    paths.extend(ignored);
    let started = Instant::now();
    let tree = dcc_native::data::code_tree::build_code_tree(&paths);
    let rows = dcc_native::data::code_tree::visible_rows(&tree, &HashSet::new());
    let built = started.elapsed();
    let started = Instant::now();
    let status = dcc_infra::workspace_git::read_workspace_git_status(&root).expect("status");
    println!(
        "{} files, {} top rows · listed in {listed:?}, built in {built:?}, status in {:?} ({} changes)",
        paths.len(),
        rows.len(),
        started.elapsed(),
        status.staged.len() + status.unstaged.len()
    );
}
