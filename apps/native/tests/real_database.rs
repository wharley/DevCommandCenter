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
