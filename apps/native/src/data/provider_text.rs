//! What Settings → Providers says about each provider (`catalog-text.ts`,
//! `provider-setup-message.ts`, `providers-settings.logic.ts`): the catalog
//! comes in English, so known providers, model descriptions and the setup
//! reasons DCC's adapters write are said through the catalog; anything else
//! shows as written. And the one line under each name in the list.

use dcc_core::domain::provider::{HealthStatus, ProviderDescriptor};

/// `KNOWN_PROVIDERS`: `catalogText.provider.<id>`.
const KNOWN_PROVIDERS: [&str; 7] = [
    "claude_code",
    "codex",
    "cursor",
    "grok",
    "antigravity",
    "gemini",
    "droid",
];

/// `MODEL_DESCRIPTION_KEYS`: exact backend description → `catalogText.model.<key>`.
const MODEL_DESCRIPTIONS: [(&str, &str); 24] = [
    (
        "Most capable Claude model for demanding reasoning and long-horizon agentic work.",
        "claudeFable",
    ),
    (
        "Everyday and complex tasks with a 1M-token context window.",
        "claudeOpus",
    ),
    ("Most efficient for simpler tasks.", "efficientSimple"),
    (
        "Fastest and cheapest Claude model for quick follow-ups and subagent work.",
        "claudeHaiku",
    ),
    (
        "Previous small model; use it on Claude Code older than 2.1.293.",
        "claudeHaikuPrevious",
    ),
    (
        "Frontier intelligence for the most demanding work.",
        "frontier",
    ),
    (
        "Latest workhorse model for coding and everyday work.",
        "latestWorkhorse",
    ),
    (
        "Fast and affordable model for easier tasks.",
        "fastAffordable",
    ),
    ("Previous generation workhorse model.", "previousWorkhorse"),
    ("Older generation workhorse model.", "olderWorkhorse"),
    (
        "Older balanced model for straightforward work.",
        "olderBalanced",
    ),
    ("Older fast and efficient model.", "olderFast"),
    ("Legacy coding model.", "legacyCoding"),
    (
        "Use Cursor's recommended model for this account.",
        "cursorAuto",
    ),
    (
        "Model reported by Cursor Agent CLI for this account.",
        "cursorReported",
    ),
    (
        "Latest Grok Build model for coding, agentic tasks, and knowledge work through the local ACP CLI.",
        "grokLatest",
    ),
    (
        "Use the default model offered to this Google account.",
        "antigravityDefault",
    ),
    (
        "Model available to the authenticated Antigravity account.",
        "antigravityAccount",
    ),
    (
        "Latest stable model for long-horizon coding and agentic workflows. CLI availability depends on the account rollout.",
        "geminiLatest",
    ),
    (
        "Stable long-context model with the broadest CLI compatibility.",
        "geminiStable",
    ),
    (
        "Use Droid's default model selection for this account.",
        "droidAuto",
    ),
    (
        "Fast Gemini option when available in the local Droid account.",
        "droidGeminiFast",
    ),
    (
        "High capability coding model routed through Droid.",
        "droidHighCapability",
    ),
    (
        "Latest high-reasoning OpenAI option exposed by Droid.",
        "droidOpenAiLatest",
    ),
];

/// `providerDescriptionText`: the key to try (then the catalog's own text).
pub fn provider_description_key(provider_id: &str) -> Option<String> {
    KNOWN_PROVIDERS
        .contains(&provider_id)
        .then(|| format!("catalogText.provider.{provider_id}"))
}

/// `modelDescriptionText`
pub fn model_description_key(description: &str) -> Option<String> {
    MODEL_DESCRIPTIONS
        .iter()
        .find(|(text, _)| *text == description.trim())
        .map(|(_, key)| format!("catalogText.model.{key}"))
}

/// `providerSetupMessage` + `localizedProviderSetupReason`: the key and its
/// `command` / `binary` values, for a reason DCC's adapters write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupMessage {
    pub key: &'static str,
    pub command: Option<String>,
    pub binary: Option<String>,
}

