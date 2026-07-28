import type { WorkspaceGitConflictStateOutput } from "@dcc/contracts";
import { hasMergeConflictMarkerFragments } from "./merge-conflict-hunks";

/**
 * A conservative signal for resolutions edited outside DCC's resolver.
 *
 * Git keeps those paths unmerged until they are staged. We only offer the
 * Inspector completion checkpoint when every result is a readable text file
 * with no remaining conflict-marker fragments. Binary, oversized, symlink,
 * submodule, and deletion resolutions still go through the file-by-file
 * resolver so their intent stays explicit.
 */
export function isMergeConflictResolutionReady(
	state: WorkspaceGitConflictStateOutput | null | undefined,
): boolean {
	return Boolean(
		state?.operation === "merge" &&
			state.conflicts.length > 0 &&
			state.conflicts.every(
				(conflict) =>
					(conflict.kind === "both-modified" ||
						conflict.kind === "both-added") &&
					conflict.result.exists &&
					!conflict.result.binary &&
					!conflict.result.truncated &&
					conflict.result.text != null &&
					!hasMergeConflictMarkerFragments(conflict.result.text),
			),
	);
}
