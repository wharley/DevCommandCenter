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
