/**
 * "Precisa de você" as the desktop computes it (`GET /api/v1/mobile/attention`,
 * the sidebar's rule ported to the backend): one reason per task, newest
 * return first. Labels are the desktop's (`sidebar.attention.reason.*`).
 *
 * A finished result (`completed`/`aborted`) only counts until the person
 * opens it — on this phone: the seen store is per device, like the
 * desktop's (`workspace-seen-results.ts`, copied in `isResultUnread`).
 */
import { t } from "./labels.ts";
import type { RawSessionEvent } from "./threadEvents.ts";

export type AttentionReason =
	| "permission"
	| "input"
	| "usageLimit"
	| "conflicts"
	| "prConflicts"
	| "checksFailing"
	| "delegatedReview"
	| "woke"
	| "setup"
	| "aborted"
	| "completed";

const REASONS = new Set<AttentionReason>([
	"permission",
	"input",
	"usageLimit",
	"conflicts",
	"prConflicts",
	"checksFailing",
	"delegatedReview",
	"woke",
	"setup",
	"aborted",
	"completed",
]);

export type AttentionEntry = {
	workspaceId: string;
	reason: AttentionReason;
	/** PR number for PR blockers, count for delegated reviews. */
	count: number | null;
	returnedAt: string | null;
	completedAt: string | null;
	/** The thread to open. */
	sessionId: string | null;
};

/** Workspace id → the result's `completedAt` the person last opened. */
export type SeenResults = Record<string, string>;

export function parseAttention(data: unknown): AttentionEntry[] {
	const items = (data as { items?: unknown } | null)?.items;
	if (!Array.isArray(items)) return [];
	return items.flatMap((raw): AttentionEntry[] => {
		const item = (raw ?? {}) as Record<string, unknown>;
		const reason = item.reason as AttentionReason;
		if (typeof item.workspaceId !== "string" || !REASONS.has(reason)) return [];
		const text = (value: unknown) => (typeof value === "string" && value ? value : null);
		return [
			{
				workspaceId: item.workspaceId,
				reason,
				count: typeof item.count === "number" ? item.count : null,
				returnedAt: text(item.returnedAt),
				completedAt: text(item.completedAt),
				sessionId: text(item.sessionId),
			},
		];
	});
}

/** A task whose agent is working, as the desktop's sidebar counts it. */
export type RunningTask = {
	workspaceId: string;
	/** The running session the row opens. */
	sessionId: string;
	sessionIds: string[];
	startedAt: string | null;
};

/**
 * `GET /api/v1/mobile/attention[?after=<snapshotId>]`. Current desktops also
 * send what runs, the open requests and a `snapshotId` naming the content;
 * sent back as `after`, it answers `unchanged` while nothing moved. Older
 * desktops send `items` only: `running` and `requests` are then null and the
 * phone reads statuses from the histories as before.
 */
export type HomeSnapshot =
	| { unchanged: true; snapshotId: string }
	| {
			unchanged: false;
			snapshotId: string | null;
			entries: AttentionEntry[];
			running: RunningTask[] | null;
			requests: RawSessionEvent[] | null;
	  };

export function parseHomeSnapshot(data: unknown, requestedSnapshotId: string | null = null): HomeSnapshot {
	const body = (data ?? {}) as Record<string, unknown>;
	const snapshotId = typeof body.snapshotId === "string" && body.snapshotId ? body.snapshotId : null;
	// Only an answer to the id this phone asked about means "keep what you have".
	if (body.unchanged === true && snapshotId && snapshotId === requestedSnapshotId) {
		return { unchanged: true, snapshotId };
	}
	const running = Array.isArray(body.running)
		? body.running.flatMap((raw): RunningTask[] => {
				const task = (raw ?? {}) as Record<string, unknown>;
				if (typeof task.workspaceId !== "string" || typeof task.sessionId !== "string") return [];
				const ids = Array.isArray(task.sessionIds) ? task.sessionIds.filter((id): id is string => typeof id === "string") : [];
				return [
					{
						workspaceId: task.workspaceId,
						sessionId: task.sessionId,
						sessionIds: ids.length > 0 ? ids : [task.sessionId],
						startedAt: typeof task.startedAt === "string" ? task.startedAt : null,
					},
				];
			})
		: null;
	const requests = Array.isArray(body.requests)
		? body.requests.filter(
				(raw): raw is RawSessionEvent =>
					!!raw &&
					typeof raw === "object" &&
					typeof (raw as RawSessionEvent).sessionId === "string" &&
					typeof (raw as RawSessionEvent).kind?.type === "string",
			)
		: null;
	return { unchanged: false, snapshotId, entries: parseAttention(data), running, requests };
}

export function attentionLabel(entry: Pick<AttentionEntry, "reason" | "count">): string {
	if (entry.reason === "delegatedReview") return t("sidebar.attention.reason.delegatedReview", { count: entry.count ?? 0 });
	return t(`sidebar.attention.reason.${entry.reason}`, { pr: entry.count ?? "" });
}

function timestampMs(value: string | null | undefined): number {
	return value ? Date.parse(value) : Number.NaN;
}

/** The desktop's `isWorkspaceResultUnread`. */
export function isResultUnread(
	completedAt: string | null | undefined,
	seenAt: string | undefined,
	baseline: string | null,
): boolean {
	const completedMs = timestampMs(completedAt);
	const baselineMs = timestampMs(baseline);
	if (Number.isNaN(completedMs) || Number.isNaN(baselineMs) || completedMs <= baselineMs) return false;
	const seenMs = timestampMs(seenAt);
	return Number.isNaN(seenMs) || seenMs < completedMs;
}

/** Drops finished results already opened here (or older than this phone's baseline). */
export function needsYouEntries(entries: AttentionEntry[], seen: SeenResults, baseline: string | null): AttentionEntry[] {
	return entries.filter(
		(entry) =>
			(entry.reason !== "completed" && entry.reason !== "aborted") ||
			isResultUnread(entry.completedAt, seen[entry.workspaceId], baseline),
	);
}

const MAX_SEEN = 200;

/** Opening the thread marks its result seen; keeps the newest entries. */
export function withSeenResult(seen: SeenResults, workspaceId: string, completedAt: string): SeenResults {
	const entries = Object.entries({ ...seen, [workspaceId]: completedAt })
		.sort(([, a], [, b]) => timestampMs(b) - timestampMs(a))
		.slice(0, MAX_SEEN);
	return Object.fromEntries(entries);
}
