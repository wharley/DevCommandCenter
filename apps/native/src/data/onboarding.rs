//! The first five minutes (`OnboardingWizard.logic.ts`): register a project,
//! understand that every task gets its own worktree, learn where chat,
//! terminal and Inspector live. Whether it opens on its own
//! (`resolveInitialOnboardingOpen`): once, until it is finished or skipped;
//! a debug build can force or skip it with `DCC_NATIVE_ONBOARDING` (the
//! desktop's `VITE_DEV_ONBOARDING`).

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Step {
    #[default]
    Project,
    Task,
    Workbench,
}

impl Step {
    pub const ALL: [Step; 3] = [Step::Project, Step::Task, Step::Workbench];

    pub fn key(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Task => "task",
            Self::Workbench => "workbench",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|step| *step == self).unwrap_or(0)
    }

    /// `getNextOnboardingStep`
    pub fn next(self) -> Option<Step> {
        Self::ALL.get(self.index() + 1).copied()
    }

    /// `getPreviousOnboardingStep`
    pub fn previous(self) -> Option<Step> {
        self.index().checked_sub(1).map(|index| Self::ALL[index])
    }

    /// `isLastOnboardingStep`
    pub fn is_last(self) -> bool {
        self.next().is_none()
    }
}

const FORCE: [&str; 6] = ["force", "1", "true", "show", "yes", "on"];
const SKIP: [&str; 6] = ["skip", "0", "false", "hide", "no", "off"];

/// The debug override's name.
pub const OVERRIDE_ENV: &str = "DCC_NATIVE_ONBOARDING";

/// `resolveInitialOnboardingOpen`
pub fn opens_on_start(complete: bool, debug: bool, token: Option<&str>) -> bool {
    let token = token.map(|token| token.trim().to_lowercase());
    if debug && let Some(token) = token.filter(|token| !token.is_empty()) {
        if FORCE.contains(&token.as_str()) {
            return true;
        }
        if SKIP.contains(&token.as_str()) {
            return false;
        }
    }
    !complete
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_steps_go_forward_and_back() {
        assert_eq!(Step::Project.next(), Some(Step::Task));
        assert_eq!(Step::Workbench.next(), None);
        assert_eq!(Step::Project.previous(), None);
        assert_eq!(Step::Workbench.previous(), Some(Step::Task));
        assert!(Step::Workbench.is_last());
        assert!(!Step::Task.is_last());
    }

    #[test]
    fn it_opens_once_unless_a_debug_build_says_otherwise() {
        assert!(opens_on_start(false, false, None));
        assert!(!opens_on_start(true, false, None));
        assert!(opens_on_start(true, true, Some(" Force ")));
        assert!(!opens_on_start(false, true, Some("off")));
        assert!(!opens_on_start(true, false, Some("force")), "only in debug");
        assert!(opens_on_start(false, true, Some("whatever")));
    }
}
