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
