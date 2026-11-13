//! The machines this DCC runs tasks on (`crate::machines`), for the app on
//! this computer: the list, pairing from a machine's link (it answers once
//! the person there approved), forgetting one, and `…/proxy/<path>`, which
//! sends a call of this API to the machine's own, through its channel — so
//! the app talks to a remote task the way it talks to a local one, with a
//! prefix. For the operator on this computer only: a paired device never
//! uses this DCC to reach another machine, and the relay does not forward
//! `/api/v1/machines`.
//!
//! `/api/v1/machine` is the other side: this DCC's own card (its name,
//! system and version), which a DCC that paired with it reads.
use std::collections::BTreeMap;

use axum::body::Bytes;
use axum::extract::OriginalUri;
use axum::http::{header, HeaderMap, Method, StatusCode};

use super::*;
use crate::machine_link::LinkError;
use crate::machines::MachineError;

const PROXY: &str = "/proxy";

pub(super) fn routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/machine", get(this_machine))
        .route("/api/v1/machines", get(list))
        .route("/api/v1/machines/pair", post(pair))
        .route("/api/v1/machines/:id", axum::routing::delete(remove))
        .route(
            "/api/v1/machines/:id/proxy/*rest",
            axum::routing::any(proxy),
        )
        .route(
            "/api/v1/machines/:id/terminals/events/stream",
            get(terminal_stream),
        )
}

fn operator(device: &Option<axum::Extension<PairedDeviceIdentity>>) -> Result<(), HttpApiError> {
    if device.is_some() {
        return Err(HttpApiError::Forbidden(
            "paired devices cannot reach other machines".into(),
        ));
    }
    Ok(())
}

fn failed(error: MachineError) -> HttpApiError {
    match error {
        MachineError::BadLink | MachineError::Itself => {
            HttpApiError::bad_request(error.to_string())
        }
        MachineError::Unknown => HttpApiError::not_found(error.to_string()),
        MachineError::Database(message) => HttpApiError::internal(message),
        MachineError::Link(LinkError::Denied | LinkError::Refused(_)) => {
            HttpApiError::Forbidden(error.to_string())
        }
        MachineError::Link(LinkError::Timeout) => HttpApiError::GatewayTimeout(error.to_string()),
        MachineError::Link(LinkError::Offline) => {
            HttpApiError::ServiceUnavailable(error.to_string())
        }
        MachineError::Link(_) => HttpApiError::BadGateway(error.to_string()),
    }
}

async fn this_machine() -> Json<Value> {
    Json(json!({
        "name": crate::machines::this_computer_name(),
        "platform": crate::machines::this_platform(),
        "version": env!("CARGO_PKG_VERSION"),
        "protocolVersion": DCC_HTTP_PROTOCOL_VERSION,
    }))
}

async fn list(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
) -> Result<Json<Value>, HttpApiError> {
    operator(&device)?;
    let registry = backend.machines();
    let machines = registry.list().await.map_err(failed)?;
    let this_computer = registry.this_fingerprint().await.map_err(failed)?;
    Ok(Json(
        json!({ "machines": machines, "thisComputer": this_computer }),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairInput {
    /// The `dcc://pair#…` link the machine showed.
    link: String,
    #[serde(default)]
    name: Option<String>,
}

async fn pair(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
    Json(input): Json<PairInput>,
) -> Result<Json<Value>, HttpApiError> {
    operator(&device)?;
    let machine = backend
        .machines()
        .pair(&input.link, input.name, || {})
        .await
        .map_err(failed)?;
    Ok(Json(json!({ "machine": machine })))
}

async fn remove(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    operator(&device)?;
    let removed = backend.machines().remove(&id).await.map_err(failed)?;
    // Its tasks stay as history, no longer followed.
    let path = backend.workspace.db_path.clone();
    let _ = tokio::task::spawn_blocking(move || {
        crate::machine_mirror::open(&path)
            .and_then(|conn| crate::machine_mirror::forget_machine(&conn, &id))
    })
    .await;
    Ok(Json(json!({ "removed": removed })))
}

/// The path and query a proxied call asks of the machine: what follows
/// `/proxy` in the address it came on.
pub(super) fn proxied_path(uri: &str) -> Option<&str> {
    let at = uri.find(PROXY)? + PROXY.len();
    Some(&uri[at..]).filter(|path| path.starts_with('/'))
}

async fn proxy(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
    Path((id, _rest)): Path<(String, String)>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, HttpApiError> {
    operator(&device)?;
    let address = uri
        .path_and_query()
        .map(|address| address.as_str())
        .unwrap_or_default();
    let path = proxied_path(address).ok_or_else(|| HttpApiError::bad_request("no path"))?;
    let mut forwarded = BTreeMap::new();
    for name in [header::CONTENT_TYPE, header::ACCEPT] {
        if let Some(value) = headers.get(&name).and_then(|value| value.to_str().ok()) {
            forwarded.insert(name.as_str().to_string(), value.to_string());
        }
    }
    let answer = backend
        .machines()
        .request(&id, method.as_str(), path, forwarded, body.to_vec())
        .await
        .map_err(failed)?;
    let status = StatusCode::from_u16(answer.status).unwrap_or(StatusCode::BAD_GATEWAY);
    let content_type = answer
        .headers
        .get("content-type")
        .cloned()
        .unwrap_or_else(|| "application/json".to_string());
    Ok((status, [(header::CONTENT_TYPE, content_type)], answer.body).into_response())
}

/// What the machine's terminals print, as the app's dock reads its own
/// (`/api/v1/terminals/events/stream`): the channel's subscription, event by
/// event. It ends when the machine leaves; the dock opens it again.
async fn terminal_stream(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
    Path(id): Path<String>,
) -> Result<
    Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>,
    HttpApiError,
> {
    use tokio_stream::wrappers::UnboundedReceiverStream;
    use tokio_stream::StreamExt as _;

    operator(&device)?;
    let events = backend
        .machines()
        .subscribe(&id, crate::remote_terminals::STREAM_PATH)
        .await
        .map_err(failed)?;
    let stream = UnboundedReceiverStream::new(events)
        .map_while(|item| match item {
            crate::machine_link::StreamItem::Event(event) => Some(event),
            crate::machine_link::StreamItem::End(_) => None,
        })
        .map(|event| {
            Ok(Event::default()
                .event(event.event)
                .data(event.data.to_string()))
        });
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proxied_call_keeps_the_machines_path_and_query() {
        assert_eq!(
            proxied_path("/api/v1/machines/m-1/proxy/api/v1/sessions/s/events?after=3"),
            Some("/api/v1/sessions/s/events?after=3")
        );
        assert_eq!(
            proxied_path("/api/v1/machines/m-1/proxy/health"),
            Some("/health")
        );
        assert_eq!(proxied_path("/api/v1/machines/m-1/proxy"), None);
        assert_eq!(proxied_path("/api/v1/machines"), None);
    }
}
