/**
 * Delivering a task from the phone (`/api/v1/mobile/workspaces/:id/delivery…`).
 *
 * `resolveCommitMode` and `resolveExecutionDockActions` are the desktop's
 * (`features/commit/WorkspaceCommitButton.logic.ts`,
 * `features/composer/ExecutionDock.actions.ts`), copied because Metro can't
 * reach apps/desktop; `delivery.test.ts` runs both copies against the
 * desktop's on the same cases and fails when they drift.
 */
import { t } from "./labels.ts";

export type DeliveryChange = { path: string; status: string; insertions: number; deletions: number; staged: boolean };

export type DeliverySummary = {
	workspaceId: string;
	workspaceState: string;
	branch: string | null;
	baseBranch: string;
	changes: DeliveryChange[];
	stagedCount: number;
	branchChanges: DeliveryChange[];
	aheadOfRemoteCount: number;
	behindOfRemoteCount: number;
	conflictCount: number;
	mergeInProgress: boolean;
	changesToken: string;
	commitMessage: string;
	beforePush: string[];
	pushFailure: { classification: string; target: string | null; canSync: boolean } | null;
};

export type PullRequest = {
	provider?: string | null;
	number?: number | null;
	title?: string | null;
	url?: string | null;
	headBranch?: string | null;
	baseBranch?: string | null;
	state?: string | null;
	isDraft?: boolean;
	mergeable?: string | null;
	mergeStateStatus?: string | null;
};

export type DeliveryForge = {
	pullRequest: PullRequest | null;
	pipeline: { status: string; webUrl: string | null; failedJobs: string[] } | null;
	requestTitle: string | null;
};

// ---------- copy of WorkspaceCommitButton.logic.ts (resolveCommitMode) ----------

export type CommitMode =
	| "create-pr"
	| "open-pr"
	| "commit-and-push"
	| "push"
	| "fix"
	| "resolve-conflicts"
	| "complete-merge"
	| "merge"
	| "merged"
	| "closed";

export type CommitModeContext = {
	branch: string;
	prStatus?: PullRequest | null;
	gitStatus?: {
		staged: unknown[];
		unstaged: unknown[];
		aheadOfRemoteCount?: number;
		conflictCount?: number;
		mergeInProgress?: boolean;
		conflictResolutionReady?: boolean;
	} | null;
};

function hasWorkingTreeChanges(gitStatus?: CommitModeContext["gitStatus"]) {
	return (gitStatus?.staged.length ?? 0) > 0 || (gitStatus?.unstaged.length ?? 0) > 0;
}

function hasFailingChecks(prStatus?: PullRequest | null) {
	const mergeStateStatus = prStatus?.mergeStateStatus?.toUpperCase();
	return mergeStateStatus === "DIRTY" || mergeStateStatus === "BLOCKED" || mergeStateStatus === "UNSTABLE";
}

export function resolveCommitMode(context: CommitModeContext): CommitMode {
	const normalized = context.branch.toLowerCase();
	const prHeadBranch = context.prStatus?.headBranch;
	const prBelongsHere = !prHeadBranch || prHeadBranch === context.branch;
	const prState = prBelongsHere ? context.prStatus?.state?.toLowerCase() : undefined;

	if (context.gitStatus?.mergeInProgress) {
		if ((context.gitStatus.conflictCount ?? 0) === 0 || context.gitStatus.conflictResolutionReady) {
			return "complete-merge";
		}
		return "resolve-conflicts";
	}
	if ((context.gitStatus?.conflictCount ?? 0) > 0) return "resolve-conflicts";
	if (prState === "merged") return "merged";
	if (prState === "closed") return "open-pr";

	if (prState === "open") {
		if (hasWorkingTreeChanges(context.gitStatus)) return "commit-and-push";
		if ((context.gitStatus?.aheadOfRemoteCount ?? 0) > 0) return "push";
		if (context.prStatus?.mergeable === "CONFLICTING") return "resolve-conflicts";
		if (hasFailingChecks(context.prStatus)) return "fix";
		return "merge";
	}

	if (hasWorkingTreeChanges(context.gitStatus)) return "commit-and-push";
	if ((context.gitStatus?.aheadOfRemoteCount ?? 0) > 0) return "push";

	if (normalized.includes("closed")) return "closed";
	if (normalized.includes("merged")) return "merged";
	if (normalized.includes("conflict")) return "resolve-conflicts";
	if (normalized.includes("merge")) return "merge";
	if (normalized.includes("fix")) return "fix";
	if (normalized.includes("pr/")) return "open-pr";
	if (normalized.includes("push")) return "push";
	return "create-pr";
}

