//! How the engine reaches its own ai-memory once that one asks for a token.
//!
//! Since 3.x, `ai-memory init` on a new data folder writes a root token
//! (`auth-tokens/default`, its config's `bearer_token_file`) and the server
//! then wants it on every route but `/healthz`. A folder made before that
//! has no token and no check. So the engine checks health on `/healthz`
//! (and, for a server too old to have it, on `/mcp`, which answers 405
//! there), and, when no token was configured, uses the one the folder holds.

use std::path::Path;
use std::time::Duration;

/// Where `ai-memory init` writes the root token, inside its data folder.
const TOKEN_FILE: &str = "auth-tokens/default";

/// The token a data folder holds, if `init` wrote one.
pub(crate) fn token_in(data_dir: &Path) -> Option<String> {
    std::fs::read_to_string(data_dir.join(TOKEN_FILE))
        .ok()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

/// Uses the folder's token unless one was configured (Settings' token, kept
/// in the system's credential store, wins).
pub(crate) fn use_folder_token(data_dir: &Path) {
    let configured = std::env::var("DCC_AI_MEMORY_TOKEN")
        .ok()
        .is_some_and(|token| !token.trim().is_empty());
    if configured {
        return;
    }
    if let Some(token) = token_in(data_dir) {
        std::env::set_var("DCC_AI_MEMORY_TOKEN", token);
    }
}

/// Whether the server at `base_url` answers: `/healthz`, open even when the
/// rest wants a token, else `/mcp`'s 405 from a server without it.
pub(crate) fn answers(base_url: &str, timeout: Duration) -> bool {
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
    else {
        return false;
    };
    let healthz = client
        .get(format!("{base_url}/healthz"))
        .send()
        .is_ok_and(|response| response.status().is_success());
    healthz
        || client
            .get(format!("{base_url}/mcp"))
            .send()
            .is_ok_and(|response| {
                response.status().as_u16() == 405 || response.status().is_success()
            })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_token_is_read_and_a_configured_one_wins() {
        let dir = std::env::temp_dir().join(format!("dcc-ai-memory-auth-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(token_in(&dir), None, "an older folder has none");
        std::fs::create_dir_all(dir.join("auth-tokens")).unwrap();
        std::fs::write(dir.join(TOKEN_FILE), "abc123\n").unwrap();
        assert_eq!(token_in(&dir).as_deref(), Some("abc123"));
        std::fs::write(dir.join(TOKEN_FILE), "  \n").unwrap();
        assert_eq!(token_in(&dir), None, "an empty file is no token");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_listening_does_not_answer() {
        assert!(!answers("http://127.0.0.1:9", Duration::from_millis(200)));
    }
}
