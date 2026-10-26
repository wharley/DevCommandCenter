//! "Enviar contexto da página ao agente" (`handleSendToAgent` →
//! `browser_extract_context`): the fixed script reads the selection, else
//! the visible text, and a small semantic map; its answer crosses the
//! trust boundary in `dcc_core` (the desktop's own), gets a map id of this
//! Browser's, and goes to the composer as the desktop's envelope. A page
//! still loading, one that changed meanwhile, or one that does not answer
//! in 2s says so in the strip.

use std::time::Duration;

use dcc_core::domain::browser_context::{
    BrowserAgentContext, BrowserSemanticMap, ContextLabels, bound_context, browser_context_script,
    format_agent_context,
};
use futures::future::{Either, select};
use gpui::{App, Context};

use super::surface::{BrowserEvent, BrowserSurface};
use crate::i18n::Translate;

/// `BROWSER_CONTEXT_TIMEOUT`
const TIMEOUT: Duration = Duration::from_secs(2);

/// `browser.agentContext.*`, for the envelope's lines.
fn labels(cx: &App) -> ContextLabels {
    let t = |key: &str| cx.t(&format!("browser.agentContext.{key}")).to_string();
    ContextLabels {
        prompt: String::new(),
        url: t("url"),
        title: t("title"),
        source: t("source"),
        truncated: t("truncated"),
        selection: t("selection"),
        visible_text: t("visibleText"),
        yes: t("yes"),
        no: t("no"),
        semantic_map: t("semanticMap"),
        map_id: t("mapId"),
        generation: t("generation"),
        visible_elements: t("visibleElements"),
        name: t("name"),
        destination: t("destination"),
        states: t("states"),
        no_visible_elements: t("noVisibleElements"),
    }
}

impl BrowserSurface {
    /// The button is on while a loaded page can be read.
    pub fn can_send(&self) -> bool {
        self.page.is_some() && !self.loading && !self.sending
    }

    fn page_url(&self) -> Option<String> {
        self.page
            .as_ref()
            .and_then(|page| page.url())
            .or_else(|| self.url.clone())
    }

    pub fn send_to_agent(&mut self, cx: &mut Context<Self>) {
        if self.sending {
            return;
        }
        let Some(page) = self.page.clone() else {
            return;
        };
        let Some(url) = self.page_url().filter(|_| !self.loading) else {
            self.error = Some("browser page is still loading".into());
            cx.notify();
            return;
        };
        let revision = self.page_loads;
        self.sending = true;
        self.error = None;
        cx.notify();
        let (reply, answer) = futures::channel::oneshot::channel();
        page.evaluate(&browser_context_script(), reply);
        let timer = cx.background_executor().timer(TIMEOUT);
        self._context = Some(cx.spawn(async move |this, cx| {
            let raw = match select(answer, timer).await {
                Either::Left((Ok(raw), _)) => Ok(raw),
                Either::Left((Err(_), _)) => {
                    Err("browser page context response was cancelled".to_string())
                }
                Either::Right(_) => Err("browser page context timed out".to_string()),
            };
            let _ = this.update(cx, |surface, cx| {
                surface.sending = false;
                match raw.and_then(|raw| surface.context_text(&raw, &url, revision, cx)) {
                    Ok(text) => cx.emit(BrowserEvent::SendToAgent(text)),
                    Err(error) => surface.error = Some(error),
                }
                cx.notify();
            });
        }));
    }

    /// The answer for the page it was asked of, as the composer gets it.
    fn context_text(
        &mut self,
        raw: &str,
        url: &str,
        revision: u64,
        cx: &App,
    ) -> Result<String, String> {
        if self.loading || self.page_loads != revision || self.page_url().as_deref() != Some(url) {
            return Err("browser page changed while reading context".into());
        }
        let bounded = bound_context(raw, url, self.title.as_deref())?;
        self.map_generation += 1;
        let generation = self.map_generation;
        let context = BrowserAgentContext {
            workspace_id: self.request.workspace_id.clone(),
            session_id: self.request.session_id.clone(),
            url: bounded.url,
            title: bounded.title,
            text: bounded.text,
            selection_only: bounded.selection_only,
            truncated: bounded.truncated || bounded.semantic_truncated,
            semantic_map: BrowserSemanticMap {
                map_id: format!("m-{}-{generation}", self.lifecycle),
                generation,
                page_load_revision: revision,
                items: bounded.items,
                truncated: bounded.semantic_truncated,
            },
        };
        Ok(format_agent_context(&context, &labels(cx))
            .trim_start()
            .to_string())
    }
}
