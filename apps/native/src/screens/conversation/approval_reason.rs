//! Two answers on a permission card besides allow and deny:
//! - "Recusar com motivo": the person writes why. Claude reads it as the
//!   tool's result; a provider that cannot take text with a denial gets it
//!   as the next message, queued for when its turn ends.
//! - "Sempre permitir": for a connector's tool (`mcp__dcc-…`), allowed now
//!   and, when the vendor marks it read-only, from then on; a tool that
//!   writes is allowed only this once, and the person is told.

use gpui::{AppContext, Context, Entity, Window};

use super::ConversationView;
use super::requests::Sending;
use crate::data::desktop_api::{DesktopApi, TurnOptions};
use crate::i18n::{Args, Translate};
use crate::ui::{TextInput, Toaster};

/// The providers that pass a denial's reason to the model themselves.
const READS_DENIAL_REASON: [&str; 1] = ["claude_code"];

/// Whether "Sempre permitir" fits the tool: a connector's, not DCC's own.
pub(crate) fn may_always_allow(tool_name: &str) -> bool {
    tool_name.starts_with("mcp__dcc-") && !tool_name.starts_with("mcp__dcc-browser-webview__")
}

/// A connector's tool by its own name (`slack_search_channels`), not the
/// long name the provider sees (`mcp__dcc-<id>__slack_search_channels`).
pub(crate) fn short_tool(tool_name: &str) -> &str {
    tool_name
        .strip_prefix("mcp__")
        .and_then(|rest| rest.split_once("__"))
        .map_or(tool_name, |(_, tool)| tool)
}

/// The next message that carries a reason to a provider that cannot read it
/// with the denial.
pub(crate) fn reason_message(tool: &str, reason: &str) -> String {
    format!("[DCC] I denied `{tool}`: {reason}")
}

impl ConversationView {
    pub(crate) fn reason_input(&self, request_id: &str) -> Option<Entity<TextInput>> {
        self.requests
            .reason
            .as_ref()
            .filter(|(id, _)| id == request_id)
            .map(|(_, input)| input.clone())
    }

    pub(crate) fn open_reason(
        &mut self,
        request_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder(cx.t("conversation.permission.reasonPlaceholder"))
                .with_max_len(500)
        });
        let handle = gpui::Focusable::focus_handle(input.read(cx), cx);
        window.focus(&handle, cx);
        self.requests.reason = Some((request_id, input));
        cx.notify();
    }

    fn endpoint(&self) -> Option<DesktopApi> {
        self.requests.endpoint.clone().or_else(DesktopApi::load)
    }

    /// Sends the denial with the reason written in its field.
    pub(crate) fn deny_with_reason(
        &mut self,
        request_id: String,
        tool: String,
        cx: &mut Context<Self>,
    ) {
        let Some(reason) = self
            .reason_input(&request_id)
            .map(|input| input.read(cx).text().trim().to_string())
            .filter(|reason| !reason.is_empty())
        else {
            return;
        };
        let session = self
            .data
            .as_ref()
            .and_then(|data| data.session_id.clone())
            .unwrap_or_default();
        let provider = self
            .data
            .as_ref()
            .and_then(|data| data.provider_id.clone())
            .unwrap_or_default();
        let queue = !READS_DENIAL_REASON.contains(&provider.as_str());
        let Some(api) = self.endpoint() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        self.requests
            .sending
            .insert(request_id.clone(), Sending::Deny);
        self.requests.reason = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let request = request_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    api.respond_permission_with(&session, &request, false, Some(&reason), false)?;
                    if queue {
                        api.queue_turn(
                            &session,
                            &reason_message(&tool, &reason),
                            &TurnOptions::default(),
                        )?;
                    }
                    Ok::<_, crate::data::desktop_api::ApiError>(())
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.requests.sending.remove(&request_id);
                if let Err(error) = result {
                    Toaster::error(error.to_string(), cx);
                }
                view.refresh_now(cx);
            });
        })
        .detach();
    }

    /// Allows now and, for a read-only connector tool, from then on.
    pub(crate) fn allow_always(
        &mut self,
        request_id: String,
        tool: String,
        cx: &mut Context<Self>,
    ) {
        let session = self
            .data
            .as_ref()
            .and_then(|data| data.session_id.clone())
            .unwrap_or_default();
        let Some(api) = self.endpoint() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        self.requests
            .sending
            .insert(request_id.clone(), Sending::Allow);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let request = request_id.clone();
            let result =
                cx.background_executor()
                    .spawn(async move {
                        api.respond_permission_with(&session, &request, true, None, true)
                    })
                    .await;
            let _ = this.update(cx, |view, cx| {
                view.requests.sending.remove(&request_id);
                match result {
                    Ok(true) => Toaster::info(
                        cx.t_with(
                            "conversation.permission.alwaysSaved",
                            &Args::default().with("tool", tool),
                        ),
                        cx,
                    ),
                    Ok(false) => Toaster::info(cx.t("conversation.permission.onlyOnce"), cx),
                    Err(error) => Toaster::error(error.to_string(), cx),
                }
                view.refresh_now(cx);
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_fits_connectors_and_a_reason_reads_as_a_message() {
        assert!(may_always_allow("mcp__dcc-slack__list_channels"));
        assert!(!may_always_allow(
            "mcp__dcc-browser-webview__dcc_browser_click"
        ));
        assert!(!may_always_allow("Bash"));
        assert_eq!(
            short_tool("mcp__dcc-e785e45c-4__slack_search_channels"),
            "slack_search_channels"
        );
        assert_eq!(short_tool("Bash"), "Bash");
        assert_eq!(short_tool("mcp__broken"), "mcp__broken");
        assert_eq!(
            reason_message("Bash", "use o script"),
            "[DCC] I denied `Bash`: use o script"
        );
    }
}
