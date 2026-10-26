//! The page excerpt a person sends to the agent from the Browser (the
//! desktop's `browser_extract_context`, and the native app's): the fixed
//! page-side script, then the trust boundary over what it answers — text
//! without control characters and bounded, links shown without
//! credentials, query or fragment, a small semantic map with opaque
//! references and a fixed role vocabulary — and the envelope the composer
//! receives (`formatBrowserAgentContext`). The page is remote and untrusted:
//! every bound is enforced here again, whatever the script returned.

use serde::{Deserialize, Serialize};
use url::Url;

pub const MAX_BROWSER_CONTEXT_CHARS: usize = 6_000;
pub const MAX_BROWSER_TITLE_CHARS: usize = 300;
/// Bounds for the deliberately small, human-requested semantic page map.
/// These constants are interpolated into the fixed page-side extraction script
/// and enforced again at the Rust trust boundary.
pub const MAX_BROWSER_SEMANTIC_CANDIDATES: usize = 300;
pub const MAX_BROWSER_SEMANTIC_SCAN_NODES: usize = 1_500;
pub const MAX_BROWSER_SEMANTIC_ANCESTORS: usize = 64;
pub const MAX_BROWSER_CONTEXT_TEXT_NODES: usize = 600;
pub const MAX_BROWSER_SEMANTIC_NAME_TEXT_NODES: usize = 24;
pub const MAX_BROWSER_SEMANTIC_ITEMS: usize = 80;
pub const MAX_BROWSER_SEMANTIC_NAME_CHARS: usize = 120;
pub const MAX_BROWSER_SEMANTIC_DESTINATION_CHARS: usize = 180;
pub const MAX_BROWSER_SEMANTIC_SERIALIZED_CHARS: usize = 5_000;
pub const MAX_BROWSER_CONTEXT_ENVELOPE_CHARS: usize = 14_000;
pub const MAX_BROWSER_CONTEXT_URL_CHARS: usize = 2_048;

/// A deliberately small, provider-neutral page excerpt requested by a human
/// from the browser toolbar. This is not an automation or general DOM API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAgentContext {
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub url: String,
    pub title: Option<String>,
    pub text: String,
    pub selection_only: bool,
    pub truncated: bool,
    pub semantic_map: BrowserSemanticMap,
}

