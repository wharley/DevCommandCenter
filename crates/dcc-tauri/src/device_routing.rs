//! What the agent is told about the iOS Simulator, turn by turn. Only where
//! the native app offers `dcc_device_*` (macOS); the full guidance only in an
//! iOS app project (Expo, React Native, Xcode), so the person never has to
//! say "simulator": asking to run, test or see the app is enough. Like T3
//! Code, the agent does not reach for it to check its own edits: with the
//! Device panel open it looks only when the request is about what the screen
//! shows, and with the panel closed it never boots one on its own (booting
//! is heavy).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

static AVAILABLE: AtomicBool = AtomicBool::new(false);
static PANEL_OPEN: AtomicBool = AtomicBool::new(false);

/// The native app registered its simulator for the agents.
pub fn set_device_tools_available(available: bool) {
    AVAILABLE.store(available, Ordering::Relaxed);
}

/// The Device panel shows a simulator now.
pub fn set_device_panel_open(open: bool) {
    PANEL_OPEN.store(open, Ordering::Relaxed);
}

/// An iOS app is built from `root`: Expo, React Native, or an Xcode project.
pub fn is_ios_app_project(root: &Path) -> bool {
    let has = |name: &str| root.join(name).exists();
    let package_mentions = |needle: &str| {
        std::fs::read_to_string(root.join("package.json"))
            .is_ok_and(|package| package.contains(needle))
    };
    let xcode_project = |dir: &Path| {
        std::fs::read_dir(dir).is_ok_and(|entries| {
            entries.flatten().any(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.ends_with(".xcodeproj") || name.ends_with(".xcworkspace")
            })
        })
    };
    package_mentions("\"expo\"")
        || package_mentions("\"react-native\"")
        || has("app.json") && package_mentions("expo")
        || has("ios/Podfile")
        || xcode_project(&root.join("ios"))
        || xcode_project(root)
}

const SHORT: &str = "For the iOS Simulator, use the dcc_device_* tools, never Computer Use: dcc_device_open shows it to the user in DCC's Device panel.";

const IOS_PROJECT: &str = "This project builds an iOS app. When the user asks to run, try, test or see the app, or whether a screen looks right — in any words, without naming the simulator — use the iOS Simulator through the dcc_device_* tools, never Computer Use. dcc_device_open shows it to the user in DCC's Device panel; read it with dcc_device_screenshot or dcc_device_describe and act with dcc_device_tap, dcc_device_swipe, dcc_device_type, dcc_device_button, dcc_device_launch, dcc_device_open_url or dcc_device_install. Get the app on it the project's way (for example `npx expo run:ios`, or `npx expo start` and dcc_device_open_url with its exp:// URL, or an xcodebuild simulator build and dcc_device_install). They need no extra approval.";

const PANEL_OPEN_NOTE: &str = "The user has the simulator open in DCC. Look at it only when this request is about what the app shows or how it behaves on screen (a layout, a style, a flow, a bug the user saw), not for every edit: skip it for logic, data, tests, configuration and anything the screen would not show. One look after the change is enough, then say what you saw. Never boot or open a simulator just to check.";

const PANEL_CLOSED_NOTE: &str = "The simulator is not open: do not boot it to check your own edits. Use it only when the user asks to run, test or see the app.";

/// The simulator guidance for this turn, or nothing without the app's
/// simulator.
pub fn guidance(project_root: Option<&Path>) -> Option<String> {
    guidance_for(
        AVAILABLE.load(Ordering::Relaxed),
        project_root.is_some_and(is_ios_app_project),
        PANEL_OPEN.load(Ordering::Relaxed),
    )
}

fn guidance_for(available: bool, ios_project: bool, panel_open: bool) -> Option<String> {
    if !available {
        return None;
    }
    if !ios_project {
        return Some(SHORT.to_string());
    }
    let note = if panel_open {
        PANEL_OPEN_NOTE
    } else {
        PANEL_CLOSED_NOTE
    };
    Some(format!("{IOS_PROJECT}\n\n{note}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ios_projects_are_recognised_by_their_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert!(!is_ios_app_project(root), "an empty folder");
        std::fs::write(
            root.join("package.json"),
            r#"{"dependencies":{"next":"15"}}"#,
        )
        .unwrap();
        assert!(!is_ios_app_project(root), "a web app");
        std::fs::write(
            root.join("package.json"),
            r#"{"dependencies":{"expo":"~54.0.0","react-native":"0.81.0"}}"#,
        )
        .unwrap();
        assert!(is_ios_app_project(root), "Expo");

        let native = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(native.path().join("ios/Duo.xcodeproj")).unwrap();
        assert!(is_ios_app_project(native.path()), "a native ios folder");
        let swift = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(swift.path().join("Feedr.xcodeproj")).unwrap();
        assert!(is_ios_app_project(swift.path()), "an Xcode project");
    }

    #[test]
    fn the_guidance_follows_the_app_the_project_and_the_panel() {
        assert_eq!(
            guidance_for(false, true, true),
            None,
            "no simulator outside the app"
        );
        let web = guidance_for(true, false, false).unwrap();
        assert!(web.contains("never Computer Use"));
        assert!(!web.contains("run, try, test"), "the short line only");

        let closed = guidance_for(true, true, false).unwrap();
        assert!(closed.contains("without naming the simulator"));
        assert!(closed.contains("do not boot it to check your own edits"));

        let open = guidance_for(true, true, true).unwrap();
        assert!(open.contains("not for every edit"));
        assert!(open.contains("Never boot or open a simulator just to check"));
    }
}
