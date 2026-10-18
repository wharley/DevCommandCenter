//! OAuth-protected MCP server with its own authorization server, for testing
//! DCC's connector sign-in offline. One origin serves the protected `/mcp`
//! endpoint, RFC 9728 and RFC 8414 metadata, dynamic registration,
//! authorization (auto-approved), token, and revocation endpoints.

use std::{
    collections::{HashMap, HashSet},
    net::Ipv4Addr,
    sync::{Arc, Mutex},
};

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Form, Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;

#[derive(Clone, Debug)]
pub struct OAuthFixtureConfig {
    pub dynamic_registration: bool,
    pub registration_forbidden: bool,
    pub metadata_document: bool,
    /// Atlassian omits `resource_metadata` from its 401 challenge.
    pub challenge_has_resource_metadata: bool,
    pub advertise_iss_parameter: bool,
    pub send_iss: bool,
    pub wrong_iss: bool,
    pub revocation: bool,
    pub access_token_ttl_secs: i64,
}

impl Default for OAuthFixtureConfig {
    fn default() -> Self {
        Self {
            dynamic_registration: true,
            registration_forbidden: false,
            metadata_document: false,
            challenge_has_resource_metadata: true,
            advertise_iss_parameter: true,
            send_iss: true,
            wrong_iss: false,
            revocation: true,
            access_token_ttl_secs: 3600,
        }
    }
}

#[derive(Default)]
struct Ledger {
    next_id: u64,
    clients: HashMap<String, Vec<String>>,
    codes: HashMap<String, IssuedCode>,
    access_tokens: HashSet<String>,
    refresh_tokens: HashSet<String>,
    registrations: usize,
    refreshes: usize,
    revoked: Vec<String>,
    last_resource: Option<String>,
}

struct IssuedCode {
    client_id: String,
    redirect_uri: String,
    challenge: String,
    resource: String,
}

#[derive(Clone)]
struct FixtureState {
    origin: String,
    config: OAuthFixtureConfig,
    ledger: Arc<Mutex<Ledger>>,
}

pub struct OAuthFixture {
    pub origin: String,
    state: FixtureState,
}

impl OAuthFixture {
    pub async fn spawn(config: OAuthFixtureConfig) -> std::io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let origin = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
        let state = FixtureState {
            origin: origin.clone(),
            config,
            ledger: Arc::default(),
        };
        let router = Router::new()
            .route("/mcp", post(mcp))
            .route(
                "/.well-known/oauth-protected-resource/mcp",
                get(protected_resource),
            )
            .route(
                "/.well-known/oauth-authorization-server",
                get(authorization_server),
            )
            .route("/register", post(register))
            .route("/authorize", get(authorize))
            .route("/token", post(token))
            .route("/revoke", post(revoke))
            .with_state(state.clone());
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(Self { origin, state })
    }

    pub fn server_url(&self) -> String {
        format!("{}/mcp", self.origin)
    }

    pub fn registrations(&self) -> usize {
        self.ledger().registrations
    }

    pub fn refreshes(&self) -> usize {
        self.ledger().refreshes
    }

    pub fn revoked(&self) -> Vec<String> {
        self.ledger().revoked.clone()
    }

    pub fn last_resource(&self) -> Option<String> {
        self.ledger().last_resource.clone()
    }

    pub fn accepts_access_token(&self, token: &str) -> bool {
        self.ledger().access_tokens.contains(token)
    }

    fn ledger(&self) -> std::sync::MutexGuard<'_, Ledger> {
        self.state.ledger.lock().expect("fixture ledger")
    }
}

impl FixtureState {
    fn ledger(&self) -> std::sync::MutexGuard<'_, Ledger> {
        self.ledger.lock().expect("fixture ledger")
    }

    fn resource(&self) -> String {
        format!("{}/mcp", self.origin)
    }
}

async fn mcp(State(state): State<FixtureState>, headers: HeaderMap) -> Response {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if token.is_some_and(|token| state.ledger().access_tokens.contains(token)) {
        return Json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "protocolVersion": crate::LATEST_PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "dcc-oauth-fixture", "version": "0" }
            }
        }))
        .into_response();
    }
    let challenge = if state.config.challenge_has_resource_metadata {
        format!(
            r#"Bearer resource_metadata="{}/.well-known/oauth-protected-resource/mcp", scope="tasks:read""#,
            state.origin
        )
    } else {
        r#"Bearer realm="fixture""#.to_string()
    };
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, challenge)],
    )
        .into_response()
}

async fn protected_resource(State(state): State<FixtureState>) -> Json<Value> {
    Json(json!({
        "resource": state.resource(),
        "authorization_servers": [state.origin],
        "scopes_supported": ["tasks:read", "tasks:write"],
    }))
}

