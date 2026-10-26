//! "Sessões passadas" (`SessionSearchDialog`): a query over every task's
//! conversations, the open task's first; ↑/↓ and Enter pick like cmdk, a
//! click too. Picking one opens it (restored first when archived).

#[cfg(test)]
mod tests;
mod view;

use std::path::PathBuf;
use std::time::Duration;

use dcc_core::domain::session::SessionSearchResult;
use gpui::{AppContext, Context, Entity, EventEmitter, Subscription, Task, Window};

use crate::data::session_search::search;
use crate::ui::text_input::{TextInput, TextInputEvent};

/// `useDeferredValue`: typing settles before the search runs.
const SETTLE: Duration = Duration::from_millis(120);

pub enum SessionSearchEvent {
    Picked(Box<SessionSearchResult>),
    Closed,
}

pub struct SessionSearch {
    pub(super) input: Entity<TextInput>,
    /// The open task's id: its conversations come first.
    pub(super) current_task: Option<String>,
    pub(super) results: Vec<SessionSearchResult>,
    pub(super) loading: bool,
    /// The query the results answer.
    pub(super) answered: String,
    pub(super) highlighted: usize,
    db_path: PathBuf,
    _input_events: Subscription,
    _search: Option<Task<()>>,
}

impl EventEmitter<SessionSearchEvent> for SessionSearch {}

impl SessionSearch {
    pub fn new(
        current_task: Option<String>,
        db_path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = crate::i18n::Translate::t(&**cx, "workbench.sessionSearch.placeholder");
        let input = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
        let events = cx.subscribe(&input, |search, _, event, cx| match event {
            TextInputEvent::Changed => search.run(cx),
            TextInputEvent::Submitted => search.pick_highlighted(cx),
            TextInputEvent::Cancelled => cx.emit(SessionSearchEvent::Closed),
            TextInputEvent::Blurred => {}
        });
        let mut search = Self {
            input,
            current_task,
            results: Vec::new(),
            loading: false,
            answered: String::new(),
            highlighted: 0,
            db_path,
            _input_events: events,
            _search: None,
        };
        search.run(cx);
        search
    }

    pub fn input(&self) -> Entity<TextInput> {
        self.input.clone()
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text().trim().to_string();
        let db_path = self.db_path.clone();
        self.loading = true;
        cx.notify();
        self._search = Some(cx.spawn(async move |this, cx| {
            if !query.is_empty() {
                cx.background_executor().timer(SETTLE).await;
            }
            let wanted = query.clone();
            let found = cx
                .background_executor()
                .spawn(async move { search(&db_path, &wanted) })
                .await;
            let _ = this.update(cx, |search, cx| {
                search.loading = false;
                search.answered = query;
                search.results = match found {
                    Ok(results) => search.ordered(results),
                    Err(error) => {
                        eprintln!("[dcc-native] session search: {error:#}");
                        Vec::new()
                    }
                };
                search.highlighted = 0;
                cx.notify();
            });
        }));
    }

    /// The open task's first, the rest after, each in the database's order.
    fn ordered(&self, results: Vec<SessionSearchResult>) -> Vec<SessionSearchResult> {
        let Some(task) = &self.current_task else {
            return results;
        };
        let (mine, others): (Vec<_>, Vec<_>) = results
            .into_iter()
            .partition(|result| &result.workspace_id.0 == task);
        mine.into_iter().chain(others).collect()
    }

    /// How many lead the list as "Workspace atual".
    pub(super) fn current_count(&self) -> usize {
        self.current_task.as_ref().map_or(0, |task| {
            self.results
                .iter()
                .take_while(|result| &result.workspace_id.0 == task)
                .count()
        })
    }

    pub(super) fn move_highlight(&mut self, down: bool, cx: &mut Context<Self>) {
        let total = self.results.len();
        if total == 0 {
            return;
        }
        self.highlighted = if down {
            (self.highlighted + 1).min(total - 1)
        } else {
            self.highlighted.saturating_sub(1)
        };
        cx.notify();
    }

    pub(super) fn pick(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(result) = self.results.get(index).cloned() {
            cx.emit(SessionSearchEvent::Picked(Box::new(result)));
        }
    }

    fn pick_highlighted(&mut self, cx: &mut Context<Self>) {
        self.pick(self.highlighted, cx);
    }
}
