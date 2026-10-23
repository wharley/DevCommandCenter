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
