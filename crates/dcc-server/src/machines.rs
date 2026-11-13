//! The machines this DCC can run tasks on: other DCCs it paired with
//! through the relay. Each is kept with the room and the key pinned when
//! pairing (`linked_machines`), beside this computer's own key for those
//! channels (`machine_link_identity`), in the app's database. A machine's
//! channel (`machine_link`) opens on first use and is kept while it lives.
//!
//! This is the side that reaches out. The side that answers is
//! `remote_connector`, where this computer shows among the paired devices
//! of the machine, as a desktop.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use dcc_remote::noise::{StaticKeypair, KEY_LEN};
use dcc_remote::offer::{fingerprint, PairingOffer};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::machine_link::{
    LinkError, LinkIdentity, LinkResponse, MachineLink, MachineTarget, StreamItem,
};

#[derive(Debug, thiserror::Error)]
pub enum MachineError {
    #[error("database error: {0}")]
    Database(String),
    #[error("that is not a DCC pairing link")]
    BadLink,
    #[error("no such machine")]
    Unknown,
    #[error(transparent)]
    Link(#[from] LinkError),
}

impl From<rusqlite::Error> for MachineError {
    fn from(error: rusqlite::Error) -> Self {
        MachineError::Database(error.to_string())
    }
}

/// A paired machine as the app lists it (no key material).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineSummary {
    pub id: String,
    pub name: String,
    /// The machine's key as the person compares it on both screens.
    pub fingerprint: String,
    pub created_at: String,
    /// Its channel is open now.
    pub connected: bool,
}

struct Machine {
    id: String,
    name: String,
    target: MachineTarget,
    created_at: String,
}

fn ensure_schema(conn: &Connection) -> Result<(), MachineError> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS linked_machines (
            id           TEXT PRIMARY KEY,
            name         TEXT NOT NULL,
            relay        TEXT NOT NULL,
            room         TEXT NOT NULL,
            public_key   BLOB NOT NULL,
            device_id    TEXT NOT NULL,
            created_at   TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS machine_link_identity (
            id           INTEGER PRIMARY KEY CHECK (id = 1),
            private_key  BLOB NOT NULL,
            public_key   BLOB NOT NULL
        );
        "#,
    )?;
    Ok(())
}

fn open(path: &Path) -> Result<Connection, MachineError> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    ensure_schema(&conn)?;
    Ok(conn)
}

/// This computer's key for its channels to machines, made once.
fn identity_keys(conn: &Connection) -> Result<StaticKeypair, MachineError> {
    let stored: Option<(Vec<u8>, Vec<u8>)> = conn
        .query_row(
            "SELECT private_key, public_key FROM machine_link_identity WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((private, public)) = stored {
        return StaticKeypair::from_parts(&private, &public)
            .map_err(|error| MachineError::Database(error.to_string()));
    }
    let keys =
        StaticKeypair::generate().map_err(|error| MachineError::Database(error.to_string()))?;
    conn.execute(
        "INSERT INTO machine_link_identity (id, private_key, public_key) VALUES (1, ?1, ?2)",
        params![keys.private.as_slice(), keys.public.as_slice()],
    )?;
    Ok(keys)
}

fn read_machine(row: &rusqlite::Row<'_>) -> rusqlite::Result<Machine> {
    let key: Vec<u8> = row.get(4)?;
    Ok(Machine {
        id: row.get(0)?,
        name: row.get(1)?,
        target: MachineTarget {
            relay: row.get(2)?,
            room: row.get(3)?,
            // A row with another length never connects.
            public_key: <[u8; KEY_LEN]>::try_from(key.as_slice()).unwrap_or([0; KEY_LEN]),
        },
        created_at: row.get(5)?,
    })
}

const MACHINE_COLUMNS: &str = "id, name, relay, room, public_key, created_at";

fn stored_machines(conn: &Connection) -> Result<Vec<Machine>, MachineError> {
    let mut statement = conn.prepare(&format!(
        "SELECT {MACHINE_COLUMNS} FROM linked_machines ORDER BY created_at, id"
    ))?;
    let machines = statement
        .query_map([], read_machine)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(machines)
}

