//! The providers the composer offers while the desktop does not answer: the
//! built-in descriptors (`dcc_providers::offline_catalog`), then the same
//! catalog checked here (`provider_catalog`: each CLI's health, discovered
//! models), both with the on/off switch the person set in the desktop
//! (`dcc_provider_availability`; a provider with no row is on). Providers
//! turned off or not set up stay out of the picker, as in the desktop.
//! Blocking: call it off the UI thread.

use std::path::Path;

use dcc_core::domain::provider::ProviderDescriptor;
use dcc_infra::db::SqliteSessionRepo;

/// Where an installed DCC keeps the Claude helper the Claude provider runs
/// (`dcc-claude-sidecar`, beside the desktop's executable).
fn installed_claude_helper() -> Option<std::path::PathBuf> {
    // This app's own bundle first (`yarn native:local` ships the helper).
    let own = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("dcc-claude-sidecar")))
        .filter(|path| path.is_file());
    if own.is_some() {
        return own;
    }
    let name = "Dev Command Center.app/Contents/MacOS/dcc-claude-sidecar";
    let mut roots = vec![std::path::PathBuf::from("/Applications")];
    roots.extend(dirs::home_dir().map(|home| home.join("Applications")));
    roots
        .into_iter()
        .map(|root| root.join(name))
        .find(|path| path.is_file())
}

/// The Claude provider's helper: the one in this app's bundle, or else the
/// installed desktop's. Call at startup, before any thread starts.
pub fn locate_claude_helper() {
    const VARIABLE: &str = "DCC_CLAUDE_SIDECAR_PATH";
    if std::env::var_os(VARIABLE).is_some() {
        return;
    }
    if let Some(helper) = installed_claude_helper() {
        // SAFETY: called from `main` before the app starts any thread.
        unsafe { std::env::set_var(VARIABLE, helper) };
    }
}

/// The built-in list with the switches: instant.
pub fn offline(db_path: &Path) -> Vec<ProviderDescriptor> {
    with_switches(dcc_providers::offline_catalog().providers, db_path)
}

/// The list with each provider's health checked here (runs their CLIs, a few
/// seconds at most), with the switches. `None` when it could not run.
pub fn checked(db_path: &Path) -> Option<Vec<ProviderDescriptor>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    let catalog = runtime.block_on(dcc_providers::provider_catalog());
    Some(with_switches(catalog.providers, db_path))
}

fn with_switches(
    mut providers: Vec<ProviderDescriptor>,
    db_path: &Path,
) -> Vec<ProviderDescriptor> {
    let Ok(sessions) = db_path
        .exists()
        .then(|| SqliteSessionRepo::open_read_only(db_path))
        .transpose()
    else {
        return providers;
    };
    let Some(sessions) = sessions else {
        return providers;
    };
    for provider in &mut providers {
        if let Ok(Some(record)) = sessions.load_provider_availability(&provider.id.0) {
            provider.enabled = record.enabled;
            provider.availability_generation = record.generation;
        }
    }
    providers
}

impl crate::data::desktop_api::DesktopApi {
    /// `set_provider_availability`: the desktop owns the switch (it stops
    /// the provider's running turns when it goes off).
    pub fn set_provider_availability(
        &self,
        provider_id: &str,
        enabled: bool,
    ) -> Result<(), crate::data::desktop_api::ApiError> {
        self.put(
            &format!("/api/v1/providers/{provider_id}/availability"),
            &serde_json::json!({ "enabled": enabled }),
        )
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use dcc_infra::db::ProviderAvailabilityRecord;

    use super::*;

    #[test]
    fn providers_turned_off_in_the_desktop_stay_off() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("database.sqlite");
        let repo = SqliteSessionRepo::open(&path).unwrap();
        repo.save_provider_availability(&ProviderAvailabilityRecord {
            provider_id: "gemini".into(),
            enabled: false,
            generation: 2,
            updated_at_ms: 1,
        })
        .unwrap();
        drop(repo);
        let providers = offline(&path);
        let gemini = providers
            .iter()
            .find(|provider| provider.id.0 == "gemini")
            .unwrap();
        assert!(!gemini.enabled);
        let claude = providers
            .iter()
            .find(|provider| provider.id.0 == "claude_code")
            .unwrap();
        assert!(claude.enabled, "no row: on");
        assert!(
            offline(&dir.path().join("missing.sqlite"))
                .iter()
                .all(|provider| provider.enabled)
        );
    }
}

#[cfg(test)]
mod machine {
    /// Runs the real checks on this machine: `cargo test -- --ignored checks_this_machine --nocapture`.
    #[test]
    #[ignore = "runs the providers' CLIs"]
    fn checks_this_machine() {
        super::locate_claude_helper();
        let started = std::time::Instant::now();
        let providers = super::checked(&crate::data::loader::database_path()).expect("ran");
        for provider in &providers {
            println!(
                "{} enabled={} health={:?} models={}",
                provider.id.0,
                provider.enabled,
                provider.health,
                provider.models.len()
            );
        }
        println!("took {:?}", started.elapsed());
    }
}
