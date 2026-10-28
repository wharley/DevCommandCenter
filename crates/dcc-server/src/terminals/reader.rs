//! A shell's reader thread: the output buffer and scrollback, its title,
//! whether it waits on the person, and the activity it reports.

use std::sync::LazyLock;

use regex::Regex;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use super::*;
use crate::http_api::{publish_desktop_terminal_event, DesktopTerminalEvent};

// Regex para detectar quando um terminal está esperando input do usuário
static WAIT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(trust|workspace\s+trust|permission|approve|confirm|\(y/n\)|\[y/n\]|\by/n\b|press\s+enter|waiting\s+for|allow\s+edit|must\s+be\s+trusted|denied|blocked|requires?\s+your|intervention|open\s+.*\s+to\s+continue|do\s+you\s+want|aguardando|permiss[aã]o|aprovar|confirmar|pressione\s+enter|pressione\s+retorno|deseja\s+continuar|requer\s+sua\s+interven[cç][aã]o)"
    )
    .unwrap()
});

pub fn normalize_terminal_title(raw: &str) -> Option<String> {
    let stripped = strip_ansi_codes(raw);

    // Remove OSC sequence residues (e.g., "11;rgb:1a1a/1a1a/1a1a")
    // Pattern: digit(s) followed by semicolon and rgb/hex color data
    let osc_cleaned = stripped
        .split_whitespace()
        .filter(|word| {
            // Filter out OSC color code patterns like "11;rgb:..." or "10;#..."
            !word.chars().next().map_or(false, |c| c.is_ascii_digit())
                || !word.contains(';')
                || (!word.contains("rgb:") && !word.contains('#'))
        })
        .collect::<Vec<_>>()
        .join(" ");

    let collapsed = osc_cleaned
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>();
    let normalized = collapsed.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn update_terminal_title_from_bytes(
    data: &[u8],
    carry: &mut Vec<u8>,
    title_state: &Arc<Mutex<Option<String>>>,
) {
    let mut bytes = Vec::with_capacity(carry.len() + data.len());
    bytes.extend_from_slice(carry);
    bytes.extend_from_slice(data);
    carry.clear();

    let mut i = 0usize;
    let mut latest_title: Option<String> = None;
    while i < bytes.len() {
        if bytes[i] != 0x1b {
            i += 1;
            continue;
        }
        if i + 1 >= bytes.len() {
            carry.extend_from_slice(&bytes[i..]);
            break;
        }
        if bytes[i + 1] != b']' {
            i += 1;
            continue;
        }

        let seq_start = i;
        i += 2;
        let kind_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i >= bytes.len() {
            carry.extend_from_slice(&bytes[seq_start..]);
            break;
        }
        if bytes[i] != b';' {
            i = seq_start + 1;
            continue;
        }

        let kind = std::str::from_utf8(&bytes[kind_start..i]).ok();
        i += 1;
        let payload_start = i;
        let mut terminated_at: Option<usize> = None;
        while i < bytes.len() {
            if bytes[i] == 0x07 {
                terminated_at = Some(i);
                break;
            }
            if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                terminated_at = Some(i);
                break;
            }
            i += 1;
        }
        let Some(term_idx) = terminated_at else {
            carry.extend_from_slice(&bytes[seq_start..]);
            break;
        };

        if matches!(kind, Some("0") | Some("2")) {
            if let Ok(raw_title) = std::str::from_utf8(&bytes[payload_start..term_idx]) {
                if let Some(title) = normalize_terminal_title(raw_title) {
                    latest_title = Some(title);
                }
            }
        }

        i = if bytes[term_idx] == 0x1b {
            term_idx + 2
        } else {
            term_idx + 1
        };
    }

    if let Some(title) = latest_title {
        if let Ok(mut guard) = title_state.lock() {
            *guard = Some(title);
        }
    }
}

pub fn waiting_excerpt_from_buffer(output_buffer: &Arc<Mutex<VecDeque<String>>>) -> Option<String> {
    let buffer = output_buffer.lock().ok()?;
    let tail: String = buffer
        .iter()
        .rev()
        .take(5)
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    let stripped = strip_ansi_codes(&tail);
    if WAIT_PATTERN.is_match(&stripped) {
        return buffer.iter().rev().find(|s| !s.trim().is_empty()).map(|s| {
            let trimmed = strip_ansi_codes(s).trim().to_string();
            const MAX_EXCERPT_CHARS: usize = 220;
            if trimmed.chars().count() > MAX_EXCERPT_CHARS {
                format!(
                    "{}…",
                    trimmed.chars().take(MAX_EXCERPT_CHARS).collect::<String>()
                )
            } else {
                trimmed
            }
        });
    }
    None
}

