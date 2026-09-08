//! Opt-in Web Push. A durable outbox observes events from both desktop and HTTP providers.
use super::*;
use crate::http_auth::PairedDeviceIdentity;
use axum::Extension;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use p256::elliptic_curve::sec1::ToEncodedPoint;
use web_push::{ContentEncoding, SubscriptionInfo, VapidSignatureBuilder, WebPushMessageBuilder};

fn schema(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS mobile_push_settings (id INTEGER PRIMARY KEY CHECK(id=1), private_key TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS mobile_push_subscriptions (
        device_id TEXT PRIMARY KEY, subscription TEXT NOT NULL, cursor INTEGER NOT NULL,
        failures INTEGER NOT NULL DEFAULT 0, next_try INTEGER NOT NULL DEFAULT 0);
      CREATE TABLE IF NOT EXISTS mobile_push_outbox (
        id INTEGER PRIMARY KEY AUTOINCREMENT, event_id TEXT UNIQUE NOT NULL, session_id TEXT NOT NULL,
        kind TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT (unixepoch()));").map_err(|e| e.to_string())?;
    // dcc_session_events may not exist until the session repository is first opened.
    let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='dcc_session_events')", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    if exists {
        conn.execute_batch("CREATE TRIGGER IF NOT EXISTS mobile_push_event_insert AFTER INSERT ON dcc_session_events
          WHEN json_extract(NEW.kind_json,'$.type') IN ('turn_completed','turn_aborted','turn_permission_requested','turn_user_input_requested')
          BEGIN INSERT OR IGNORE INTO mobile_push_outbox(event_id,session_id,kind)
          VALUES(NEW.event_id,NEW.session_id,json_extract(NEW.kind_json,'$.type')); END;").map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn private_key(conn: &rusqlite::Connection) -> Result<String, String> {
    schema(conn)?;
    let key = p256::SecretKey::random(&mut rand::rngs::OsRng);
    conn.execute(
        "INSERT OR IGNORE INTO mobile_push_settings(id,private_key) VALUES(1,?1)",
        [URL_SAFE_NO_PAD.encode(key.to_bytes())],
    )
    .map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT private_key FROM mobile_push_settings WHERE id=1",
        [],
        |r| r.get(0),
    )
    .map_err(|e| e.to_string())
}

fn device(identity: Option<Extension<PairedDeviceIdentity>>) -> Result<String, HttpApiError> {
    identity.map(|Extension(id)| id.0).ok_or_else(|| {
        HttpApiError::BadRequest("Pareie este dispositivo para ativar notificações.".into())
    })
}

pub(super) async fn config(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    identity: Option<Extension<PairedDeviceIdentity>>,
) -> Result<Json<Value>, HttpApiError> {
    let id = device(identity)?;
    db_read(config, move |conn| {
        let key = private_key(conn)?;
        let bytes = URL_SAFE_NO_PAD.decode(key).map_err(|e| e.to_string())?;
        let secret = p256::SecretKey::from_slice(&bytes).map_err(|e| e.to_string())?;
        let enabled: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM mobile_push_subscriptions WHERE device_id=?1)", [id], |r| r.get(0)).map_err(|e| e.to_string())?;
        Ok(Json(json!({ "publicKey":URL_SAFE_NO_PAD.encode(secret.public_key().to_encoded_point(false).as_bytes()), "enabled":enabled })))
    }).await
}

fn validate_endpoint(endpoint: &str) -> Result<(), HttpApiError> {
    let url = url::Url::parse(endpoint)
        .map_err(|_| HttpApiError::BadRequest("Endereço push inválido.".into()))?;
    let host = url.host_str().unwrap_or("");
    let allowed = host == "web.push.apple.com"
        || host == "fcm.googleapis.com"
        || host == "updates.push.services.mozilla.com"
        || host.ends_with(".push.services.mozilla.com")
        || host.ends_with(".notify.windows.com");
    if !allowed
        || url.scheme() != "https"
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || endpoint.len() > 4096
    {
        return Err(HttpApiError::BadRequest(
            "Serviço push não suportado por este DCC.".into(),
        ));
    }
    Ok(())
}

