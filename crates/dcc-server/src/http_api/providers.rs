//! Settings → Providers for the native app: turning a provider on or off
//! (`set_provider_availability`) goes through the desktop, which owns the
//! runtime registry and stops the provider's running turns.
use super::*;

#[derive(Deserialize)]
pub(super) struct AvailabilityInput {
    enabled: bool,
}

pub(super) async fn set_availability(
    State(backend): State<Arc<HttpBackend>>,
    Path(provider_id): Path<String>,
    Json(input): Json<AvailabilityInput>,
) -> Result<Json<Value>, HttpApiError> {
    let availability = backend
        .sessions
        .set_provider_enabled(provider_id.trim(), input.enabled)
        .await
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    Ok(Json(json!({ "availability": availability })))
}
