import type { CommitMode } from "@/features/commit/WorkspaceCommitButton.logic";
export type ExecutionDockRunMode = CommitMode | "commit" | "sync-base";

export type ExecutionDockActionId =
	| "primary"
	| "commit"
	| "commit-and-push"
	| "push"
	| "sync-base"
	| "create-pr"
	| "create-draft-pr"
	| "open-pr"
	| "merge"
	| "review";

export type ExecutionDockAction = {
	id: ExecutionDockActionId;
	mode?: ExecutionDockRunMode;
	kind: "execute" | "review" | "create-request";
	disabled: boolean;
	primary?: boolean;
};

type ExecutionDockActionContext = {
	mode: CommitMode | null;
	loading: boolean;
	multiProject: boolean;
	hasLocalChanges: boolean;
	hasBranchChanges: boolean;
	hasAheadCommits: boolean;
	hasChangeRequest: boolean;
	hasOpenRequest: boolean;
};

/**
 * Maps the existing commit state machine to Dock affordances. This is only a
 * presentation/action map: it does not resolve Git state a second time.
 */
export function resolveExecutionDockActions(
	context: ExecutionDockActionContext,
): ExecutionDockAction[] {
	const { mode, loading, multiProject } = context;
	if (multiProject) {
		return [
			{
				id: "primary",
				kind: "execute",
				disabled: loading,
				primary: true,
			},
		];
	}

	const primary: ExecutionDockAction =
		mode === "create-pr" && !context.hasBranchChanges
			? { id: "primary", mode: "sync-base", kind: "execute", disabled: loading, primary: true }
			: mode === "create-pr"
				? { id: "primary", mode, kind: "create-request", disabled: loading, primary: true }
			: mode === "merge"
				? { id: "primary", mode, kind: "execute", disabled: loading, primary: true }
			: mode === "fix" || mode === "resolve-conflicts" || mode === "complete-merge"
					? { id: "primary", mode, kind: "review", disabled: loading, primary: true }
				: { id: "primary", mode: mode ?? undefined, kind: "execute", disabled: loading, primary: true };

	const requestActions: ExecutionDockAction[] = context.hasChangeRequest
		? [
				{
					id: "open-pr" as const,
					mode: "open-pr" as const,
					kind: "execute" as const,
					disabled: loading,
				},
				{
					id: "merge" as const,
					mode: "merge" as const,
					kind: "execute" as const,
					disabled: loading || mode !== "merge",
				},
			]
		: [
				{
					id: "create-pr" as const,
					mode: "create-pr" as const,
					kind: "create-request" as const,
					disabled: loading || context.hasOpenRequest || !context.hasBranchChanges,
				},
				{
					id: "create-draft-pr" as const,
					mode: "create-pr" as const,
					kind: "create-request" as const,
					disabled: loading || context.hasOpenRequest || !context.hasBranchChanges,
				},
			];

	const secondaryActions: ExecutionDockAction[] = [
		{ id: "commit-and-push", mode: "commit-and-push", kind: "execute", disabled: loading },
		{ id: "commit", mode: "commit", kind: "execute", disabled: loading || !context.hasLocalChanges },
		{ id: "push", mode: "push", kind: "execute", disabled: loading || !context.hasAheadCommits },
		{ id: "sync-base", mode: "sync-base", kind: "execute", disabled: loading },
		...requestActions,
	];

	return [primary, ...secondaryActions].filter((action, index) => {
		if (index === 0) return true;
		return action.mode !== primary.mode;
	});
}