fn stored_machine(conn: &Connection, id: &str) -> Result<Option<Machine>, MachineError> {
    Ok(conn
        .query_row(
            &format!("SELECT {MACHINE_COLUMNS} FROM linked_machines WHERE id = ?1"),
            params![id],
            read_machine,
        )
        .optional()?)
}

/// This computer's name as a machine's person reads it when approving, and
/// as other DCCs list it: the host's name without its `.local`.
pub fn this_computer_name() -> String {
    let name = std::process::Command::new("hostname")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default();
    let name = name.trim_end_matches(".local").trim();
    if name.is_empty() {
        "DCC".to_string()
    } else {
        name.chars().take(80).collect()
    }
}

/// The system's name on the wire; what makes this side a desktop.
pub fn this_platform() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    }
}

/// The paired machines and their open channels.
pub struct MachineRegistry {
    db_path: PathBuf,
    links: tokio::sync::Mutex<HashMap<String, MachineLink>>,
    /// The mirrored tasks being followed now, by workspace id: one
    /// follower each.
    following: std::sync::Mutex<std::collections::HashSet<String>>,
}

impl MachineRegistry {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            links: tokio::sync::Mutex::new(HashMap::new()),
            following: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Takes the task to follow; false when it is already followed.
    pub fn start_following(&self, workspace_id: &str) -> bool {
        self.following
            .lock()
            .map(|mut following| following.insert(workspace_id.to_string()))
            .unwrap_or(false)
    }

    pub fn stop_following(&self, workspace_id: &str) {
        if let Ok(mut following) = self.following.lock() {
            following.remove(workspace_id);
        }
    }

    async fn with_db<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Connection) -> Result<T, MachineError> + Send + 'static,
    ) -> Result<T, MachineError> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || work(&open(&path)?))
            .await
            .map_err(|error| MachineError::Database(error.to_string()))?
    }

    async fn identity(&self) -> Result<LinkIdentity, MachineError> {
        let keys = self.with_db(identity_keys).await?;
        Ok(LinkIdentity {
            keys,
            name: this_computer_name(),
            platform: this_platform().to_string(),
        })
    }

    pub async fn list(&self) -> Result<Vec<MachineSummary>, MachineError> {
        let machines = self.with_db(stored_machines).await?;
        let links = self.links.lock().await;
        Ok(machines
            .into_iter()
            .map(|machine| MachineSummary {
                connected: links.get(&machine.id).is_some_and(MachineLink::is_open),
                fingerprint: fingerprint(&machine.target.public_key),
                id: machine.id,
                name: machine.name,
                created_at: machine.created_at,
            })
            .collect())
    }

    /// Pairs from the link a machine showed and waits for its person to
    /// approve this computer. `name` is how the app lists it; `on_pending`
    /// runs once the machine is asking them.
    pub async fn pair(
        &self,
        link: &str,
        name: Option<String>,
        on_pending: impl FnOnce() + Send,
    ) -> Result<MachineSummary, MachineError> {
        let offer = PairingOffer::parse(link.trim()).map_err(|_| MachineError::BadLink)?;
        let identity = self.identity().await?;
        let channel = MachineLink::pair(&offer, &identity, on_pending).await?;
        let print = fingerprint(&offer.desktop_public);
        let name = name
            .map(|name| name.trim().chars().take(80).collect::<String>())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("DCC {print}"));
        let id = uuid::Uuid::new_v4().to_string();
        let (row_id, row_name, device_id) =
            (id.clone(), name.clone(), channel.device_id().to_string());
        let (relay, room, key) = (
            offer.relay.clone(),
            offer.room.clone(),
            offer.desktop_public.to_vec(),
        );
        let (id, created_at) = self
            .with_db(move |conn| {
                // Pairing again with the same machine replaces its row.
                let known: Option<String> = conn
                    .query_row(
                        "SELECT id FROM linked_machines WHERE public_key = ?1",
                        params![key],
                        |row| row.get(0),
                    )
                    .optional()?;
                let id = known.unwrap_or(row_id);
                conn.execute(
                    "INSERT INTO linked_machines (id, name, relay, room, public_key, device_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(id) DO UPDATE SET name = excluded.name, relay = excluded.relay,
                         room = excluded.room, device_id = excluded.device_id",
                    params![id, row_name, relay, room, key, device_id],
                )?;
                let created_at: String = conn.query_row(
                    "SELECT created_at FROM linked_machines WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )?;
                Ok((id, created_at))
            })
            .await?;
        self.links.lock().await.insert(id.clone(), channel);
        Ok(MachineSummary {
            id,
            name,
            fingerprint: print,
            created_at,
            connected: true,
        })
    }

    /// Forgets a machine and closes its channel. The machine still lists
    /// this computer until its person revokes it there.
    pub async fn remove(&self, id: &str) -> Result<bool, MachineError> {
        self.links.lock().await.remove(id);
        let id = id.to_string();
        self.with_db(move |conn| {
            Ok(conn.execute("DELETE FROM linked_machines WHERE id = ?1", params![id])? > 0)
        })
        .await
    }

    /// The machine's channel, opened when there is none or it broke.
    async fn link(&self, id: &str) -> Result<MachineLink, MachineError> {
        let mut links = self.links.lock().await;
        if let Some(open) = links.get(id).filter(|link| link.is_open()) {
            return Ok(open.clone());
        }
        let wanted = id.to_string();
        let machine = self
            .with_db(move |conn| stored_machine(conn, &wanted))
            .await?
            .ok_or(MachineError::Unknown)?;
        let identity = self.identity().await?;
        let channel = MachineLink::connect(&machine.target, &identity).await?;
        links.insert(id.to_string(), channel.clone());
        Ok(channel)
    }

    /// One call of the machine's API.
    pub async fn request(
        &self,
        id: &str,
        method: &str,
        path: &str,
        headers: BTreeMap<String, String>,
        body: Vec<u8>,
    ) -> Result<LinkResponse, MachineError> {
        let link = self.link(id).await?;
        Ok(link.request(method, path, headers, body).await?)
    }
}

