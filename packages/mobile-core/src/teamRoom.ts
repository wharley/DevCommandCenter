/**
 * The team room as the desktop serves it to the phone
 * (`GET /api/v1/mobile/room`): the latest lines of every agent's own
 * conversation together in time — what the person said and each agent's
 * final answer with its receipt. A message is written once
 * (`POST /api/v1/mobile/room/messages`) and the desktop's rule picks who
 * answers. Like the other snapshots, a phone that sends back the
 * `snapshotId` it holds gets `{ unchanged, snapshotId }` while nothing moved.
 */

export type RoomSpeaker = "person" | "agent";

export type RoomReceipt = { seconds: number | null; tokens: number | null };

export type RoomLine = {
	key: string;
	agentId: string;
	sessionId: string;
	speaker: RoomSpeaker;
	text: string;
	at: string;
	receipt: RoomReceipt | null;
};

export type RoomSnapshot =
	| { unchanged: true; snapshotId: string }
	| { unchanged: false; snapshotId: string | null; lines: RoomLine[] };

/** Why the desktop picked who answers (`RouteReason`). */
export type RoomReason = "mention" | "role" | "continues" | "default";

export type RoomSent = { agentId: string; agentName: string; reason: RoomReason; sessionId: string };

function text(value: unknown): string | null {
	return typeof value === "string" && value.length > 0 ? value : null;
}

function count(value: unknown): number | null {
	return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function parseLine(raw: unknown): RoomLine | null {
	if (!raw || typeof raw !== "object") return null;
	const value = raw as Record<string, unknown>;
	const key = text(value.key);
	const agentId = text(value.agentId);
	const sessionId = text(value.sessionId);
	const body = text(value.text);
	if (!key || !agentId || !sessionId || !body) return null;
	const receipt = value.receipt as Record<string, unknown> | null | undefined;
	return {
		key,
		agentId,
		sessionId,
		speaker: value.speaker === "agent" ? "agent" : "person",
		text: body,
		at: text(value.at) ?? "",
		receipt: receipt && typeof receipt === "object" ? { seconds: count(receipt.seconds), tokens: count(receipt.tokens) } : null,
	};
}

/** The route's answer; `unchanged` only counts when it answers `after`. */
export function parseRoom(data: unknown, after: string | null): RoomSnapshot | null {
	if (!data || typeof data !== "object") return null;
	const value = data as Record<string, unknown>;
	const snapshotId = text(value.snapshotId);
	if (value.unchanged === true) {
		return snapshotId && snapshotId === after ? { unchanged: true, snapshotId } : null;
	}
	if (!Array.isArray(value.lines)) return null;
	return {
		unchanged: false,
		snapshotId,
		lines: value.lines.flatMap((raw) => {
			const line = parseLine(raw);
			return line ? [line] : [];
		}),
	};
}

const REASONS: RoomReason[] = ["mention", "role", "continues", "default"];

export function parseRoomSent(data: unknown): RoomSent | null {
	if (!data || typeof data !== "object") return null;
	const value = data as Record<string, unknown>;
	const agentId = text(value.agentId);
	const agentName = text(value.agentName);
	const sessionId = text(value.sessionId);
	if (!agentId || !agentName || !sessionId) return null;
	const reason = REASONS.includes(value.reason as RoomReason) ? (value.reason as RoomReason) : "default";
	return { agentId, agentName, reason, sessionId };
}

/** `agents.room.reason.*` on the desktop. */
export function roomReasonLabel(reason: RoomReason): string {
	switch (reason) {
		case "mention":
			return "você chamou";
		case "role":
			return "pelo papel";
		case "continues":
			return "segue o assunto";
		case "default":
			return "padrão";
	}
}

/** The first line of a message one agent passed to another
 *  (`↪ Revisor → QA: …`), the rest being what the first one said. */
export function handoffHeading(body: string): string | null {
	return body.startsWith("↪ ") ? (body.split("\n")[0] ?? null) : null;
}

/** "42 s · 1.200 tokens" under an answer. */
export function roomReceiptLabel(receipt: RoomReceipt | null): string {
	if (!receipt) return "";
	const parts: string[] = [];
	if (receipt.seconds !== null) {
		parts.push(receipt.seconds < 60 ? `${receipt.seconds} s` : `${Math.round(receipt.seconds / 60)} min`);
	}
	if (receipt.tokens !== null) parts.push(`${receipt.tokens.toLocaleString("pt-BR")} tokens`);
	return parts.join(" · ");
}

/** The agents whose name starts with `query` (any case), then those that contain it. */
export function matchingAgents<T extends { name: string }>(agents: readonly T[], query: string): T[] {
	const wanted = query.toLowerCase();
	const starts = agents.filter((agent) => agent.name.toLowerCase().startsWith(wanted));
	const contains = agents.filter(
		(agent) => !agent.name.toLowerCase().startsWith(wanted) && agent.name.toLowerCase().includes(wanted),
	);
	return [...starts, ...contains];
}