/// Verifica se o terminal está aguardando input do usuário
/// Retorna (needs_attention, excerpt_opcional)
pub fn check_needs_attention(
    data: &str,
    output_buffer: &Arc<Mutex<VecDeque<String>>>,
) -> (bool, Option<String>) {
    // OSC 9;9; protocol (Ghostty)
    if data.contains("\x1b]9;9;") {
        return (true, Some("Notification request".to_string()));
    }

    // Verificar últimas linhas do buffer para padrões de espera
    if let Ok(buffer) = output_buffer.lock() {
        // Pegar últimas 5 linhas
        let tail: String = buffer
            .iter()
            .rev()
            .take(5)
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");

        // Strip ANSI para análise mais limpa
        let stripped = strip_ansi_codes(&tail);

        if WAIT_PATTERN.is_match(&stripped) {
            // Extrair excerpt da última linha não vazia
            let excerpt = buffer
                .iter()
                .rev()
                .find(|s| !s.trim().is_empty())
                .map(|s| {
                    let trimmed = strip_ansi_codes(s).trim().to_string();
                    const MAX_EXCERPT_CHARS: usize = 220;
                    if trimmed.chars().count() > MAX_EXCERPT_CHARS {
                        format!(
                            "{}…",
                            trimmed.chars().take(MAX_EXCERPT_CHARS).collect::<String>()
                        )
                    } else {
                        trimmed
                    }
                })
                .unwrap_or_default();

            return (true, Some(excerpt));
        }
    }

    (false, None)
}

/// Remove códigos ANSI básicos para análise de texto
pub fn strip_ansi_codes(input: &str) -> String {
    // This runs for every terminal output batch and activity poll. Compile once,
    // preserving the exact matching/replacement behavior of the three passes.
    use std::sync::LazyLock;
    static RE_SGR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\x1b\[[\d;?]*[A-Za-z]").unwrap());
    static RE_OSC1: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\][^\x07]*\x07").unwrap());
    static RE_OSC2: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\][^\x1b\\]*\\").unwrap());
    // Remove SGR sequences: ESC [ ... m
    let s = RE_SGR.replace_all(input, "");

    // Remove OSC sequences: ESC ] ... BEL or ESC ]... ESC \
    let s = RE_OSC1.replace_all(&s, "");

    RE_OSC2.replace_all(&s, "").to_string()
}

