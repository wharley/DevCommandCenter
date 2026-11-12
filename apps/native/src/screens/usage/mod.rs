//! "DCC Usage" (`UsageDialog`): Claude Code and Codex on this machine,
//! inside and outside DCC, read from their local logs. Opening it (and ⟳)
//! reads what the logs gained since last time into the index, on a
//! background thread with its progress shown; the period's rows are then
//! kept here and every filter, metric and grouping is recomputed from
//! them off the UI thread. Nothing runs while the dialog is closed.

mod charts;
mod filters;
mod heatmap;
mod sections;
#[cfg(test)]
mod tests;
mod view;

use std::path::PathBuf;
use std::sync::Arc;

use dcc_core::domain::usage_logs::{
    GroupBy, Metric, Scope, UsageFilter, UsageRows, UsageSummary, Via, aggregate,
};
use dcc_infra::usage_logs::Sources;
use futures::StreamExt;
use gpui::{Context, EventEmitter, Task};

pub enum UsageEvent {
    Closed,
}

/// What the index is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Indexing {
    Idle,
    /// Reading the logs: files done of total.
    Reading(usize, usize),
}

pub struct Usage {
    db_path: PathBuf,
    /// The log folders to read instead of this machine's (tests).
    sources: Option<Sources>,
    filter: UsageFilter,
    metric: Metric,
    group_by: GroupBy,
    rows: Option<Arc<UsageRows>>,
    summary: Option<Arc<UsageSummary>>,
    indexing: Indexing,
    /// Rows or the summary are being read.
    loading: bool,
    failed: bool,
    _index: Option<Task<()>>,
    _load: Option<Task<()>>,
    _compute: Option<Task<()>>,
}

impl EventEmitter<UsageEvent> for Usage {}

