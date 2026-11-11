import { t, type LabelKey, type ToolKind } from "./labels.ts";

/**
 * A room's turns at work as the desktop serves them to the phone
 * (`GET /api/v1/mobile/room`'s `live`, `stopped` and `queued`, read from
 * `dcc_core::domain::team_room_live`): an agent's open turn — what it does
 * now, its last steps and the answer streamed so far — or a running task an
 * agent opened from the room; a turn that stopped, with the prompt to try
 * again; and the messages queued while a turn runs. Worded here as the
 * desktop's live rows word them ("Lendo src/cache.rs", "Pensando…").
 */

export type RoomStepState = "running" | "done" | "failed";

export type RoomLiveStep = { kind: ToolKind; target: string | null; state: RoomStepState };

export type RoomLive = {
	/** The conversation followed: one row per conversation. */
	key: string;
	/** An agent's own conversation in the room, or a task it opened from there. */
	kind: "conversation" | "task";
	agentId: string;
	sessionId: string;
	turnId: string;
	startedAt: string | null;
	doing: "reading" | "thinking" | "tool";
	/** The last steps, oldest first. */
	steps: RoomLiveStep[];
	/** The steps before those, counted. */
	earlier: number;
	/** The answer so far; empty for a task. */
	text: string;
	task: { title: string; workspaceId: string | null } | null;
};

/** "Dev parou antes de terminar": a stop of the last day. */
export type RoomStopped = {
	agentId: string;
	sessionId: string;
	turnId: string;
	reason: string | null;
	/** The person's prompt to send again; none for a turn DCC wrote. */
	retry: { sessionId: string; prompt: string; turnId: string } | null;
};

/** A message queued in a conversation at work, out once its turn ends. */
export type RoomQueued = { id: string; agentId: string; sessionId: string; text: string; at: string };

/** Under an answer that ends with "@Teammate …": pass it, or what was done. */
export type RoomHandoff = { to: string; toName: string; request: string; state: "pending" | "passed" | "auto" };

/** One step of an answer, listed when its folded line is opened. */
export type RoomStep = { kind: ToolKind; target: string | null; failed: boolean };

const KINDS: ToolKind[] = ["command", "edit", "read", "search", "web", "plan", "agent", "mcp", "other"];

function record(raw: unknown): Record<string, unknown> | null {
	return raw && typeof raw === "object" ? (raw as Record<string, unknown>) : null;
}

function text(value: unknown): string | null {
	return typeof value === "string" && value.length > 0 ? value : null;
}

function kindOf(value: unknown): ToolKind {
	return KINDS.includes(value as ToolKind) ? (value as ToolKind) : "other";
}

function list<T>(raw: unknown, parse: (item: unknown) => T | null): T[] {
	return Array.isArray(raw)
		? raw.flatMap((item) => {
				const parsed = parse(item);
				return parsed ? [parsed] : [];
			})
		: [];
}

function parseLiveOne(raw: unknown): RoomLive | null {
	const value = record(raw);
	const agentId = text(value?.agentId);
	const sessionId = text(value?.sessionId);
	if (!value || !agentId || !sessionId) return null;
	const task = record(value.task);
	return {
		key: sessionId,
		kind: value.kind === "task" ? "task" : "conversation",
		agentId,
		sessionId,
		turnId: text(value.turnId) ?? "",
		startedAt: text(value.startedAt),
		doing: value.doing === "tool" || value.doing === "thinking" ? value.doing : "reading",
		steps: list(value.steps, (item) => {
			const step = record(item);
			if (!step) return null;
			const state: RoomStepState = step.state === "running" || step.state === "failed" ? step.state : "done";
			return { kind: kindOf(step.kind), target: text(step.target), state };
		}),
		earlier: typeof value.earlier === "number" && value.earlier > 0 ? value.earlier : 0,
		text: typeof value.text === "string" ? value.text : "",
		task: task ? { title: text(task.title) ?? "Tarefa", workspaceId: text(task.workspaceId) } : null,
	};
}

export function parseRoomLive(raw: unknown): RoomLive[] {
	return list(raw, parseLiveOne);
}

export function parseRoomStopped(raw: unknown): RoomStopped[] {
	return list(raw, (item) => {
		const value = record(item);
		const agentId = text(value?.agentId);
		const sessionId = text(value?.sessionId);
		if (!value || !agentId || !sessionId) return null;
		const retry = record(value.retry);
		const prompt = text(retry?.prompt);
		return {
			agentId,
			sessionId,
			turnId: text(value.turnId) ?? "",
			reason: text(value.reason),
			retry: retry && prompt ? { sessionId: text(retry.sessionId) ?? sessionId, prompt, turnId: text(retry.turnId) ?? "" } : null,
		};
	});
}

export function parseRoomQueued(raw: unknown): RoomQueued[] {
	return list(raw, (item) => {
		const value = record(item);
		const id = text(value?.id);
		const agentId = text(value?.agentId);
		const sessionId = text(value?.sessionId);
		const body = text(value?.text);
		if (!value || !id || !agentId || !sessionId || !body) return null;
		return { id, agentId, sessionId, text: body, at: text(value.at) ?? "" };
	});
}

