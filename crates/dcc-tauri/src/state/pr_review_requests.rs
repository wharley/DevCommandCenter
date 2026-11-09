//! Pull requests an agent asks DCC to review with `dcc_review_pull_request`
//! (`dcc_core::domain::pr_review_request`): kept until the app takes each
//! one and runs the hub's review.

use dcc_core::domain::pr_review_request::parse_pull_request_url;

use super::*;

impl SessionCommandState {
    /// The links the agent of `session_id` sends; its name and how many
    /// were added come back. A link that is not a pull request's is refused.
    pub fn request_pr_reviews_for_session(
        &self,
        session_id: &SessionId,
        urls: &[String],
    ) -> Result<(String, usize)> {
        let agent = self
            .session_repo
            .load_session_agent(session_id)?
            .ok_or_else(|| {
                dcc_core::CoreError::InvalidInput("this session runs as no DCC agent".into())
            })?;
        let pulls = urls
            .iter()
            .map(|url| {
                parse_pull_request_url(url)
                    .map(|(_, repository, number)| (url.trim().to_string(), repository, number))
                    .ok_or_else(|| {
                        dcc_core::CoreError::InvalidInput(format!(
                            "not a GitHub pull request link: {url}"
                        ))
                    })
            })
            .collect::<Result<Vec<_>>>()?;
        let added = self
            .session_repo
            .add_pr_review_requests(&agent.id, &session_id.0, &pulls)?;
        Ok((agent.name, added))
    }

    pub fn take_pr_review_request(&self, id: &str) -> Result<bool> {
        self.session_repo.take_pr_review_request(id)
    }
}
