//! A pull request's check, looked at from the hub: whether its link is a
//! GitHub Actions job (the only kind whose log is read and that can be run
//! again from here), the failing step's excerpt, and the re-run.

use serde_json::{Value, json};

use super::desktop_api::{ApiError, DesktopApi};
use super::pr_hub::Item;

/// `…/actions/runs/<run>/job/<job>`: a job of GitHub Actions.
pub fn is_actions_job(link: &str) -> bool {
    link.split_once("/actions/runs/")
        .and_then(|(_, rest)| rest.split_once("/job/"))
        .is_some_and(|(run, job)| {
            let starts_with_digit = |text: &str| text.starts_with(|c: char| c.is_ascii_digit());
            starts_with_digit(run) && starts_with_digit(job)
        })
}

fn body(item: &Item, link: &str) -> Value {
    json!({
        "repositoryRoot": item.repository_root,
        "forgeLogin": item.forge_login,
        "detailsUrl": link,
    })
}

impl DesktopApi {
    /// What failed in the check's job: the failing step, cut to its errors.
    pub fn pr_hub_check_log(&self, item: &Item, link: &str) -> Result<String, ApiError> {
        let value = self.post("/api/v1/pull-request-hub/check-log", &body(item, link))?;
        Ok(value
            .get("excerpt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string())
    }

    /// Runs the check's job again.
    pub fn pr_hub_check_rerun(&self, item: &Item, link: &str) -> Result<(), ApiError> {
        self.post("/api/v1/pull-request-hub/check-rerun", &body(item, link))
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_actions_job_has_a_log_to_read_here() {
        assert!(is_actions_job(
            "https://github.com/acme/shop/actions/runs/123/job/456"
        ));
        assert!(!is_actions_job(
            "https://github.com/acme/shop/actions/runs/123"
        ));
        assert!(!is_actions_job(
            "https://ci.example.com/actions/runs/x/job/y"
        ));
        assert!(!is_actions_job(""));
    }
}
