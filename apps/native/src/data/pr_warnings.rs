//! What to say of a project the hub's list could not read: the forge CLI's
//! error boiled down to its cause, without the request it printed.

const MAX_REASON: usize = 160;

/// The cause in a CLI's error: what follows the request (`Get "url": …`)
/// when it has that shape, on one line, cut short.
pub fn reason(message: &str) -> String {
    let flat = message.split_whitespace().collect::<Vec<_>>().join(" ");
    let flat = flat.trim_start_matches("ERROR").trim();
    // Go's `Get "https://…": dial tcp: …`: the cause is after the address.
    let cause = flat
        .rfind("\": ")
        .map(|at| flat[at + 3..].trim())
        .filter(|cause| !cause.is_empty())
        .unwrap_or(flat);
    if cause.chars().count() > MAX_REASON {
        let cut: String = cause.chars().take(MAX_REASON).collect();
        format!("{}…", cut.trim_end())
    } else {
        cause.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cause_is_kept_and_the_request_dropped() {
        let glab = "ERROR  \n   \n  Get \"https://gitlab.example.com/api/v4/projects/a%2Fb-   \n  c/merge_requests?state=opened\": dial tcp: lookup gitlab.example.com: no such host";
        assert_eq!(
            reason(glab),
            "dial tcp: lookup gitlab.example.com: no such host"
        );
        assert_eq!(reason("  gh: not logged in \n"), "gh: not logged in");
        assert!(reason(&"x".repeat(400)).ends_with('…'));
    }
}