pub fn setup_message(reason: &str) -> Option<SetupMessage> {
    let lower = reason.to_lowercase();
    let message = |key| SetupMessage {
        key,
        command: None,
        binary: None,
    };
    if lower.contains("personal google sign-in is no longer supported") {
        return Some(message("providerSetup.personalSignInUnsupported"));
    }
    if lower
        .find("configured ")
        .is_some_and(|at| lower[at..].contains("executable path is invalid"))
    {
        return Some(message("providerSetup.invalidExecutable"));
    }
    if lower.contains("antigravity acp runtime not found")
        || lower.contains("antigravity acp runtime is not on path")
    {
        return Some(message("providerSetup.runtimeMissing"));
    }
    let signed_out = ["not authenticated", "not signed in", "not logged in"]
        .iter()
        .any(|phrase| contains_words(&lower, phrase));
    if signed_out {
        let command = reason
            .split('`')
            .nth(1)
            .filter(|_| reason.matches('`').count() >= 2)
            .map(str::trim)
            .filter(|command| !command.is_empty())
            .map(str::to_string);
        return Some(SetupMessage {
            key: if command.is_some() {
                "providerSetup.notSignedInCommand"
            } else {
                "providerSetup.notSignedIn"
            },
            command,
            binary: None,
        });
    }
    if let Some(rest) = lower.strip_prefix("failed to execute ")
        && rest.contains("no such file or directory")
    {
        let length = rest
            .find(|c: char| c.is_whitespace() || c == ':')
            .unwrap_or(rest.len());
        if length > 0 {
            let start = "failed to execute ".len();
            return Some(SetupMessage {
                key: "providerSetup.notInstalled",
                command: None,
                binary: Some(reason[start..start + length].to_string()),
            });
        }
    }
    None
}

/// `\bphrase\b`
fn contains_words(text: &str, phrase: &str) -> bool {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    text.match_indices(phrase).any(|(at, _)| {
        !word(text[..at].chars().next_back()) && !word(text[at + phrase.len()..].chars().next())
    })
}

/// The reason a provider needs setup, when its health has one.
pub fn health_reason(provider: &ProviderDescriptor) -> Option<&str> {
    match &provider.health {
        HealthStatus::Healthy => None,
        HealthStatus::Degraded { reason } | HealthStatus::Unhealthy { reason } => Some(reason),
    }
}

/// The dot's color under a provider's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Ready,
    Attention,
    Off,
}

/// `providerRailStatus`: `settings.model.<key>`, with the count for accounts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RailStatus {
    pub tone: Tone,
    pub key: &'static str,
    pub count: Option<i64>,
}

/// Off, needing setup or ready — and, with accounts, how many sign-ins it
/// has (the system login counts as one).
pub fn rail_status(provider: &ProviderDescriptor, named_accounts: usize) -> RailStatus {
    let status = |tone, key| RailStatus {
        tone,
        key,
        count: None,
    };
    if !provider.enabled {
        return status(Tone::Off, "settings.model.railOff");
    }
    if health_reason(provider).is_some() {
        return status(Tone::Attention, "settings.model.railAttention");
    }
    if provider.capabilities.supports_accounts && named_accounts > 0 {
        return RailStatus {
            tone: Tone::Ready,
            key: "settings.model.railAccounts",
            count: Some(named_accounts as i64 + 1),
        };
    }
    status(Tone::Ready, "settings.model.railReady")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_reasons_read_like_the_desktop() {
        let read = |reason: &str| setup_message(reason);
        assert_eq!(
            read("Grok is not signed in: run `grok login` first").unwrap(),
            SetupMessage {
                key: "providerSetup.notSignedInCommand",
                command: Some("grok login".into()),
                binary: None,
            }
        );
        assert_eq!(
            read("not authenticated").unwrap().key,
            "providerSetup.notSignedIn"
        );
        assert_eq!(read("knot signed int"), None);
        assert_eq!(
            read("the configured Codex executable path is invalid")
                .unwrap()
                .key,
            "providerSetup.invalidExecutable"
        );
        let missing =
            read("failed to execute codex: No such file or directory (os error 2)").unwrap();
        assert_eq!(missing.key, "providerSetup.notInstalled");
        assert_eq!(missing.binary.as_deref(), Some("codex"));
        assert_eq!(
            read("Antigravity ACP runtime is not on PATH").unwrap().key,
            "providerSetup.runtimeMissing"
        );
        assert_eq!(read("something else"), None);
        assert_eq!(
            model_description_key(" Older fast and efficient model. ").as_deref(),
            Some("catalogText.model.olderFast")
        );
        assert_eq!(provider_description_key("nope"), None);
    }

    #[test]
    fn the_rail_says_off_setup_accounts_or_ready() {
        let mut providers = dcc_providers::offline_catalog().providers;
        let provider = providers
            .iter_mut()
            .find(|provider| provider.capabilities.supports_accounts)
            .unwrap();
        provider.health = HealthStatus::Healthy;
        assert_eq!(rail_status(provider, 0).key, "settings.model.railReady");
        let accounts = rail_status(provider, 1);
        assert_eq!(
            (accounts.key, accounts.count),
            ("settings.model.railAccounts", Some(2))
        );
        provider.health = HealthStatus::Degraded { reason: "x".into() };
        assert_eq!(rail_status(provider, 1).tone, Tone::Attention);
        provider.enabled = false;
        assert_eq!(rail_status(provider, 1).tone, Tone::Off);
    }
}
