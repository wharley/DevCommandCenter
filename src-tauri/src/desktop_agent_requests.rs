//! The desktop's real agent-request queues behind the shared
//! `agent_requests` shape: the Browser-open broker and computer use.
//!
//! Every answer — the desktop dialogs, the native approval panel and a phone
//! through the HTTP API — goes through [`DesktopAgentRequests`], so the first
//! one wins on the same ledger and the queues keep their own checks (lease,
//! expiry, the Browser's native lifecycle ACK, the computer-use grant).
//!
//! Allowing a Browser open claims the request and asks the main window to
//! open the Browser (`browser-agent-open-approved`); the renderer completes
//! it with the lifecycle ACK as before. Allowing computer use arms the grant
//! for the picked apps, as the desktop dialog does.
use std::sync::Arc;

use async_trait::async_trait;
use dev_command_center_tauri::agent_requests::{
    now_ms, AgentRequestAnswer, AgentRequestApp, AgentRequestDecision, AgentRequestDetail,
    AgentRequestError, AgentRequestOutcome, AgentRequestSource, AnswerLedger, AnswerOrigin,
    PendingAgentRequest,
};
use tauri::{AppHandle, Emitter, State};

use crate::browser_agent_requests::{
    BrowserAgentOpenStatus, BrowserAgentRequestBroker, BrowserAgentRequestDecision,
    BrowserAgentRequestView, BrowserAgentResolveInput,
};
use crate::browser_commands::BrowserState;
use crate::computer_use_commands::{ComputerUseRespondControlRequestOutput, ComputerUseState};

pub const BROWSER_OPEN_APPROVED_EVENT: &str = "browser-agent-open-approved";

#[derive(Clone)]
pub struct DesktopAgentRequests {
    app: AppHandle,
    broker: BrowserAgentRequestBroker,
    browser: BrowserState,
    computer: ComputerUseState,
    ledger: Arc<AnswerLedger>,
}

fn decision(allow: bool) -> AgentRequestDecision {
    if allow {
        AgentRequestDecision::Allowed
    } else {
        AgentRequestDecision::Denied
    }
}

fn browser_request(view: BrowserAgentRequestView) -> PendingAgentRequest {
    PendingAgentRequest {
        request_id: view.request_id,
        session_id: view.session_id,
        workspace_id: view.workspace_id,
        provider_id: view.provider_id,
        reason: view.reason,
        expires_at_ms: view.expires_at_ms,
        detail: AgentRequestDetail::BrowserOpen { url: view.url },
    }
}

impl DesktopAgentRequests {
    pub fn new(
        app: AppHandle,
        broker: BrowserAgentRequestBroker,
        browser: BrowserState,
        computer: ComputerUseState,
    ) -> Self {
        Self {
            app,
            broker,
            browser,
            computer,
            ledger: Arc::new(AnswerLedger::default()),
        }
    }

    fn is_computer_request(&self, request_id: &str) -> bool {
        self.computer
            .pending_control_requests()
            .requests
            .iter()
            .any(|request| request.request_id == request_id)
    }

    fn answered_or_gone(&self, request_id: &str) -> AgentRequestError {
        match self.ledger.outcome(request_id) {
            Some(outcome) => AgentRequestError::AlreadyAnswered(outcome),
            None => AgentRequestError::NotFound,
        }
    }

    /// Allows a Browser open: claims it so no one else can answer and
    /// returns it for the renderer to open. `notify` asks the main window to
    /// open it (the panel and a phone); the in-app dialog opens it itself.
    pub fn allow_browser(
        &self,
        request_id: &str,
        by: AnswerOrigin,
        notify: bool,
    ) -> Result<(BrowserAgentRequestView, AgentRequestOutcome), AgentRequestError> {
        let claim = self
            .ledger
            .claim(request_id, AgentRequestDecision::Allowed, by)
            .map_err(AgentRequestError::AlreadyAnswered)?;
        let view = self.broker.claim(request_id).ok_or(AgentRequestError::NotFound)?;
        let outcome = claim.settle();
        if notify {
            let _ = self
                .app
                .emit_to("main", BROWSER_OPEN_APPROVED_EVENT, &view);
        }
        Ok((view, outcome))
    }

    pub fn deny_browser(
        &self,
        request_id: &str,
        by: AnswerOrigin,
    ) -> Result<AgentRequestOutcome, AgentRequestError> {
        let claim = self
            .ledger
            .claim(request_id, AgentRequestDecision::Denied, by)
            .map_err(AgentRequestError::AlreadyAnswered)?;
        let view = self.broker.view(request_id).ok_or(AgentRequestError::NotFound)?;
        let result = self.broker.resolve(
            &self.browser,
            BrowserAgentResolveInput {
                request_id: view.request_id,
                workspace_id: view.workspace_id,
                session_id: view.session_id,
                decision: BrowserAgentRequestDecision::Deny,
                lifecycle_token: None,
            },
        );
        match result.status {
            BrowserAgentOpenStatus::Denied => Ok(claim.settle()),
            // Expired or cancelled while we answered: nothing was decided.
            _ => Err(AgentRequestError::NotFound),
        }
    }

