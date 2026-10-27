//! Settings → Conta (`forge-cli.ts`, `ForgeCliIntegrationCard`): who the
//! GitHub and GitLab CLIs are signed in as on each host, the hosts they
//! know, the login DCC uses there, and `gh`/`glab auth login` for the
//! embedded terminal. Over `/api/v1/forge/*`.

use serde::Deserialize;
use serde_json::{Value, json};

use super::desktop_api::{ApiError, DesktopApi};

/// `ForgeCliProvider`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Forge {
    Github,
    Gitlab,
}

impl Forge {
    pub const ALL: [Forge; 2] = [Forge::Github, Forge::Gitlab];

    pub fn key(self) -> &'static str {
        match self {
            Forge::Github => "github",
            Forge::Gitlab => "gitlab",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Forge::Github => "GitHub",
            Forge::Gitlab => "GitLab",
        }
    }

    pub fn cli(self) -> &'static str {
        match self {
            Forge::Github => "gh",
            Forge::Gitlab => "glab",
        }
    }

    /// `DEFAULT_FORGE_HOSTS`
    pub fn default_host(self) -> &'static str {
        match self {
            Forge::Github => "github.com",
            Forge::Gitlab => "gitlab.com",
        }
    }

    /// `normalizeForgeHost`
    pub fn host(self, typed: &str) -> String {
        let typed = typed.trim();
        if typed.is_empty() {
            self.default_host().to_string()
        } else {
            typed.to_string()
        }
    }

    /// `buildForgeCliDisplayCommand`
    pub fn login_command(self, host: &str) -> String {
        let host = self.host(host);
        match self {
            Forge::Github if host == "github.com" => "gh auth login".into(),
            Forge::Github => format!("gh auth login --hostname {host}"),
            Forge::Gitlab => format!("glab auth login --hostname {host}"),
        }
    }

    /// `buildForgeCliShellCommand`: the host quoted for the shell.
    pub fn shell_command(self, host: &str) -> String {
        let host = self.host(host);
        let quoted = format!("'{}'", host.replace('\'', "'\\''"));
        match self {
            Forge::Github if host == "github.com" => "gh auth login".into(),
            Forge::Github => format!("gh auth login --hostname {quoted}"),
            Forge::Gitlab => format!("glab auth login --hostname {quoted}"),
        }
    }
}

/// `ForgeCliAccountEntry`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CliAccount {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub email: Option<String>,
}

impl CliAccount {
    /// `name · @login`, or the login.
    pub fn label(&self) -> String {
        match &self.name {
            Some(name) if !name.is_empty() => format!("{name} · @{}", self.login),
            _ => self.login.clone(),
        }
    }

    /// `[name, email, @login].filter(Boolean).join(" · ")`
    pub fn title(&self) -> String {
        [
            self.name.clone(),
            self.email.clone(),
            Some(format!("@{}", self.login)),
        ]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
    }
}

/// `ForgeCliAccountsOutput`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CliAccounts {
    /// `ready` | `error`
    pub status: String,
    pub login: Option<String>,
    pub selected_login: Option<String>,
    pub accounts: Vec<CliAccount>,
    pub message: String,
    pub login_command: String,
}

impl CliAccounts {
    /// What the card shows when the CLI could not be read.
    pub fn unreadable(forge: Forge, host: &str, message: String) -> Self {
        Self {
            status: "error".into(),
            message,
            login_command: forge.login_command(host),
            ..Self::default()
        }
    }

    pub fn ready(&self) -> bool {
        self.status == "ready"
    }

    /// The chosen login while it is still signed in, else the CLI's own,
    /// else the first.
    pub fn effective_login(&self) -> Option<String> {
        self.selected_login
            .clone()
            .filter(|login| self.accounts.iter().any(|account| &account.login == login))
            .or_else(|| self.login.clone())
            .or_else(|| self.accounts.first().map(|account| account.login.clone()))
    }

    pub fn logins(&self) -> Vec<String> {
        self.accounts
            .iter()
            .map(|account| account.login.clone())
            .collect()
    }
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
}

impl DesktopApi {
    /// `getForgeCliAccounts`
    pub fn forge_cli_accounts(
        &self,
        forge: Forge,
        host: &str,
        force: bool,
    ) -> Result<CliAccounts, ApiError> {
        decode(self.post(
            "/api/v1/forge/accounts",
            &json!({ "provider": forge.key(), "host": forge.host(host), "forceRefresh": force }),
        )?)
    }

    /// `getForgeCliStatus`: the logins on the host.
    pub fn forge_cli_logins(&self, forge: Forge, host: &str) -> Result<Vec<String>, ApiError> {
        let value = self.post(
            "/api/v1/forge/cli-status",
            &json!({ "provider": forge.key(), "host": forge.host(host), "forceRefresh": true }),
        )?;
        decode(
            value
                .get("logins")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
        )
    }

    /// `getForgeCliHosts`
    pub fn forge_cli_hosts(&self, forge: Forge) -> Result<Vec<String>, ApiError> {
        let value = self.post(
            "/api/v1/forge/hosts",
            &json!({ "provider": forge.key(), "forceRefresh": null }),
        )?;
        decode(
            value
                .get("hosts")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
        )
    }

    /// `setForgeCliSelectedLogin`
    pub fn select_forge_login(
        &self,
        forge: Forge,
        host: &str,
        login: &str,
    ) -> Result<(), ApiError> {
        self.post(
            "/api/v1/forge/select-login",
            &json!({ "provider": forge.key(), "host": forge.host(host), "login": login }),
        )
        .map(|_| ())
    }

    /// `backfillForgeRepoBindings`: how many repositories got the login.
    pub fn backfill_forge_bindings(&self) -> Result<usize, ApiError> {
        let value = self.post("/api/v1/forge/backfill-bindings", &json!({}))?;
        Ok(value.get("bound").and_then(Value::as_u64).unwrap_or(0) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_commands_and_logins_read_like_the_desktop() {
        assert_eq!(Forge::Github.host("  "), "github.com");
        assert_eq!(Forge::Github.login_command(""), "gh auth login");
        assert_eq!(
            Forge::Github.login_command("git.zedy.com"),
            "gh auth login --hostname git.zedy.com"
        );
        assert_eq!(
            Forge::Gitlab.shell_command("it's.lab"),
            "glab auth login --hostname 'it'\\''s.lab'"
        );
        let accounts: CliAccounts = serde_json::from_value(json!({
            "provider": "github", "cliName": "gh", "hostname": "github.com",
            "status": "ready", "login": "ana", "selectedLogin": "gone",
            "accounts": [
                { "login": "ana", "name": "Ana Lima", "email": "ana@x.com", "active": true, "selected": false },
                { "login": "bia", "active": false, "selected": false },
            ],
            "message": "ok", "loginCommand": "gh auth login",
        }))
        .unwrap();
        assert!(accounts.ready());
        // The chosen login is no longer signed in: the CLI's own.
        assert_eq!(accounts.effective_login().as_deref(), Some("ana"));
        assert_eq!(accounts.accounts[0].label(), "Ana Lima · @ana");
        assert_eq!(accounts.accounts[0].title(), "Ana Lima · ana@x.com · @ana");
        assert_eq!(accounts.accounts[1].label(), "bia");
    }
}