// ---------- copy of ExecutionDock.actions.ts (resolveExecutionDockActions) ----------

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

export type ExecutionDockActionContext = {
	mode: CommitMode | null;
	loading: boolean;
	multiProject: boolean;
	hasLocalChanges: boolean;
	hasBranchChanges: boolean;
	hasAheadCommits: boolean;
	hasChangeRequest: boolean;
	hasOpenRequest: boolean;
};

export function resolveExecutionDockActions(context: ExecutionDockActionContext): ExecutionDockAction[] {
	const { mode, loading, multiProject } = context;
	if (multiProject) {
		return [{ id: "primary", kind: "execute", disabled: loading, primary: true }];
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
				{ id: "open-pr", mode: "open-pr", kind: "execute", disabled: loading },
				{ id: "merge", mode: "merge", kind: "execute", disabled: loading || mode !== "merge" },
			]
		: [
				{
					id: "create-pr",
					mode: "create-pr",
					kind: "create-request",
					disabled: loading || context.hasOpenRequest || !context.hasBranchChanges,
				},
				{
					id: "create-draft-pr",
					mode: "create-pr",
					kind: "create-request",
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

	return [primary, ...secondaryActions].filter((action, index) => index === 0 || action.mode !== primary.mode);
}

// ---------- the phone's view of them ----------

/** The summary as the desktop panel feeds `resolveCommitMode`. */
export function commitModeFor(summary: DeliverySummary, forge: DeliveryForge | null): CommitMode {
	return resolveCommitMode({
		branch: summary.branch ?? "",
		prStatus: forge?.pullRequest ?? null,
		gitStatus: {
			staged: summary.changes.filter((c) => c.staged),
			unstaged: summary.changes.filter((c) => !c.staged),
			aheadOfRemoteCount: summary.aheadOfRemoteCount,
			conflictCount: summary.conflictCount,
			mergeInProgress: summary.mergeInProgress,
		},
	});
}

/** Actions the phone offers. Merge, base sync and conflict review stay on the desktop. */
export const PHONE_ACTIONS = new Set<ExecutionDockActionId>(["commit", "commit-and-push", "push", "create-pr", "create-draft-pr", "open-pr"]);

export function deliveryActions(summary: DeliverySummary, forge: DeliveryForge | null, busy: boolean): {
	mode: CommitMode;
	actions: ExecutionDockAction[];
	/** The desktop's next step, when the phone can take it (shown first, highlighted). */
	primaryId: ExecutionDockActionId | null;
	/** Why the desktop's next step needs the desktop, when it does. */
	desktopOnly: string | null;
} {
	const mode = commitModeFor(summary, forge);
	const prState = forge?.pullRequest?.state?.trim() ?? "";
	const all = resolveExecutionDockActions({
		mode,
		loading: busy,
		multiProject: false,
		hasLocalChanges: summary.changes.length > 0,
		hasBranchChanges: summary.branchChanges.length > 0,
		hasAheadCommits: summary.aheadOfRemoteCount > 0,
		hasChangeRequest: Boolean(prState),
		hasOpenRequest: prState.toLowerCase() === "open",
	});
	// The primary as its concrete action, so it shows once and in front.
	const concrete = all.map((action): ExecutionDockAction => {
		if (action.id !== "primary") return action;
		const id: ExecutionDockActionId | null =
			action.mode === "commit-and-push" || action.mode === "push" || action.mode === "open-pr"
				? action.mode
				: action.mode === "create-pr"
					? "create-pr"
					: null;
		return id ? { ...action, id } : action;
	});
	const actions = concrete.filter((action) => PHONE_ACTIONS.has(action.id));
	const primaryId = concrete[0]?.primary && PHONE_ACTIONS.has(concrete[0].id) ? concrete[0].id : null;
	const conflicts = summary.conflictCount > 0 || summary.mergeInProgress;
	const pr = forge?.pullRequest ?? null;
	const desktopOnly = conflicts
		? "Há conflitos para resolver: faça isso no computador."
		: pr && mode === "resolve-conflicts"
			? "O PR está em conflito com a base: resolva no computador."
			: pr && mode === "fix"
				? "O PR tem checks falhando ou está bloqueado: peça a correção ao agente ou veja no computador."
				: pr && mode === "merge"
					? "Mesclar o PR fica no computador."
					: null;
	return { mode, actions, primaryId, desktopOnly };
}

export function deliveryActionLabel(action: ExecutionDockAction, requestLabel = "PR"): string {
	switch (action.id) {
		case "commit":
			return t("composer.executionDock.actions.commit");
		case "commit-and-push":
			return t("commit.modes.commit-and-push.idle");
		case "push":
			return t("commit.modes.push.idle");
		case "create-pr":
			return t("commit.modes.create-pr.idle", { requestLabel });
		case "create-draft-pr":
			return t("composer.executionDock.actions.createDraft", { requestLabel });
		case "open-pr":
			return t("commit.modes.open-pr.idle", { requestLabel });
		case "sync-base":
			return t("composer.executionDock.actions.syncBase");
		default:
			return action.mode ? t(`commit.modes.${action.mode}.idle` as Parameters<typeof t>[0], { requestLabel }) : "";
	}
}

export function changeTotals(changes: DeliveryChange[]): { files: number; additions: number; deletions: number } {
	return changes.reduce(
		(sum, change) => ({ files: sum.files + 1, additions: sum.additions + change.insertions, deletions: sum.deletions + change.deletions }),
		{ files: 0, additions: 0, deletions: 0 },
	);
}

/** PR state in words, plus the CI line. */
export function pullRequestStateText(pr: PullRequest): string {
	const state = pr.state?.toLowerCase();
	if (state === "merged") return "mesclado";
	if (state === "closed") return "fechado";
	if (pr.isDraft) return "rascunho";
	return "aberto";
}

export function pipelineText(pipeline: DeliveryForge["pipeline"]): string | null {
	if (!pipeline) return null;
	const status = pipeline.status.toLowerCase();
	if (status === "success" || status === "passed") return "CI passou";
	if (status === "failed" || status === "failure") {
		return pipeline.failedJobs.length > 0 ? `CI falhou: ${pipeline.failedJobs.slice(0, 3).join(", ")}` : "CI falhou";
	}
	if (status === "running" || status === "pending" || status === "created" || status === "in_progress" || status === "queued") return "CI rodando";
	if (status === "canceled" || status === "cancelled") return "CI cancelado";
	return `CI: ${pipeline.status}`;
}

/**
 * What the thread's diff chip says: the branch and everything it differs
 * from its base by — committed on the branch plus still local. A file in
 * both counts once, with both sets of lines.
 */
export function workspaceDiffStat(summary: DeliverySummary): {
	branch: string | null;
	files: number;
	additions: number;
	deletions: number;
} {
	const paths = new Set<string>();
	let additions = 0;
	let deletions = 0;
	for (const change of [...summary.branchChanges, ...summary.changes]) {
		paths.add(change.path);
		additions += change.insertions;
		deletions += change.deletions;
	}
	return { branch: summary.branch, files: paths.size, additions, deletions };
}
