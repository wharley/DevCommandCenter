//! The open pull requests the person reviewed that got commits or replies
//! from their author since, and that they have not opened again: the count
//! beside "Pull Requests", the tag on the card and one system notification
//! each. Read from one search across every repository (`pr_hub_reviewed`),
//! by the pull request's URL; opening one marks what came as seen. Kept in
//! the native app's own file (`pr-review-seen.json`), as the time of the
//! latest activity seen and the latest announced.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use gpui::{App, Global};
use serde::{Deserialize, Serialize};

use super::pr_hub::{Item, Reviewed};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Marks {
    /// The latest activity the person saw, by pull request URL.
    seen: BTreeMap<String, String>,
    /// The latest activity announced, by pull request URL.
    announced: BTreeMap<String, String>,
    /// A search was read before: the first one announces nothing old.
    started: bool,
}

pub struct PrNews {
    path: Option<PathBuf>,
    marks: Marks,
    reviewed: Vec<Reviewed>,
}

impl Global for PrNews {}

fn at(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

/// `left` came after `right`; an unreadable time never does.
fn later(left: &str, right: &str) -> bool {
    match (at(left), at(right)) {
        (Some(left), Some(right)) => left > right,
        _ => false,
    }
}

/// The latest activity after the person's review, when there is any.
fn activity(reviewed: &Reviewed) -> Option<&str> {
    let since = &reviewed.since;
    (since.commits + since.replies > 0)
        .then_some(since.last_activity.as_deref())
        .flatten()
}

impl PrNews {
    pub fn init(cx: &mut App) {
        cx.set_global(Self::load(crate::prefs::native_file("pr-review-seen.json")));
    }

    pub fn load(path: PathBuf) -> Self {
        let marks = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            marks,
            reviewed: Vec::new(),
        }
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(raw) = serde_json::to_string(&self.marks) {
            let _ = std::fs::write(path, raw);
        }
    }

    /// Something came after the person's review that they did not open.
    pub fn is_new(&self, reviewed: &Reviewed) -> bool {
        let Some(last) = activity(reviewed) else {
            return false;
        };
        self.marks
            .seen
            .get(&reviewed.url)
            .is_none_or(|seen| later(last, seen))
    }

    pub fn count(&self) -> usize {
        self.reviewed.iter().filter(|pr| self.is_new(pr)).count()
    }

    /// The pull requests with news, by URL: the hub's order when it opens.
    pub fn news_urls(&self) -> Vec<String> {
        self.reviewed
            .iter()
            .filter(|pr| self.is_new(pr))
            .map(|pr| pr.url.clone())
            .collect()
    }

    /// The search's entry for a pull request of the hub's list.
    pub fn entry(&self, url: &str) -> Option<&Reviewed> {
        self.reviewed.iter().find(|pr| pr.url == url)
    }

    /// A fresh search. Pull requests no longer listed (merged, closed) are
    /// forgotten; the ones with news not announced yet come back, once each
    /// — none from the very first search, whose news is old.
    pub fn read(&mut self, reviewed: Vec<Reviewed>) -> Vec<Reviewed> {
        let listed = |url: &String| reviewed.iter().any(|pr| &pr.url == url);
        self.marks.seen.retain(|url, _| listed(url));
        self.marks.announced.retain(|url, _| listed(url));
        let mut announce = Vec::new();
        for pr in &reviewed {
            let Some(last) = activity(pr).filter(|_| self.is_new(pr)) else {
                continue;
            };
            let told = self
                .marks
                .announced
                .get(&pr.url)
                .is_some_and(|told| !later(last, told));
            if !told {
                self.marks
                    .announced
                    .insert(pr.url.clone(), last.to_string());
                if self.marks.started {
                    announce.push(pr.clone());
                }
            }
        }
        self.marks.started = true;
        self.reviewed = reviewed;
        self.save();
        announce
    }

    /// The person opened it: what came so far is seen. Whether it changed.
    pub fn mark_seen(&mut self, url: &str) -> bool {
        let Some(pr) = self.entry(url) else {
            return false;
        };
        let Some(last) = activity(pr).filter(|_| self.is_new(pr)) else {
            return false;
        };
        let last = last.to_string();
        self.marks.seen.insert(url.to_string(), last);
        self.save();
        true
    }

    #[cfg(test)]
    pub(crate) fn in_memory() -> Self {
        Self {
            path: None,
            marks: Marks::default(),
            reviewed: Vec::new(),
        }
    }
}

/// "2 commits novos · @daniel respondeu no PR": what came, as the code
/// tab's "Desde sua revisão" says it.
pub fn news_text(reviewed: &Reviewed, cx: &App) -> Option<String> {
    use crate::i18n::{Args, Translate};
    let since = &reviewed.since;
    let mut parts = Vec::new();
    if since.commits > 0 {
        parts.push(
            cx.t_with(
                "pullRequests.code.sinceCommits",
                &Args::count(i64::from(since.commits)),
            )
            .to_string(),
        );
    }
    if let Some(author) = reviewed.author.clone().filter(|_| since.replies > 0) {
        parts.push(
            cx.t_with(
                "pullRequests.code.sinceReplied",
                &Args::default().with("author", author),
            )
            .to_string(),
        );
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// The person reviewed this pull request of the hub's list.
pub fn reviewed(item: &Item, cx: &App) -> bool {
    cx.try_global::<PrNews>()
        .is_some_and(|news| news.entry(&item.url).is_some())
}

/// A pull request of the hub's list with news (false before any search).
pub fn is_new(item: &Item, cx: &App) -> bool {
    cx.try_global::<PrNews>()
        .and_then(|news| Some(news.is_new(news.entry(&item.url)?)))
        .unwrap_or(false)
}

/// The news of a pull request of the hub's list, as its tag says it.
pub fn item_news_text(item: &Item, cx: &App) -> Option<String> {
    let news = cx.try_global::<PrNews>()?;
    news_text(news.entry(&item.url)?, cx)
}

/// The person's latest review of a pull request of the hub's list.
pub fn my_review(item: &Item, cx: &App) -> Option<String> {
    let news = cx.try_global::<PrNews>()?;
    Some(news.entry(&item.url)?.state.clone())
}

#[cfg(test)]
#[path = "pr_news_tests.rs"]
mod tests;