/// A page-local, opaque reference map intended to make a later, separately
/// permissioned interaction contract possible. The references are not CSS,
/// XPath, DOM ids, or locators; this command exposes no automation API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSemanticMap {
    pub map_id: String,
    pub generation: u64,
    pub page_load_revision: u64,
    pub items: Vec<BrowserSemanticItem>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSemanticItem {
    /// Opaque only within `map_id`; it is deliberately not a page locator.
    pub reference: String,
    /// Fixed provider-neutral vocabulary such as heading, button or link.
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pressed: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BrowserContextExtraction {
    pub text: String,
    pub selection_only: bool,
    pub truncated: bool,
    pub document_identity: Option<f64>,
    #[serde(default)]
    pub semantic_map: BrowserSemanticMapExtraction,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSemanticMapExtraction {
    #[serde(default)]
    pub items: Vec<BrowserSemanticItemExtraction>,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSemanticItemExtraction {
    pub role: String,
    pub level: Option<u8>,
    pub name: String,
    pub destination: Option<String>,
    pub disabled: Option<bool>,
    pub checked: Option<bool>,
    pub selected: Option<bool>,
    pub expanded: Option<bool>,
    pub pressed: Option<bool>,
    #[serde(default)]
    pub options: Vec<String>,
    /// Page-side ordinal and element shape are trust-boundary metadata. They
    /// are stripped from the public map and retained only in the backend
    /// record used for the next, separately-consented action.
    pub ordinal: Option<usize>,
    pub tag: Option<String>,
    pub input_type: Option<String>,
    pub content_editable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSemanticTargetRecord {
    pub reference: String,
    pub ordinal: usize,
    pub role: String,
    pub name: String,
    pub destination: Option<String>,
    pub disabled: Option<bool>,
    pub checked: Option<bool>,
    pub selected: Option<bool>,
    pub expanded: Option<bool>,
    pub pressed: Option<bool>,
    pub tag: String,
    pub input_type: Option<String>,
    pub content_editable: bool,
}

/// Removes control sequences and bounds text crossing the native WebView
/// boundary. The page-side script also limits output, but this is the final
/// trust boundary because a remote page controls the DOM and JS globals.
pub fn normalize_browser_context_text(raw: &str, max_chars: usize) -> (String, bool) {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut text = String::new();
    let mut chars = 0usize;
    let mut truncated = false;
    for character in normalized.chars() {
        if character.is_control() && character != '\n' && character != '\t' {
            continue;
        }
        if chars == max_chars {
            truncated = true;
            break;
        }
        text.push(character);
        chars += 1;
    }
    (text.trim().to_string(), truncated)
}

pub fn normalize_browser_semantic_text(raw: &str, max_chars: usize) -> (String, bool) {
    let mut text = String::new();
    let mut chars = 0usize;
    let mut truncated = false;
    let mut previous_space = false;
    for character in raw.chars() {
        let character = if character.is_control() || character.is_whitespace() {
            ' '
        } else {
            character
        };
        if character == ' ' && previous_space {
            continue;
        }
        if chars == max_chars {
            truncated = true;
            break;
        }
        previous_space = character == ' ';
        text.push(character);
        chars += 1;
    }
    (text.trim().to_string(), truncated)
}

pub fn validate_browser_document_identity(value: Option<f64>) -> Result<f64, String> {
    match value {
        Some(value) if value.is_finite() && value > 0.0 => Ok(value),
        _ => Err("browser document identity is unavailable".to_string()),
    }
}

/// Produces a display-only link destination. The page is untrusted, so the
/// backend removes credentials, query strings and fragments even if the fixed
/// extraction script was modified by page globals or returned hostile JSON.
pub fn sanitize_browser_link_destination(raw: &str) -> (Option<String>, bool) {
    let Ok(mut url) = Url::parse(raw.trim()) else {
        return (None, !raw.trim().is_empty());
    };
    if !matches!(url.scheme().to_ascii_lowercase().as_str(), "https" | "http")
        || url.host_str().is_none()
    {
        return (None, true);
    }
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    let destination = url.to_string();
    let (destination, truncated) =
        normalize_browser_semantic_text(&destination, MAX_BROWSER_SEMANTIC_DESTINATION_CHARS);
    if truncated || destination.is_empty() {
        (None, true)
    } else {
        (Some(destination), false)
    }
}

pub fn canonical_semantic_role(raw: &str) -> Option<&'static str> {
    match raw {
        "heading" => Some("heading"),
        "button" => Some("button"),
        "link" => Some("link"),
        "textbox" => Some("textbox"),
        "select" => Some("select"),
        "checkbox" => Some("checkbox"),
        "radio" => Some("radio"),
        "switch" => Some("switch"),
        "slider" => Some("slider"),
        "spinbutton" => Some("spinbutton"),
        "option" => Some("option"),
        "tab" => Some("tab"),
        "menuitem" => Some("menuitem"),
        "file-input" => Some("file-input"),
        _ => None,
    }
}

pub fn semantic_item_serialized_chars(items: &[BrowserSemanticItem]) -> usize {
    serde_json::to_string(items)
        .map(|serialized| serialized.chars().count())
        .unwrap_or(usize::MAX)
}

pub fn normalize_browser_semantic_map(
    extracted: BrowserSemanticMapExtraction,
    max_serialized_chars: usize,
) -> (
    Vec<BrowserSemanticItem>,
    Vec<BrowserSemanticTargetRecord>,
    bool,
) {
    let mut truncated = extracted.truncated || extracted.items.len() > MAX_BROWSER_SEMANTIC_ITEMS;
    let mut items = Vec::new();
    let mut targets = Vec::new();
    for extracted in extracted.items.into_iter().take(MAX_BROWSER_SEMANTIC_ITEMS) {
        let Some(role) = canonical_semantic_role(extracted.role.as_str()) else {
            // A malformed page response must not smuggle an arbitrary schema
            // into a future provider/MCP contract.
            truncated = true;
            continue;
        };
        let (name, name_truncated) =
            normalize_browser_semantic_text(&extracted.name, MAX_BROWSER_SEMANTIC_NAME_CHARS);
        let (destination, destination_truncated) = if role == "link" {
            extracted
                .destination
                .as_deref()
                .map(sanitize_browser_link_destination)
                .unwrap_or((None, false))
        } else {
            (None, extracted.destination.is_some())
        };
        let supports_checked = matches!(role, "checkbox" | "radio" | "switch");
        let supports_selected = matches!(role, "select" | "option");
        let level = if role == "heading" {
            match extracted.level {
                Some(level @ 1..=6) => Some(level),
                _ => {
                    truncated = true;
                    continue;
                }
            }
        } else {
            if extracted.level.is_some() {
                truncated = true;
            }
            None
        };
        let item = BrowserSemanticItem {
            reference: format!("e{}", items.len() + 1),
            role: role.to_string(),
            level,
            name,
            destination,
            disabled: extracted.disabled,
            checked: supports_checked.then_some(extracted.checked).flatten(),
            selected: supports_selected.then_some(extracted.selected).flatten(),
            expanded: extracted.expanded,
            pressed: (role == "button").then_some(extracted.pressed).flatten(),
            options: if role == "select" {
                extracted
                    .options
                    .iter()
                    .take(30)
                    .map(|label| {
                        normalize_browser_semantic_text(label, MAX_BROWSER_SEMANTIC_NAME_CHARS).0
                    })
                    .filter(|label| !label.is_empty())
                    .collect()
            } else {
                Vec::new()
            },
        };
        if name_truncated || destination_truncated {
            truncated = true;
        }
        if item.name.is_empty() && item.destination.is_none() {
            truncated = true;
            continue;
        }
        let Some(ordinal) = extracted.ordinal else {
            truncated = true;
            continue;
        };
        let Some(tag) = extracted.tag.as_deref() else {
            truncated = true;
            continue;
        };
        let tag = tag.to_ascii_lowercase();
        if ordinal == 0
            || ordinal > MAX_BROWSER_SEMANTIC_SCAN_NODES
            || targets
                .last()
                .is_some_and(|previous: &BrowserSemanticTargetRecord| ordinal <= previous.ordinal)
            || tag.is_empty()
            || tag.len() > 32
            || !tag.bytes().all(|byte| byte.is_ascii_alphanumeric())
        {
            truncated = true;
            continue;
        }
        let input_type = extracted
            .input_type
            .map(|input_type| input_type.to_ascii_lowercase());
        let Some(content_editable) = extracted.content_editable else {
            truncated = true;
            continue;
        };
        if input_type.as_deref().is_some_and(|value| {
            value.is_empty()
                || value.len() > 32
                || !value.bytes().all(|byte| byte.is_ascii_alphanumeric())
        }) {
            truncated = true;
            continue;
        }
        let mut candidate_items = items.clone();
        candidate_items.push(item);
        if semantic_item_serialized_chars(&candidate_items) > max_serialized_chars {
            truncated = true;
            break;
        }
        let item = candidate_items.last().expect("candidate item was pushed");
        targets.push(BrowserSemanticTargetRecord {
            reference: item.reference.clone(),
            ordinal,
            role: item.role.clone(),
            name: item.name.clone(),
            destination: item.destination.clone(),
            disabled: item.disabled,
            checked: item.checked,
            selected: item.selected,
            expanded: item.expanded,
            pressed: item.pressed,
            tag,
            input_type,
            content_editable,
        });
        items = candidate_items;
    }
    (items, targets, truncated)
}

pub fn browser_context_script() -> String {
    // This code is initiated by the Rust command after scope validation. It
    // does not expose an IPC bridge or a callable DCC API to the remote page.
    format!(
        r#"(() => {{
  try {{
    const documentIdentity = (() => {{
      try {{ const value = performance.timeOrigin; return Number.isFinite(value) && value > 0 ? value : null; }}
      catch (_) {{ return null; }}
    }})();
    const maxTextChars = {MAX_BROWSER_CONTEXT_CHARS};
    const maxCandidates = {MAX_BROWSER_SEMANTIC_CANDIDATES};
    const maxScanNodes = {MAX_BROWSER_SEMANTIC_SCAN_NODES};
    const maxAncestors = {MAX_BROWSER_SEMANTIC_ANCESTORS};
    const maxTextNodes = {MAX_BROWSER_CONTEXT_TEXT_NODES};
    const maxShadowRoots = 8;
    const maxNameTextNodes = {MAX_BROWSER_SEMANTIC_NAME_TEXT_NODES};
    const maxItems = {MAX_BROWSER_SEMANTIC_ITEMS};
    const maxNameChars = {MAX_BROWSER_SEMANTIC_NAME_CHARS};
    const maxDestinationChars = {MAX_BROWSER_SEMANTIC_DESTINATION_CHARS};
    const maxMapChars = {MAX_BROWSER_SEMANTIC_SERIALIZED_CHARS};
    const clean = (value, max = maxNameChars) => String(value ?? "").slice(0, Math.max(max * 4, max))
      .replace(/[\u0000-\u001f\u007f-\u009f]/g, " ")
      .replace(/\s+/g, " ")
      .trim()
      .slice(0, max);
    let visibilityLimitReached = false;
    const composedParent = (element) => {{
      if (!element) return null;
      if (element.parentElement) return element.parentElement;
      try {{
        const root = element.getRootNode?.();
        return root?.host instanceof Element ? root.host : null;
      }} catch (_) {{ return null; }}
    }};
    const isVisible = (element) => {{
      let node = element;
      for (let depth = 0; node && depth < maxAncestors; depth += 1, node = composedParent(node)) {{
        if (node.hidden || node.inert || node.getAttribute("aria-hidden") === "true") return false;
        const style = window.getComputedStyle(node);
        if (style.display === "none" || style.visibility === "hidden" || Number.parseFloat(style.opacity) === 0) return false;
      }}
      if (node) {{ visibilityLimitReached = true; return false; }}
      const rect = element.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0
        && rect.right > 0 && rect.bottom > 0
        && rect.left < window.innerWidth && rect.top < window.innerHeight;
    }};
    let semanticTextTruncated = false;
    const semanticValue = (value, max = maxNameChars) => {{
      const raw = String(value ?? "");
      const text = clean(raw, max);
      if (raw.length > max || text.length >= max) semanticTextTruncated = true;
      return text;
    }};
    // Text nodes can expose authored defaults or live user input from controls.
    // Walk only a bounded ancestor chain and omit the node unless it can be
    // proven outside every value-bearing or editable subtree.
    const textNodeSafety = (node) => {{
      let element = node?.parentElement || null;
      for (let depth = 0; depth < maxAncestors; depth += 1) {{
        if (!element) return {{ safe: false, truncated: true }};
        const tag = element.tagName.toLowerCase();
        if (["input", "textarea", "select", "option"].includes(tag) || element.isContentEditable) {{
          return {{ safe: false, truncated: false }};
        }}
        if (element === document.body) return {{ safe: true, truncated: false }};
        element = composedParent(element);
      }}
      return {{ safe: false, truncated: true }};
    }};
    const boundedText = (root, maxChars, nodeBudget) => {{
      if (!root) return {{ text: "", truncated: false, scanned: 0 }};
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      const parts = [];
      let used = 0;
      let seen = 0;
      let truncated = false;
      let node;
      while (seen < nodeBudget && (node = walker.nextNode())) {{
        seen += 1;
        const safety = textNodeSafety(node);
        if (!safety.safe) {{
          if (safety.truncated) truncated = true;
          continue;
        }}
        const parent = node.parentElement;
        if (!parent || !isVisible(parent)) continue;
        const remaining = Math.max(0, maxChars - used);
        const raw = String(node.data ?? "");
        const part = clean(raw, remaining);
        if (!part) continue;
        parts.push(part);
        used += part.length + 1;
        if (raw.length > remaining || used >= maxChars) {{ truncated = true; break; }}
      }}
      if (seen >= nodeBudget) truncated = true;
      return {{ text: clean(parts.join(" "), maxChars), truncated, scanned: seen }};
    }};
    let selectionPolicyTruncated = false;
    const safeSelection = (() => {{
      try {{
        const selection = window.getSelection?.();
        if (!selection || selection.rangeCount !== 1 || selection.isCollapsed) return null;
        const range = selection.getRangeAt(0);
        // A multi-node range could cross an editable/control subtree even if
        // both endpoints look safe, so only one verified text node is allowed.
        if (range.startContainer !== range.endContainer || range.startContainer.nodeType !== Node.TEXT_NODE) return null;
        const selectionSafety = textNodeSafety(range.startContainer);
        if (!selectionSafety.safe) {{
          if (selectionSafety.truncated) selectionPolicyTruncated = true;
          return null;
        }}
        const raw = selection.toString();
        return {{ text: clean(raw, maxTextChars), truncated: raw.length > maxTextChars }};
      }} catch (_) {{
        selectionPolicyTruncated = true;
        return null;
      }}
    }})();
    // Visible text is bounded independently. Include explicitly open Shadow
    // DOM (for example framework error overlays), but keep a shared text-node
    // budget and walk the composed ancestor chain so editable/control content
    // remains excluded. Closed Shadow DOM is never inspected.
    const textRoots = (() => {{
      const roots = document.body ? [document.body] : [];
      let scanned = 0;
      let truncated = false;
      const addOpenShadow = (element) => {{
        let shadow = null;
        try {{ shadow = element?.shadowRoot ?? null; }} catch (_) {{}}
        if (!shadow || roots.includes(shadow)) return;
        if (roots.length > maxShadowRoots) {{ truncated = true; return; }}
        roots.push(shadow);
      }};
      // Framework overlays are commonly direct body children; inspect these
      // first so a large application DOM cannot consume the discovery budget.
      const directChildren = document.body?.children;
      const directLimit = Math.min(64, directChildren?.length ?? 0);
      for (let index = 0; index < directLimit; index += 1) addOpenShadow(directChildren[index]);
      if ((directChildren?.length ?? 0) > directLimit) truncated = true;
      for (let rootIndex = 0; rootIndex < roots.length && scanned < maxScanNodes; rootIndex += 1) {{
        const walker = document.createTreeWalker(roots[rootIndex], NodeFilter.SHOW_ELEMENT);
        let element;
        while (scanned < maxScanNodes && (element = walker.nextNode())) {{
          scanned += 1;
          addOpenShadow(element);
        }}
      }}
      if (scanned >= maxScanNodes) truncated = true;
      // Overlays are visually on top of the page, so reserve the shared text
      // budget for their open roots before scanning the underlying body.
      return {{ roots: [...roots.slice(1), ...roots.slice(0, 1)], truncated }};
    }})();
    const composedVisibleText = () => {{
      const parts = [];
      let usedChars = 0;
      let usedNodes = 0;
      let truncated = textRoots.truncated;
      for (const root of textRoots.roots) {{
        const remainingChars = Math.max(0, maxTextChars - usedChars);
        const remainingNodes = Math.max(0, maxTextNodes - usedNodes);
        if (remainingChars === 0 || remainingNodes === 0) {{ truncated = true; break; }}
        const part = boundedText(root, remainingChars, remainingNodes);
        usedNodes += part.scanned;
        if (part.truncated) truncated = true;
        if (!part.text) continue;
        parts.push(part.text);
        usedChars += part.text.length + 1;
      }}
      return {{ text: clean(parts.join(" "), maxTextChars), truncated }};
    }};
    const visibleText = safeSelection
      ? safeSelection
      : composedVisibleText();
    const labelledByName = (element) => {{
      const ids = (element.getAttribute("aria-labelledby") || "").trim().split(/\s+/).filter(Boolean).slice(0, 4);
      const parts = [];
      for (const id of ids) {{
        const label = document.getElementById(id);
        if (label && label !== element && isVisible(label)) {{
          const text = boundedText(label, maxNameChars, maxNameTextNodes);
          if (text.truncated) semanticTextTruncated = true;
          parts.push(text.text);
        }}
      }}
      return semanticValue(parts.filter(Boolean).join(" "));
    }};
    const accessibleName = (element) => {{
      const ariaLabel = semanticValue(element.getAttribute("aria-label"));
      if (ariaLabel) return ariaLabel;
      const labelledBy = labelledByName(element);
      if (labelledBy) return labelledBy;
      const labels = "labels" in element && element.labels ? Array.from(element.labels).slice(0, 3) : [];
      const associated = semanticValue(labels.map((label) => {{
        const text = boundedText(label, maxNameChars, maxNameTextNodes);
        if (text.truncated) semanticTextTruncated = true;
        return text.text;
      }}).join(" "));
      if (associated) return associated;
      // These are bounded author-provided labels, never runtime control values.
      const alt = semanticValue(element.getAttribute("alt"));
      if (alt) return alt;
      const title = semanticValue(element.getAttribute("title"));
      if (title) return title;
      const placeholder = semanticValue(element.getAttribute("placeholder"));
      if (placeholder) return placeholder;
      if (element.isContentEditable || ["input", "textarea", "select"].includes(element.tagName.toLowerCase())) return "";
      const text = boundedText(element, maxNameChars, maxNameTextNodes);
      if (text.truncated) semanticTextTruncated = true;
      return text.text;
    }};
    const canonicalRole = (element) => {{
      const tag = element.tagName.toLowerCase();
      const explicit = (element.getAttribute("role") || "").toLowerCase();
      if (tag === "input") {{
        const inputType = (element.getAttribute("type") || "text").toLowerCase();
        // Never map value-bearing secrets, even when a page supplies a
        // misleading ARIA role.
        if (inputType === "hidden" || inputType === "password") return null;
      }}
      if (/^h[1-6]$/.test(tag)) return "heading";
      if (explicit === "heading") {{
        const level = Number.parseInt(element.getAttribute("aria-level") || "", 10);
        return level >= 1 && level <= 6 ? "heading" : null;
      }}
      if (tag === "button" || explicit === "button") return "button";
      if ((tag === "a" && element.hasAttribute("href")) || explicit === "link") return "link";
      if (tag === "textarea" || element.isContentEditable || ["textbox", "searchbox"].includes(explicit)) return "textbox";
      if (tag === "select" || ["combobox", "listbox"].includes(explicit)) return "select";
      if (explicit === "checkbox") return "checkbox";
      if (explicit === "radio") return "radio";
      if (explicit === "switch") return "switch";
      if (explicit === "slider") return "slider";
      if (explicit === "spinbutton") return "spinbutton";
      if (explicit === "option") return "option";
    if (["tab", "menuitem"].includes(explicit)) return explicit;
      if (tag !== "input") return null;
      const type = (element.getAttribute("type") || "text").toLowerCase();
      if (type === "hidden" || type === "password") return null;
      if (["button", "submit", "reset", "image"].includes(type)) return "button";
      if (type === "checkbox") return "checkbox";
      if (type === "radio") return "radio";
      if (type === "range") return "slider";
      if (type === "number") return "spinbutton";
      if (type === "file") return "file-input";
      return "textbox";
    }};
    const booleanState = (element, property, ariaName) => {{
      try {{
        const aria = element.getAttribute(ariaName);
        if (aria === "true") return true;
        if (aria === "false") return false;
        return typeof element[property] === "boolean" ? element[property] : undefined;
      }} catch (_) {{ return undefined; }}
    }};
    const isCandidate = (element) => {{
      const tag = element.tagName.toLowerCase();
      if ((element.isContentEditable && !element.parentElement?.isContentEditable) || ["button", "input", "textarea", "select"].includes(tag) || (tag === "a" && element.hasAttribute("href")) || /^h[1-6]$/.test(tag)) return true;
      return ["heading", "button", "link", "textbox", "searchbox", "combobox", "listbox", "checkbox", "radio", "switch", "slider", "spinbutton", "option", "tab", "menuitem"].includes((element.getAttribute("role") || "").toLowerCase());
    }};
    const map = {{ items: [], truncated: false }};
    // The actionable semantic map intentionally stays in the document tree:
    // Shadow DOM and iframe targets need distinct action anchoring and are not
    // represented by this first provider-neutral control contract.
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_ELEMENT);
    let scanned = 0;
    let candidates = 0;
    let element;
    while (scanned < maxScanNodes && (element = walker.nextNode())) {{
      scanned += 1;
      if (!isCandidate(element)) continue;
      candidates += 1;
      if (candidates > maxCandidates) {{ map.truncated = true; break; }}
      if (map.items.length >= maxItems) {{ map.truncated = true; break; }}
      if (!isVisible(element)) continue;
      const role = canonicalRole(element);
      if (!role) continue;
      const tag = element.tagName.toLowerCase();
      const inputType = tag === "input"
        ? (element.getAttribute("type") || "text").toLowerCase()
        : undefined;
      const item = {{ role, name: accessibleName(element), ordinal: scanned, tag, inputType, contentEditable: Boolean(element.isContentEditable) }};
      if (tag === "select") {{
        item.options = Array.from(element.options).slice(0, 30).filter(option => !option.disabled && !option.hidden).map(option => semanticValue(option.label));
        if (element.options.length > 30) map.truncated = true;
      }}
      if (role === "heading") {{
        const tag = element.tagName.toLowerCase();
        const level = /^h[1-6]$/.test(tag)
          ? Number.parseInt(tag.slice(1), 10)
          : Number.parseInt(element.getAttribute("aria-level") || "", 10);
        if (level < 1 || level > 6) {{ map.truncated = true; continue; }}
        item.level = level;
      }}
      if (role === "link") {{
        try {{ item.destination = semanticValue(new URL(element.href, window.location.href).toString(), maxDestinationChars); }} catch (_) {{}}
      }}
      const disabled = role === "heading" ? undefined : booleanState(element, "disabled", "aria-disabled");
      const checked = ["checkbox", "radio", "switch"].includes(role) ? booleanState(element, "checked", "aria-checked") : undefined;
      const selected = ["select", "option"].includes(role) ? booleanState(element, "selected", "aria-selected") : undefined;
      const expanded = booleanState(element, "ariaExpanded", "aria-expanded");
      const pressed = role === "button" ? booleanState(element, "ariaPressed", "aria-pressed") : undefined;
      if (disabled !== undefined) item.disabled = disabled;
      if (checked !== undefined) item.checked = checked;
      if (selected !== undefined) item.selected = selected;
      if (expanded !== undefined) item.expanded = expanded;
      if (pressed !== undefined) item.pressed = pressed;
      if (!item.name && !item.destination) continue;
      if (JSON.stringify([...map.items, item]).length > maxMapChars) {{ map.truncated = true; break; }}
      map.items.push(item);
    }}
    if (scanned >= maxScanNodes || visibilityLimitReached || semanticTextTruncated) map.truncated = true;
    return {{
      text: visibleText.text,
      selectionOnly: Boolean(safeSelection),
      truncated: visibleText.truncated || selectionPolicyTruncated,
      documentIdentity,
      semanticMap: map,
    }};
  }} catch (_) {{
    return {{ text: "", selectionOnly: false, truncated: false, semanticMap: {{ items: [], truncated: true }} }};
  }}
}})()"#,
    )
}

