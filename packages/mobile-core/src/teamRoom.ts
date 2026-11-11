import { parseAgentQuestions, type AgentQuestion } from "./questions.ts";
import {
	parseRoomHandoff,
	parseRoomLive,
	parseRoomQueued,
	parseRoomStopped,
	type RoomHandoff,
	type RoomLive,
	type RoomQueued,
	type RoomStopped,
} from "./roomLive.ts";

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

/** What an answer's turn did, counted by kind (the desktop's folded "Trabalhou 4 min · rodou 1 comando"). */
export type RoomSteps = {
	seconds: number | null;
	tools: number;
	failures: number;
	counts: Record<string, number>;
};

/** A task the answer reports: opened by the agent, with what it changed. */
export type RoomTask = {
	workspaceId: string;
	sessionId: string | null;
	title: string;
	state: "working" | "turnEnded" | "closed" | "failed";
	files: number;
	additions: number;
	deletions: number;
};

export type RoomLine = {
	key: string;
	agentId: string;
	sessionId: string;
	speaker: RoomSpeaker;
	text: string;
	at: string;
	receipt: RoomReceipt | null;
	/** Null when the turn used no tool, or on a desktop that does not say. */
	steps: RoomSteps | null;
	/** The tasks the answer reports, the first one first. */
	tasks: RoomTask[];
	/** DCC woke the agent with this line (a routine, a source's news): not the person's words. */
	wake: { kind: "routine" | "source"; about: string } | null;
	/** The turn an answer ends, to list its steps; null on a desktop that does not say. */
	turnId: string | null;
	/** The teammate an answer suggests passing to, and what became of it. */
	handoff: RoomHandoff | null;
};

/** An agent's permission request waiting in the room; answered with the session's `respond-permission`. */
export type RoomApproval = {
	sessionId: string;
	requestId: string;
	agentId: string;
	agentName: string;
	toolName: string;
	/** What it wants to do, in one line (a command, a file, a connector's tool). */
	summary: string;
	/** "Sempre permitir" fits it (a connector's tool). */
	mayAlwaysAllow: boolean;
};

/** An agent's question waiting in the room (its ask-user tool); answered with the session's `respond-user-input`. */
export type RoomQuestion = {
	sessionId: string;
	requestId: string;
	agentId: string;
	agentName: string;
	/** The same shape the thread's question card reads. */
	questions: AgentQuestion[];
};

export type RoomSnapshot =
	| { unchanged: true; snapshotId: string }
	| {
			unchanged: false;
			snapshotId: string | null;
			lines: RoomLine[];
			/** Empty on a desktop that does not serve them yet. */
			approvals: RoomApproval[];
			/** Empty on a desktop that does not serve them yet. */
			questions: RoomQuestion[];
			/** The agents mid-turn in the room; null when the desktop does not say. */
			working: string[] | null;
			/** The turns at work, the stops to try again and the messages queued; empty on an older desktop. */
			live: RoomLive[];
			stopped: RoomStopped[];
			queued: RoomQueued[];
			/** Names the settled part (the lines), sent back as `lines`. */
			linesId: string | null;
			/** The lines are those of `linesId` sent back: `lines` is empty, keep the ones held. */
			linesUnchanged: boolean;
	  };

/** Why the desktop picked who answers (`RouteReason`). */
export type RoomReason = "mention" | "role" | "continues" | "default";

export type RoomSent = {
	agentId: string;
	agentName: string;
	reason: RoomReason;
	sessionId: string;
	/** The conversation was at work: the message waits in its queue. */
	queued: boolean;
};

function parseApproval(raw: unknown): RoomApproval | null {
	if (!raw || typeof raw !== "object") return null;
	const value = raw as Record<string, unknown>;
	const sessionId = text(value.sessionId);
	const requestId = text(value.requestId);
	const agentId = text(value.agentId);
	if (!sessionId || !requestId || !agentId) return null;
	return {
		sessionId,
		requestId,
		agentId,
		agentName: text(value.agentName) ?? "",
		toolName: text(value.toolName) ?? "",
		summary: text(value.summary) ?? "",
		mayAlwaysAllow: value.mayAlwaysAllow === true,
	};
}

function parseQuestion(raw: unknown): RoomQuestion | null {
	if (!raw || typeof raw !== "object") return null;
	const value = raw as Record<string, unknown>;
	const sessionId = text(value.sessionId);
	const requestId = text(value.requestId);
	const agentId = text(value.agentId);
	const questions = parseAgentQuestions(value.questions);
	if (!sessionId || !requestId || !agentId || questions.length === 0) return null;
	return { sessionId, requestId, agentId, agentName: text(value.agentName) ?? "", questions };
}

function text(value: unknown): string | null {
	return typeof value === "string" && value.length > 0 ? value : null;
}

