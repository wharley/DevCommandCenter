import type { Delegation, WorkspaceSessionSummary } from "@dcc/contracts";

/** Delegations that still have a child working (or about to). */
export function isDelegationActive(delegation: Delegation): boolean {
	return (
		delegation.status === "draft" ||
		delegation.status === "queued" ||
		delegation.status === "running"
	);
}

function newestFirst(left: Delegation, right: Delegation) {
	return right.createdAt.localeCompare(left.createdAt);
}

/** The delegations a session started, newest first. */
export function childDelegationsOf(
	delegations: readonly Delegation[],
	parentSessionId: string | null,
): Delegation[] {
	if (!parentSessionId) return [];
	return delegations
		.filter((delegation) => delegation.parentSessionId === parentSessionId)
		.sort(newestFirst);
}

/** The delegation that created `sessionId`, when it is a delegated child. */
export function parentDelegationOf(
	delegations: readonly Delegation[],
	sessionId: string | null,
): Delegation | null {
	if (!sessionId) return null;
	return (
		delegations
			.filter((delegation) => delegation.childSessionId === sessionId)
			.sort(newestFirst)[0] ?? null
	);
}

export type LineageSessionRow = {
	summary: WorkspaceSessionSummary;
	/** 0 for top-level sessions, 1 for a delegated child under its parent. */
	depth: number;
};

/**
 * Orders sessions so each delegated child sits right under the session that
 * delegated it, keeping the original order otherwise. A child whose parent is
 * not in the list stays top-level.
 */
export function nestSessionsByLineage(
	sessions: readonly WorkspaceSessionSummary[],
	delegations: readonly Delegation[],
): LineageSessionRow[] {
	const ids = new Set(sessions.map((summary) => summary.session.id));
	const parentOf = new Map<string, string>();
	for (const delegation of delegations) {
		const child = delegation.childSessionId;
		if (child && ids.has(child) && ids.has(delegation.parentSessionId) && child !== delegation.parentSessionId) {
			parentOf.set(child, delegation.parentSessionId);
		}
	}
	const childrenOf = new Map<string, WorkspaceSessionSummary[]>();
	for (const summary of sessions) {
		const parent = parentOf.get(summary.session.id);
		if (parent) {
			childrenOf.set(parent, [...(childrenOf.get(parent) ?? []), summary]);
		}
	}
	const rows: LineageSessionRow[] = [];
	for (const summary of sessions) {
		if (parentOf.has(summary.session.id)) continue;
		rows.push({ summary, depth: 0 });
		for (const child of childrenOf.get(summary.session.id) ?? []) {
			rows.push({ summary: child, depth: 1 });
		}
	}
	return rows;
}

/** The task as the requester wrote it (stored, or read back from the prompt). */
export function delegationInstruction(delegation: Delegation): string {
	const stored = delegation.instruction?.trim();
	if (stored) return stored;
	const marker = "\nInstruction:\n";
	const start = delegation.prompt.indexOf(marker);
	if (start < 0) return delegation.prompt.trim();
	const rest = delegation.prompt.slice(start + marker.length);
	const end = rest.search(/\n\n(?:Git context|Mission spec|Recent parent session context):\n/);
	return (end < 0 ? rest : rest.slice(0, end)).trim();
}

/** "há 5 min" / "5 min ago" for a past ISO timestamp. */
export function relativeTimeLabel(iso: string | null | undefined, locale: string, now = Date.now()): string | null {
	const at = iso ? Date.parse(iso) : Number.NaN;
	if (Number.isNaN(at)) return null;
	const seconds = Math.round((at - now) / 1000);
	const format = new Intl.RelativeTimeFormat(locale, { numeric: "auto", style: "short" });
	const abs = Math.abs(seconds);
	if (abs < 60) return format.format(seconds, "second");
	if (abs < 3600) return format.format(Math.round(seconds / 60), "minute");
	if (abs < 86_400) return format.format(Math.round(seconds / 3600), "hour");
	return format.format(Math.round(seconds / 86_400), "day");
}