/// The page's answer within every bound, before a map id is issued.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundedContext {
    pub document_identity: f64,
    pub url: String,
    pub title: Option<String>,
    pub text: String,
    pub selection_only: bool,
    /// The page cut its text, or the text, title or URL were cut here.
    pub truncated: bool,
    pub items: Vec<BrowserSemanticItem>,
    pub targets: Vec<BrowserSemanticTargetRecord>,
    pub semantic_truncated: bool,
}

/// What the context script answered (`raw`, its JSON) for the page at
/// `url` titled `title`, bounded: the text first, then a semantic map in
/// what the envelope has left.
pub fn bound_context(raw: &str, url: &str, title: Option<&str>) -> Result<BoundedContext, String> {
    let extracted: BrowserContextExtraction = serde_json::from_str(raw)
        .map_err(|_| "browser page context response was invalid".to_string())?;
    let document_identity = validate_browser_document_identity(extracted.document_identity)?;
    let (text, text_truncated) =
        normalize_browser_context_text(&extracted.text, MAX_BROWSER_CONTEXT_CHARS);
    let (title, title_truncated) = title
        .map(|title| normalize_browser_context_text(title, MAX_BROWSER_TITLE_CHARS))
        .unwrap_or_default();
    let (url, url_truncated) = normalize_browser_context_text(url, MAX_BROWSER_CONTEXT_URL_CHARS);
    let fixed_envelope_chars = text.chars().count() + title.chars().count() + url.chars().count();
    let semantic_budget = MAX_BROWSER_SEMANTIC_SERIALIZED_CHARS
        .min(MAX_BROWSER_CONTEXT_ENVELOPE_CHARS.saturating_sub(fixed_envelope_chars));
    let (items, targets, semantic_truncated) =
        normalize_browser_semantic_map(extracted.semantic_map, semantic_budget);
    Ok(BoundedContext {
        document_identity,
        url,
        title: (!title.is_empty()).then_some(title),
        text,
        selection_only: extracted.selection_only,
        truncated: extracted.truncated || text_truncated || title_truncated || url_truncated,
        items,
        targets,
        semantic_truncated,
    })
}

