/**
 * The home screen's triage: which agents need the person, which are running
 * and which are just recent. Transport-agnostic — the native app passes the
 * encrypted relay channel.
 */
import type { AttentionEntry } from "./attention.ts";
import { pendingQuestions } from "./questions.ts";
import { permissionQuestion, type RawSessionEvent } from "./threadEvents.ts";

export type SessionSearchResult = {
	sessionId: string;
	threadTitle: string | null;
	snippet: string | null;
	providerId: string | null;
	model: string | null;
	workspaceName: string | null;
	workspaceBranch: string | null;
	workspaceId: string | null;
	projectId: string | null;
	updatedAt: string;
	archivedAt: string | null;
};

export type Comb = {
	id: string;
	name: string | null;
	branch: string | null;
	projectId: string | null;
	projectName: string | null;
	projectPath?: string | null;
	projectIcon?: string | null;
	projectColor?: string | null;
	worktreePath: string | null;
	status: string | null;
	lastOpenedAt: string | null;
};

export type PendingItem = {
	thread: SessionSearchResult;
	requestId: string;
	question: string;
	/** Empty for an agent question: it is answered inside the thread. */
	choices: Array<{ id: string; label: string }>;
	at: string;
};

/** Per-session outcome derived from its event stream (recent threads only). */
export type LiveStatus = "running" | "completed" | "aborted";

export type Scan = {
	sessions: SessionSearchResult[];
	/** sessionId → derived state from a recent-events sweep. */
	statuses: Map<string, LiveStatus>;
	needsYou: PendingItem[];
};

export type TriageGroups = {
	needsYou: PendingItem[];
	running: SessionSearchResult[];
	recent: SessionSearchResult[];
};

export type JsonFetcher = <T>(path: string) => Promise<T>;

export const RECENT_SCAN_WINDOW_MS = 48 * 3600 * 1000;
export const RECENT_SCAN_LIMIT = 20;

const DEFAULT_CHOICES = [
	{ id: "allow", label: "Permitir" },
	{ id: "deny", label: "Negar" },
];

/**
 * Only surface sessions whose workspace is still live: drop session-level
 * archives, plus anything under a workspace that was archived or deleted.
 * Deleted combs are absent from the list; archived ones carry status.
 * Guard: if the combs fetch came back empty we can't trust the set, so we
 * skip the workspace filter rather than hide everything.
 */
export function liveSessions(
	sessions: SessionSearchResult[],
	combs: Comb[],
): SessionSearchResult[] {
	const activeWorkspaces =
		combs.length > 0
			? new Set(combs.filter((c) => c.status !== "archived").map((c) => c.id))
			: null;
	return sessions.filter((s) => {
		if (s.archivedAt) return false;
		if (activeWorkspaces && s.workspaceId && !activeWorkspaces.has(s.workspaceId))
			return false;
		return true;
	});
}

export function recentForSweep(
	sessions: SessionSearchResult[],
	now = Date.now(),
): SessionSearchResult[] {
	const cutoff = now - RECENT_SCAN_WINDOW_MS;
	return sessions
		.filter((t) => {
			const ts = Date.parse(t.updatedAt);
			return Number.isFinite(ts) && ts > cutoff;
		})
		.slice(0, RECENT_SCAN_LIMIT);
}

/** Status and open requests of one session, from its full history. */
export function classifySession(
	thread: SessionSearchResult,
	events: RawSessionEvent[],
): { status: LiveStatus | null; needsYou: PendingItem[] } {
	const needsYou: PendingItem[] = [];
	for (const request of pendingQuestions(events))
		needsYou.push({
			thread,
			requestId: request.requestId,
			question: request.questions.map((q) => q.question).join(" · "),
			choices: [],
			at: request.at,
		});
	const requested = new Map<string, RawSessionEvent>();
	const resolved = new Set<string>();
	let live = false;
	let lastEnd: "completed" | "aborted" | null = null;
	for (const event of events) {
		const kind = event.kind;
		switch (kind?.type) {
			case "turn_started":
				live = true;
				break;
			case "turn_completed":
			case "session_completed":
				live = false;
				lastEnd = "completed";
				break;
			case "turn_aborted":
			case "session_aborted":
				live = false;
				lastEnd = "aborted";
				break;
			case "turn_permission_requested": {
				const id = typeof kind.requestId === "string" ? kind.requestId : event.eventId;
				requested.set(id, event);
				break;
			}
			case "turn_permission_resolved": {
				const id = typeof kind.requestId === "string" ? kind.requestId : "";
				resolved.add(id);
				break;
			}
		}
	}
	for (const [id, event] of requested) {
		if (resolved.has(id)) continue;
		const kind = event.kind ?? {};
		needsYou.push({
			thread,
			requestId: id,
			question: typeof kind.question === "string" ? kind.question : permissionQuestion(kind),
			choices: Array.isArray(kind.choices)
				? (kind.choices as Array<{ id: string; label: string }>)
				: DEFAULT_CHOICES,
			at: event.occurredAt,
		});
	}
	return { status: live ? "running" : lastEnd, needsYou };
}

