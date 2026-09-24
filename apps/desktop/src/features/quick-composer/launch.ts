import type { QuickLaunch } from "./api";

export type LaunchDependencies = {
	begin: (launch: QuickLaunch) => Promise<QuickLaunch>;
	checkpoint: (launch: QuickLaunch) => Promise<QuickLaunch>;
	createWorkspace: (launch: QuickLaunch) => Promise<string>;
	startSession: (launch: QuickLaunch) => Promise<string>;
	send: (launch: QuickLaunch) => Promise<void>;
	onProgress: (launch: QuickLaunch) => void;
};

/** Persist intent before each external mutation. An interrupted mutation is
 * never replayed: keep the prompt and any known task/session for human recovery.
 * CAS checkpoints also prevent two rapid submissions from launching twice. */
export async function executeQuickLaunch(
	initial: QuickLaunch,
	deps: LaunchDependencies,
): Promise<QuickLaunch> {
	let current = await deps.begin(initial);
	deps.onProgress(current);
	if (current.phase !== "pending") return current;
	const checkpoint = async (patch: Partial<QuickLaunch>) => {
		current = await deps.checkpoint({
			...current,
			...patch,
			revision: current.revision + 1,
		});
		deps.onProgress(current);
	};
	// Claim outside the error handler. A losing renderer must not fail a job
	// already claimed by another invocation.
	await checkpoint({ phase: "creating" });
	try {
		const workspaceId = await deps.createWorkspace(current);
		// Keep identities even if the disk checkpoint itself fails.
		current = { ...current, workspaceId };
		await checkpoint({ phase: "workspaceReady" });
		await checkpoint({ phase: "starting" });
		const sessionId = await deps.startSession(current);
		current = { ...current, sessionId };
		await checkpoint({ phase: "sessionReady" });
		await checkpoint({ phase: "sending" });
		await deps.send(current);
		await checkpoint({ phase: "completed" });
	} catch (cause) {
		const error = cause instanceof Error ? cause.message : String(cause);
		try {
			await checkpoint({ phase: "failed", error });
		} catch {
			// A lost checkpoint response may have advanced the journal. Never
			// retry a creation/send to repair that uncertainty.
			current = { ...current, phase: "failed", error };
			deps.onProgress(current);
		}
	}
	return current;
}
