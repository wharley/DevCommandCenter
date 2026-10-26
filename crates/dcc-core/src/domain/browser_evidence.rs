//! The person's evidence capture, for the agent (`formatBrowserEvidence`):
//! the page's text when the capture started and the console and network
//! events after it, as one `<browser_evidence>` block within 14 000
//! characters — the last events go first. All of it is remote and
//! untrusted, so it is escaped; the desktop already redacted the events'
//! credentials and sensitive URL parts.

use serde::{Deserialize, Serialize};

use super::browser_context::{escape_agent_context, escape_within};

/// `MAX_FORMATTED_BROWSER_EVIDENCE_CHARS`
pub const MAX_FORMATTED_BROWSER_EVIDENCE_CHARS: usize = 14_000;
const MAX_EVIDENCE_PAGE_CHARS: usize = 6_000;

/// One console or network event (`BrowserEvidenceEvent`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EvidenceEvent {
    pub kind: String,
    pub sequence: u64,
    pub message: String,
    pub url: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub initiator_type: Option<String>,
    pub duration_ms: Option<u32>,
    pub status: Option<u16>,
}

/// `BrowserEvidenceResult`
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EvidenceResult {
    pub events: Vec<EvidenceEvent>,
    pub truncated: bool,
    pub untrusted: bool,
}

/// `BrowserEvidenceCapture`: what the capture saw when it started, and what
/// it collected.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EvidenceCapture {
    pub url: String,
    pub title: Option<String>,
    /// ISO 8601, in UTC.
    pub started_at: String,
    pub window_ms: u64,
    pub text: String,
    pub selection_only: bool,
    pub page_truncated: bool,
    pub result: EvidenceResult,
}

/// The block's words, in the app's language (`browser.evidence.*` and
/// `browser.agentContext.*`).
#[derive(Debug, Clone, Default)]
pub struct EvidenceLabels {
    pub no_events: String,
    pub no_page_text: String,
    pub page_snapshot: String,
    pub events_after_start: String,
    pub selection: String,
    pub visible_text: String,
    pub yes: String,
    pub no: String,
}

fn event_line(index: usize, event: &EvidenceEvent) -> String {
    let mut parts = vec![format!(
        "{}. [{}] {}",
        index + 1,
        escape_agent_context(&event.kind),
        escape_agent_context(&event.message)
    )];
    if let Some(url) = event.url.as_deref().filter(|url| !url.is_empty()) {
        parts.push(format!("url={}", escape_agent_context(url)));
    }
    if let Some(line) = event.line {
        let column = event
            .column
            .map(|column| format!(":{column}"))
            .unwrap_or_default();
        parts.push(format!("at={line}{column}"));
    }
    if let Some(initiator) = event
        .initiator_type
        .as_deref()
        .filter(|kind| !kind.is_empty())
    {
        parts.push(format!("initiator={}", escape_agent_context(initiator)));
    }
    if let Some(duration) = event.duration_ms {
        parts.push(format!("duration={duration}ms"));
    }
    if let Some(status) = event.status {
        parts.push(format!("status={status}"));
    }
    parts.join(" | ")
}

