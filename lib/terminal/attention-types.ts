/** Main → renderer: pane agent/terminal may need user attention. */
export interface TerminalAttentionPayload {
  /** Legacy/event payload from Rust side today. */
  ptyId?: string;
  status?: "waiting" | "idle" | string;
  /** Pane-oriented payload shape (future/current web typings). */
  paneId?: string;
  workspaceId?: string;
  reason?: "keyword" | "idle";
  excerpt?: string;
  projectId?: string;
  createdAt?: number;
}
