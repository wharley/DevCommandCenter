//! What Claude Code and Codex used on this machine, by the day, read from
//! their logs through the same index as DCC Usage. For clients that draw
//! their own history (DCC Bot); the dashboard's charts stay in the app.
use super::*;
use dcc_core::domain::usage_logs::{daily_history, RuntimeHistory};
use dcc_infra::usage_logs::{discover, index_path, Resolver, UsageIndex};

/// One refresh of the index at a time.
static REFRESH: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HistoryQuery {
    /// Today and the days before it; 30 by default, at most 90.
    days: Option<u32>,
    /// Minutes east of UTC the days are cut in; the machine's own zone
    /// when absent.
    offset_minutes: Option<i32>,
}

pub(super) async fn history(
    State(backend): State<Arc<HttpBackend>>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let days = query.days.unwrap_or(30).clamp(1, 90);
    let zone = match query.offset_minutes {
        Some(minutes) => Some(
            chrono::FixedOffset::east_opt(minutes.saturating_mul(60))
                .ok_or_else(|| HttpApiError::bad_request("offsetMinutes is out of range"))?,
        ),
        None => None,
    };
    let db_path = backend.workspace.db_path.clone();
    let _one = REFRESH.lock().await;
    let (scanned, runtimes) = tokio::task::spawn_blocking(move || read(&db_path, days, zone))
        .await
        .map_err(|error| HttpApiError::internal(error.to_string()))?
        .map_err(HttpApiError::internal)?;
    Ok(Json(
        json!({ "days": days, "scanned": scanned, "runtimes": runtimes }),
    ))
}

fn read(
    db_path: &FsPath,
    days: u32,
    zone: Option<chrono::FixedOffset>,
) -> Result<(i64, Vec<RuntimeHistory>), String> {
    let (sources, hints) = discover(Some(db_path));
    let mut index = UsageIndex::open(&index_path(db_path)).map_err(|error| error.to_string())?;
    index
        .refresh(&sources, &mut Resolver::new(hints), &mut |_, _| {})
        .map_err(|error| error.to_string())?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    // A day of slack for the zone; `daily_history` cuts at its midnight.
    let since = now_ms - (i64::from(days) + 1) * 86_400_000;
    let rows = index
        .load_rows(Some(since))
        .map_err(|error| error.to_string())?;
    let runtimes = match zone {
        Some(zone) => daily_history(&rows, &zone, now_ms, days),
        None => daily_history(&rows, &chrono::Local, now_ms, days),
    };
    Ok((now_ms, runtimes))
}
