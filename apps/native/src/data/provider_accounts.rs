//! Settings → Providers → Contas (`provider-accounts.ts`, `provider-api.ts`):
//! the named sign-ins the desktop keeps per provider (each with its own
//! config folder), who each runtime says it is signed in as, and signing in
//! through the browser. All of it goes over `/api/v1/provider-accounts`.

use std::time::Duration;

use dcc_core::domain::provider::{ProviderAccount, ProviderAccountStatus};
use serde::Deserialize;
use serde_json::{Value, json};

use super::desktop_api::{ApiError, DesktopApi};

/// A browser sign-in may take this long before it gives up.
const SIGN_IN_WAIT: Duration = Duration::from_secs(15 * 60);

/// What the desktop answers when the person cancelled the sign-in.
pub const SIGN_IN_CANCELLED: &str = "sign-in cancelled";

/// `ProviderAccountStatusOutput`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    pub status: Option<ProviderAccountStatus>,
    /// What to run in a terminal to sign this account in.
    pub login_command: Option<String>,
}

impl AccountStatus {
    pub fn signed_in(&self) -> bool {
        self.status.as_ref().is_some_and(|status| status.logged_in)
    }

    /// `[email, plan].filter(Boolean).join(" · ")`
    pub fn who(&self) -> Option<String> {
        let status = self.status.as_ref().filter(|status| status.logged_in)?;
        let parts: Vec<&str> = [status.email.as_deref(), status.plan.as_deref()]
            .into_iter()
            .flatten()
            .filter(|part| !part.is_empty())
            .collect();
        (!parts.is_empty()).then(|| parts.join(" · "))
    }
}

/// `accountsOfProvider`: one provider's accounts, oldest first.
pub fn of_provider<'a>(
    accounts: &'a [ProviderAccount],
    provider_id: &str,
) -> Vec<&'a ProviderAccount> {
    let mut mine: Vec<&ProviderAccount> = accounts
        .iter()
        .filter(|account| account.provider_id.0 == provider_id)
        .collect();
    mine.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    mine
}

fn account_body(provider_id: &str, account_id: Option<&str>) -> Value {
    json!({ "providerId": provider_id, "accountId": account_id })
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
}

impl DesktopApi {
    /// `listProviderAccounts`; the list is also what a pick is checked against.
    pub fn provider_accounts(&self) -> Result<Vec<ProviderAccount>, ApiError> {
        let value = self.get("/api/v1/provider-accounts")?;
        let accounts: Vec<ProviderAccount> =
            decode(value.get("accounts").cloned().unwrap_or(Value::Null))?;
        super::account_selection::remember(&accounts);
        Ok(accounts)
    }

    /// `createProviderAccount`
    pub fn create_provider_account(
        &self,
        provider_id: &str,
        label: &str,
    ) -> Result<ProviderAccount, ApiError> {
        let value = self.post(
            "/api/v1/provider-accounts",
            &json!({ "providerId": provider_id, "label": label }),
        )?;
        decode(value.get("account").cloned().unwrap_or(Value::Null))
    }

    /// `renameProviderAccount`
    pub fn rename_provider_account(&self, account_id: &str, label: &str) -> Result<(), ApiError> {
        self.put(
            &format!(
                "/api/v1/provider-accounts/{}",
                super::desktop_api::encode(account_id)
            ),
            &json!({ "label": label }),
        )
        .map(|_| ())
    }

    /// `removeProviderAccount`: its folder and login stay on disk.
    pub fn remove_provider_account(&self, account_id: &str) -> Result<(), ApiError> {
        self.request_waiting(
            "DELETE",
            &format!(
                "/api/v1/provider-accounts/{}",
                super::desktop_api::encode(account_id)
            ),
            None,
            Duration::from_secs(20),
        )
        .map(|_| ())
    }

    /// `getProviderAccountStatus`; `None` is the system login.
    pub fn provider_account_status(
        &self,
        provider_id: &str,
        account_id: Option<&str>,
    ) -> Result<AccountStatus, ApiError> {
        decode(self.post(
            "/api/v1/provider-accounts/status",
            &account_body(provider_id, account_id),
        )?)
    }

    /// `signInProviderAccount`: answers once the person authorized in the
    /// browser, cancelled (`SIGN_IN_CANCELLED`) or the sign-in gave up.
    pub fn sign_in_provider_account(
        &self,
        provider_id: &str,
        account_id: Option<&str>,
    ) -> Result<ProviderAccountStatus, ApiError> {
        let value = self.request_waiting(
            "POST",
            "/api/v1/provider-accounts/sign-in",
            Some(&account_body(provider_id, account_id)),
            SIGN_IN_WAIT,
        )?;
        decode(value.get("status").cloned().unwrap_or(Value::Null))
    }

    /// `cancelProviderAccountSignIn`
    pub fn cancel_provider_sign_in(
        &self,
        provider_id: &str,
        account_id: Option<&str>,
    ) -> Result<(), ApiError> {
        self.post(
            "/api/v1/provider-accounts/sign-in/cancel",
            &account_body(provider_id, account_id),
        )
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_and_order_read_like_the_desktop() {
        let status: AccountStatus = serde_json::from_value(json!({
            "status": { "loggedIn": true, "email": "ana@zedy.com", "plan": "max" },
            "loginCommand": "claude login",
        }))
        .unwrap();
        assert!(status.signed_in());
        assert_eq!(status.who().as_deref(), Some("ana@zedy.com · max"));
        let signed_out: AccountStatus = serde_json::from_value(
            json!({ "status": { "loggedIn": false }, "loginCommand": null }),
        )
        .unwrap();
        assert!(!signed_out.signed_in());
        assert_eq!(signed_out.who(), None);
        let accounts: Vec<ProviderAccount> = serde_json::from_value(json!([
            { "id": "b", "providerId": "claude_code", "label": "B", "configDir": "/b", "managed": true, "createdAt": "2026-10-02" },
            { "id": "x", "providerId": "codex", "label": "X", "configDir": "/x", "managed": true, "createdAt": "2026-10-01" },
            { "id": "a", "providerId": "claude_code", "label": "A", "configDir": "/a", "managed": true, "createdAt": "2026-10-01" },
        ]))
        .unwrap();
        let ids: Vec<&str> = of_provider(&accounts, "claude_code")
            .iter()
            .map(|account| account.id.as_str())
            .collect();
        assert_eq!(ids, ["a", "b"]);
    }
}