    /// The desktop dialog's and the phone's computer-use answer.
    pub async fn respond_computer(
        &self,
        request_id: &str,
        allowed: bool,
        allowed_bundle_ids: &[String],
        by: AnswerOrigin,
    ) -> Result<(ComputerUseRespondControlRequestOutput, AgentRequestOutcome), AgentRequestError>
    {
        if allowed && allowed_bundle_ids.is_empty() {
            return Err(AgentRequestError::Invalid(
                "Escolha pelo menos um app que o agente pode controlar.".into(),
            ));
        }
        let claim = self
            .ledger
            .claim(request_id, decision(allowed), by)
            .map_err(AgentRequestError::AlreadyAnswered)?;
        if !self.is_computer_request(request_id) {
            return Err(AgentRequestError::NotFound);
        }
        match self
            .computer
            .respond_control_request(request_id, allowed, allowed_bundle_ids)
            .await
        {
            Ok(output) => {
                if allowed {
                    if let Some(session_id) = output.grant.session_id.as_deref() {
                        let _ = self.app.emit("computer-use-preview-cleared", session_id);
                    }
                }
                Ok((output, claim.settle()))
            }
            Err(message) if message.contains("no longer available") || message.contains("timed out") => {
                Err(AgentRequestError::NotFound)
            }
            Err(message) => Err(AgentRequestError::Failed(message)),
        }
    }

    fn computer_requests(&self) -> Vec<PendingAgentRequest> {
        let now = now_ms();
        self.computer
            .pending_control_requests()
            .requests
            .into_iter()
            .map(|request| PendingAgentRequest {
                request_id: request.request_id,
                session_id: request.session_id,
                workspace_id: request.workspace_id,
                provider_id: request.provider_id,
                reason: request.reason,
                expires_at_ms: now.saturating_add(request.remaining_ms),
                detail: AgentRequestDetail::ComputerControl {
                    apps: Vec::new(),
                    blocker: None,
                },
            })
            .collect()
    }
}

fn sorted(mut requests: Vec<PendingAgentRequest>) -> Vec<PendingAgentRequest> {
    requests.sort_by(|a, b| (a.expires_at_ms, &a.request_id).cmp(&(b.expires_at_ms, &b.request_id)));
    requests
}

#[async_trait]
impl AgentRequestSource for DesktopAgentRequests {
    fn pending(&self) -> Vec<PendingAgentRequest> {
        let mut requests: Vec<_> = self
            .broker
            .pending_all()
            .into_iter()
            .map(browser_request)
            .collect();
        requests.extend(self.computer_requests());
        sorted(requests)
    }

    async fn pending_detailed(&self) -> Vec<PendingAgentRequest> {
        let mut requests = self.pending();
        for request in &mut requests {
            let AgentRequestDetail::ComputerControl { apps, blocker } = &mut request.detail else {
                continue;
            };
            let computer = self.computer.clone();
            let session_id = request.session_id.clone();
            // Lists the Mac's windows: off the async threads.
            let Ok(status) =
                tokio::task::spawn_blocking(move || computer.status(Some(&session_id), true)).await
            else {
                *blocker = Some("Não deu para ler o estado do computador.".into());
                continue;
            };
            for target in status.targets {
                if !apps.iter().any(|app| app.bundle_id == target.bundle_id) {
                    apps.push(AgentRequestApp {
                        bundle_id: target.bundle_id,
                        name: target.name,
                    });
                }
            }
            *blocker = if !status.supported {
                Some("O controle do computador não funciona neste Mac.".into())
            } else if !status.runtime_attached {
                Some("A conversa ainda não está pronta no computador.".into())
            } else if !status.accessibility.granted || !status.screen_recording.granted {
                Some("Conceda Acessibilidade e Gravação de tela ao DCC no Mac.".into())
            } else if apps.is_empty() {
                Some("Nenhum app com janela aberta no Mac.".into())
            } else {
                None
            };
        }
        requests
    }

    async fn respond(
        &self,
        request_id: &str,
        answer: AgentRequestAnswer,
        by: AnswerOrigin,
    ) -> Result<AgentRequestOutcome, AgentRequestError> {
        if self.broker.view(request_id).is_some() {
            return if answer.allow {
                self.allow_browser(request_id, by, true).map(|(_, outcome)| outcome)
            } else {
                self.deny_browser(request_id, by)
            };
        }
        if self.is_computer_request(request_id) {
            return self
                .respond_computer(request_id, answer.allow, &answer.allowed_bundle_ids, by)
                .await
                .map(|(_, outcome)| outcome);
        }
        Err(self.answered_or_gone(request_id))
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserAgentClaimInput {
    pub request_id: String,
    pub workspace_id: String,
    pub session_id: String,
}

/// The in-app dialog's "Permitir": claims the request before the renderer
/// opens the Browser, so a phone answering at the same time loses.
#[tauri::command]
pub fn browser_agent_claim(
    requests: State<'_, DesktopAgentRequests>,
    input: BrowserAgentClaimInput,
) -> Result<BrowserAgentRequestView, String> {
    let matches = requests.broker.view(&input.request_id).is_some_and(|view| {
        view.workspace_id == input.workspace_id && view.session_id == input.session_id
    });
    if !matches {
        return Err(requests.answered_or_gone(&input.request_id).to_string());
    }
    requests
        .allow_browser(&input.request_id, AnswerOrigin::Desktop, false)
        .map(|(view, _)| view)
        .map_err(|error| error.to_string())
}
