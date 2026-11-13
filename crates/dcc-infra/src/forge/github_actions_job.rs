//! A GitHub Actions job a pull request's check points at: found from the
//! check's link, and run again when the person asks.

/// The run and the job of an Actions link
/// (`…/actions/runs/<run>/job/<job>`); none for a check of another system.
pub fn run_and_job(link: &str) -> Option<(u64, u64)> {
    let (_, rest) = link.split_once("/actions/runs/")?;
    let (run, job) = rest.split_once("/job/")?;
    let digits = |text: &str| -> Option<u64> {
        text.split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()
    };
    Some((digits(run)?, digits(job)?))
}

/// Runs one job of a workflow run again.
pub fn rerun_job(
    root: &str,
    host: &str,
    namespace: &str,
    repo: &str,
    job_id: u64,
    login: Option<&str>,
) -> Result<(), String> {
    let endpoint = format!("/repos/{namespace}/{repo}/actions/jobs/{job_id}/rerun");
    super::github_merge::run(
        root,
        host,
        login,
        &["api", "--hostname", host, "-X", "POST", &endpoint],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_actions_link_names_a_job() {
        assert_eq!(
            run_and_job("https://github.com/acme/shop/actions/runs/123/job/456?pr=7"),
            Some((123, 456))
        );
        assert_eq!(
            run_and_job("https://github.com/acme/shop/actions/runs/123"),
            None
        );
        assert_eq!(run_and_job("https://ci.example.com/build/9"), None);
    }
}
