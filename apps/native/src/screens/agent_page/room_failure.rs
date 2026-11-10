//! Why a review the room shows failed: the provider's words in one line,
//! or what to do when it is a sign-in that expired.

use gpui::SharedString;

use crate::i18n::Translate;

/// The provider's error points at its sign-in.
pub(crate) fn is_sign_in(reason: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    [
        "authenticate",
        "oauth",
        "not logged in",
        "login",
        "unauthorized",
    ]
    .iter()
    .any(|word| reason.contains(word))
}

pub(super) fn reason_line(reason: &str, cx: &gpui::App) -> SharedString {
    if is_sign_in(reason) {
        return cx.t("agents.room.card.signInExpired");
    }
    let line = reason.lines().next().unwrap_or(reason).trim();
    let mut short: String = line.chars().take(160).collect();
    if line.chars().count() > 160 {
        short.push('…');
    }
    short.into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_expired_sign_in_is_told_apart() {
        assert!(super::is_sign_in(
            "provider error: Failed to authenticate: OAuth session expired and could not be refreshed"
        ));
        assert!(!super::is_sign_in("gh: pull request not found"));
    }
}
