import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { BrowserOpenRequest } from "./browser-api";

type Labels = Record<"title" | "description" | "destination" | "conversation" | "provider" | "reason" | "remaining" | "allow" | "deny", string>;
type Conversation = { workspaceId: string; sessionId: string; title: string };

export async function configureBrowserApprovalPanel(labels: Labels, conversations: Conversation[]) {
	if (isTauri()) await invoke("browser_approval_panel_configure", { labels, conversations });
}

export async function dismissBrowserApprovalPanel(requestId: string) {
	if (isTauri()) await invoke("browser_approval_panel_dismiss", { requestId });
}

/**
 * A Browser open allowed outside the in-app dialog — the native approval
 * panel or a paired phone. The backend already claimed the request (first
 * answer wins); the renderer only opens the Browser and ACKs its lifecycle.
 */
export async function onBrowserOpenApproved(handler: (request: BrowserOpenRequest) => void) {
	if (!isTauri()) return () => {};
	return listen<BrowserOpenRequest>("browser-agent-open-approved", (event) => handler(event.payload));
}