function count(value: unknown): number | null {
	return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function parseSteps(raw: unknown): RoomSteps | null {
	if (!raw || typeof raw !== "object") return null;
	const value = raw as Record<string, unknown>;
	const tools = count(value.tools) ?? 0;
	if (tools <= 0 || !value.counts || typeof value.counts !== "object") return null;
	const counts: Record<string, number> = {};
	for (const [kind, amount] of Object.entries(value.counts as Record<string, unknown>)) {
		const n = count(amount);
		if (n !== null && n > 0) counts[kind] = n;
	}
	return { seconds: count(value.seconds), tools, failures: count(value.failures) ?? 0, counts };
}

const TASK_STATES: RoomTask["state"][] = ["working", "turnEnded", "closed", "failed"];

function parseTask(raw: unknown): RoomTask | null {
	if (!raw || typeof raw !== "object") return null;
	const value = raw as Record<string, unknown>;
	const workspaceId = text(value.workspaceId);
	if (!workspaceId) return null;
	return {
		workspaceId,
		sessionId: text(value.sessionId),
		title: text(value.title) ?? "Tarefa",
		state: TASK_STATES.includes(value.state as RoomTask["state"]) ? (value.state as RoomTask["state"]) : "turnEnded",
		files: count(value.files) ?? 0,
		additions: count(value.additions) ?? 0,
		deletions: count(value.deletions) ?? 0,
	};
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
		steps: parseSteps(value.steps),
		wake:
			value.kind === "routine" || value.kind === "source"
				? { kind: value.kind, about: text(value.about) ?? "" }
				: null,
		tasks: (Array.isArray(value.reports) ? value.reports : value.task ? [value.task] : []).flatMap((item) => {
			const task = parseTask(item);
			return task ? [task] : [];
		}),
		turnId: text(value.turnId),
		handoff: value.speaker === "agent" ? parseRoomHandoff(value.handoff) : null,
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
	const linesUnchanged = value.linesUnchanged === true;
	if (!Array.isArray(value.lines) && !linesUnchanged) return null;
	return {
		unchanged: false,
		snapshotId,
		lines: (Array.isArray(value.lines) ? value.lines : []).flatMap((raw) => {
			const line = parseLine(raw);
			return line ? [line] : [];
		}),
		live: parseRoomLive(value.live),
		stopped: parseRoomStopped(value.stopped),
		queued: parseRoomQueued(value.queued),
		linesId: text(value.linesId),
		linesUnchanged,
		approvals: Array.isArray(value.approvals)
			? value.approvals.flatMap((raw) => {
					const approval = parseApproval(raw);
					return approval ? [approval] : [];
				})
			: [],
		questions: Array.isArray(value.questions)
			? value.questions.flatMap((raw) => {
					const question = parseQuestion(raw);
					return question ? [question] : [];
				})
			: [],
		working: Array.isArray(value.working) ? value.working.filter((id): id is string => typeof id === "string") : null,
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
	return { agentId, agentName, reason, sessionId, queued: value.queued === true };
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

// ---------- how the room reads on the phone ----------

/** Answers by the same agent this close together share one face (the desktop's `room_flow`). */
const GROUP_MINUTES = 5;

export type RoomItem =
	| { type: "day"; key: string; label: string }
	/** `first`: the line opens a group, so it carries the agent's face, name and time. */
	| { type: "line"; key: string; line: RoomLine; first: boolean };

function dayOf(at: string): string {
	const when = new Date(at);
	return Number.isNaN(when.getTime()) ? "" : `${when.getFullYear()}-${when.getMonth()}-${when.getDate()}`;
}

/** "Hoje", "Ontem" or "12 de nov." */
export function roomDayLabel(at: string, now: Date = new Date()): string {
	const when = new Date(at);
	if (Number.isNaN(when.getTime())) return "";
	const day = (date: Date) => new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
	const days = Math.round((day(now) - day(when)) / 86_400_000);
	if (days <= 0) return "Hoje";
	if (days === 1) return "Ontem";
	const months = ["jan.", "fev.", "mar.", "abr.", "mai.", "jun.", "jul.", "ago.", "set.", "out.", "nov.", "dez."];
	return `${when.getDate()} de ${months[when.getMonth()]}`;
}

/** "09:41" beside an agent's name. */
export function roomClock(at: string): string {
	const when = new Date(at);
	if (Number.isNaN(when.getTime())) return "";
	return `${String(when.getHours()).padStart(2, "0")}:${String(when.getMinutes()).padStart(2, "0")}`;
}

/**
 * The lines as the list draws them: a pill at each new day, and an agent's
 * answers within 5 minutes of its last one grouped under one face.
 */
export function roomItems(lines: RoomLine[], now: Date = new Date()): RoomItem[] {
	const items: RoomItem[] = [];
	let day = "";
	let previous: RoomLine | null = null;
	for (const line of lines) {
		const lineDay = dayOf(line.at);
		if (lineDay && lineDay !== day) {
			day = lineDay;
			items.push({ type: "day", key: `day-${lineDay}`, label: roomDayLabel(line.at, now) });
			previous = null;
		}
		const grouped =
			line.speaker === "agent" &&
			previous !== null &&
			previous.speaker === "agent" &&
			previous.agentId === line.agentId &&
			new Date(line.at).getTime() - new Date(previous.at).getTime() <= GROUP_MINUTES * 60_000;
		items.push({ type: "line", key: line.key, line, first: !grouped });
		previous = line;
	}
	return items;
}

function plain(value: string): string {
	return value.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase();
}

export type MentionPart = { text: string; agentId: string | null };

/**
 * A message split at each `@Agent`, matched as the desktop's router does
 * (any case, no accents, the longest name first) and never inside an e-mail
 * or a path: the `@` must open the text or follow a space or a bracket.
 */
export function splitMentions(body: string, agents: ReadonlyArray<{ id: string; name: string }>): MentionPart[] {
	const names = agents
		.filter((agent) => agent.name.trim().length > 0)
		.map((agent) => ({ id: agent.id, plain: plain(agent.name) }))
		.sort((left, right) => right.plain.length - left.plain.length);
	const parts: MentionPart[] = [];
	const flat = plain(body);
	let from = 0;
	let index = 0;
	// Removing accents keeps the length for the usual pt-BR letters; a text
	// where it does not is left whole.
	if (flat.length !== body.length || names.length === 0) return [{ text: body, agentId: null }];
	while (index < body.length) {
		const opens = body[index] === "@" && (index === 0 || /[\s([{>]/.test(body[index - 1] ?? ""));
		const match = opens
			? names.find((name) => {
					const end = index + 1 + name.plain.length;
					return flat.startsWith(name.plain, index + 1) && !/[A-Za-z0-9_\u00C0-\u024F]/.test(body[end] ?? "");
				})
			: undefined;
		if (!match) {
			index += 1;
			continue;
		}
		const end = index + 1 + match.plain.length;
		if (index > from) parts.push({ text: body.slice(from, index), agentId: null });
		parts.push({ text: body.slice(index, end), agentId: match.id });
		from = end;
		index = end;
	}
	if (from < body.length) parts.push({ text: body.slice(from), agentId: null });
	return parts;
}

/** "Dev pergunta" over a question's card. */
export function questionTitle(question: RoomQuestion): string {
	return `${question.agentName || "O agente"} pergunta`;
}

/** "Dev quer rodar um comando" over an approval's summary. */
export function approvalTitle(approval: RoomApproval): string {
	const who = approval.agentName || "O agente";
	const tool = approval.toolName.replace(/^mcp__/, "").replace(/__/g, " · ");
	return tool ? `${who} quer usar ${tool}` : `${who} pede permissão`;
}

function plural(n: number, one: string, many: string): string {
	return `${n} ${n === 1 ? one : many}`;
}

/** "Trabalhou 4 min · rodou 1 comando · leu 2 arquivos · 1 falhou", as the desktop folds a turn. */
export function roomStepsLabel(steps: RoomSteps): string {
	const duration =
		steps.seconds === null ? "" : steps.seconds < 60 ? ` ${steps.seconds} s` : ` ${Math.round(steps.seconds / 60)} min`;
	const parts = [`Trabalhou${duration}`];
	const say = (kind: string, word: (n: number) => string) => {
		const n = steps.counts[kind] ?? 0;
		if (n > 0) parts.push(word(n));
	};
	say("command", (n) => `rodou ${plural(n, "comando", "comandos")}`);
	say("edit", (n) => `editou ${plural(n, "arquivo", "arquivos")}`);
	say("read", (n) => `leu ${plural(n, "arquivo", "arquivos")}`);
	say("search", (n) => plural(n, "busca", "buscas"));
	say("web", (n) => plural(n, "pesquisa", "pesquisas"));
	say("agent", (n) => plural(n, "subagente", "subagentes"));
	say("mcp", (n) => plural(n, "ferramenta", "ferramentas"));
	if (steps.failures > 0) parts.push(steps.failures === 1 ? "1 falhou" : `${steps.failures} falharam`);
	return parts.join(" · ");
}

/** `agents.report.state.*` on the desktop. */
export function roomTaskStateLabel(state: RoomTask["state"]): string {
	switch (state) {
		case "working":
			return "Trabalhando";
		case "turnEnded":
			return "Terminou o turno";
		case "closed":
			return "Fechada";
		case "failed":
			return "Falhou";
	}
}

/** "Fonte · menções no Slack (Slack)" for a line DCC wrote to wake the agent. */
export function roomWakeLabel(wake: NonNullable<RoomLine["wake"]>): string {
	const what = wake.kind === "source" ? "Fonte" : "Rotina";
	return wake.about ? `${what} · ${wake.about}` : what;
}
