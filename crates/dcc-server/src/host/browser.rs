//! The Browser's state for the native app's Browser
//! (`http_api::browser_bridge`): its lifecycle reports, the agent's control,
//! evidence, the audit and the open requests, on the host's own state.

use serde_json::Value;

use dcc_tauri::state::SessionCommandState;

#[derive(Clone)]
pub struct HostBrowser {
    pub browser: crate::browser_commands::BrowserState,
    pub sessions: SessionCommandState,
    pub broker: crate::browser_agent_requests::BrowserAgentRequestBroker,
    pub requests: crate::desktop_agent_requests::DesktopAgentRequests,
}

impl crate::http_api::DesktopBrowser for HostBrowser {
    fn open(
        &self,
        workspace_id: String,
        session_id: Option<String>,
        url: String,
    ) -> crate::http_api::BrowserFuture<u64> {
        let this = self.clone();
        Box::pin(async move {
            let state = this.browser.clone();
            let sessions = this.sessions.clone();
            crate::browser_commands::remote_browser_open(
                &state,
                &sessions,
                &workspace_id,
                session_id.as_deref(),
                &url,
            )
            .await
        })
    }

    fn report(
        &self,
        lifecycle_token: u64,
        report: crate::http_api::PageReport,
    ) -> crate::http_api::BrowserFuture<()> {
        let this = self.clone();
        Box::pin(async move {
            let state = this.browser.clone();
            crate::browser_commands::remote_browser_report(&state, lifecycle_token, report).await
        })
    }

    fn close(&self, lifecycle_token: u64) -> crate::http_api::BrowserFuture<()> {
        let this = self.clone();
        Box::pin(async move {
            let state = this.browser.clone();
            crate::browser_commands::remote_browser_close(&state, Some(lifecycle_token)).await
        })
    }

    fn disconnected(&self) -> crate::http_api::BrowserFuture<()> {
        let this = self.clone();
        Box::pin(async move {
            let state = this.browser.clone();
            crate::browser_commands::remote_browser_close(&state, None).await
        })
    }

    fn control(
        &self,
        workspace_id: String,
        session_id: Option<String>,
        lifecycle_token: u64,
        armed: Option<bool>,
    ) -> crate::http_api::BrowserFuture<Value> {
        let this = self.clone();
        Box::pin(async move {
            let status = match armed {
                Some(true) => {
                    crate::browser_commands::browser_arm_control(
                        &this.browser,
                        &this.sessions,
                        workspace_id,
                        session_id,
                        lifecycle_token,
                    )
                    .await?
                }
                Some(false) => {
                    crate::browser_commands::browser_disarm_control(
                        &this.browser,
                        &this.sessions,
                        &this.broker,
                        workspace_id,
                        session_id,
                        lifecycle_token,
                    )
                    .await?
                }
                None => {
                    crate::browser_commands::browser_control_status(
                        &this.browser,
                        &this.sessions,
                        workspace_id,
                        session_id,
                        lifecycle_token,
                    )
                    .await?
                }
            };
            serde_json::to_value(status).map_err(|error| error.to_string())
        })
    }

    fn evidence_start(
        &self,
        workspace_id: String,
        session_id: Option<String>,
        lifecycle_token: u64,
    ) -> crate::http_api::BrowserFuture<Value> {
        let this = self.clone();
        Box::pin(async move {
            // What the WebView's menu does: read the page, anchor on it.
            let context = crate::browser_commands::browser_extract_context(
                &this.browser,
                &this.sessions,
                workspace_id,
                session_id,
                lifecycle_token,
            )
            .await?;
            let handle = crate::browser_commands::browser_start_evidence_capture(
                &this.browser,
                &this.sessions,
                crate::browser_commands::BrowserActionAnchor {
                    workspace_id: context.workspace_id.clone(),
                    session_id: context.session_id.clone(),
                    lifecycle_token,
                    map_id: context.semantic_map.map_id.clone(),
                    generation: context.semantic_map.generation,
                    url: context.url.clone(),
                    page_load_revision: context.semantic_map.page_load_revision,
                },
            )
            .await?;
            Ok(serde_json::json!({
                "captureId": handle.capture_id,
                "remainingMs": handle.remaining_ms,
                "url": context.url,
                "title": context.title,
                "text": context.text,
                "selectionOnly": context.selection_only,
                "truncated": context.truncated,
            }))
        })
    }

