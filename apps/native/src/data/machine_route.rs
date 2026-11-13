//! The desktop API of another machine, reached through the relay
//! (`dcc_server::machine_link`): what a `DesktopApi` uses instead of the
//! loopback socket when the task it serves runs on that machine. A call is
//! the same request, sent as a frame over the pinned, encrypted channel.

use std::collections::BTreeMap;
use std::time::Duration;

use dcc_server::machine_link::{LinkError, MachineLink};
use serde_json::Value;

use super::desktop_api::ApiError;

#[derive(Clone)]
pub struct MachineRoute {
    /// The machine's id in this app's list; two routes to it are the same.
    pub id: String,
    link: MachineLink,
    /// Where the link's socket runs; calls wait on it from other threads.
    runtime: tokio::runtime::Handle,
}

impl std::fmt::Debug for MachineRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MachineRoute")
            .field("id", &self.id)
            .finish()
    }
}

impl PartialEq for MachineRoute {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for MachineRoute {}

impl MachineRoute {
    pub fn new(id: impl Into<String>, link: MachineLink, runtime: tokio::runtime::Handle) -> Self {
        Self {
            id: id.into(),
            link,
            runtime,
        }
    }

    /// False once the machine left: the app shows it offline and reconnects.
    pub fn is_open(&self) -> bool {
        self.link.is_open()
    }

    /// One call, waiting at most `wait` for its answer: the status and the
    /// body, as the loopback socket gives them.
    pub(crate) fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        wait: Duration,
    ) -> Result<(u16, Vec<u8>), ApiError> {
        let mut headers = BTreeMap::new();
        headers.insert("accept".to_string(), "application/json".to_string());
        let payload = match body {
            Some(body) => {
                headers.insert("content-type".to_string(), "application/json".to_string());
                body.to_string().into_bytes()
            }
            None => Vec::new(),
        };
        let (link, method, path) = (self.link.clone(), method.to_string(), path.to_string());
        let (answered, answer) = std::sync::mpsc::channel();
        self.runtime.spawn(async move {
            let _ = answered.send(link.request(&method, &path, headers, payload).await);
        });
        match answer.recv_timeout(wait) {
            Ok(Ok(response)) => Ok((response.status, response.body)),
            Ok(Err(error)) => Err(link_error(error)),
            Err(_) => Err(ApiError::Failed(
                "the machine did not answer in time".into(),
            )),
        }
    }
}

/// A machine that left reads like a desktop that closed: the same "not
/// running" the app already knows how to show.
fn link_error(error: LinkError) -> ApiError {
    match error {
        LinkError::Closed | LinkError::Relay(_) | LinkError::Timeout => ApiError::DesktopClosed,
        LinkError::Refused(_) | LinkError::Denied => ApiError::Unauthorized,
        LinkError::Protocol(error) => ApiError::Failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_that_left_reads_as_a_closed_desktop() {
        assert_eq!(link_error(LinkError::Closed), ApiError::DesktopClosed);
        assert_eq!(link_error(LinkError::Timeout), ApiError::DesktopClosed);
        assert_eq!(link_error(LinkError::Refused(4499)), ApiError::Unauthorized);
        assert_eq!(link_error(LinkError::Denied), ApiError::Unauthorized);
    }
}