async fn authorization_server(State(state): State<FixtureState>) -> Json<Value> {
    let mut metadata = json!({
        "issuer": state.origin,
        "authorization_endpoint": format!("{}/authorize", state.origin),
        "token_endpoint": format!("{}/token", state.origin),
        "code_challenge_methods_supported": ["S256"],
        "client_id_metadata_document_supported": state.config.metadata_document,
        "authorization_response_iss_parameter_supported": state.config.advertise_iss_parameter,
    });
    if state.config.dynamic_registration {
        metadata["registration_endpoint"] = json!(format!("{}/register", state.origin));
    }
    if state.config.revocation {
        metadata["revocation_endpoint"] = json!(format!("{}/revoke", state.origin));
    }
    Json(metadata)
}

async fn register(State(state): State<FixtureState>, Json(body): Json<Value>) -> Response {
    if state.config.registration_forbidden {
        return StatusCode::FORBIDDEN.into_response();
    }
    let redirect_uris = body
        .get("redirect_uris")
        .and_then(Value::as_array)
        .map(|uris| {
            uris.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if redirect_uris.is_empty() || body.get("application_type") != Some(&json!("native")) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_client_metadata"})),
        )
            .into_response();
    }
    let mut ledger = state.ledger();
    ledger.next_id += 1;
    ledger.registrations += 1;
    let client_id = format!("client-{}", ledger.next_id);
    ledger.clients.insert(client_id.clone(), redirect_uris);
    (StatusCode::CREATED, Json(json!({ "client_id": client_id }))).into_response()
}

async fn authorize(
    State(state): State<FixtureState>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let field = |name: &str| query.get(name).cloned().unwrap_or_default();
    let (client_id, redirect_uri) = (field("client_id"), field("redirect_uri"));
    let registered = state
        .ledger()
        .clients
        .get(&client_id)
        .is_some_and(|uris| uris.contains(&redirect_uri));
    if !registered
        || field("response_type") != "code"
        || field("code_challenge_method") != "S256"
        || field("code_challenge").len() != 43
        || field("resource") != state.resource()
        || field("state").is_empty()
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let mut ledger = state.ledger();
    ledger.next_id += 1;
    let code = format!("code-{}", ledger.next_id);
    ledger.codes.insert(
        code.clone(),
        IssuedCode {
            client_id,
            redirect_uri: redirect_uri.clone(),
            challenge: field("code_challenge"),
            resource: field("resource"),
        },
    );
    let mut location = url::Url::parse(&redirect_uri).expect("registered redirect");
    {
        let mut pairs = location.query_pairs_mut();
        pairs
            .append_pair("code", &code)
            .append_pair("state", &field("state"));
        if state.config.send_iss {
            let issuer = if state.config.wrong_iss {
                "https://attacker.example".to_string()
            } else {
                state.origin.clone()
            };
            pairs.append_pair("iss", &issuer);
        }
    }
    (
        StatusCode::FOUND,
        [(header::LOCATION, location.to_string())],
    )
        .into_response()
}

async fn token(
    State(state): State<FixtureState>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let field = |name: &str| form.get(name).cloned().unwrap_or_default();
    let invalid_grant = || {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_grant"})),
        )
            .into_response()
    };
    let mut ledger = state.ledger();
    match field("grant_type").as_str() {
        "authorization_code" => {
            let Some(issued) = ledger.codes.remove(&field("code")) else {
                return invalid_grant();
            };
            let verifier_hash = URL_SAFE_NO_PAD.encode(Sha256::digest(field("code_verifier")));
            if issued.client_id != field("client_id")
                || issued.redirect_uri != field("redirect_uri")
                || issued.challenge != verifier_hash
                || issued.resource != field("resource")
            {
                return invalid_grant();
            }
            ledger.last_resource = Some(field("resource"));
        }
        "refresh_token" => {
            // Single-use refresh tokens, like several real vendors.
            if !ledger.refresh_tokens.remove(&field("refresh_token")) {
                return invalid_grant();
            }
            ledger.refreshes += 1;
            ledger.last_resource = Some(field("resource"));
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "unsupported_grant_type"})),
            )
                .into_response()
        }
    }
    ledger.next_id += 1;
    let access_token = format!("access-{}", ledger.next_id);
    let refresh_token = format!("refresh-{}", ledger.next_id);
    ledger.access_tokens.insert(access_token.clone());
    ledger.refresh_tokens.insert(refresh_token.clone());
    Json(json!({
        "access_token": access_token,
        "token_type": "Bearer",
        "expires_in": state.config.access_token_ttl_secs,
        "refresh_token": refresh_token,
        "scope": "tasks:read",
    }))
    .into_response()
}

async fn revoke(
    State(state): State<FixtureState>,
    Form(form): Form<HashMap<String, String>>,
) -> StatusCode {
    let token = form.get("token").cloned().unwrap_or_default();
    let mut ledger = state.ledger();
    ledger.access_tokens.remove(&token);
    ledger.refresh_tokens.remove(&token);
    ledger.revoked.push(token);
    StatusCode::OK
}