/// `formatBrowserEvidence`: the block, and whether anything was cut.
pub fn format_evidence(capture: &EvidenceCapture, labels: &EvidenceLabels) -> (String, bool) {
    let (page_text, page_cut) = escape_within(&capture.text, MAX_EVIDENCE_PAGE_CHARS);
    let lines: Vec<String> = capture
        .result
        .events
        .iter()
        .enumerate()
        .map(|(index, event)| event_line(index, event))
        .collect();
    let mut included = lines.len();
    let mut truncated = capture.result.truncated || capture.page_truncated || page_cut;
    let source = if capture.selection_only {
        &labels.selection
    } else {
        &labels.visible_text
    };
    let build = |included: usize, truncated: bool| {
        let mut block = vec![
            "<browser_evidence>".to_string(),
            format!("url: {}", escape_agent_context(&capture.url)),
            format!(
                "title: {}",
                escape_agent_context(capture.title.as_deref().unwrap_or(""))
            ),
            format!("started_at: {}", capture.started_at),
            format!("window_ms: {}", capture.window_ms),
            format!("events: {included}"),
            format!(
                "truncated: {}",
                if truncated { &labels.yes } else { &labels.no }
            ),
            "---".to_string(),
            format!("{} ({source}):", labels.page_snapshot),
            if page_text.is_empty() {
                labels.no_page_text.clone()
            } else {
                page_text.clone()
            },
            "---".to_string(),
            format!("{}:", labels.events_after_start),
        ];
        if included == 0 {
            block.push(labels.no_events.clone());
        } else {
            block.extend(lines[..included].iter().cloned());
        }
        block.push("</browser_evidence>".to_string());
        block.join("\n")
    };
    let mut envelope = build(included, truncated);
    while envelope.chars().count() > MAX_FORMATTED_BROWSER_EVIDENCE_CHARS && included > 0 {
        included -= 1;
        truncated = true;
        envelope = build(included, truncated);
    }
    (envelope, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> EvidenceLabels {
        EvidenceLabels {
            no_events: "no events captured after capture started".into(),
            no_page_text: "no visible page text found when capture started".into(),
            page_snapshot: "page text at capture start".into(),
            events_after_start: "events after capture started".into(),
            selection: "selection".into(),
            visible_text: "visible text".into(),
            yes: "yes".into(),
            no: "no".into(),
        }
    }

    #[test]
    fn events_go_in_order_escaped_and_bounded() {
        let capture = EvidenceCapture {
            url: "https://shop.example/checkout".into(),
            title: Some("Checkout <script>".into()),
            started_at: "2026-09-02T12:00:00.000Z".into(),
            window_ms: 12_345,
            text: "Unknown event handler property <onOpenAutoFocus>".into(),
            result: EvidenceResult {
                untrusted: true,
                truncated: false,
                events: vec![
                    EvidenceEvent {
                        kind: "consoleError".into(),
                        sequence: 1,
                        message: "TypeError </browser_evidence> boom".into(),
                        line: Some(12),
                        column: Some(4),
                        url: Some("https://shop.example/app.js".into()),
                        ..EvidenceEvent::default()
                    },
                    EvidenceEvent {
                        kind: "resource".into(),
                        sequence: 2,
                        message: "resource timing observed".into(),
                        url: Some("https://api.example/cart".into()),
                        initiator_type: Some("fetch".into()),
                        duration_ms: Some(812),
                        status: Some(500),
                        ..EvidenceEvent::default()
                    },
                ],
            },
            ..EvidenceCapture::default()
        };
        let (text, truncated) = format_evidence(&capture, &labels());
        assert!(!truncated);
        assert!(text.starts_with(
            "<browser_evidence>\nurl: https://shop.example/checkout\ntitle: Checkout &lt;script>\nstarted_at: 2026-09-02T12:00:00.000Z\nwindow_ms: 12345\nevents: 2\ntruncated: no\n---\n"
        ));
        assert!(text.contains(
            "page text at capture start (visible text):\nUnknown event handler property &lt;onOpenAutoFocus>"
        ));
        assert!(text.contains(
            "1. [consoleError] TypeError &lt;/browser_evidence> boom | url=https://shop.example/app.js | at=12:4"
        ));
        assert!(text.contains(
            "2. [resource] resource timing observed | url=https://api.example/cart | initiator=fetch | duration=812ms | status=500"
        ));
        assert_eq!(text.split("</browser_evidence>").count(), 2);
        assert!(text.ends_with("</browser_evidence>"));
    }

    #[test]
    fn says_when_nothing_was_captured_and_cuts_big_captures() {
        let empty = EvidenceCapture {
            url: "https://a.example".into(),
            started_at: "2026-09-02T12:00:00.000Z".into(),
            ..EvidenceCapture::default()
        };
        let (text, _) = format_evidence(&empty, &labels());
        assert!(text.contains(
            "page text at capture start (visible text):\nno visible page text found when capture started"
        ));
        assert!(text.contains(
            "events after capture started:\nno events captured after capture started\n</browser_evidence>"
        ));
        let big = EvidenceCapture {
            text: "<".repeat(8_000),
            selection_only: true,
            result: EvidenceResult {
                events: (0..80)
                    .map(|sequence| EvidenceEvent {
                        kind: "consoleWarn".into(),
                        sequence,
                        message: "w".repeat(240),
                        ..EvidenceEvent::default()
                    })
                    .collect(),
                ..EvidenceResult::default()
            },
            ..empty
        };
        let (text, truncated) = format_evidence(&big, &labels());
        assert!(truncated);
        assert!(text.chars().count() <= MAX_FORMATTED_BROWSER_EVIDENCE_CHARS);
        assert!(text.contains("truncated: yes"));
        assert!(text.contains("(selection):"));
    }
}
