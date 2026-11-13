//! The machines this DCC runs tasks on, for Settings' "Conexões": the
//! paired ones, pairing from a machine's link, forgetting one, and a
//! machine's own card (name, system, version), read through its channel —
//! which is also how the app learns it is online.

use serde::Deserialize;
use serde_json::{Value, json};

use super::desktop_api::{ApiError, CreateTaskRequest, CreatedTask, DesktopApi, encode};
use super::tasks::Project;

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

/// The paired machines, and the code the person on a machine compares
/// with the one this computer shows before approving it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MachineList {
    pub machines: Vec<Machine>,
    pub this_computer: String,
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

/// How a mirrored task's paths start (`dcc_server::machine_mirror`).
const MACHINE_PATH_SCHEME: &str = "dcc-machine://";

/// The machine a task's path is on; none for a task of this computer.
pub fn machine_of_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix(MACHINE_PATH_SCHEME)?;
    let id = rest.split('/').next().filter(|id| !id.is_empty())?;
    Some(id.to_string())
}

/// A row's project name, saying so when the task runs on a machine:
/// "vendeagora-app · em outra máquina".
pub fn with_place(project: Option<String>, path: &str, elsewhere: &str) -> Option<String> {
    if machine_of_path(path).is_none() {
        return project;
    }
    Some(match project {
        Some(project) => format!("{project} · {elsewhere}"),
        None => elsewhere.to_string(),
    })
}

/// The API a task's calls go to: its machine's through this computer's
/// server, or none for a local task (each call then loads the local one).
pub fn api_for_path(path: Option<&str>) -> Option<DesktopApi> {
    let machine = machine_of_path(path?)?;
    DesktopApi::load().map(|api| api.on_machine(&machine))
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
}

impl DesktopApi {
    pub fn machines(&self) -> Result<Vec<Machine>, ApiError> {
        self.machine_list().map(|list| list.machines)
    }

    /// The machines and this computer's code for pairing with them.
    pub fn machine_list(&self) -> Result<MachineList, ApiError> {
        decode(self.get("/api/v1/machines")?)
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

    /// The projects a machine offers for a new task (its catalog).
    pub fn machine_projects(&self, id: &str) -> Result<Vec<Project>, ApiError> {
        let catalog = self.on_machine(id).get("/api/v1/mobile/catalog")?;
        let repositories: Vec<dcc_core::domain::repository::Repository> = decode(
            catalog
                .get("repositories")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
        )?;
        Ok(repositories.iter().map(Project::from_repository).collect())
    }

    /// "Nova tarefa" on a machine: created there and mirrored here, so it
    /// is in this app's list when this answers.
    pub fn create_machine_task(
        &self,
        id: &str,
        request: &CreateTaskRequest,
    ) -> Result<CreatedTask, ApiError> {
        let value = self.post(
            &format!("/api/v1/machines/{}/tasks", encode(id)),
            &request.body(),
        )?;
        decode(value)
    }

    /// Follows the mirrored tasks again (their machines' streams); cheap
    /// when there is none or they are already followed.
    pub fn follow_machine_tasks(&self) -> Result<(), ApiError> {
        self.post("/api/v1/machines/tasks/follow", &json!({}))
            .map(|_| ())
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
    fn a_tasks_machine_is_read_from_its_path() {
        assert_eq!(
            machine_of_path("dcc-machine://m-1/Users/gio/app").as_deref(),
            Some("m-1")
        );
        assert_eq!(machine_of_path("/Users/me/app"), None);
        assert_eq!(machine_of_path("dcc-machine:///x"), None);
        assert!(api_for_path(Some("/Users/me/app")).is_none());
        assert_eq!(
            with_place(
                Some("app".into()),
                "dcc-machine://m-1/srv/app",
                "em outra máquina"
            ),
            Some("app · em outra máquina".into())
        );
        assert_eq!(
            with_place(Some("app".into()), "/Users/me/app", "em outra máquina"),
            Some("app".into())
        );
        assert!(api_for_path(None).is_none());
    }

    #[test]
    fn this_computers_provider_account_never_goes_to_a_machine() {
        let (port, server) = crate::test_support::fake_desktop("{}".into());
        let body = json!({ "workspaceId": "w-1", "providerRuntime": { "configDir": "/Users/me/.claude-zedy" } });
        DesktopApi::new(port, "k")
            .on_machine("m-1")
            .post("/api/v1/sessions/start", &body)
            .unwrap();
        let (request, sent) = server.join().unwrap();
        assert_eq!(
            request,
            "POST /api/v1/machines/m-1/proxy/api/v1/sessions/start"
        );
        let sent: Value = serde_json::from_str(&sent).unwrap();
        assert_eq!(sent, json!({ "workspaceId": "w-1" }));
        // A local call keeps it.
        let (port, server) = crate::test_support::fake_desktop("{}".into());
        DesktopApi::new(port, "k")
            .post("/api/v1/sessions/start", &body)
            .unwrap();
        let sent: Value = serde_json::from_str(&server.join().unwrap().1).unwrap();
        assert!(sent.get("providerRuntime").is_some());
    }

    #[test]
    fn a_machines_calls_go_through_its_proxy() {
        let api = DesktopApi::new(9876, "k");
        let machine = api.on_machine("m 1");
        assert_eq!(machine.machine(), Some("m 1"));
        assert_eq!(api.machine(), None);
    }
}