/// `MAX_FORMATTED_BROWSER_CONTEXT_CHARS`
pub const MAX_FORMATTED_BROWSER_CONTEXT_CHARS: usize = 15_000;

/// The envelope's words, in the app's language (`browser.agentContext.*`).
#[derive(Debug, Clone, Default)]
pub struct ContextLabels {
    pub prompt: String,
    pub url: String,
    pub title: String,
    pub source: String,
    pub truncated: String,
    pub selection: String,
    pub visible_text: String,
    pub yes: String,
    pub no: String,
    pub semantic_map: String,
    pub map_id: String,
    pub generation: String,
    pub visible_elements: String,
    pub name: String,
    pub destination: String,
    pub states: String,
    pub no_visible_elements: String,
}

/// `escapeBrowserAgentContext`: remote text stays inside the envelope.
pub fn escape_agent_context(value: &str) -> String {
    value.replace('<', "&lt;")
}

/// `escapeBrowserAgentContextWithin`: escaped, up to `max` characters.
fn escape_within(value: &str, max: usize) -> (String, bool) {
    let mut escaped = String::new();
    let mut count = 0;
    for character in value.chars() {
        let next = if character == '<' { "&lt;" } else { "" };
        let width = if next.is_empty() { 1 } else { next.len() };
        if count + width > max {
            return (escaped, true);
        }
        if next.is_empty() {
            escaped.push(character);
        } else {
            escaped.push_str(next);
        }
        count += width;
    }
    (escaped, false)
}