enum IndexMessage {
    Progress(usize, usize),
    Done(anyhow::Result<UsageRows>),
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl Usage {
    pub fn new(db_path: PathBuf, cx: &mut Context<Self>) -> Self {
        // Tests that open the dialog through the shell never read this
        // machine's logs (nor write an index beside its database).
        if cfg!(test) {
            let scratch = std::env::temp_dir().join("dcc-native-usage-tests");
            return Self::with_sources(
                scratch.join("database.sqlite"),
                Some(Sources::default()),
                cx,
            );
        }
        Self::with_sources(db_path, None, cx)
    }

    pub(crate) fn with_sources(
        db_path: PathBuf,
        sources: Option<Sources>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut usage = Self {
            db_path,
            sources,
            filter: UsageFilter::default(),
            metric: Metric::default(),
            group_by: GroupBy::default(),
            rows: None,
            summary: None,
            indexing: Indexing::Idle,
            loading: false,
            failed: false,
            _index: None,
            _load: None,
            _compute: None,
        };
        usage.refresh(cx);
        usage
    }

    /// Reads the logs' new lines into the index, then the period's rows.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if matches!(self.indexing, Indexing::Reading(..)) {
            return;
        }
        self.indexing = Indexing::Reading(0, 0);
        self.failed = false;
        cx.notify();
        let db_path = self.db_path.clone();
        let sources = self.sources.clone();
        let period = self.filter.period_days;
        let since = crate::data::usage::since_ms(period, now_ms());
        let (sender, mut receiver) = futures::channel::mpsc::unbounded();
        let work = cx.background_executor().spawn(async move {
            let progress_sender = sender.clone();
            let mut last = 0;
            let mut progress = |done: usize, total: usize| {
                // A notice every few files is enough for the counter.
                if done == total || done == 0 || done >= last + 8 {
                    last = done;
                    let _ = progress_sender.unbounded_send(IndexMessage::Progress(done, total));
                }
            };
            let read = crate::data::usage::index_and_load(&db_path, sources, since, &mut progress)
                .map(|(_, rows)| rows);
            let _ = sender.unbounded_send(IndexMessage::Done(read));
        });
        self._index = Some(cx.spawn(async move |this, cx| {
            let _work = work;
            while let Some(message) = receiver.next().await {
                let done = matches!(message, IndexMessage::Done(_));
                let alive = this.update(cx, |usage, cx| {
                    match message {
                        IndexMessage::Progress(done, total) => {
                            usage.indexing = Indexing::Reading(done, total);
                        }
                        IndexMessage::Done(read) => {
                            usage.indexing = Indexing::Idle;
                            if usage.filter.period_days == period || read.is_err() {
                                usage.take_rows(read, cx);
                            } else {
                                // The period changed while reading.
                                usage.reload(cx);
                            }
                        }
                    }
                    cx.notify();
                });
                if done || alive.is_err() {
                    break;
                }
            }
        }));
    }

    /// The period's rows from the index as it is (a new period).
    fn reload(&mut self, cx: &mut Context<Self>) {
        if matches!(self.indexing, Indexing::Reading(..)) {
            // The running refresh reads the new period's rows when it ends.
            return;
        }
        self.loading = true;
        cx.notify();
        let db_path = self.db_path.clone();
        let since = crate::data::usage::since_ms(self.filter.period_days, now_ms());
        self._load = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { crate::data::usage::load_rows(&db_path, since) })
                .await;
            let _ = this.update(cx, |usage, cx| usage.take_rows(read, cx));
        }));
    }

    fn take_rows(&mut self, read: anyhow::Result<UsageRows>, cx: &mut Context<Self>) {
        match read {
            Ok(rows) => {
                self.rows = Some(Arc::new(rows));
                self.compute(cx);
            }
            Err(error) => {
                eprintln!("[dcc-native] usage: {error:#}");
                self.loading = false;
                self.failed = true;
                self.summary = None;
                cx.notify();
            }
        }
    }

    /// The dashboard for the current filter, metric and grouping.
    fn compute(&mut self, cx: &mut Context<Self>) {
        let Some(rows) = self.rows.clone() else {
            return;
        };
        self.loading = true;
        cx.notify();
        let (filter, metric, group_by) = (self.filter.clone(), self.metric, self.group_by);
        self._compute = Some(cx.spawn(async move |this, cx| {
            let summary = cx
                .background_executor()
                .spawn(async move {
                    aggregate(&rows, &filter, metric, group_by, &chrono::Local, now_ms())
                })
                .await;
            let _ = this.update(cx, |usage, cx| {
                usage.loading = false;
                usage.summary = Some(Arc::new(summary));
                cx.notify();
            });
        }));
    }

    fn set_period(&mut self, period: Option<u32>, cx: &mut Context<Self>) {
        if self.filter.period_days != period {
            self.filter.period_days = period;
            self.reload(cx);
        }
    }

    fn toggle_runtime(&mut self, index: usize, cx: &mut Context<Self>) {
        self.filter.runtimes[index] = !self.filter.runtimes[index];
        // Never both off: turning off the last one shows the other.
        if !self.filter.runtimes.iter().any(|on| *on) {
            self.filter.runtimes[1 - index] = true;
        }
        self.compute(cx);
    }

    fn set_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        if self.filter.scope != scope {
            self.filter.scope = scope;
            self.compute(cx);
        }
    }

    fn toggle_via(&mut self, via: Via, cx: &mut Context<Self>) {
        if let Some(at) = self
            .filter
            .hidden_vias
            .iter()
            .position(|hidden| *hidden == via)
        {
            self.filter.hidden_vias.remove(at);
        } else {
            self.filter.hidden_vias.push(via);
        }
        self.compute(cx);
    }

    fn set_metric(&mut self, metric: Metric, cx: &mut Context<Self>) {
        if self.metric != metric {
            self.metric = metric;
            self.compute(cx);
        }
    }

    fn set_group_by(&mut self, group_by: GroupBy, cx: &mut Context<Self>) {
        if self.group_by != group_by {
            self.group_by = group_by;
            self.compute(cx);
        }
    }
}
