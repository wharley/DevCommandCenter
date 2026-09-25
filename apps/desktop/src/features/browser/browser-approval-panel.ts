import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type Labels = Record<"title" | "description" | "destination" | "conversation" | "provider" | "reason" | "remaining" | "allow" | "deny", string>;
type Conversation = { workspaceId: string; sessionId: string; title: string };

export async function configureBrowserApprovalPanel(labels: Labels, conversations: Conversation[]) {
	if (isTauri()) await invoke("browser_approval_panel_configure", { labels, conversations });
}

export async function dismissBrowserApprovalPanel(requestId: string) {
	if (isTauri()) await invoke("browser_approval_panel_dismiss", { requestId });
}

export async function onBrowserPanelAllow(handler: (requestId: string) => void) {
	if (!isTauri()) return () => {};
	return listen<string>("browser-approval-panel-allow", (event) => handler(event.payload));
}
