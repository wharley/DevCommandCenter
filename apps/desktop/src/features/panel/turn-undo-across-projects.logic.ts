import type {
	ExecuteGuardedUndoOutput,
	PrepareGuardedUndoOutput,
	TurnReviewSummary,
} from "@dcc/contracts";
import { canPrepareGuardedUndo } from "./turn-review.logic";

/** One project of a multi-project turn and its review of that turn. */
export type TurnUndoProject = {
	workspaceId: string;
	name: string;
	review: TurnReviewSummary | null;
};

export type TurnUndoTarget = {
	workspaceId: string;
	name: string;
	snapshotId: string;
};

export type TurnUndoPlan = {
	/** Projects whose changes in this turn Undo can restore. */
	targets: TurnUndoTarget[];
	/** Projects the turn changed but Undo cannot restore. */
	uncovered: { workspaceId: string; name: string }[];
};

/**
 * Splits the projects of one turn into the ones Undo restores and the ones it
 * cannot. A project the turn did not change is in neither list.
 */
export function planTurnUndo(projects: TurnUndoProject[]): TurnUndoPlan {
	const targets: TurnUndoTarget[] = [];
	const uncovered: TurnUndoPlan["uncovered"] = [];
	for (const { workspaceId, name, review } of projects) {
		if (!review || review.files.length === 0) continue;
		if (canPrepareGuardedUndo(review.guardedUndo, review.activeUndo)) {
			targets.push({ workspaceId, name, snapshotId: review.snapshotId });
		} else {
			uncovered.push({ workspaceId, name });
		}
	}
	return { targets, uncovered };
}

export type PreparedTurnUndo = TurnUndoTarget & {
	preview: Extract<PrepareGuardedUndoOutput, { status: "ready" }>;
};

export type TurnUndoPreparation =
	| { status: "ready"; prepared: PreparedTurnUndo[] }
	| {
			status: "blocked";
			/** The first project that cannot be undone right now. */
			target: TurnUndoTarget;
			result: "blocked" | "unavailable";
			reasonCode: string;
	  };

/**
 * Prepares every project before anything is restored. Preparation is
 * read-only, so stopping at the first project that is not free leaves every
 * project exactly as it is.
 */
export async function prepareTurnUndo(
	targets: TurnUndoTarget[],
	prepare: (snapshotId: string) => Promise<PrepareGuardedUndoOutput>,
): Promise<TurnUndoPreparation> {
	const prepared: PreparedTurnUndo[] = [];
	for (const target of targets) {
		const preview = await prepare(target.snapshotId);
		if (preview.status === "ready" && preview.unrelatedPathsAreNotTargets) {
			prepared.push({ ...target, preview });
			continue;
		}
		return {
			status: "blocked",
			target,
			result: preview.status === "ready" ? "unavailable" : preview.status,
			reasonCode:
				preview.status === "ready" ? "invalid_persisted_record" : preview.reasonCode,
		};
	}
	return { status: "ready", prepared };
}

export type TurnUndoStepResult = {
	workspaceId: string;
	name: string;
	status: ExecuteGuardedUndoOutput["status"] | "failed" | "not_attempted";
	reasonCode: string | null;
};

/**
 * Restores the prepared projects one by one. Each project is restored as a
 * whole or not at all; there is no transaction across projects, so the first
 * project that does not complete stops the rest and every project reports
 * what happened to it.
 */
export async function executeTurnUndo(
	prepared: PreparedTurnUndo[],
	execute: (previewToken: string) => Promise<ExecuteGuardedUndoOutput>,
): Promise<TurnUndoStepResult[]> {
	const results: TurnUndoStepResult[] = [];
	let stopped = false;
	for (const project of prepared) {
		const base = { workspaceId: project.workspaceId, name: project.name };
		if (stopped) {
			results.push({ ...base, status: "not_attempted", reasonCode: null });
			continue;
		}
		try {
			const outcome = await execute(project.preview.previewToken);
			results.push({ ...base, status: outcome.status, reasonCode: outcome.reasonCode });
			stopped = outcome.status !== "completed";
		} catch {
			results.push({ ...base, status: "failed", reasonCode: null });
			stopped = true;
		}
	}
	return results;
}
