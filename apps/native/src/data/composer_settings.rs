//! How the next turn runs, as the composer chooses it: the approval mode
//! (per task and provider), effort (per task), Planejar (per conversation,
//! falling back to the task) and the fast answer style (everywhere). Kept in
//! `composer-settings.json`; the desktop keeps its own in the webview with
//! the same rules (`draftStorage.ts`, `effort.ts`, `WorkspaceComposer.logic.ts`).

use std::collections::HashMap;
use std::path::Path;

use dcc_core::domain::provider::ProviderApprovalPolicy;
use serde::{Deserialize, Serialize};

pub const DEFAULT_EFFORT_LEVELS: [&str; 3] = ["low", "medium", "high"];
pub const DEFAULT_EFFORT: &str = "medium";

/// `EFFORT_RANK`
fn effort_rank(level: &str) -> u8 {
    match level {
        "minimal" => 0,
        "low" => 1,
        "balanced" | "medium" => 2,
        "high" => 3,
        "xhigh" => 4,
        "max" => 5,
        "ultrathink" => 6,
        _ => 2,
    }
}

/// `clampEffort`: the level itself, else the nearest supported below, else above.
pub fn clamp_effort(effort: &str, supported: &[String]) -> String {
    if supported.is_empty() {
        return DEFAULT_EFFORT.to_string();
    }
    if supported.iter().any(|level| level == effort) {
        return effort.to_string();
    }
    let mut ranked: Vec<&String> = supported.iter().collect();
    ranked.sort_by_key(|level| effort_rank(level));
    let target = effort_rank(effort);
    ranked
        .iter()
        .rev()
        .find(|level| effort_rank(level) <= target)
        .or_else(|| ranked.iter().find(|level| effort_rank(level) >= target))
        .map_or_else(|| supported[0].clone(), |level| (*level).clone())
}

/// `highestAvailableEffort`
pub fn highest_effort(supported: &[String]) -> String {
    supported
        .iter()
        .max_by_key(|level| effort_rank(level))
        .cloned()
        .unwrap_or_else(|| DEFAULT_EFFORT.to_string())
}

/// `isUltrathinkPrompt`: the word on its own.
pub fn is_ultrathink_prompt(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    lower.match_indices("ultrathink").any(|(index, word)| {
        let before = lower[..index].chars().next_back();
        let after = lower[index + word.len()..].chars().next();
        let boundary = |c: Option<char>| c.is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        boundary(before) && boundary(after)
    })
}

/// `resolveEffectiveEffort`
pub fn effective_effort(
    selected: &str,
    supported: &[String],
    ultrathink: bool,
    prompt: &str,
) -> String {
    if ultrathink || is_ultrathink_prompt(prompt) {
        highest_effort(supported)
    } else {
        clamp_effort(selected, supported)
    }
}

pub fn policy_key(policy: ProviderApprovalPolicy) -> &'static str {
    match policy {
        ProviderApprovalPolicy::ReadOnly => "read_only",
        ProviderApprovalPolicy::Ask => "ask",
        ProviderApprovalPolicy::Auto => "auto",
        ProviderApprovalPolicy::FullAccess => "full_access",
    }
}

