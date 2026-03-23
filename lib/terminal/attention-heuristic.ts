/**
 * Best-effort detection of “agent may be waiting for the user” from PTY text.
 * Used by the host nativo; manter a lógica sem dependências pesadas.
 */

/** Strip common ANSI SGR and OSC sequences (enough for CLI TUI output). */
export function stripAnsiForAttention(input: string): string {
  let s = input.replace(/\x1b\[[\d;?]*[A-Za-z]/g, "");
  s = s.replace(/\x1b\][^\x07]*\x07/g, "");
  s = s.replace(/\x1b\][^\x1b\\]*\\/g, "");
  return s;
}

/** Match likely prompts / blocks in the last few lines only (reduces false positives in long logs). */
const LIKELY_WAIT_RE =
  /(trust|workspace\s+trust|permission|approve|confirm|\(y\/n\)|\[y\/n\]|\by\/n\b|press\s+enter|waiting\s+for|allow\s+edit|must\s+be\s+trusted|denied|blocked|requires?\s+your|intervention|open\s+.*\s+to\s+continue|do\s+you\s+want)/i;

export function tailLooksLikeWaitingForUser(plainText: string): boolean {
  const tail = plainText.slice(-8000);
  const lines = tail.split(/\r?\n/).filter((l) => l.trim().length > 0);
  const lastFew = lines.slice(-5).join("\n");
  return LIKELY_WAIT_RE.test(lastFew);
}

/** Last non-empty line, trimmed and capped for toast excerpt. */
export function excerptFromPlainTail(plainText: string, maxLen = 220): string {
  const lines = plainText.split(/\r?\n/).filter((l) => l.trim().length > 0);
  const last = lines[lines.length - 1] ?? "";
  const t = last.trim();
  return t.length <= maxLen ? t : `${t.slice(0, maxLen)}…`;
}
