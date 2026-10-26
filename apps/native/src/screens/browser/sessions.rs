//! "Sessões" (`BrowserSessionImportDialog`): a login the person already has
//! in Chrome or Arc on this Mac, for the site open in the Browser. The
//! profiles come from metadata alone; importing reads that site's cookies
//! (macOS asks for the browser's Keychain key), sets them in this Browser's
//! store and reloads the page. It stops when the page moved meanwhile, and
//! a reply for a dialog that closed or reopened is dropped.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use dcc_infra::browser_sessions::{BrowserSessionProfile, PreparedImport};
use futures::future::{Either, select};
use gpui::{Context, Task};

use super::surface::BrowserSurface;
use crate::i18n::{Args, Translate};
use crate::ui::Toaster;

/// How long the cookie store may take (`apply_prepared_cookies`).
const APPLY_TIMEOUT: Duration = Duration::from_secs(8);

/// A profile's cookies for a URL (`prepare_import`), off the main thread.
pub type Prepare = Arc<dyn Fn(&str, &str) -> Result<PreparedImport, String> + Send + Sync>;

/// Where profiles and cookies come from (the real profiles; a fake in tests).
#[derive(Clone)]
pub struct SessionSource {
    pub list: Rc<dyn Fn() -> Vec<BrowserSessionProfile>>,
    pub prepare: Prepare,
}

impl SessionSource {
    pub fn real() -> Self {
        Self {
            list: Rc::new(dcc_infra::browser_sessions::list_profiles),
            prepare: Arc::new(dcc_infra::browser_sessions::prepare_import),
        }
    }
}

pub struct SessionsDialog {
    /// The URL it opened for (`currentUrl`).
    pub url: Option<String>,
    pub profiles: Vec<BrowserSessionProfile>,
    pub selected: Option<String>,
    pub importing: bool,
    pub error: Option<String>,
    /// Which opening this is: a reply for another is dropped.
    opening: u64,
    _import: Option<Task<()>>,
}

impl SessionsDialog {
    /// `siteFromUrl`: the host, when the URL has one.
    pub fn site(&self) -> Option<String> {
        let url = url::Url::parse(self.url.as_deref()?).ok()?;
        let host = url.host_str()?;
        Some(match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_string(),
        })
    }

    pub fn selected_profile(&self) -> Option<&BrowserSessionProfile> {
        let id = self.selected.as_deref()?;
        self.profiles.iter().find(|profile| profile.id == id)
    }
}

/// `displayBrowserName`
pub fn browser_name(browser: &str) -> String {
    match browser.trim().to_lowercase().as_str() {
        "arc" => "Arc".into(),
        "chrome" => "Google Chrome".into(),
        _ => browser.to_string(),
    }
}

impl BrowserSurface {
    /// The "Sessões" button: the profiles for the page shown now.
    pub fn open_sessions(&mut self, cx: &mut Context<Self>) {
        if self.page.is_none() {
            return;
        }
        self.session_openings += 1;
        let url = self
            .page
            .as_ref()
            .and_then(|page| page.url())
            .or_else(|| self.url.clone());
        let mut dialog = SessionsDialog {
            url,
            profiles: (self.session_source.list)(),
            selected: None,
            importing: false,
            error: None,
            opening: self.session_openings,
            _import: None,
        };
        if !cfg!(target_os = "macos") {
            dialog.error = Some("Session import is currently available on macOS only.".into());
        }
        self.sessions = Some(dialog);
        cx.notify();
    }

    pub fn close_sessions(&mut self, cx: &mut Context<Self>) {
        if self
            .sessions
            .as_ref()
            .is_some_and(|dialog| !dialog.importing)
        {
            self.sessions = None;
            cx.notify();
        }
    }

    pub fn select_session_profile(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(dialog) = self.sessions.as_mut().filter(|dialog| !dialog.importing) {
            dialog.selected = Some(id);
            cx.notify();
        }
    }

