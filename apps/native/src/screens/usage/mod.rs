//! "DCC Usage" (`UsageDialog`): tokens, turns, coverage and cost for the
//! last 7, 30 (the default) or 90 days or all of it, for every project or
//! only the open task's; the providers' share, the days, the providers and
//! the models. Read from the database; ⟳ reads it again.

mod charts;
mod sections;
#[cfg(test)]
mod tests;
mod view;

use std::collections::HashMap;
use std::path::PathBuf;

use dcc_core::domain::usage::UsageDashboard;
use gpui::{Context, EventEmitter, Task};

pub enum UsageEvent {
    Closed,
}

pub struct Usage {
    db_path: PathBuf,
    /// `periodDays`: 7, 30, 90 or `None` for everything.
    period: Option<u32>,
    /// The open task's project, when there is one.
    project_id: Option<String>,
    project_only: bool,
    /// Provider labels by id.
    labels: HashMap<String, String>,
    dashboard: Option<UsageDashboard>,
    /// A read is running (`isFetching`).
    fetching: bool,
    failed: bool,
    _load: Option<Task<()>>,
}

impl EventEmitter<UsageEvent> for Usage {}

impl Usage {
    pub fn new(db_path: PathBuf, project_id: Option<String>, cx: &mut Context<Self>) -> Self {
        let labels = crate::data::provider_catalog::offline(&db_path)
            .into_iter()
            .map(|provider| (provider.id.0, provider.label))
            .collect();
        let mut usage = Self {
            db_path,
            period: Some(30),
            project_id,
            project_only: false,
            labels,
            dashboard: None,
            fetching: false,
            failed: false,
            _load: None,
        };
        usage.load(false, cx);
        usage
    }

    /// Reads the dashboard; a new period or scope starts over (`isPending`),
    /// a refresh keeps what is shown while it reads.
    fn load(&mut self, keep: bool, cx: &mut Context<Self>) {
        if !keep {
            self.dashboard = None;
        }
        self.fetching = true;
        self.failed = false;
        cx.notify();
        let db_path = self.db_path.clone();
        let period = self.period;
        let project = self.project_only.then(|| self.project_id.clone()).flatten();
        self._load = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { crate::data::usage::load(&db_path, period, project) })
                .await;
            let _ = this.update(cx, |usage, cx| {
                usage.fetching = false;
                match read {
                    Ok(dashboard) => usage.dashboard = Some(dashboard),
                    // `isError`: the error shows instead of the old numbers.
                    Err(error) => {
                        eprintln!("[dcc-native] usage: {error:#}");
                        usage.dashboard = None;
                        usage.failed = true;
                    }
                }
                cx.notify();
            });
        }));
    }

    fn set_period(&mut self, period: Option<u32>, cx: &mut Context<Self>) {
        if self.period != period {
            self.period = period;
            self.load(false, cx);
        }
    }

    fn toggle_project(&mut self, cx: &mut Context<Self>) {
        self.project_only = !self.project_only;
        self.load(false, cx);
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.load(true, cx);
    }

    fn label(&self, provider: &str) -> String {
        self.labels
            .get(provider)
            .cloned()
            .unwrap_or_else(|| provider.to_string())
    }
}