/**
 * One sweep that powers all three triage groups. We fetch the session
 * index, then fan out events only for threads touched in the last 48h
 * (capped) — the same budget the old permission counter spent, now reused
 * to also detect which agents are mid-turn.
 */
export async function runScan(
	fetchJson: JsonFetcher,
): Promise<{ scan: Scan; combs: Comb[] }> {
	const [sessRaw, combs] = await Promise.all([
		fetchJson<SessionSearchResult[]>("/api/v1/sessions/search?limit=60"),
		fetchJson<Comb[]>("/api/v1/combs").catch(() => [] as Comb[]),
	]);
	const sessions = liveSessions(sessRaw, combs);
	const sweeps = await Promise.all(
		recentForSweep(sessions).map((thread) =>
			fetchJson<RawSessionEvent[]>(
				`/api/v1/sessions/${encodeURIComponent(thread.sessionId)}/events`,
			)
				.then((events) => ({ thread, events }))
				.catch(() => ({ thread, events: [] as RawSessionEvent[] })),
		),
	);
	const statuses = new Map<string, LiveStatus>();
	const needsYou: PendingItem[] = [];
	for (const { thread, events } of sweeps) {
		const result = classifySession(thread, events);
		if (result.status) statuses.set(thread.sessionId, result.status);
		needsYou.push(...result.needsYou);
	}
	needsYou.sort((a, b) => b.at.localeCompare(a.at));
	return { scan: { sessions, statuses, needsYou }, combs };
}

export function triageGroups(scan: Scan, workspaceFilter: string | null = null): TriageGroups {
	const needsYou = workspaceFilter
		? scan.needsYou.filter((p) => p.thread.workspaceId === workspaceFilter)
		: scan.needsYou;
	const blocked = new Set(needsYou.map((p) => p.thread.sessionId));
	const visible = scan.sessions.filter(
		(s) => !workspaceFilter || s.workspaceId === workspaceFilter,
	);
	const running = visible.filter(
		(s) => scan.statuses.get(s.sessionId) === "running" && !blocked.has(s.sessionId),
	);
	const runningIds = new Set(running.map((s) => s.sessionId));
	const recent = visible
		.filter((s) => !blocked.has(s.sessionId) && !runningIds.has(s.sessionId))
		.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
	return { needsYou, running, recent };
}

export function withoutPending(scan: Scan, sessionId: string, requestId: string): Scan {
	return {
		...scan,
		needsYou: scan.needsYou.filter(
			(p) => !(p.thread.sessionId === sessionId && p.requestId === requestId),
		),
	};
}

export function searchSessions(
	sessions: SessionSearchResult[],
	query: string,
	workspaceFilter: string | null = null,
): SessionSearchResult[] | null {
	const q = query.trim().toLowerCase();
	if (!q) return null;
	return sessions.filter((s) => {
		if (workspaceFilter && s.workspaceId !== workspaceFilter) return false;
		return [s.threadTitle, s.workspaceName, s.workspaceBranch, s.projectId, s.snippet]
			.filter(Boolean)
			.join(" ")
			.toLowerCase()
			.includes(q);
	});
}

/** Active workspaces, matching the query; ones with threads float up, then recency. */
export function sortCombs(
	combs: Comb[],
	sessions: SessionSearchResult[],
	query = "",
): Comb[] {
	const counts = new Map<string, number>();
	for (const s of sessions) {
		if (s.workspaceId) counts.set(s.workspaceId, (counts.get(s.workspaceId) ?? 0) + 1);
	}
	const q = query.trim().toLowerCase();
	const active = combs.filter((c) => c.status !== "archived");
	const base = q
		? active.filter((c) =>
				[c.name, c.projectName, c.projectId, c.branch]
					.filter(Boolean)
					.join(" ")
					.toLowerCase()
					.includes(q),
			)
		: active;
	return [...base].sort((a, b) => {
		const an = counts.get(a.id) ?? 0;
		const bn = counts.get(b.id) ?? 0;
		if (an > 0 !== bn > 0) return bn - an;
		return (b.lastOpenedAt ?? "").localeCompare(a.lastOpenedAt ?? "");
	});
}

