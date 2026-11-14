//! The end of a task an agent opened: the person merged its pull request,
//! and DCC tells the agent the work started with (`task_origin`'s first
//! step) — merged, live in production, or merged with a failed deployment,
//! as GitHub registered it — so that agent closes it with the person and,
//! when the request came from a connector, proposes the reply there. DCC
//! says only what it read: "live" only for a production deployment of the
//! merge commit that succeeded.

use super::task_origin::{Started, TaskOrigin};

/// How the message opens in the agent's conversation.
pub const SHIPPED_MARK: &str = "[DCC] Delivered";

/// What DCC read of the pull request after its merge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Merged; no production deployment of its commit was seen.
    Merged,
    Live {
        environment: String,
        at: Option<String>,
    },
    DeployFailed {
        environment: String,
    },
}

impl Outcome {
    /// Its name on the message's first line, read back by the room.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Merged => "merged",
            Self::Live { .. } => "live",
            Self::DeployFailed { .. } => "deploy failed",
        }
    }
}

fn started_line(origin: &TaskOrigin) -> String {
    match origin.steps.first().map(|step| &step.started) {
        Some(Started::Source {
            connector,
            watch,
            excerpt,
            link,
        }) => {
            let link = link
                .as_deref()
                .map(|link| format!(" ({link})"))
                .unwrap_or_default();
            format!(
                "It started from {connector}, which you watch (\"{watch}\"): \"{excerpt}\"{link}."
            )
        }
        Some(Started::Person { excerpt }) => {
            format!("It started with the person asking you: \"{excerpt}\".")
        }
        Some(Started::Routine { request }) => {
            format!("It started from your routine: \"{request}\".")
        }
        Some(Started::Passed { from, request, .. }) => {
            format!("It started with {from} passing you: \"{request}\".")
        }
        Some(Started::Event { line }) => format!("It started from: {line}"),
        None => String::new(),
    }
}

/// What the agent the work started with is told.
pub fn shipped_prompt(
    title: &str,
    pull_request: &str,
    outcome: &Outcome,
    origin: &TaskOrigin,
) -> String {
    let stands = match outcome {
        Outcome::Merged => "The person merged it. DCC saw no production deployment of its commit, so say that it merged, not that it is live.".to_string(),
        Outcome::Live { environment, at } => format!(
            "The person merged it and its commit is live in {environment}{}.",
            at.as_deref().map(|at| format!(" since {at}")).unwrap_or_default()
        ),
        Outcome::DeployFailed { environment } => format!(
            "The person merged it, but the deployment of its commit to {environment} FAILED: say so, do not say it is live."
        ),
    };
    let reply = match origin.steps.first().map(|step| &step.started) {
        Some(Started::Source { connector, .. }) => format!(
            " If someone in {connector} is waiting for an answer, propose the reply there with dcc_propose_reply (it waits as a card the person sends): what was done and where it stands, no more than DCC told you here."
        ),
        _ => String::new(),
    };
    format!(
        "{SHIPPED_MARK} \"{title}\" ({pull_request}): {}.\n\nThis is DCC, not a message from the person. The task \"{title}\" that started with you is done: pull request {pull_request}. {stands}\n{}\n\nTell the person in one or two sentences, in their language.{reply} Do not open a task and do not pass this on.",
        outcome.as_str(),
        started_line(origin)
    )
}

/// The task's name and how it stands (`merged`, `live`, `deploy failed`),
/// when `text` is such a message: what the room's line says.
pub fn shipped_line(text: &str) -> Option<(&str, &str)> {
    let first = text.strip_prefix(SHIPPED_MARK)?.lines().next()?.trim();
    let rest = first.strip_prefix('"')?;
    let (title, rest) = rest.split_once("\" (")?;
    let (_, state) = rest.rsplit_once("): ")?;
    Some((title, state.trim_end_matches('.')))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::task_origin::OriginStep;

    fn origin(started: Started) -> TaskOrigin {
        TaskOrigin {
            steps: vec![OriginStep {
                started,
                agent_id: "sup".into(),
                agent: "Suporte".into(),
                session_id: "s".into(),
                room: None,
                at: String::new(),
            }],
            opened_at: String::new(),
        }
    }

    fn slack() -> Started {
        Started::Source {
            connector: "Slack".into(),
            watch: "menções a mim".into(),
            excerpt: "o CEP não calcula frete".into(),
            link: Some("https://acme.slack.com/p1".into()),
        }
    }

    #[test]
    fn live_is_said_only_when_it_was_read() {
        let live = shipped_prompt(
            "Corrigir CEP",
            "acme/loja#1014",
            &Outcome::Live {
                environment: "Production".into(),
                at: Some("2026-11-13T19:52:22Z".into()),
            },
            &origin(slack()),
        );
        assert!(live.starts_with("[DCC] Delivered \"Corrigir CEP\" (acme/loja#1014): live."));
        assert!(live.contains("live in Production since 2026-11-13T19:52:22Z"));
        assert!(live.contains("https://acme.slack.com/p1"));
        assert!(live.contains("dcc_propose_reply"));
        assert_eq!(shipped_line(&live), Some(("Corrigir CEP", "live")));

        let merged = shipped_prompt(
            "Corrigir CEP",
            "acme/loja#1014",
            &Outcome::Merged,
            &origin(slack()),
        );
        assert!(merged.contains("not that it is live"));
        assert_eq!(shipped_line(&merged), Some(("Corrigir CEP", "merged")));

        let failed = shipped_prompt(
            "Corrigir CEP",
            "acme/loja#1014",
            &Outcome::DeployFailed {
                environment: "Production".into(),
            },
            &origin(slack()),
        );
        assert!(failed.contains("FAILED"));
        assert_eq!(
            shipped_line(&failed),
            Some(("Corrigir CEP", "deploy failed"))
        );
    }

    #[test]
    fn a_reply_is_proposed_only_where_the_request_came_from_outside() {
        let asked = shipped_prompt(
            "Corrigir CEP",
            "acme/loja#1014",
            &Outcome::Merged,
            &origin(Started::Person {
                excerpt: "corrige o CEP".into(),
            }),
        );
        assert!(asked.contains("the person asking you"));
        assert!(!asked.contains("dcc_propose_reply"));
        // Not one of these messages.
        assert_eq!(shipped_line("corrige o CEP"), None);
        assert_eq!(
            shipped_line("[DCC] Source \"x\" (Slack): 1 new item(s)."),
            None
        );
    }
}