pub(super) async fn subscribe(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    identity: Option<Extension<PairedDeviceIdentity>>,
    Json(subscription): Json<SubscriptionInfo>,
) -> Result<Json<Value>, HttpApiError> {
    let id = device(identity)?;
    validate_endpoint(&subscription.endpoint)?;
    let public = URL_SAFE_NO_PAD
        .decode(&subscription.keys.p256dh)
        .map_err(|_| HttpApiError::BadRequest("Chave push inválida.".into()))?;
    p256::PublicKey::from_sec1_bytes(&public)
        .map_err(|_| HttpApiError::BadRequest("Chave push inválida.".into()))?;
    if URL_SAFE_NO_PAD
        .decode(&subscription.keys.auth)
        .map(|v| v.len())
        .unwrap_or(0)
        != 16
    {
        return Err(HttpApiError::BadRequest(
            "Autenticação push inválida.".into(),
        ));
    }
    db_read(config, move |conn| {
        schema(conn)?;
        conn.execute("INSERT INTO mobile_push_subscriptions(device_id,subscription,cursor) VALUES(?1,?2,(SELECT COALESCE(MAX(id),0) FROM mobile_push_outbox))
          ON CONFLICT(device_id) DO UPDATE SET subscription=excluded.subscription, failures=0,next_try=0", rusqlite::params![id, serde_json::to_string(&subscription).map_err(|e| e.to_string())?]).map_err(|e| e.to_string())?;
        Ok(Json(json!({"ok":true})))
    }).await
}

pub(super) async fn unsubscribe(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    identity: Option<Extension<PairedDeviceIdentity>>,
) -> Result<Json<Value>, HttpApiError> {
    let id = device(identity)?;
    db_read(config, move |conn| {
        schema(conn)?;
        conn.execute(
            "DELETE FROM mobile_push_subscriptions WHERE device_id=?1",
            [id],
        )
        .map_err(|e| e.to_string())?;
        Ok(Json(json!({"ok":true})))
    })
    .await
}

struct Delivery {
    device: String,
    subscription: SubscriptionInfo,
    seq: i64,
    session: String,
    kind: String,
    key: String,
}

async fn pending(config: Arc<RwLock<HttpConfig>>) -> Result<Vec<Delivery>, HttpApiError> {
    db_read(config, |conn| {
        schema(conn)?;
        // Revocation and expiry stop delivery even if the browser never returns to unsubscribe.
        conn.execute("DELETE FROM mobile_push_subscriptions WHERE device_id NOT IN (SELECT device_id FROM paired_devices
          WHERE revoked_at IS NULL AND julianday('now')-julianday(created_at) <= 30)", []).map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM mobile_push_outbox WHERE created_at < unixepoch()-86400", []).map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT s.device_id,s.subscription,e.id,e.session_id,e.kind FROM mobile_push_subscriptions s
          JOIN mobile_push_outbox e ON e.id=(SELECT MIN(id) FROM mobile_push_outbox WHERE id>s.cursor)
          WHERE s.next_try<=unixepoch() LIMIT 10").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?))).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            let (device, sub, seq, session, kind) = row.map_err(|e| e.to_string())?;
            if let Ok(subscription) = serde_json::from_str(&sub) { out.push(Delivery { device, subscription, seq, session, kind, key: private_key(conn)? }); }
        }
        Ok(out)
    }).await
}

async fn deliver(client: &reqwest::Client, delivery: &Delivery) -> Result<u16, String> {
    validate_endpoint(&delivery.subscription.endpoint).map_err(|e| e.message().to_owned())?;
    let title = match delivery.kind.as_str() {
        "turn_completed" => "Tarefa concluída",
        "turn_aborted" => "Tarefa interrompida",
        _ => "O agente precisa de você",
    };
    // No source code, prompts or command contents are sent to the lock screen.
    let payload = json!({"title":title,"body":"Abra o DCC para acompanhar a tarefa.","url":format!("/m/threads/{}", delivery.session),"tag":format!("dcc-{}-{}",delivery.session,delivery.kind)}).to_string();
    let mut signature = VapidSignatureBuilder::from_base64(&delivery.key, &delivery.subscription)
        .map_err(|e| e.to_string())?;
    signature.add_claim("sub", "https://github.com/wharley/DevCommandCenter");
    let mut builder = WebPushMessageBuilder::new(&delivery.subscription);
    builder.set_vapid_signature(signature.build().map_err(|e| e.to_string())?);
    builder.set_payload(ContentEncoding::Aes128Gcm, payload.as_bytes());
    builder.set_ttl(3600);
    let request = web_push::request_builder::build_request::<reqwest::Body>(
        builder.build().map_err(|e| e.to_string())?,
    );
    let (parts, body) = request.into_parts();
    let response = client
        .post(parts.uri.to_string())
        .headers(parts.headers)
        .body(body)
        .send()
        .await
        .map_err(|_| "Serviço push indisponível".to_owned())?;
    Ok(response.status().as_u16())
}