export function parseRoomHandoff(raw: unknown): RoomHandoff | null {
	const value = record(raw);
	const to = text(value?.to);
	if (!value || !to) return null;
	const state = value.state === "passed" || value.state === "auto" ? value.state : "pending";
	return { to, toName: text(value.toName) ?? "", request: text(value.request) ?? "", state };
}

/** `GET /api/v1/mobile/room/steps`: the steps, and how many were left out. */
export function parseRoomSteps(data: unknown): { steps: RoomStep[]; more: number } | null {
	const value = record(data);
	if (!value || !Array.isArray(value.steps)) return null;
	return {
		steps: list(value.steps, (item) => {
			const step = record(item);
			return step ? { kind: kindOf(step.kind), target: text(step.target), failed: step.failed === true } : null;
		}),
		more: typeof value.more === "number" && value.more > 0 ? value.more : 0,
	};
}

/** With no answer yet after this long, the label says it takes a while. */
const LONG_SECONDS = 60;

/** "1:24", "1:02:05": how long a turn has run. */
export function liveClock(seconds: number): string {
	const total = Math.max(0, Math.floor(seconds));
	const hours = Math.floor(total / 3600);
	const minutes = Math.floor((total % 3600) / 60);
	const rest = String(total % 60).padStart(2, "0");
	return hours > 0 ? `${hours}:${String(minutes).padStart(2, "0")}:${rest}` : `${minutes}:${rest}`;
}

/** Seconds since the turn started, at `nowMs`. */
export function liveSeconds(live: Pick<RoomLive, "startedAt">, nowMs: number): number {
	const started = live.startedAt ? Date.parse(live.startedAt) : Number.NaN;
	return Number.isNaN(started) ? 0 : Math.max(0, Math.floor((nowMs - started) / 1000));
}

/** "Lendo src/cache.rs", "Rodou yarn test": the verb, live or done, and its target. */
export function stepText(step: { kind: ToolKind; target: string | null }, live: boolean): string {
	const verb = t(`conversation.toolCall.verbs.${step.kind}.${live ? "live" : "done"}` as LabelKey);
	const target = step.target?.trim();
	return target ? `${verb} ${target}` : verb;
}

/** What the turn does now, on one line, as the desktop's live row says it. */
export function liveLabel(live: RoomLive, seconds: number): string {
	const running = [...live.steps].reverse().find((step) => step.state === "running");
	if (running) return stepText(running, true);
	if (!live.text.trim() && seconds >= LONG_SECONDS) return t("agents.room.live.long");
	// The answer is coming in: it writes, it no longer thinks.
	if (live.text.trim()) return t("agents.room.live.writing");
	return t(live.doing === "reading" ? "agents.room.live.reading" : "agents.room.live.thinking");
}

/** "Dev · “Corrigir domínio”" over a task at work, else the agent's name. */
export function liveTitle(live: RoomLive, agentName: string): string {
	return live.task ? t("agents.room.task.title", { agent: agentName, task: live.task.title }) : agentName;
}

/** "Dev parou antes de terminar · exit 1". */
export function stoppedLabel(stopped: RoomStopped, agentName: string): string {
	const said = t("agents.room.live.stopped", { agent: agentName });
	return stopped.reason ? `${said} · ${stopped.reason}` : said;
}

/** "+3 passos" over the last steps listed. */
export function moreStepsLabel(count: number): string {
	return t("agents.room.steps.more", { count });
}

/** "Na fila · sai quando o Dev terminar". */
export function queuedLabel(agentName: string): string {
	return t("agents.room.queued", { agent: agentName });
}

/** "1 nova", "3 novas" on the button that takes the room back to its end. */
export function newLinesLabel(count: number): string {
	return t("agents.room.newLines", { count });
}

/** Under an answer that suggests a teammate: the question, or what was done. */
export function handoffLabel(handoff: RoomHandoff, agentName: string): string {
	const vars = { agent: agentName, teammate: handoff.toName };
	switch (handoff.state) {
		case "pending":
			return t("agents.room.handoff.suggests", vars);
		case "passed":
			return t("agents.room.handoff.passed", vars);
		case "auto":
			return t("agents.room.handoff.autoPassed", vars);
	}
}

/**
 * `next` keeping each item of `previous` that did not change (same key, same
 * content), and `previous` itself when nothing did: a read that brings the
 * same room redraws nothing, and one new line redraws only that line.
 */
export function keepUnchanged<T extends object>(previous: readonly T[], next: T[], keyOf: (item: T) => string): T[] {
	const before = new Map(previous.map((item) => [keyOf(item), item]));
	let same = previous.length === next.length;
	const merged = next.map((item, index) => {
		const old = before.get(keyOf(item));
		if (old && JSON.stringify(old) === JSON.stringify(item)) {
			if (previous[index] !== old) same = false;
			return old;
		}
		same = false;
		return item;
	});
	return same ? (previous as T[]) : merged;
}

/** How many lines came after `seenKey` (the newest one the person saw). */
export function linesSince(lines: ReadonlyArray<{ key: string }>, seenKey: string | null): number {
	if (!seenKey) return 0;
	const index = lines.findIndex((line) => line.key === seenKey);
	return index < 0 ? 0 : lines.length - 1 - index;
}