/// The policies the menu lists, in its order (`POLICY_ORDER`).
pub fn menu_policies(supported: &[ProviderApprovalPolicy]) -> Vec<ProviderApprovalPolicy> {
    [
        ProviderApprovalPolicy::Ask,
        ProviderApprovalPolicy::Auto,
        ProviderApprovalPolicy::FullAccess,
    ]
    .into_iter()
    .filter(|policy| supported.contains(policy))
    .collect()
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffortChoice {
    pub effort: String,
    #[serde(default)]
    pub ultrathink: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposerSettings {
    /// `task|provider` → policy key.
    #[serde(default)]
    approval: HashMap<String, String>,
    /// task → effort.
    #[serde(default)]
    effort: HashMap<String, EffortChoice>,
    /// `session:<id>` / `workspace:<id>` → Planejar on.
    #[serde(default)]
    plan: HashMap<String, bool>,
    /// "Resposta direta" (`dcc.composer.response-style`).
    #[serde(default)]
    pub fast: bool,
}

impl ComposerSettings {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string(self).unwrap_or_default())
    }

    /// `loadApprovalPolicy`: the stored one when still supported, else
    /// "auto" when there is one, else the first. `None` when the provider
    /// manages permissions itself.
    pub fn approval(
        &self,
        task: &str,
        provider: &str,
        supported: &[ProviderApprovalPolicy],
    ) -> Option<ProviderApprovalPolicy> {
        let fallback = if supported.contains(&ProviderApprovalPolicy::Auto) {
            Some(ProviderApprovalPolicy::Auto)
        } else {
            supported.first().copied()
        }?;
        let stored = self.approval.get(&format!("{task}|{provider}"));
        Some(
            stored
                .and_then(|stored| {
                    supported
                        .iter()
                        .copied()
                        .find(|policy| policy_key(*policy) == stored)
                })
                .unwrap_or(fallback),
        )
    }

    pub fn set_approval(&mut self, task: &str, provider: &str, policy: ProviderApprovalPolicy) {
        self.approval
            .insert(format!("{task}|{provider}"), policy_key(policy).to_string());
    }

    pub fn effort(&self, task: &str) -> EffortChoice {
        self.effort.get(task).cloned().unwrap_or(EffortChoice {
            effort: DEFAULT_EFFORT.to_string(),
            ultrathink: false,
        })
    }

    pub fn set_effort(&mut self, task: &str, choice: EffortChoice) {
        self.effort.insert(task.to_string(), choice);
    }

    /// `resolvePlanModeState`
    pub fn plan(&self, task: &str, session: Option<&str>) -> bool {
        if let Some(session) = session
            && let Some(on) = self.plan.get(&format!("session:{session}"))
        {
            return *on;
        }
        self.plan
            .get(&format!("workspace:{task}"))
            .copied()
            .unwrap_or(false)
    }

    /// `setPlanModeState`: both the conversation and the task remember it.
    pub fn set_plan(&mut self, task: &str, session: Option<&str>, on: bool) {
        self.plan.insert(format!("workspace:{task}"), on);
        if let Some(session) = session {
            self.plan.insert(format!("session:{session}"), on);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levels(levels: &[&str]) -> Vec<String> {
        levels.iter().map(|level| level.to_string()).collect()
    }

    #[test]
    fn effort_clamps_down_then_up() {
        let supported = levels(&["low", "high"]);
        assert_eq!(clamp_effort("medium", &supported), "low");
        assert_eq!(clamp_effort("minimal", &supported), "low");
        assert_eq!(clamp_effort("max", &supported), "high");
        assert_eq!(clamp_effort("x", &[]), "medium");
        assert_eq!(
            effective_effort("low", &supported, false, "pense, ultrathink!"),
            "high"
        );
        assert!(!is_ultrathink_prompt("ultrathinking"));
    }

    #[test]
    fn approval_defaults_to_auto_and_keeps_a_supported_choice() {
        use ProviderApprovalPolicy::*;
        let mut settings = ComposerSettings::default();
        assert_eq!(
            settings.approval("t", "codex", &[Ask, Auto, FullAccess]),
            Some(Auto)
        );
        assert_eq!(settings.approval("t", "x", &[Ask]), Some(Ask));
        assert_eq!(settings.approval("t", "x", &[]), None);
        settings.set_approval("t", "codex", FullAccess);
        assert_eq!(
            settings.approval("t", "codex", &[Ask, Auto, FullAccess]),
            Some(FullAccess)
        );
        assert_eq!(settings.approval("t", "codex", &[Ask, Auto]), Some(Auto));
        assert_eq!(
            menu_policies(&[ReadOnly, FullAccess, Ask]),
            vec![Ask, FullAccess]
        );
    }

    #[test]
    fn plan_mode_prefers_the_conversation() {
        let mut settings = ComposerSettings::default();
        settings.set_plan("t", None, true);
        assert!(settings.plan("t", Some("s")));
        settings.set_plan("t", Some("s"), false);
        assert!(!settings.plan("t", Some("s")));
        assert!(!settings.plan("t", None));
    }
}
