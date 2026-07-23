import { useEffect, useState } from "react";
import type { TerminalScopeTarget } from "./terminal-scope";
import {
	getTerminalRuntimeId,
	useProjectTerminals,
} from "./terminal-tabs-store";
import {
	getTerminalSnapshot,
	subscribeTerminalStore,
	type TerminalStatus,
} from "./terminal-store";

const MISSING_WORKTREE_SCOPE = "__missing-worktree-terminal-scope__";
const MISSING_PROJECT_SCOPE = "__missing-project-terminal-scope__";

export function countActiveTerminalStatuses(
	statuses: Array<TerminalStatus | "ready">,
): number {
	return statuses.filter(
		(status) => status === "running" || status === "starting",
	).length;
}

export function useActiveTerminalCount(
	scopes: TerminalScopeTarget[] | undefined,
): number {
	const [terminalStatusVersion, setTerminalStatusVersion] = useState(0);
	const worktreeScope =
		scopes?.find((scope) => scope.kind === "worktree") ?? null;
	const projectScope =
		scopes?.find((scope) => scope.kind === "project") ?? null;
	const worktreeTerminals = useProjectTerminals(
		worktreeScope?.scopeKey ?? MISSING_WORKTREE_SCOPE,
	);
	const projectTerminals = useProjectTerminals(
		projectScope?.scopeKey ?? MISSING_PROJECT_SCOPE,
	);

	useEffect(
		() =>
			subscribeTerminalStore(() =>
				setTerminalStatusVersion((version) => version + 1),
			),
		[],
	);

	const scopedTabs = [
		...(worktreeScope
			? worktreeTerminals.tabs.map((tab) => ({
					scopeKey: worktreeScope.scopeKey,
					terminalId: tab.id,
				}))
			: []),
		...(projectScope
			? projectTerminals.tabs.map((tab) => ({
					scopeKey: projectScope.scopeKey,
					terminalId: tab.id,
				}))
			: []),
	];

	// The version is intentionally read here: status changes keep the tab arrays
	// stable, while the runtime snapshots change independently.
	void terminalStatusVersion;
	return countActiveTerminalStatuses(
		scopedTabs.map(
			(tab) =>
				getTerminalSnapshot(
					getTerminalRuntimeId(tab.scopeKey, tab.terminalId),
				)?.status ?? "ready",
		),
	);
}
