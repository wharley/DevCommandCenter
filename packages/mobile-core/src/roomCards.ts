import { t } from "./labels.ts";
import type { ResidentAgentAvatar } from "./residentAgents.ts";

/**
 * A room's cards as the desktop serves them to the phone (`cards` on
 * `GET /api/v1/mobile/room`), the same ones the desktop's room shows there:
 * an agent's conversation in a task that finished or waits on the person,
 * a pull request review with its verdict, a check-in; and DCC's own — an
 * agent past its monthly limit, an agent, routine or source an agent
 * proposed — which the room shows after everything, as the desktop does —
 * and a reply an agent proposed, at its time, which waits on the person to
 * send it ("Enviar no Slack") or discard it.
 */

export type RoomCardKind =
	| "taskResult"
	| "taskWaiting"
	| "prReview"
	| "checkIn"
	| "limit"
	| "agentProposal"
	| "routineProposal"
	| "sourceProposal"
	| "replyProposal";

export type RoomVerdict = "approve" | "requestChanges" | "undecided";

export type RoomCardAction = "open" | "delivery" | "create" | "discard" | "continue" | "send";

export type RoomCard = {
	key: string;
	kind: RoomCardKind;
	/** None for a check-in. */
	agentId: string | null;
	/** The task's name, the pull request, a check-in's title, a proposal's name. */
	title: string;
	/** When it happened; null for one that is about now (a limit). */
	at: string | null;
	unread: boolean;
	/** `agents.room.card.trigger.<id>`: what woke the agent. */
	trigger: string | null;
	verdict: RoomVerdict | null;
	workspaceId: string | null;
	sessionId: string | null;
	prId: string | null;
	actions: RoomCardAction[];
	/** A pull request review still at work, or one that failed. */
	running: boolean;
	failed: boolean;
	checkIn: { kind: "reviewRequested" | "issueAssigned" | "taskStalled"; place: string; url: string | null; minutes: number } | null;
	/** The agent's monthly limit, in tokens. */
	limit: number | null;
	proposal: {
		id: string;
		name: string;
		role: string;
		avatar: ResidentAgentAvatar;
		projects: string[];
		firstRequest: string;
	} | null;
	routine: { id: string; when: string } | null;
	source: { id: string; connector: string; watch: string; every: number } | null;
	/** The reply's text as the agent wrote it, where it goes, and why the last try failed. */
	reply: { id: string; connector: string; destination: string; text: string; error: string | null } | null;
};

const KINDS: RoomCardKind[] = [
	"taskResult",
	"taskWaiting",
	"prReview",
	"checkIn",
	"limit",
	"agentProposal",
	"routineProposal",
	"sourceProposal",
	"replyProposal",
];
const VERDICTS: RoomVerdict[] = ["approve", "requestChanges", "undecided"];
const ACTIONS: RoomCardAction[] = ["open", "delivery", "create", "discard", "continue", "send"];
const CHECK_INS = ["reviewRequested", "issueAssigned", "taskStalled"] as const;

/** DCC's own cards: they sit after the lines, not at their time. */
export const ROOM_OWN_CARDS: ReadonlySet<RoomCardKind> = new Set(["limit", "agentProposal", "routineProposal", "sourceProposal"]);

function text(value: unknown): string | null {
	return typeof value === "string" && value.length > 0 ? value : null;
}

