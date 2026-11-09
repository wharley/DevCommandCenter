//! The pull requests the person reviewed, searched again every 5 minutes
//! while the app runs (`PrNews`): the count beside "Pull Requests" follows
//! them, and news that has not been announced posts one system notification
//! — only while the person is in another app, without bouncing the Dock.
//!
//! One search covers every repository (one `gh` for the GitHub account),
//! so the cost does not grow with the projects; the hub's list is not read.
//! The same pass brings the agents' check-in: pull requests asking for the
//! person's review, issues assigned to them, and the running turns with no
//! sign of life (one indexed read per running conversation).

use std::time::Duration;

use gpui::{App, BorrowAppContext, Context, Entity, EventEmitter, Subscription, Task};

use crate::data::check_ins::{CheckIn, Running, stalled};
use crate::data::desktop_api::DesktopApi;
use crate::data::pr_hub::{Asked, Failing, Received, Reviewed};
use crate::data::pr_news::{PrNews, news_text};
use crate::screens::sidebar::Sidebar;

const FIRST_CHECK: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_secs(5 * 60);

/// What each search brought for the Reviewer's triggers: the reviewed pull
/// requests with news just announced, and the person's own failing ones.
pub struct PrNewsArrived {
    pub news: Vec<Reviewed>,
    pub failing: Vec<Failing>,
    /// The person's own pull requests with reviews from someone else.
    pub received: Vec<Received>,
    /// Pull requests asking for the person's review.
    pub requested: Vec<Asked>,
    /// Issues assigned to the person.
    pub issues: Vec<Asked>,
    /// Running turns with no sign of life for a while.
    pub stalled: Vec<CheckIn>,
}

/// Each running turn's last sign of life, from DCC's database.
fn with_last_events(mut running: Vec<Running>) -> Vec<Running> {
    let db = crate::data::loader::database_path();
    let Ok(repo) = dcc_infra::db::SqliteSessionRepo::open_read_only(&db) else {
        return Vec::new();
    };
    for turn in &mut running {
        let id = dcc_core::domain::session::SessionId(turn.session_id.clone());
        turn.last_event_at = repo.last_event_at_sync(&id).ok().flatten();
    }
    running
}

pub struct PrWatch {
    _count: Subscription,
    _checks: Option<Task<()>>,
}

impl PrWatch {
    pub fn new(sidebar: Entity<Sidebar>, cx: &mut Context<Self>) -> Self {
        let turns = sidebar.clone();
        let count = cx.observe_global::<PrNews>(move |_, cx| {
            let count = cx.global::<PrNews>().count();
            sidebar.update(cx, |sidebar, cx| sidebar.set_pr_news(count, cx));
        });
        // Tests drive the hub themselves; a test never reads the real desktop.
        let checks = (!cfg!(test)).then(|| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(FIRST_CHECK).await;
                loop {
                    let running = turns.read_with(cx, |sidebar, _| sidebar.running_turns());
                    let (read, running) = cx
                        .background_executor()
                        .spawn(async move {
                            (
                                DesktopApi::load().map(|api| api.pr_hub_reviewed()),
                                with_last_events(running),
                            )
                        })
                        .await;
                    if let Some(Ok(list)) = read {
                        let alive = this.update(cx, |_, cx| {
                            if !cx.has_global::<PrNews>() {
                                return;
                            }
                            let (failing, received) = (list.failing, list.received);
                            let (requested, issues) = (list.requested, list.issues);
                            let stalled = stalled(&running, chrono::Utc::now());
                            let news =
                                cx.update_global::<PrNews, _>(|news, _| news.read(list.items));
                            for pr in &news {
                                announce(pr, cx);
                            }
                            cx.emit(PrNewsArrived {
                                news,
                                failing,
                                received,
                                requested,
                                issues,
                                stalled,
                            });
                        });
                        if alive.is_err() {
                            break;
                        }
                    }
                    cx.background_executor().timer(EVERY).await;
                }
            })
        });
        Self {
            _count: count,
            _checks: checks,
        }
    }
}

impl EventEmitter<PrNewsArrived> for PrWatch {}

/// "perf(checkout): L1 em memória" — "acme/app #1014 · 1 commit novo".
fn announce(pr: &Reviewed, cx: &App) {
    let Some(text) = news_text(pr, cx) else {
        return;
    };
    let body = format!("{} #{} · {text}", pr.repository, pr.number);
    crate::ui::attention::notify_away(&pr.title, &body);
}