    /// `importSelected`: the site's cookies read (Keychain asked), set in
    /// this Browser's store, the page reloaded.
    pub fn import_session(&mut self, cx: &mut Context<Self>) {
        let ready = !self.loading;
        let revision = self.page_loads;
        let Some(dialog) = self.sessions.as_mut() else {
            return;
        };
        let (Some(url), Some(profile)) = (dialog.url.clone(), dialog.selected.clone()) else {
            return;
        };
        if dialog.importing || dialog.site().is_none() {
            return;
        }
        if !ready {
            dialog.error = Some("browser changed page; reopen Sessions and try again".into());
            cx.notify();
            return;
        }
        dialog.importing = true;
        dialog.error = None;
        let opening = dialog.opening;
        let prepare = self.session_source.prepare.clone();
        cx.notify();
        let task = cx.spawn(async move |this, cx| {
            let prepared = cx
                .background_executor()
                .spawn({
                    let url = url.clone();
                    async move { prepare(&profile, &url) }
                })
                .await;
            let applied = this.update(cx, |surface, _| {
                let prepared = prepared?;
                surface.apply_session(prepared, &url, revision)
            });
            let outcome = match applied {
                Ok(Ok((skipped, answer))) => {
                    match select(answer, cx.background_executor().timer(APPLY_TIMEOUT)).await {
                        Either::Left((Ok((imported, applied_skipped)), _)) => {
                            Ok((imported, skipped + applied_skipped))
                        }
                        Either::Left((Err(_), _)) => {
                            Err("Browser cookie import was cancelled".into())
                        }
                        Either::Right(_) => Err("Browser cookie import timed out".into()),
                    }
                }
                Ok(Err(error)) => Err(error),
                Err(_) => return,
            };
            let _ = this.update(cx, |surface, cx| {
                surface.finish_import(opening, outcome, cx)
            });
        });
        if let Some(dialog) = self.sessions.as_mut() {
            dialog._import = Some(task);
        }
    }

    /// The page is still the one the import was for: its cookies go in.
    fn apply_session(
        &self,
        prepared: PreparedImport,
        url: &str,
        revision: u64,
    ) -> Result<(usize, futures::channel::oneshot::Receiver<(usize, usize)>), String> {
        let page = self.page.clone().ok_or("browser is not open")?;
        let current = page.url().or_else(|| self.url.clone());
        if self.loading || self.page_loads != revision || current.as_deref() != Some(url) {
            return Err("browser changed page; reopen Sessions and try again".into());
        }
        let (reply, answer) = futures::channel::oneshot::channel();
        let skipped = prepared.skipped;
        if prepared.cookies.is_empty() {
            let _ = reply.send((0, 0));
        } else {
            page.set_cookies(prepared.cookies, reply);
        }
        Ok((skipped, answer))
    }

    fn finish_import(
        &mut self,
        opening: u64,
        outcome: Result<(usize, usize), String>,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self
            .sessions
            .as_mut()
            .filter(|dialog| dialog.opening == opening)
        else {
            return;
        };
        dialog.importing = false;
        match outcome {
            Ok((imported, skipped)) if imported > 0 => {
                let text = cx.t_with(
                    "browser.sessions.result",
                    &Args::default()
                        .with("imported", imported.to_string())
                        .with("skipped", skipped.to_string()),
                );
                Toaster::success(text, cx);
                self.sessions = None;
                self.reload(cx);
            }
            Ok(_) => dialog.error = Some(cx.t("browser.sessions.notImported").to_string()),
            Err(error) => dialog.error = Some(error),
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::browser_name;

    #[test]
    fn the_browsers_read_like_the_desktop() {
        assert_eq!(browser_name("chrome"), "Google Chrome");
        assert_eq!(browser_name(" Arc "), "Arc");
        assert_eq!(browser_name("brave"), "brave");
    }
}
