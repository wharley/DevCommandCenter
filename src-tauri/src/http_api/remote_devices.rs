//! The paired phones and the relay for Settings' "Conexões" in the native
//! app: the devices (`pair_list_devices`), revoking one (which also closes
//! its live connection), the pairing audit log and the relay's address.
//! Like the relay's other routes, for the operator on this machine only:
//! a paired device is refused, and the relay never forwards
//! `/api/v1/remote/`.
use super::*;

pub(super) fn routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/remote/devices", get(devices))
        .route("/api/v1/remote/devices/revoke", post(revoke))
        .route("/api/v1/remote/audit", get(audit))
        .route("/api/v1/remote/relay", post(set_relay))
}

fn operator(device: &Option<axum::Extension<PairedDeviceIdentity>>) -> Result<(), HttpApiError> {
    if device.is_some() {
        return Err(HttpApiError::Forbidden(
            "paired devices cannot manage remote access".into(),
        ));
    }
    Ok(())
}

async fn with_pairing_db<T: Send + 'static>(
    backend: &HttpBackend,
    read: impl FnOnce(&rusqlite::Connection) -> Result<T, crate::pairing::PairingError> + Send + 'static,
) -> Result<T, HttpApiError> {
    let path = backend.workspace.db_path.clone();
    tokio::task::spawn_blocking(move || {
        let conn = crate::pairing::open_pairing_db(&path)?;
        read(&conn)
    })
    .await
    .map_err(|error| HttpApiError::internal(error.to_string()))?
    .map_err(|error| HttpApiError::internal(error.to_string()))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevicesQuery {
    #[serde(default)]
    include_revoked: bool,
}

async fn devices(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
    Query(query): Query<DevicesQuery>,
) -> Result<Json<Value>, HttpApiError> {
    operator(&device)?;
    let devices = with_pairing_db(&backend, move |conn| {
        crate::pairing::list_paired_devices(conn, query.include_revoked)
    })
    .await?;
    Ok(Json(json!({ "devices": devices })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RevokeInput {
    device_id: String,
}

async fn revoke(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
    Json(input): Json<RevokeInput>,
) -> Result<Json<Value>, HttpApiError> {
    operator(&device)?;
    let target = input.device_id.clone();
    let revoked = with_pairing_db(&backend, move |conn| {
        crate::pairing::revoke_device(conn, &target, None)
    })
    .await?;
    // Revoking a relay phone also closes its live connection.
    if let Some(hub) = backend.remote() {
        hub.device_revoked(&input.device_id);
    }
    Ok(Json(json!({ "revoked": revoked })))
}

#[derive(Debug, Deserialize)]
struct AuditQuery {
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    100
}

async fn audit(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Value>, HttpApiError> {
    operator(&device)?;
    let entries = with_pairing_db(&backend, move |conn| {
        crate::pairing::list_audit_log(conn, query.limit)
    })
    .await?;
    Ok(Json(json!({ "entries": entries })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RelayInput {
    #[serde(default)]
    relay_url: Option<String>,
}

/// `remote_access_set_relay`: saved (or cleared, turning remote access
/// off), then the status.
async fn set_relay(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
    Json(input): Json<RelayInput>,
) -> Result<Json<Value>, HttpApiError> {
    let remote = operator_remote(&backend, device)?;
    remote
        .set_relay_url(input.relay_url)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(remote.status())))
}