fn semantic_line(item: &BrowserSemanticItem, labels: &ContextLabels) -> String {
    let flag = |name: &str, value: Option<bool>| value.map(|value| format!("{name}={value}"));
    let states: Vec<String> = [
        flag("disabled", item.disabled),
        flag("checked", item.checked),
        flag("selected", item.selected),
        flag("expanded", item.expanded),
        flag("pressed", item.pressed),
    ]
    .into_iter()
    .flatten()
    .collect();
    let level = item
        .level
        .filter(|level| *level > 0)
        .map(|level| format!(" level={level}"))
        .unwrap_or_default();
    [
        Some(format!(
            "- [{}] {}{level}",
            escape_agent_context(&item.reference),
            escape_agent_context(&item.role)
        )),
        Some(format!(
            "{}: {}",
            labels.name,
            escape_agent_context(&item.name)
        )),
        item.destination.as_ref().map(|destination| {
            format!(
                "{}: {}",
                labels.destination,
                escape_agent_context(destination)
            )
        }),
        (!item.options.is_empty()).then(|| {
            let options: Vec<String> = item
                .options
                .iter()
                .map(|option| escape_agent_context(option))
                .collect();
            format!("options: {}", options.join(", "))
        }),
        (!states.is_empty()).then(|| format!("{}: {}", labels.states, states.join(", "))),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" | ")
}