pub fn spawn_terminal_reader_thread(
    mut reader: Box<dyn Read + Send>,
    app: TerminalEvents,
    pty_id: String,
    pane_id: Option<String>,
    title_state: Arc<Mutex<Option<String>>>,
    stop_flag: Arc<AtomicBool>,
    output_buffer: Arc<Mutex<VecDeque<String>>>,
    conn: Arc<Mutex<Connection>>,
) -> JoinHandle<()> {
    /// Batching alinhado a superset-sh (`apps/desktop/src/main/terminal-host/pty-subprocess.ts`):
    /// ~60fps + limite por frame evita inundar o IPC com micro-chunks e mantém redesenhos de TUI (`\r`, escapes)
    /// mais coerentes num único `terminal-output` antes do xterm.js — sem alterar o stream final concatenado.
    const FLUSH_INTERVAL: Duration = Duration::from_millis(16);
    const MAX_BATCH_BYTES: usize = 128 * 1024;

    thread::spawn(move || {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let last_persist = Arc::new(Mutex::new(
            Instant::now() - PANE_SCROLLBACK_PERSIST_INTERVAL,
        ));
        let mut title_remainder: Vec<u8> = Vec::new();

        let read_thread = thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(buffer[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let mut pending: Vec<u8> = Vec::new();

        let mut flush_pending = |pending: &mut Vec<u8>| {
            if pending.is_empty() {
                return;
            }
            update_terminal_title_from_bytes(pending, &mut title_remainder, &title_state);
            let data = String::from_utf8_lossy(pending).to_string();
            pending.clear();

            append_terminal_line(&output_buffer, &data);

            if let Some(ref pid) = pane_id {
                if let Ok(mut lp) = last_persist.lock() {
                    if lp.elapsed() >= PANE_SCROLLBACK_PERSIST_INTERVAL {
                        *lp = Instant::now();
                        let lines: Vec<String> = output_buffer
                            .lock()
                            .ok()
                            .map(|g| g.iter().cloned().collect())
                            .unwrap_or_default();
                        if let Ok(c) = conn.lock() {
                            if let Err(e) = persist_pane_scrollback_compressed(&c, pid, &lines) {
                                eprintln!("[DCC] pane scrollback persist (throttled): {e}");
                            }
                        }
                    }
                }
            }

            let (needs_attention, excerpt) = check_needs_attention(&data, &output_buffer);

            if needs_attention {
                let excerpt_key = excerpt
                    .as_ref()
                    .map(|s| s.chars().take(64).collect::<String>())
                    .unwrap_or_default();
                let activity_excerpt = excerpt.clone();
                let notification_id = format!(
                    "{}:{}:{}",
                    pane_id.as_deref().unwrap_or_else(|| pty_id.as_str()),
                    "needs_input",
                    excerpt_key
                );
                let mut payload = serde_json::json!({
                    "ptyId": pty_id,
                    "status": "waiting",
                    "phase": "needs_input",
                    "notificationId": notification_id
                });

                if let Some(ref pid) = pane_id {
                    payload["paneId"] = serde_json::json!(pid);
                }

                if let Some(ref exc) = excerpt {
                    payload["excerpt"] = serde_json::json!(exc);
                }

                let _ = app.emit("terminal-attention", payload);
                let mut activity_payload = serde_json::json!({
                    "ptyId": pty_id,
                    "status": "waiting"
                });
                if let Some(ref pid) = pane_id {
                    activity_payload["paneId"] = serde_json::json!(pid);
                }
                if let Some(exc) = activity_excerpt {
                    activity_payload["excerpt"] = serde_json::json!(exc);
                }
                let _ = app.emit("terminal-activity", activity_payload);
            }

            let _ = app.emit(
                "terminal-output",
                serde_json::json!({
                  "ptyId": pty_id,
                  "data": data,
                  "stream": "stdout"
                }),
            );
            publish_desktop_terminal_event(DesktopTerminalEvent::Output {
                pty_id: pty_id.clone(),
                data,
            });
        };

        loop {
            match rx.recv_timeout(FLUSH_INTERVAL) {
                Ok(chunk) => {
                    pending.extend_from_slice(&chunk);
                    while let Ok(more) = rx.try_recv() {
                        pending.extend_from_slice(&more);
                        if pending.len() >= MAX_BATCH_BYTES {
                            flush_pending(&mut pending);
                        }
                    }
                    if pending.len() >= MAX_BATCH_BYTES {
                        flush_pending(&mut pending);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if !pending.is_empty() {
                        flush_pending(&mut pending);
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if !pending.is_empty() {
                        flush_pending(&mut pending);
                    }
                    break;
                }
            }
        }

        let _ = read_thread.join();

        if let Some(ref pid) = pane_id {
            let lines: Vec<String> = output_buffer
                .lock()
                .ok()
                .map(|g| g.iter().cloned().collect())
                .unwrap_or_default();
            if let Ok(c) = conn.lock() {
                if let Err(e) = persist_pane_scrollback_compressed(&c, pid, &lines) {
                    eprintln!("[DCC] pane scrollback persist (final): {e}");
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_terminal_title_collapses_whitespace_and_strips_controls() {
        let title = normalize_terminal_title("  Claude\tCode  \u{1b}[31m- Dev \u{1b}[0m  ");
        assert_eq!(title.as_deref(), Some("Claude Code - Dev"));
    }

    #[test]
    fn normalize_terminal_title_removes_osc_color_codes() {
        let title = normalize_terminal_title("service-dashboard git:(main) 11;rgb:1a1a/1a1a/1a1a");
        assert_eq!(title.as_deref(), Some("service-dashboard git:(main)"));
    }

    #[test]
    fn normalize_terminal_title_removes_multiple_osc_patterns() {
        let title = normalize_terminal_title("myapp 10;#ffffff 11;rgb:0000/0000/0000 test");
        assert_eq!(title.as_deref(), Some("myapp test"));
    }

    #[test]
    fn parses_bel_terminated_osc_title_in_single_chunk() {
        let title_state = Arc::new(Mutex::new(None));
        let mut carry = Vec::new();
        update_terminal_title_from_bytes(b"\x1b]2;Codex - project\x07", &mut carry, &title_state);

        assert!(carry.is_empty());
        assert_eq!(
            title_state
                .lock()
                .ok()
                .and_then(|guard| guard.clone())
                .as_deref(),
            Some("Codex - project")
        );
    }

    #[test]
    fn parses_st_terminated_osc_title_across_chunks() {
        let title_state = Arc::new(Mutex::new(None));
        let mut carry = Vec::new();

        update_terminal_title_from_bytes(b"\x1b]0;Claude ", &mut carry, &title_state);
        assert!(!carry.is_empty());
        assert!(title_state
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
            .is_none());

        update_terminal_title_from_bytes(b"Code\x1b\\", &mut carry, &title_state);
        assert!(carry.is_empty());
        assert_eq!(
            title_state
                .lock()
                .ok()
                .and_then(|guard| guard.clone())
                .as_deref(),
            Some("Claude Code")
        );
    }
}
