//! Settings → Conta against a fake desktop: who is signed in, the login DCC
//! uses (following the CLI's, then chosen), the known hosts, GitLab, and
//! the embedded sign-in finding the new login.

use gpui::TestAppContext;

use super::SectionId;
use super::tests::{click, open};

fn accounts(selected: Option<&str>, logins: &[&str]) -> String {
    serde_json::json!({
        "provider": "github", "cliName": "gh", "hostname": "github.com", "status": "ready",
        "login": logins.first(), "selectedLogin": selected,
        "accounts": logins.iter().map(|login| serde_json::json!({
            "login": login, "name": null, "avatarUrl": null, "email": null,
            "active": false, "selected": false,
        })).collect::<Vec<_>>(),
        "message": "Conectado ao github.com", "loginCommand": "gh auth login",
    })
    .to_string()
}

#[gpui::test]
fn the_forge_login_is_followed_chosen_and_signed_in(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    let (settings, cx, _) = open(Some(SectionId::Account), cx);
    // No desktop: the card says the CLI could not be read.
    assert!(cx.debug_bounds("settings-account-forge-card").is_some());
    assert!(cx.debug_bounds("settings-account-who").is_none());
    let hosts = r#"{"provider":"github","cliName":"gh","hosts":["github.com","git.zedy.com"]}"#;
    let gitlab = serde_json::json!({
        "provider": "gitlab", "cliName": "glab", "hostname": "gitlab.com", "status": "error",
        "login": null, "selectedLogin": null, "accounts": [], "message": "glab sem login",
        "loginCommand": "glab auth login --hostname gitlab.com",
    })
    .to_string();
    let owner = "/api/v1/terminals/by-owner/forge-connect%3Agithub%3Agithub.com";
    let (port, server) = fake_desktop_routes(vec![
        (
            "/api/v1/forge/accounts",
            200,
            accounts(None, &["ana", "bia"]),
        ),
        ("/api/v1/forge/hosts", 200, hosts.into()),
        ("/api/v1/forge/select-login", 200, r#"{"ok":true}"#.into()),
        (
            "/api/v1/forge/accounts",
            200,
            accounts(Some("ana"), &["ana", "bia"]),
        ),
        ("/api/v1/forge/hosts", 200, hosts.into()),
        ("/api/v1/forge/select-login", 200, r#"{"ok":true}"#.into()),
        (
            "/api/v1/forge/accounts",
            200,
            accounts(Some("bia"), &["ana", "bia"]),
        ),
        ("/api/v1/forge/hosts", 200, hosts.into()),
        ("/api/v1/forge/accounts", 200, gitlab),
        ("/api/v1/forge/hosts", 200, r#"{"hosts":[]}"#.into()),
        (
            owner,
            200,
            r#"{"ptyId":"pty-1","existing":false,"chunks":["? Where do you use GitHub?"]}"#.into(),
        ),
        (
            "/api/v1/forge/cli-status",
            200,
            r#"{"logins":["ana","bia"]}"#.into(),
        ),
        ("/api/v1/terminals/pty-1/kill", 200, r#"{"ok":true}"#.into()),
        (
            "/api/v1/forge/cli-status",
            200,
            r#"{"logins":["ana","bia","cid"]}"#.into(),
        ),
        (
            "/api/v1/forge/backfill-bindings",
            200,
            r#"{"bound":1}"#.into(),
        ),
    ]);
    let dir = tempfile::tempdir().unwrap();
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.set_api(
                crate::data::desktop_api::DesktopApi::new(port, "k"),
                dir.path().to_path_buf(),
                cx,
            )
        })
    });
    cx.run_until_parked();
    // Ready with two logins; DCC follows the CLI's (ana), the hosts it knows.
    assert!(cx.debug_bounds("settings-account-who").is_some());
    assert!(
        cx.debug_bounds("settings-account-host-git.zedy.com")
            .is_some()
    );
    click(cx, "settings-account-login-bia");
    cx.run_until_parked();
    // GitLab: not signed in, "Conectar no app".
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.select_forge(crate::data::forge_cli::Forge::Gitlab, cx)
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-account-who").is_none());
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.select_forge(crate::data::forge_cli::Forge::Github, cx)
        })
    });
    cx.run_until_parked();
    // The embedded sign-in, then the new login found and bound.
    click(cx, "settings-account-connect");
    assert!(cx.debug_bounds("forge-connect").is_some());
    click(cx, "forge-connect-close");
    let seen = server.join().unwrap();
    cx.run_until_parked();
    let requests: Vec<&str> = seen.iter().map(|(request, _)| request.as_str()).collect();
    assert!(seen[2].1.contains(r#""login":"ana""#), "{}", seen[2].1);
    assert!(seen[5].1.contains(r#""login":"bia""#), "{}", seen[5].1);
    assert!(
        seen[8].1.contains(r#""provider":"gitlab""#),
        "{}",
        seen[8].1
    );
    assert!(
        requests.contains(&"POST /api/v1/forge/backfill-bindings"),
        "{requests:?}"
    );
    assert!(cx.debug_bounds("forge-connect").is_none());
    cx.update(|_, cx| assert!(settings.read(cx).account.connect.is_none()));
}