/// `formatBrowserAgentContext`: `<browser_page>` and
/// `<browser_semantic_map>` within 15 000 characters — the map's items go
/// first, then the text is cut.
pub fn format_agent_context(context: &BrowserAgentContext, labels: &ContextLabels) -> String {
    let source = if context.selection_only {
        &labels.selection
    } else {
        &labels.visible_text
    };
    let all_items: Vec<String> = context
        .semantic_map
        .items
        .iter()
        .map(|item| semantic_line(item, labels))
        .collect();
    let mut included = all_items.len();
    let mut map_truncated = context.semantic_map.truncated;
    let mut text = escape_agent_context(&context.text);
    let mut frontend_truncated = false;
    let build = |included: usize, map_truncated: bool, text: &str, frontend_truncated: bool| {
        let yes_no = |value: bool| if value { &labels.yes } else { &labels.no };
        let mut lines = vec![
            labels.prompt.clone(),
            String::new(),
            "<browser_page>".to_string(),
            format!("{}: {}", labels.url, escape_agent_context(&context.url)),
            format!(
                "{}: {}",
                labels.title,
                escape_agent_context(context.title.as_deref().unwrap_or_default())
            ),
            format!("{}: {source}", labels.source),
            format!(
                "{}: {}",
                labels.truncated,
                yes_no(context.truncated || map_truncated || frontend_truncated)
            ),
            "---".to_string(),
            text.to_string(),
            "</browser_page>".to_string(),
            String::new(),
            "<browser_semantic_map>".to_string(),
            format!("{}: {}", labels.semantic_map, yes_no(map_truncated)),
            format!(
                "{}: {}",
                labels.map_id,
                escape_agent_context(&context.semantic_map.map_id)
            ),
            format!("{}: {}", labels.generation, context.semantic_map.generation),
            format!("{}:", labels.visible_elements),
        ];
        if included > 0 {
            lines.extend(all_items[..included].iter().cloned());
        } else {
            lines.push(format!("- {}", labels.no_visible_elements));
        }
        lines.push("</browser_semantic_map>".to_string());
        lines.join("\n")
    };
    let mut envelope = build(included, map_truncated, &text, frontend_truncated);
    while envelope.chars().count() > MAX_FORMATTED_BROWSER_CONTEXT_CHARS && included > 0 {
        included -= 1;
        map_truncated = true;
        envelope = build(included, map_truncated, &text, frontend_truncated);
    }
    if envelope.chars().count() > MAX_FORMATTED_BROWSER_CONTEXT_CHARS {
        let before = text.clone();
        frontend_truncated = true;
        let overhead = build(included, map_truncated, "", frontend_truncated)
            .chars()
            .count();
        let (bounded, cut) = escape_within(
            &context.text,
            MAX_FORMATTED_BROWSER_CONTEXT_CHARS.saturating_sub(overhead),
        );
        frontend_truncated = frontend_truncated || cut || before != bounded;
        text = bounded;
        envelope = build(included, map_truncated, &text, frontend_truncated);
    }
    envelope
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(text: &str, items: &str) -> String {
        format!(
            r#"{{"text":{},"selectionOnly":false,"truncated":false,"documentIdentity":12.5,"semanticMap":{{"items":{items},"truncated":false}}}}"#,
            serde_json::to_string(text).unwrap()
        )
    }

    #[test]
    fn the_answer_is_bounded_and_links_lose_their_secrets() {
        let items = r#"[{"role":"link","name":" Entrar ","destination":"https://u:p@ex.com/login?next=1#x","ordinal":3,"tag":"A","contentEditable":false},{"role":"script","name":"x","ordinal":4,"tag":"div","contentEditable":false},{"role":"heading","level":2,"name":"Planos","ordinal":5,"tag":"h2","contentEditable":false}]"#;
        let bounded = bound_context(
            &raw("olá\u{0007}\r\nmundo", items),
            "https://ex.com/",
            Some("Ex"),
        )
        .unwrap();
        assert_eq!(bounded.text, "olá\nmundo");
        assert_eq!(bounded.title.as_deref(), Some("Ex"));
        assert_eq!(bounded.items.len(), 2);
        assert_eq!(bounded.items[0].reference, "e1");
        assert_eq!(bounded.items[0].name, "Entrar");
        assert_eq!(
            bounded.items[0].destination.as_deref(),
            Some("https://ex.com/login")
        );
        assert_eq!(bounded.items[1].level, Some(2));
        assert!(bounded.semantic_truncated, "the unknown role is dropped");
        assert!(bound_context("not json", "https://ex.com/", None).is_err());
        assert!(bound_context(
            &raw("x", "[]").replace("12.5", "null"),
            "https://ex.com/",
            None
        )
        .is_err());
    }

    fn labels() -> ContextLabels {
        ContextLabels {
            prompt: String::new(),
            url: "URL".into(),
            title: "Título".into(),
            source: "Origem".into(),
            truncated: "Truncado".into(),
            selection: "texto selecionado".into(),
            visible_text: "texto visível da página".into(),
            yes: "sim".into(),
            no: "não".into(),
            semantic_map: "Mapa semântico truncado".into(),
            map_id: "ID do mapa".into(),
            generation: "Geração".into(),
            visible_elements: "Elementos visíveis".into(),
            name: "Nome".into(),
            destination: "Destino".into(),
            states: "Estados".into(),
            no_visible_elements: "Nenhum elemento visível elegível".into(),
        }
    }

    fn context(text: &str, items: Vec<BrowserSemanticItem>) -> BrowserAgentContext {
        BrowserAgentContext {
            workspace_id: "w".into(),
            session_id: None,
            url: "https://ex.com/".into(),
            title: Some("<Ex>".into()),
            text: text.into(),
            selection_only: true,
            truncated: false,
            semantic_map: BrowserSemanticMap {
                map_id: "m-1-1".into(),
                generation: 1,
                page_load_revision: 1,
                items,
                truncated: false,
            },
        }
    }

    fn item(reference: &str) -> BrowserSemanticItem {
        BrowserSemanticItem {
            reference: reference.into(),
            role: "button".into(),
            level: None,
            name: "Salvar".into(),
            destination: None,
            disabled: Some(false),
            checked: None,
            selected: None,
            expanded: None,
            pressed: Some(true),
            options: Vec::new(),
        }
    }

    #[test]
    fn the_envelope_reads_like_the_desktop() {
        let text = format_agent_context(&context("a <b>", vec![item("e1")]), &labels());
        assert_eq!(
            text,
            [
                "",
                "",
                "<browser_page>",
                "URL: https://ex.com/",
                "Título: &lt;Ex>",
                "Origem: texto selecionado",
                "Truncado: não",
                "---",
                "a &lt;b>",
                "</browser_page>",
                "",
                "<browser_semantic_map>",
                "Mapa semântico truncado: não",
                "ID do mapa: m-1-1",
                "Geração: 1",
                "Elementos visíveis:",
                "- [e1] button | Nome: Salvar | Estados: disabled=false, pressed=true",
                "</browser_semantic_map>",
            ]
            .join("\n")
        );
        let empty = format_agent_context(&context("x", Vec::new()), &labels());
        assert!(empty.contains("- Nenhum elemento visível elegível"));
    }

    #[test]
    fn a_long_page_drops_items_then_cuts_the_text() {
        let items: Vec<BrowserSemanticItem> = (1..=80).map(|n| item(&format!("e{n}"))).collect();
        let long = "x".repeat(20_000);
        let text = format_agent_context(&context(&long, items), &labels());
        assert!(text.chars().count() <= MAX_FORMATTED_BROWSER_CONTEXT_CHARS);
        assert!(text.contains("Truncado: sim"));
        assert!(text.contains("- Nenhum elemento visível elegível"));
    }
}
