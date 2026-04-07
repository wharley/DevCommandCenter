/**
 * Estados de alto nível inspirados em maestro_status (Maestro MCP).
 * O backend pode emitir apenas um subset; o renderer trata legado via `status`.
 */
export type AgentTerminalPhase = "idle" | "working" | "needs_input" | "finished" | "error";

/** Main → renderer: pane agent/terminal may need user attention. */
export interface TerminalAttentionPayload {
  /** Legacy/event payload from Rust side today. */
  ptyId?: string;
  status?: "waiting" | "idle" | string;
  /** Estado explícito (preferir a `status` legado quando presente). */
  phase?: AgentTerminalPhase;
  /** Pane-oriented payload shape (future/current web typings). */
  paneId?: string;
  workspaceId?: string;
  reason?: "keyword" | "idle";
  excerpt?: string;
  projectId?: string;
  createdAt?: number;
}

/** Deriva fase a partir do payload (novo + compatível com `status`). */
export function resolveAttentionPhase(payload: TerminalAttentionPayload): AgentTerminalPhase {
  if (payload.phase) return payload.phase;
  const s = payload.status;
  if (s === "waiting") return "needs_input";
  if (s === "idle" || payload.reason === "idle") return "idle";
  return "needs_input";
}