function num(value: unknown): number | null {
	return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function object(value: unknown): Record<string, unknown> | null {
	return value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function parseCard(raw: unknown): RoomCard | null {
	const value = object(raw);
	if (!value) return null;
	const key = text(value.key);
	const kind = KINDS.includes(value.kind as RoomCardKind) ? (value.kind as RoomCardKind) : null;
	if (!key || !kind) return null;
	const agentId = text(value.agentId);
	// Every card but a check-in speaks for an agent.
	if (!agentId && kind !== "checkIn") return null;
	const check = object(value.checkIn);
	const checkKind = check && CHECK_INS.includes(check.kind as (typeof CHECK_INS)[number]) ? (check.kind as (typeof CHECK_INS)[number]) : null;
	if (kind === "checkIn" && !checkKind) return null;
	const proposal = object(value.proposal);
	const avatar = object(proposal?.avatar) ?? {};
	const routine = object(value.routine);
	const source = object(value.source);
	const proposalId = proposal ? text(proposal.id) : null;
	const routineId = routine ? text(routine.id) : null;
	const sourceId = source ? text(source.id) : null;
	const reply = object(value.reply);
	const replyId = reply ? text(reply.id) : null;
	const replyText = reply ? text(reply.text) : null;
	if (kind === "agentProposal" && !proposalId) return null;
	if (kind === "routineProposal" && !routineId) return null;
	if (kind === "sourceProposal" && !sourceId) return null;
	if (kind === "replyProposal" && (!replyId || !replyText)) return null;
	return {
		key,
		kind,
		agentId,
		title: text(value.title) ?? "",
		at: text(value.at),
		unread: value.unread === true,
		trigger: text(value.trigger),
		verdict: VERDICTS.includes(value.verdict as RoomVerdict) ? (value.verdict as RoomVerdict) : null,
		workspaceId: text(value.workspaceId),
		sessionId: text(value.sessionId),
		prId: text(value.prId),
		actions: Array.isArray(value.actions) ? value.actions.filter((action): action is RoomCardAction => ACTIONS.includes(action as RoomCardAction)) : [],
		running: value.running === true,
		failed: value.failed === true,
		checkIn:
			check && checkKind
				? { kind: checkKind, place: text(check.place) ?? "", url: text(check.url), minutes: num(check.minutes) ?? 20 }
				: null,
		limit: num(value.limit),
		proposal:
			proposal && proposalId
				? {
						id: proposalId,
						name: text(proposal.name) ?? "",
						role: text(proposal.role) ?? "",
						avatar: {
							color: text(avatar.color) ?? "blue",
							arms: num(avatar.arms) ?? 3,
							eyes: text(avatar.eyes) ?? "round",
							shape: text(avatar.shape) ?? "octopus",
							accessory: text(avatar.accessory) ?? "none",
						},
						projects: Array.isArray(proposal.projects) ? proposal.projects.filter((name): name is string => typeof name === "string") : [],
						firstRequest: text(proposal.firstRequest) ?? "",
					}
				: null,
		routine: routine && routineId ? { id: routineId, when: text(routine.when) ?? "" } : null,
		source:
			source && sourceId
				? { id: sourceId, connector: text(source.connector) ?? "", watch: text(source.watch) ?? "", every: num(source.every) ?? 0 }
				: null,
		reply:
			reply && replyId && replyText
				? {
						id: replyId,
						connector: text(reply.connector) ?? "",
						destination: text(reply.destination) ?? "",
						text: replyText,
						error: text(reply.error),
					}
				: null,
	};
}

/** The route's `cards`; empty on a desktop that does not serve them yet. */
export function parseRoomCards(raw: unknown): RoomCard[] {
	if (!Array.isArray(raw)) return [];
	return raw.flatMap((item) => {
		const card = parseCard(item);
		return card ? [card] : [];
	});
}

/** "1M", "2.5M", "500k", as the desktop's limit. */
export function tokensShort(tokens: number): string {
	if (tokens >= 1_000_000) {
		const millions = tokens / 1_000_000;
		return Math.abs(millions - Math.round(millions)) < 0.05 ? `${Math.round(millions)}M` : `${millions.toFixed(1)}M`;
	}
	if (tokens >= 1_000) return `${Math.floor(tokens / 1_000)}k`;
	return String(tokens);
}

/** The card's one line, in the desktop's words: "Dev terminou em Checkout". */
export function roomCardText(card: RoomCard, agentName: string): string {
	const vars = { agent: agentName, task: card.title };
	switch (card.kind) {
		case "taskResult":
			return t("agents.room.card.done", vars);
		case "taskWaiting":
			return t("agents.room.card.waiting", vars);
		case "prReview":
			return t(card.running ? "agents.room.card.reviewing" : card.failed ? "agents.room.card.reviewFailed" : "agents.room.card.reviewed", vars);
		case "checkIn":
			return t(`agents.room.checkIn.${card.checkIn?.kind ?? "taskStalled"}`, {
				title: card.title,
				minutes: card.checkIn?.minutes ?? 20,
			});
		case "limit":
			return t("agents.room.limit.text", { agent: agentName, limit: tokensShort(card.limit ?? 0) });
		case "agentProposal":
			return t("agents.proposal.by", { agent: agentName });
		case "routineProposal":
			return t("agents.routines.proposal", { agent: agentName, when: card.routine?.when ?? "" });
		case "sourceProposal":
			return t("agents.sources.proposal", { agent: agentName });
		case "replyProposal":
			return t("agents.replies.title", { agent: agentName });
	}
}

/** The line under a proposal: what it does, where and its first request; a routine's request; a source's watch. */
export function roomCardDetail(card: RoomCard): string[] {
	if (card.proposal) {
		const where = card.proposal.projects.length
			? t("agents.proposal.projects", { projects: card.proposal.projects.join(", ") })
			: t("agents.proposal.everyProject");
		return [
			card.proposal.role,
			where,
			card.proposal.firstRequest ? t("agents.proposal.first", { request: card.proposal.firstRequest }) : "",
		].filter((part) => part.length > 0);
	}
	if (card.routine) return card.title ? [card.title] : [];
	if (card.source) {
		return [t("agents.sources.what", { connector: card.source.connector, watch: card.source.watch, every: card.source.every })];
	}
	if (card.reply) return [[card.reply.connector, card.reply.destination].filter(Boolean).join(" · ")];
	return [];
}

/** "Gatilho · CI falhou"; null when the desktop has no word for it. */
export function roomTriggerLabel(trigger: string | null): string | null {
	if (!trigger) return null;
	const label = t(`agents.room.card.trigger.${trigger}` as Parameters<typeof t>[0]);
	return label.startsWith("agents.") ? null : label;
}

/** "Pode aprovar", "Pedir mudanças", "Falta decidir". */
export function roomVerdictLabel(verdict: RoomVerdict): string {
	return t(`pullRequests.verdict.${verdict}`);
}
