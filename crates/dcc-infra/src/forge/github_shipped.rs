//! Whether a merged pull request is live, read from GitHub with no model:
//! the pull request's merge commit, then the deployments GitHub holds for
//! that commit (Vercel, Netlify and Actions environments register theirs)
//! in a production environment, with the latest status of each. A handful
//! of reads at most. A repository that registers no deployment says only
//! that it merged; a later deployment that replaced this one still counts
//! as having gone live.

use serde_json::Value;

use super::github_read::{deployments_argv, view_argv};
use super::github_read_run::{pull_request_view, read_deployments, str_at, Gh, GithubTarget};

/// Deployments of the merge commit looked at.
const LIMIT: u32 = 6;

/// Where a pull request stands on its way out.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Shipped {
    /// Still open, or closed without merging.
    NotMerged,
    /// Merged; no production deployment of its commit is registered (yet).
    Merged,
    /// A production deployment of its commit is on its way.
    Deploying { environment: String },
    /// Its commit is in production.
    Live {
        environment: String,
        at: Option<String>,
        url: Option<String>,
    },
    /// The production deployment of its commit failed.
    DeployFailed {
        environment: String,
        at: Option<String>,
    },
}

/// A production environment, by its name ("Production", "prod").
fn is_production(environment: &str) -> bool {
    environment.to_lowercase().contains("prod")
}

/// Where the pull request stands: `pr` as `pull_request_view` shapes it,
/// `deployments` the ones of its merge commit, newest first, each with its
/// latest status.
pub fn verdict(pr: &Value, deployments: &[Value]) -> Shipped {
    let merged = str_at(pr, "/mergedAt").is_some()
        || str_at(pr, "/state").is_some_and(|state| state.eq_ignore_ascii_case("merged"));
    if !merged {
        return Shipped::NotMerged;
    }
    let Some(deployment) = deployments.iter().find(|deployment| {
        str_at(deployment, "/environment").is_some_and(|environment| is_production(&environment))
    }) else {
        return Shipped::Merged;
    };
    let environment = str_at(deployment, "/environment").unwrap_or_default();
    let at = str_at(deployment, "/latestStatus/at");
    match str_at(deployment, "/latestStatus/state").as_deref() {
        // `inactive`: a newer deployment took its place; it was live.
        Some("success" | "inactive") => Shipped::Live {
            environment,
            at,
            url: str_at(deployment, "/latestStatus/url").filter(|url| !url.is_empty()),
        },
        Some("failure" | "error") => Shipped::DeployFailed { environment, at },
        _ => Shipped::Deploying { environment },
    }
}

/// Where a pull request stands, and when it merged.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShippedRead {
    #[serde(flatten)]
    pub shipped: Shipped,
    #[serde(default)]
    pub merged_at: Option<String>,
}

/// Reads where pull request `number` stands.
pub fn read_shipped(target: &GithubTarget, number: u64) -> Result<ShippedRead, String> {
    let gh = Gh::new(target)?;
    let pr = pull_request_view(&gh.json(&view_argv(&target.repo, number))?);
    let merged_at = str_at(&pr, "/mergedAt");
    let Some(merge) = str_at(&pr, "/mergeCommit") else {
        return Ok(ShippedRead {
            shipped: verdict(&pr, &[]),
            merged_at,
        });
    };
    let argv = deployments_argv(&gh.host, &target.repo, Some(&merge), None, LIMIT);
    let deployments = read_deployments(&gh, &target.repo, &argv)?;
    Ok(ShippedRead {
        shipped: verdict(&pr, &deployments),
        merged_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn merged() -> Value {
        json!({"number": 1014, "state": "MERGED", "mergedAt": "2026-11-13T19:40:00Z", "mergeCommit": "fbbe565"})
    }

    fn deployment(environment: &str, state: Option<&str>) -> Value {
        let mut value = json!({"environment": environment, "sha": "fbbe565"});
        if let Some(state) = state {
            value["latestStatus"] = json!({
                "state": state, "at": "2026-11-13T19:52:22Z", "url": "https://loja.example.com"
            });
        }
        value
    }

    #[test]
    fn a_pull_request_is_live_only_when_its_commit_reached_production() {
        assert_eq!(verdict(&json!({"state": "OPEN"}), &[]), Shipped::NotMerged);
        // Merged, nothing registered: only that.
        assert_eq!(verdict(&merged(), &[]), Shipped::Merged);
        // A preview is not production.
        assert_eq!(
            verdict(&merged(), &[deployment("Preview", Some("success"))]),
            Shipped::Merged
        );
        assert_eq!(
            verdict(
                &merged(),
                &[
                    deployment("Preview", Some("success")),
                    deployment("Production", Some("success"))
                ]
            ),
            Shipped::Live {
                environment: "Production".into(),
                at: Some("2026-11-13T19:52:22Z".into()),
                url: Some("https://loja.example.com".into()),
            }
        );
        // Replaced by a newer one: it was live.
        assert!(matches!(
            verdict(&merged(), &[deployment("production", Some("inactive"))]),
            Shipped::Live { .. }
        ));
        assert_eq!(
            verdict(&merged(), &[deployment("Production", Some("in_progress"))]),
            Shipped::Deploying {
                environment: "Production".into()
            }
        );
        assert_eq!(
            verdict(&merged(), &[deployment("Production", None)]),
            Shipped::Deploying {
                environment: "Production".into()
            }
        );
        assert_eq!(
            verdict(&merged(), &[deployment("Production", Some("failure"))]),
            Shipped::DeployFailed {
                environment: "Production".into(),
                at: Some("2026-11-13T19:52:22Z".into()),
            }
        );
    }

    #[test]
    fn the_verdict_travels_as_json() {
        let live = Shipped::Live {
            environment: "Production".into(),
            at: None,
            url: None,
        };
        let value = serde_json::to_value(&live).unwrap();
        assert_eq!(value["state"], "live");
        assert_eq!(serde_json::from_value::<Shipped>(value).unwrap(), live);
        assert_eq!(
            serde_json::to_value(Shipped::Merged).unwrap()["state"],
            "merged"
        );
        // With when it merged, in one object.
        let read = ShippedRead {
            shipped: live,
            merged_at: Some("2026-11-13T19:40:00Z".into()),
        };
        let value = serde_json::to_value(&read).unwrap();
        assert_eq!(value["state"], "live");
        assert_eq!(value["mergedAt"], "2026-11-13T19:40:00Z");
        assert_eq!(serde_json::from_value::<ShippedRead>(value).unwrap(), read);
    }
}
