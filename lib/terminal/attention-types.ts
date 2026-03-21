/** Main → renderer: pane agent/terminal may need user attention. */
export interface TerminalAttentionPayload {
  paneId: string;
  reason: "keyword" | "idle";
  excerpt?: string;
}