/** Each worktree opens its most-recent session. */
export function latestSessionByWorkspace(
	sessions: SessionSearchResult[],
): Map<string, SessionSearchResult> {
	const map = new Map<string, SessionSearchResult>();
	for (const s of sessions) {
		if (!s.workspaceId) continue;
		const cur = map.get(s.workspaceId);
		if (!cur || s.updatedAt.localeCompare(cur.updatedAt) > 0) map.set(s.workspaceId, s);
	}
	return map;
}

/** Group worktrees under their project (desktop's mental model). */
export function combsByProject(combs: Comb[]): Array<[string, Comb[]]> {
	const map = new Map<string, Comb[]>();
	for (const c of combs) {
		const key = c.projectName ?? c.projectId ?? "—";
		const list = map.get(key) ?? [];
		list.push(c);
		map.set(key, list);
	}
	return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]));
}

/** Collapse any provider variant to a clean brand word (claude_interactive →
 *  "claude", openai/gpt → "codex", etc.). */
export function normalizeProvider(id: string): string {
	const v = id.toLowerCase();
	if (v.includes("claude") || v.includes("anthropic")) return "claude";
	if (v.includes("codex") || v.includes("openai") || v.includes("gpt")) return "codex";
	if (v.includes("cursor")) return "cursor";
	if (v.includes("gemini") || v.includes("google")) return "gemini";
	if (v.includes("droid") || v.includes("factory")) return "droid";
	return v.replace(/[_-]/g, " ");
}

export function sessionTitle(s: SessionSearchResult): string {
	return s.threadTitle?.trim() || s.workspaceName || s.projectId || "Sessão sem título";
}

export function workspaceLabel(s: {
	workspaceName: string | null;
	projectId?: string | null;
	workspaceBranch: string | null;
}): string {
	return [s.workspaceName ?? s.projectId, s.workspaceBranch].filter(Boolean).join(" · ");
}

export function formatRelative(iso: string, now = Date.now()): string {
	const date = new Date(iso);
	const time = date.getTime();
	if (!Number.isFinite(time)) return iso;
	const diffSec = (now - time) / 1000;
	if (diffSec < 60) return "agora";
	if (diffSec < 3600) return `${Math.floor(diffSec / 60)} min atrás`;
	if (diffSec < 86400) return `${Math.floor(diffSec / 3600)} h atrás`;
	const days = Math.floor(diffSec / 86400);
	if (days < 30) return `${days} d atrás`;
	return date.toLocaleDateString("pt-BR");
}

/** One row of "Precisa de você": the desktop's reason, plus the inline answer when the scan has it. */
export type NeedsYouRow = {
	entry: AttentionEntry;
	thread: SessionSearchResult | null;
	/** Permission or question found in the thread's history (answerable from the card). */
	pending: PendingItem | null;
};

/**
 * The home's groups with the desktop's attention list: "Precisa de você" in
 * its order, and those tasks' threads left out of "Rodando" and "Recentes".
 * `waitingSessions` are threads with a Browser/computer-use request open.
 */
export function triageWithAttention(
	scan: Scan,
	entries: AttentionEntry[],
	waitingSessions: ReadonlySet<string> = new Set(),
	workspaceFilter: string | null = null,
): { needsYou: NeedsYouRow[]; running: SessionSearchResult[]; recent: SessionSearchResult[] } {
	const visible = scan.sessions.filter((s) => !workspaceFilter || s.workspaceId === workspaceFilter);
	const byId = new Map(scan.sessions.map((s) => [s.sessionId, s]));
	const needsYou = entries
		.filter((entry) => !workspaceFilter || entry.workspaceId === workspaceFilter)
		.map((entry): NeedsYouRow => {
			const pending =
				entry.reason === "permission" || entry.reason === "input"
					? (scan.needsYou.find(
							(p) =>
								p.thread.sessionId === entry.sessionId &&
								(entry.reason === "permission" ? p.choices.length > 0 : p.choices.length === 0),
						) ?? null)
					: null;
			return { entry, thread: entry.sessionId ? (byId.get(entry.sessionId) ?? null) : null, pending };
		});
	const listedWorkspaces = new Set(needsYou.map((row) => row.entry.workspaceId));
	const listedSessions = new Set(needsYou.flatMap((row) => (row.entry.sessionId ? [row.entry.sessionId] : [])));
	const away = (s: SessionSearchResult) =>
		waitingSessions.has(s.sessionId) || listedSessions.has(s.sessionId) || (s.workspaceId !== null && listedWorkspaces.has(s.workspaceId));
	const running = visible.filter((s) => scan.statuses.get(s.sessionId) === "running" && !away(s));
	const runningIds = new Set(running.map((s) => s.sessionId));
	const recent = visible
		.filter((s) => !away(s) && !runningIds.has(s.sessionId))
		.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
	return { needsYou, running, recent };
}
