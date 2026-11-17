//! The Guide (`dcc_core::domain::agent_guide`) joins the team when the app
//! starts, once: made here and not when the database opens, so only the
//! app writes it. A person who deleted it does not get it back.

/// Off the main thread: the app does not wait for it to draw.
pub fn ensure() {
    std::thread::spawn(|| {
        let made = dcc_infra::db::SqliteSessionRepo::open(&super::loader::database_path())
            .and_then(|repo| repo.ensure_guide());
        if let Err(error) = made {
            eprintln!("[dcc-native] the Guide could not be made: {error}");
        }
    });
}