pub(super) fn start(config: Arc<RwLock<HttpConfig>>) {
    tokio::spawn(async move {
        let Ok(client) = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
        else {
            return;
        };
        loop {
            if let Ok(deliveries) = pending(config.clone()).await {
                for delivery in deliveries {
                    let status = deliver(&client, &delivery).await;
                    let _ = db_read(config.clone(), move |conn| {
                        (|| -> Result<(), rusqlite::Error> {
                        match status {
                            Ok(200..=299) => { conn.execute("UPDATE mobile_push_subscriptions SET cursor=?2,failures=0,next_try=0 WHERE device_id=?1", rusqlite::params![delivery.device,delivery.seq])?; }
                            Ok(404 | 410) => { conn.execute("DELETE FROM mobile_push_subscriptions WHERE device_id=?1", [delivery.device])?; }
                            _ => { conn.execute("UPDATE mobile_push_subscriptions SET failures=MIN(failures+1,6),next_try=unixepoch()+MIN(300,5*(1 << MIN(failures,6))) WHERE device_id=?1", [delivery.device])?; }
                        }
                        Ok(())
                        })().map_err(|e| e.to_string())
                    }).await;
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_browser_push_services_are_accepted() {
        for endpoint in [
            "https://web.push.apple.com/Q123",
            "https://fcm.googleapis.com/fcm/send/abc",
            "https://updates.push.services.mozilla.com/wpush/v2/abc",
        ] {
            assert!(validate_endpoint(endpoint).is_ok());
        }
        for endpoint in [
            "http://fcm.googleapis.com/test",
            "https://localhost/test",
            "https://127.0.0.1/test",
            "https://fcm.googleapis.com.attacker.example/test",
            "https://user@web.push.apple.com/test",
            "https://web.push.apple.com:444/test",
        ] {
            assert!(validate_endpoint(endpoint).is_err());
        }
    }
    #[test]
    fn vapid_identity_is_stable_and_usable() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let first = private_key(&conn).unwrap();
        assert_eq!(first, private_key(&conn).unwrap());
        assert!(VapidSignatureBuilder::from_base64_no_sub(&first).is_ok());
    }
    #[test]
    fn outbox_observes_only_actionable_durable_events() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE dcc_session_events(event_id TEXT,session_id TEXT,kind_json TEXT)",
        )
        .unwrap();
        schema(&conn).unwrap();
        for (id, kind) in [
            ("1", "turn_delta"),
            ("2", "turn_completed"),
            ("3", "turn_permission_requested"),
            ("4", "turn_user_input_requested"),
            ("5", "turn_aborted"),
        ] {
            conn.execute(
                "INSERT INTO dcc_session_events VALUES(?1,'session',?2)",
                rusqlite::params![id, json!({"type":kind}).to_string()],
            )
            .unwrap();
        }
        let count: i64 = conn
            .query_row("SELECT count(*) FROM mobile_push_outbox", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 4);
    }
    #[tokio::test]
    async fn revoked_device_is_removed_before_delivery() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("push.sqlite");
        let conn = rusqlite::Connection::open(&db).unwrap();
        crate::pairing::ensure_pairing_schema(&conn).unwrap();
        schema(&conn).unwrap();
        conn.execute("INSERT INTO mobile_push_subscriptions(device_id,subscription,cursor) VALUES('missing-device','{}',0)", []).unwrap();
        assert!(pending(Arc::new(RwLock::new(HttpConfig {
            db_path: db,
            ..Default::default()
        })))
        .await
        .unwrap()
        .is_empty());
        let count: i64 = conn
            .query_row("SELECT count(*) FROM mobile_push_subscriptions", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }
}
