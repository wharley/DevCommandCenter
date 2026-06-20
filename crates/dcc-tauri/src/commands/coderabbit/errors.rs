use super::CodeRabbitReviewErrorKind;

pub(crate) fn classify_review_error(
    errors: &[String],
    stderr: &str,
    stdout: &str,
    timed_out: bool,
    exit_code: Option<i32>,
) -> Option<CodeRabbitReviewErrorKind> {
    if timed_out {
        return Some(CodeRabbitReviewErrorKind::Timeout);
    }

    let mut text = String::new();
    for error in errors {
        text.push_str(error);
        text.push('\n');
    }
    text.push_str(stderr);
    text.push('\n');
    text.push_str(stdout);
    let text = text.to_ascii_lowercase();

    if text.trim().is_empty() && exit_code.is_none() {
        return None;
    }

    if contains_any(
        &text,
        &[
            "not found",
            "no such file",
            "cannot find",
            "failed to spawn",
            "os error 2",
        ],
    ) && contains_any(&text, &["coderabbit", " cr ", "`cr`", "cli"])
    {
        return Some(CodeRabbitReviewErrorKind::CliUnavailable);
    }
    if contains_any(
        &text,
        &[
            "auth",
            "login",
            "log in",
            "unauthorized",
            "not authenticated",
            "token expired",
            "invalid token",
            "401",
        ],
    ) {
        return Some(CodeRabbitReviewErrorKind::Auth);
    }
    if contains_any(
        &text,
        &[
            "permission",
            "forbidden",
            "access denied",
            "not allowed",
            "organization",
            "403",
        ],
    ) {
        return Some(CodeRabbitReviewErrorKind::Permission);
    }
    if contains_any(
        &text,
        &[
            "rate limit",
            "too many requests",
            "quota",
            "throttle",
            "429",
        ],
    ) {
        return Some(CodeRabbitReviewErrorKind::RateLimit);
    }
    if contains_any(
        &text,
        &[
            "no diff",
            "no changes",
            "nothing to review",
            "empty diff",
            "no files changed",
        ],
    ) {
        return Some(CodeRabbitReviewErrorKind::NoDiff);
    }
    if contains_any(
        &text,
        &[
            "network",
            "timeout",
            "timed out",
            "connection refused",
            "connection reset",
            "dns",
            "tls",
            "certificate",
        ],
    ) {
        return Some(CodeRabbitReviewErrorKind::Network);
    }
    if contains_any(
        &text,
        &[
            "not a git repository",
            "git",
            "merge-base",
            "base branch",
            "revision",
            "bad object",
        ],
    ) {
        return Some(CodeRabbitReviewErrorKind::Git);
    }
    if contains_any(
        &text,
        &["config", "configuration", "yaml", "toml", "invalid option"],
    ) {
        return Some(CodeRabbitReviewErrorKind::Config);
    }
    if contains_any(&text, &["not valid json", "jsonl", "parse"]) {
        return Some(CodeRabbitReviewErrorKind::Parse);
    }

    Some(CodeRabbitReviewErrorKind::Unknown)
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::classify_review_error;
    use crate::commands::coderabbit::CodeRabbitReviewErrorKind;

    #[test]
    fn classifies_common_cli_failures() {
        assert_eq!(
            classify_review_error(&[], "", "", true, None),
            Some(CodeRabbitReviewErrorKind::Timeout)
        );
        assert_eq!(
            classify_review_error(
                &["not authenticated; run cr auth login".into()],
                "",
                "",
                false,
                Some(1)
            ),
            Some(CodeRabbitReviewErrorKind::Auth)
        );
        assert_eq!(
            classify_review_error(
                &["HTTP 429 rate limit exceeded".into()],
                "",
                "",
                false,
                Some(1)
            ),
            Some(CodeRabbitReviewErrorKind::RateLimit)
        );
        assert_eq!(
            classify_review_error(
                &["nothing to review: no diff".into()],
                "",
                "",
                false,
                Some(0)
            ),
            Some(CodeRabbitReviewErrorKind::NoDiff)
        );
    }

    #[test]
    fn keeps_unknown_for_unmatched_failures() {
        assert_eq!(
            classify_review_error(&["unexpected failure".into()], "", "", false, Some(1)),
            Some(CodeRabbitReviewErrorKind::Unknown)
        );
    }
}
