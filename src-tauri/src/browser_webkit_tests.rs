//! Opt-in native integration test. It uses a separate WKWebView process and the
//! production context/action/snapshot implementations, with no external accounts.
use super::*;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};

struct Fixture(Child);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "opens an isolated macOS WKWebView fixture; run explicitly on a desktop"]
fn webkit_tracks_main_document_through_frames_redirects_and_spa_navigation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("browser-navigation-fixture");
    assert!(Command::new("xcrun")
        .args([
            "clang",
            "-fobjc-arc",
            "-fblocks",
            "-framework",
            "AppKit",
            "-framework",
            "WebKit"
        ])
        .arg(root.join("../scripts/fixtures/browser-navigation-driver.m"))
        .arg(root.join("native/browser_navigation_macos.m"))
        .arg("-o")
        .arg(&binary)
        .status()
        .unwrap()
        .success());
    let mut server = Fixture(
        Command::new("python3")
            .arg(root.join("../scripts/fixtures/browser-navigation-server.py"))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut port = String::new();
    BufReader::new(server.0.stdout.take().unwrap())
        .read_line(&mut port)
        .unwrap();
    let base = format!("http://127.0.0.1:{}", port.trim().parse::<u16>().unwrap());
    let mut fixture = Fixture(
        Command::new(binary)
            .env("DCC_NAVIGATION_FIXTURE_URL", format!("{base}/start"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut input = fixture.0.stdin.take().unwrap();
    let mut output = BufReader::new(fixture.0.stdout.take().unwrap());
    let read = |output: &mut BufReader<std::process::ChildStdout>| -> Value {
        let mut line = String::new();
        assert!(
            output.read_line(&mut line).unwrap() > 0,
            "navigation fixture closed"
        );
        serde_json::from_str(&line).unwrap()
    };
    assert_eq!(read(&mut output)["ready"], true);
    let mut eval = |command: Value| -> Value {
        writeln!(input, "{command}").unwrap();
        input.flush().unwrap();
        read(&mut output)
    };
    let assert_ready = |result: &Value, path: &str| {
        assert_eq!(result["url"], format!("{base}{path}"), "{result}");
        assert_eq!(result["loading"], false, "{result}");
        let events = result["events"].as_array().unwrap();
        assert!(
            !events.is_empty(),
            "native tracker emitted no events: {result}"
        );
        assert!(
            events
                .iter()
                .all(|event| !event["url"].as_str().unwrap().contains("/frame")),
            "subframe URL contaminated the main document: {result}"
        );
        let last = events.last().unwrap();
        assert_eq!(last["url"], format!("{base}{path}"), "{result}");
        assert_eq!(last["loading"], false, "{result}");
    };
    // Initial redirect and iframe must settle at the top-level destination.
    assert_ready(&eval(json!({})), "/main");
    // A later iframe redirect must not replace the main URL or leave it loading.
    assert_ready(
        &eval(json!({"script": "document.querySelector('#frame').click()"})),
        "/main",
    );
    // SPA transitions have no didFinishNavigation event.
    assert_ready(
        &eval(json!({"clear": true, "script": "document.querySelector('#spa').click()"})),
        "/profile",
    );
    assert_ready(
        &eval(json!({"clear": true, "script": "history.replaceState({}, '', '/settings')"})),
        "/settings",
    );
    assert_ready(
        &eval(json!({"clear": true, "script": "history.back()"})),
        "/main",
    );
    let reloaded = eval(json!({"clear": true, "script": "location.reload()"}));
    assert_ready(&reloaded, "/main");
    assert!(
        reloaded["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["loading"] == true),
        "reload never invalidated the document: {reloaded}"
    );
    // Replacing the tracker must stop callbacks from the old lifecycle.
    let next = eval(json!({"clear": true, "token": 18, "script": "location.href='/next'"}));
    assert_ready(&next, "/next");
    assert!(
        next["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|event| event["token"] == 18),
        "{next}"
    );
    // Product reuses the view by navigating before installing the new token.
    // The initial observation must never report the previous page as ready.
    let reused = eval(json!({"clear": true, "token": 19, "navigate": format!("{base}/start")}));
    assert_ready(&reused, "/main");
    assert!(
        reused["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["token"] == 19)
            .all(|event| event["url"] != format!("{base}/next") || event["loading"] == true),
        "reused viewport reported the previous document ready: {reused}"
    );
}

#[test]
#[ignore = "opens an isolated macOS WKWebView fixture; run explicitly on a desktop"]
fn webkit_executes_production_context_actions_and_snapshot() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("browser-fixture");
    assert!(Command::new("xcrun")
        .args([
            "clang",
            "-fobjc-arc",
            "-fblocks",
            "-framework",
            "AppKit",
            "-framework",
            "WebKit"
        ])
        .arg(root.join("../scripts/fixtures/browser-webkit-driver.m"))
        .arg(root.join("native/browser_capture_macos.m"))
        .arg(root.join("native/browser_input_macos.m"))
        .arg("-o")
        .arg(&binary)
        .status()
        .unwrap()
        .success());
    let mut fixture = Fixture(
        Command::new(binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut input = fixture.0.stdin.take().unwrap();
    let mut output = BufReader::new(fixture.0.stdout.take().unwrap());
    let read = |output: &mut BufReader<std::process::ChildStdout>| -> Value {
        let mut line = String::new();
        assert!(
            output.read_line(&mut line).unwrap() > 0,
            "native fixture closed"
        );
        serde_json::from_str(&line).unwrap()
    };
    assert_eq!(read(&mut output)["ready"], true);
    let mut eval = |command: Value| -> Value {
        writeln!(input, "{command}").unwrap();
        input.flush().unwrap();
        read(&mut output)
    };
    // The native fixture receives WebKit's supplied popup configuration. This
    // proves an OAuth-style blank opener retains `window.opener` and Browser
    // session data without touching a real account.
    let popup = eval(json!({"popupProbe":true}));
    assert_eq!(popup["hasOpener"], true, "{popup}");
    assert_eq!(popup["sameStoreObject"], true, "{popup}");
    let url = eval(json!({"script":"location.href"}))
        .as_str()
        .unwrap()
        .to_string();
    for (name, text) in [
        ("Name", Some("Browser verified")),
        ("Quantity", Some("3")),
        ("Save", None),
    ] {
        let raw = eval(json!({"script":browser_context_script()}));
        assert!(!raw.to_string().contains("fixture-only-secret"));
        let context: BrowserContextExtraction = serde_json::from_value(raw).unwrap();
        let identity = validate_browser_document_identity(context.document_identity).unwrap();
        let (_, targets, _) = normalize_browser_semantic_map(
            context.semantic_map,
            MAX_BROWSER_SEMANTIC_SERIALIZED_CHARS,
        );
        let target = targets
            .iter()
            .find(|target| target.name == name)
            .unwrap_or_else(|| panic!("missing {name}: {targets:?}"));
        assert!(if text.is_some() {
            fill_target_is_compatible(target)
        } else {
            click_target_is_compatible(target)
        });
        if let Some(text) = text {
            let script =
                browser_semantic_action_script(target, Some(text), &url, identity).unwrap();
            let result = eval(json!({"script":script}));
            assert_eq!(result["ok"], true, "{name}: {result}");
        } else {
            let point = eval(
                json!({"script": browser_element_action_script(target, "point", "", &url, identity).unwrap()}),
            );
            assert_eq!(point["ok"], true, "{point}");
            assert_eq!(
                eval(json!({"click": {"x": point["x"], "y": point["y"]}}))["ok"],
                true
            );
        }
    }
    assert_eq!(
        eval(json!({"script":"document.querySelector('#result').textContent"})),
        "Browser verified:3"
    );

    eval(
        json!({"script": r#"document.querySelector('#result').insertAdjacentHTML('afterend', `<select aria-label='Store' onchange="document.body.dataset.store=this.value"><option value='a'>First store</option><option value='b'>Second store</option><option disabled>Unavailable</option></select><div contenteditable='true' role='textbox' aria-label='Notes' style='border:1px solid'>Old note</div><form onsubmit="event.preventDefault();document.body.dataset.submitted='yes'"><input aria-label='Search' onkeydown="document.body.dataset.trusted=String(event.isTrusted)"><button>Submit</button></form>`);"#}),
    );
    for (name, mode, value) in [
        ("Store", "select", "Second store"),
        ("Notes", "fill", "New note"),
        ("Search", "focus", ""),
    ] {
        let raw = eval(json!({"script": browser_context_script()}));
        let context: BrowserContextExtraction = serde_json::from_value(raw).unwrap();
        let identity = validate_browser_document_identity(context.document_identity).unwrap();
        let (items, targets, _) = normalize_browser_semantic_map(
            context.semantic_map,
            MAX_BROWSER_SEMANTIC_SERIALIZED_CHARS,
        );
        let target = targets
            .iter()
            .find(|target| target.name == name)
            .unwrap_or_else(|| panic!("missing {name}: {targets:?}"));
        if mode == "select" {
            assert_eq!(
                items.iter().find(|item| item.name == name).unwrap().options,
                vec!["First store", "Second store"]
            );
        }
        assert_eq!(
            eval(
                json!({"script": browser_element_action_script(target, mode, value, &url, identity).unwrap()})
            )["ok"],
            true,
            "{name}"
        );
        if mode == "focus" {
            assert_eq!(eval(json!({"key": "Enter"}))["ok"], true);
        }
    }
    assert_eq!(eval(json!({"script": "document.body.dataset.store"})), "b");
    assert_eq!(
        eval(json!({"script": "document.querySelector('[contenteditable]').textContent"})),
        "New note"
    );
    assert_eq!(
        eval(json!({"script": "document.body.dataset.submitted"})),
        "yes"
    );
    assert_eq!(
        eval(json!({"script": "document.body.dataset.trusted"})),
        "true"
    );
    assert_eq!(
        eval(json!({"script": "document.body.dataset.clickTrusted"})),
        "true"
    );
    let capture = eval(json!({"capture":true}));
    assert!(capture["bytes"].as_u64().unwrap_or(0) > 1000, "{capture}");
    use base64::Engine;
    let png = base64::engine::general_purpose::STANDARD
        .decode(capture["png"].as_str().unwrap())
        .unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    if let Some(path) = std::env::var_os("DCC_BROWSER_TEST_SCREENSHOT") {
        std::fs::write(path, png).unwrap();
    }
    eval(json!({"script":browser_scroll_script(0.0, 500.0)}));
    assert!(eval(json!({"script":"window.scrollY"})).as_f64().unwrap() >= 499.0);
    // A full document replacement must reject an old identity even at same URL.
    let stale = browser_semantic_action_script(&targets_for_stale_test(), None, &url, 1.0).unwrap();
    assert_eq!(eval(json!({"script":stale}))["ok"], false);
}

fn targets_for_stale_test() -> BrowserSemanticTargetRecord {
    BrowserSemanticTargetRecord {
        reference: "e1".into(),
        ordinal: 1,
        role: "button".into(),
        name: "Save".into(),
        destination: None,
        disabled: None,
        checked: None,
        selected: None,
        expanded: None,
        pressed: None,
        tag: "div".into(),
        input_type: None,
        content_editable: false,
    }
}
