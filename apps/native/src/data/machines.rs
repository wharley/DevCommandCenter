//! The machines this DCC runs tasks on, for Settings' "Conexões": the
//! paired ones, pairing from a machine's link, forgetting one, and a
//! machine's own card (name, system, version), read through its channel —
//! which is also how the app learns it is online.

use serde::Deserialize;
use serde_json::{Value, json};

use super::desktop_api::{ApiError, DesktopApi, encode};

/// The person on the machine has three minutes to approve; a little more
/// than that here, so its answer is the one that arrives.
const PAIR_WAIT: std::time::Duration = std::time::Duration::from_secs(200);

/// `MachineSummary`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Machine {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
    pub created_at: String,
    pub connected: bool,
}

/// What a machine says of itself (`/api/v1/machine`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MachineCard {
    pub name: String,
    pub platform: String,
    pub version: String,
}

impl MachineCard {
    /// "macOS · v0.3.6"
    pub fn label(&self) -> String {
        let system = match self.platform.as_str() {
            "macos" => "macOS",
            "linux" => "Linux",
            "windows" => "Windows",
            other => other,
        };
        if self.version.is_empty() {
            system.to_string()
        } else {
            format!("{system} · v{}", self.version)
        }
    }
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
}

impl DesktopApi {
    pub fn machines(&self) -> Result<Vec<Machine>, ApiError> {
        let value = self.get("/api/v1/machines")?;
        decode(
            value
                .get("machines")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
        )
    }

    /// Pairs from the link the machine showed; answers once the person
    /// there approved (or refused, or three minutes passed).
    pub fn pair_machine(&self, link: &str) -> Result<Machine, ApiError> {
        let value = self.request_waiting(
            "POST",
            "/api/v1/machines/pair",
            Some(&json!({ "link": link })),
            PAIR_WAIT,
        )?;
        decode(value.get("machine").cloned().unwrap_or(Value::Null))
    }

    pub fn remove_machine(&self, id: &str) -> Result<bool, ApiError> {
        let value = self.delete(&format!("/api/v1/machines/{}", encode(id)))?;
        Ok(value
            .get("removed")
            .and_then(Value::as_bool)
            .unwrap_or(false))
    }

    /// The machine's card, through its channel: an answer means it is on.
    pub fn machine_card(&self, id: &str) -> Result<MachineCard, ApiError> {
        decode(self.on_machine(id).get("/api/v1/machine")?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machines_card_reads_as_its_system_and_version() {
        let card = MachineCard {
            name: "servidor".into(),
            platform: "linux".into(),
            version: "0.3.6".into(),
        };
        assert_eq!(card.label(), "Linux · v0.3.6");
        assert_eq!(MachineCard::default().label(), "");
    }

    #[test]
    fn a_machines_calls_go_through_its_proxy() {
        let api = DesktopApi::new(9876, "k");
        let machine = api.on_machine("m 1");
        assert_eq!(machine.machine(), Some("m 1"));
        assert_eq!(api.machine(), None);
    }
}
