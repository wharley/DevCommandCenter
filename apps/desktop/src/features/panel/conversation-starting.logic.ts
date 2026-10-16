import type { McpRuntimeState, McpRuntimeStatus } from "@dcc/contracts";
import type { WorkspaceMessage } from "./thread-projection";

/** What the startup row says between send and the agent's first activity. */
export type ConversationStartupStage =
	| "routing"
	| "creating"
	| "sending"
	| "connecting"
	| "waiting";

export type McpStartupProgress = { ready: number; total: number };

/** Each stage is read from real state, never advanced on a timer. */
export function conversationStartupStage(input: {
	isModelRouting: boolean;
	sessionId: string | null;
	lastTurnState: string | null;
	mcp: McpStartupProgress | null;
}): ConversationStartupStage {
	if (input.isModelRouting) return "routing";
	if (!input.sessionId) return "creating";
	// Codex holds the prompt and Claude holds the first message until the
	// projected MCP servers attach; that wait is the one worth naming.
	if (input.mcp) return "connecting";
	return input.lastTurnState === "running" ? "waiting" : "sending";
}

const MCP_STARTING_STATES = new Set<McpRuntimeState>([
	"probingServer",
	"serverReachable",
	"attachingProvider",
]);
const MCP_SETTLED_STATES = new Set<McpRuntimeState>([
	"connected",
	"failed",
	"unsupported",
]);

/**
 * Servers the provider is still bringing up, out of those it will use.
 * Null when nothing is pending, so the row only mentions tools while they
 * are what the agent is waiting for.
 */
export function mcpStartupProgress(
	statuses: readonly McpRuntimeStatus[],
	providerId: string | null | undefined,
): McpStartupProgress | null {
	const relevant = statuses.filter(
		(status) =>
			(!providerId || status.providerId === providerId) &&
			(MCP_STARTING_STATES.has(status.state) || MCP_SETTLED_STATES.has(status.state)),
	);
	const starting = relevant.filter((status) => MCP_STARTING_STATES.has(status.state)).length;
	if (starting === 0) return null;
	return { ready: relevant.length - starting, total: relevant.length };
}

/**
 * The prompt the person just sent, shown in its final place from the first
 * frame. Projection adds it once a session exists; before that (routing,
 * session creation) it is added here.
 */
export function appendStartupPrompt(
	messages: WorkspaceMessage[],
	prompt: string | null,
): WorkspaceMessage[] {
	const trimmed = prompt?.trim();
	if (!trimmed) return messages;
	const latestUserIndex = messages.findLastIndex((message) => message.role === "user");
	const latestUser = messages[latestUserIndex];
	const answered = messages
		.slice(latestUserIndex + 1)
		.some((message) => message.role === "assistant" || Boolean(message.delegation));
	if (latestUser && !answered && latestUser.content.trim() === trimmed) return messages;
	return [
		...messages,
		{ id: "pending-user", role: "user", label: "User", content: trimmed },
	];
}

/**
 * A persisted conversation already renders an optimistic user message while a
 * turn is being accepted. Keep visible feedback below it until the runtime
 * emits the first real assistant activity.
 */
export function shouldShowConversationStarting(
	messages: WorkspaceMessage[],
	pendingPrompt: string | null,
	lastTurnState: string | null,
): boolean {
	const latestUserIndex = messages.findLastIndex(
		(message) => message.role === "user",
	);
	if (latestUserIndex < 0) return false;

	const latestUserMessage = messages[latestUserIndex];
	const isAwaitingActivity =
		Boolean(pendingPrompt?.trim()) ||
		(lastTurnState === "running" && Boolean(latestUserMessage?.turnId));
	if (!isAwaitingActivity) return false;

	return !messages.slice(latestUserIndex + 1).some(
		(message) => message.role === "assistant" || Boolean(message.delegation),
	);
}
