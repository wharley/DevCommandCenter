use dcc_core::domain::{
    model_registry,
    provider::{
        Capabilities, HealthStatus, McpOauthSupport, McpSupportLevel, ProviderApprovalPolicy,
        ProviderDescriptor, ProviderId, TurnControlSupport,
    },
};

use std::path::{Path, PathBuf};

use crate::{claude_sdk_sidecar::ClaudeSdkSidecarAdapter, common::stable_cli_capabilities};

pub fn adapter() -> ClaudeSdkSidecarAdapter {
    ClaudeSdkSidecarAdapter::new(
        "claude_code",
        "Claude Code",
        "Claude SDK-backed provider for agentic coding and tool use.",
        claude_code_capabilities(),
        true,
    )
}

fn claude_code_capabilities() -> Capabilities {
    let mut capabilities = stable_cli_capabilities();
    // The sidecar keeps the turn's prompt open and folds guidance in mid-turn.
    capabilities.supports_steering = true;
    capabilities.mcp_support = McpSupportLevel::NativeConfig;
    capabilities.mcp_oauth_support = McpOauthSupport::ManagedDuringTurn;
    capabilities.can_request_delegation = true;
    capabilities.supports_multi_root = true;
    capabilities.approval_policies = vec![
        // Enforced by the runtime sandbox; used for read-only delegated children.
        ProviderApprovalPolicy::ReadOnly,
        ProviderApprovalPolicy::Ask,
        ProviderApprovalPolicy::Auto,
        ProviderApprovalPolicy::FullAccess,
    ];
    capabilities.supports_runtime_home = true;
    capabilities.supports_accounts = true;
    capabilities.supports_account_usage = true;
    capabilities.plan_mode_support = TurnControlSupport::Native;
    capabilities.fast_mode_support = TurnControlSupport::Native;
    capabilities.supports_compaction_command = true;
    capabilities
}

pub fn descriptor(health: HealthStatus) -> ProviderDescriptor {
    ProviderDescriptor {
        id: ProviderId("claude_code".to_string()),
        label: "Claude Code".to_string(),
        description: "Claude SDK-backed provider for agentic coding and tool use.".to_string(),
        models: model_registry::CLAUDE_CODE
            .iter()
            .map(|m| m.to_descriptor())
            .collect(),
        capabilities: claude_code_capabilities(),
        health,
        enabled: true,
        availability_generation: 0,
        stable: true,
    }
}

/// Entries of the system Claude config a new account reuses, so it starts
/// with the person's instructions, skills, agents and settings. The login,
/// conversation history and `.claude.json` (which names the signed-in
/// account) stay per account.
const CLAUDE_SHARED_CONFIG_ENTRIES: &[&str] = &[
    "CLAUDE.md",
    "settings.json",
    "keybindings.json",
    "skills",
    "agents",
    "commands",
    "output-styles",
    "hooks",
    "plugins",
];

/// The config directory of the system Claude login.
pub fn claude_system_config_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".claude")))
}

/// Links the shared entries of the system config into an account directory.
/// Entries the account already has, or the system lacks, are left alone.
pub fn link_claude_shared_config(account_dir: &Path) -> std::io::Result<()> {
    let Some(system_dir) = claude_system_config_dir() else {
        return Ok(());
    };
    link_shared_entries(&system_dir, account_dir)
}

fn link_shared_entries(system_dir: &Path, account_dir: &Path) -> std::io::Result<()> {
    for entry in CLAUDE_SHARED_CONFIG_ENTRIES {
        let source = system_dir.join(entry);
        let target = account_dir.join(entry);
        if !source.exists() || target.symlink_metadata().is_ok() {
            continue;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&source, &target)?;
    }
    Ok(())
}

/// What the person runs to sign an account in. Claude keeps the login under
/// the directory, so the same variable selects it later.
pub fn claude_account_login_command(config_dir: &str) -> String {
    format!(
        "CLAUDE_CONFIG_DIR='{}' claude auth login",
        config_dir.replace('\'', r"'\''")
    )
}

#[cfg(test)]
mod account_tests {
    use super::*;

    #[test]
    fn links_only_shared_entries_the_system_config_has() {
        let root = std::env::temp_dir().join(format!("dcc-claude-link-{}", uuid::Uuid::new_v4()));
        let system = root.join("system");
        let account = root.join("account");
        std::fs::create_dir_all(system.join("skills")).unwrap();
        std::fs::write(system.join("CLAUDE.md"), "rules").unwrap();
        std::fs::write(system.join(".credentials.json"), "secret").unwrap();
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(account.join("settings.json"), "{}").unwrap();

        link_shared_entries(&system, &account).unwrap();

        assert!(account.join("skills").symlink_metadata().unwrap().is_symlink());
        assert_eq!(std::fs::read_to_string(account.join("CLAUDE.md")).unwrap(), "rules");
        assert!(!account.join(".credentials.json").exists());
        assert!(!account.join("settings.json").symlink_metadata().unwrap().is_symlink());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn login_command_quotes_the_directory() {
        assert_eq!(
            claude_account_login_command("/tmp/it's"),
            r"CLAUDE_CONFIG_DIR='/tmp/it'\''s' claude auth login"
        );
    }
}
