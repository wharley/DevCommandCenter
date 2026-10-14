import type { TFunction } from "i18next";

/**
 * The branch a task works on, the same way everywhere it shows (composer,
 * sidebar, review). Git answers the literal "HEAD" for a detached checkout,
 * which is how a task worktree starts; that is shown as "no branch" next to
 * the base it came from, never as "HEAD" in one place and "main" in another.
 */
export function workingBranchLabel(
	currentBranch: string | null | undefined,
	baseBranch: string | null | undefined,
	t: TFunction<"common">,
): string | null {
	const base = baseBranch?.trim() || null;
	if (currentBranch === "HEAD") {
		return base && base !== "HEAD"
			? t("workingBranch.detached", { branch: base })
			: t("workingBranch.detachedNoBase");
	}
	return currentBranch?.trim() || base;
}