    fn evidence_read(
        &self,
        workspace_id: String,
        session_id: Option<String>,
        capture_id: String,
    ) -> crate::http_api::BrowserFuture<Value> {
        let this = self.clone();
        Box::pin(async move {
            let evidence = crate::browser_commands::browser_read_evidence_capture(
                &this.browser,
                &this.sessions,
                workspace_id,
                session_id,
                capture_id,
            )
            .await?;
            serde_json::to_value(evidence).map_err(|error| error.to_string())
        })
    }

    fn audit(
        &self,
        workspace_id: String,
        session_id: Option<String>,
        lifecycle_token: u64,
        limit: usize,
    ) -> crate::http_api::BrowserFuture<Value> {
        let this = self.clone();
        Box::pin(async move {
            let records = crate::browser_commands::browser_read_audit(
                &this.browser,
                &this.sessions,
                workspace_id,
                session_id,
                lifecycle_token,
                limit,
            )
            .await?;
            serde_json::to_value(records).map_err(|error| error.to_string())
        })
    }

    fn open_requests(&self) -> Value {
        let broker = &self.broker;
        serde_json::to_value(broker.pending_all()).unwrap_or_else(|_| Value::Array(Vec::new()))
    }

    fn open_allow(
        &self,
        request_id: String,
        workspace_id: String,
        session_id: String,
    ) -> crate::http_api::BrowserFuture<Value> {
        let this = self.clone();
        Box::pin(async move {
            let view = crate::desktop_agent_requests::browser_agent_claim(
                &this.requests,
                crate::desktop_agent_requests::BrowserAgentClaimInput {
                    request_id,
                    workspace_id,
                    session_id,
                },
            )?;
            serde_json::to_value(view).map_err(|error| error.to_string())
        })
    }

    fn open_deny(
        &self,
        request_id: String,
        workspace_id: String,
        session_id: String,
    ) -> crate::http_api::BrowserFuture<()> {
        let this = self.clone();
        Box::pin(async move {
            let result = crate::browser_agent_requests::browser_agent_resolve(
                &this.broker,
                &this.browser,
                &this.requests,
                crate::browser_agent_requests::BrowserAgentResolveInput {
                    request_id,
                    workspace_id,
                    session_id,
                    decision: crate::browser_agent_requests::BrowserAgentRequestDecision::Deny,
                    lifecycle_token: None,
                },
            );
            match result.status {
                crate::browser_agent_requests::BrowserAgentOpenStatus::Denied => Ok(()),
                _ => Err(result
                    .message
                    .unwrap_or_else(|| "browser open request is unavailable".into())),
            }
        })
    }

    fn open_ack(
        &self,
        request_id: String,
        workspace_id: String,
        session_id: String,
        lifecycle_token: u64,
    ) -> crate::http_api::BrowserFuture<Value> {
        let this = self.clone();
        Box::pin(async move {
            let result = crate::browser_agent_requests::browser_agent_resolve(
                &this.broker,
                &this.browser,
                &this.requests,
                crate::browser_agent_requests::BrowserAgentResolveInput {
                    request_id,
                    workspace_id,
                    session_id,
                    decision: crate::browser_agent_requests::BrowserAgentRequestDecision::Allow,
                    lifecycle_token: Some(lifecycle_token),
                },
            );
            // `acknowledgeBrowserOpen`
            if result.status != crate::browser_agent_requests::BrowserAgentOpenStatus::Approved
                || result.lifecycle_token != Some(lifecycle_token)
            {
                return Err(result
                    .message
                    .unwrap_or_else(|| "Browser approval could not be confirmed".into()));
            }
            serde_json::to_value(result).map_err(|error| error.to_string())
        })
    }
}