impl MachineRegistry {
    /// One of the machine's event streams, on its channel.
    pub async fn subscribe(
        &self,
        id: &str,
        path: &str,
    ) -> Result<tokio::sync::mpsc::UnboundedReceiver<StreamItem>, MachineError> {
        let link = self.link(id).await?;
        Ok(link.subscribe(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_computers_key_is_made_once_and_machines_are_listed_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.sqlite");
        let conn = open(&path).unwrap();
        let first = identity_keys(&conn).unwrap();
        let again = identity_keys(&open(&path).unwrap()).unwrap();
        assert_eq!(first.public, again.public);

        for (id, name, at) in [("b", "Servidor", "2026-11-02"), ("a", "Mac", "2026-11-01")] {
            conn.execute(
                "INSERT INTO linked_machines (id, name, relay, room, public_key, device_id, created_at)
                 VALUES (?1, ?2, 'wss://relay', 'room', ?3, 'd', ?4)",
                params![id, name, vec![9u8; KEY_LEN], at],
            )
            .unwrap();
        }
        let names: Vec<String> = stored_machines(&conn)
            .unwrap()
            .into_iter()
            .map(|machine| machine.name)
            .collect();
        assert_eq!(names, ["Mac", "Servidor"]);
        let machine = stored_machine(&conn, "b").unwrap().unwrap();
        assert_eq!(machine.target.public_key, [9u8; KEY_LEN]);
        assert!(stored_machine(&conn, "zz").unwrap().is_none());
    }

    #[tokio::test]
    async fn a_link_that_is_not_dccs_is_refused_before_anything_is_sent() {
        let dir = tempfile::tempdir().unwrap();
        let registry = MachineRegistry::new(dir.path().join("db.sqlite"));
        let refused = registry.pair("https://example.com", None, || {}).await;
        assert!(matches!(refused, Err(MachineError::BadLink)));
        assert!(registry.list().await.unwrap().is_empty());
        assert!(!registry.remove("nope").await.unwrap());
    }
}
