export type CommitMode =
	| "create-pr"
	| "open-pr"
	| "commit-and-push"
	| "push"
	| "fix"
	| "resolve-conflicts"
	| "merge"
	| "merged"
	| "closed";

export type CommitButtonStatus = "idle" | "busy" | "done" | "error";

export function resolveCommitMode(branch: string) {
	const normalized = branch.toLowerCase();
	if (normalized.includes("closed")) return "closed" as const;
	if (normalized.includes("merged")) return "merged" as const;
	if (normalized.includes("conflict")) return "resolve-conflicts" as const;
	if (normalized.includes("merge")) return "merge" as const;
	if (normalized.includes("fix")) return "fix" as const;
	if (normalized.includes("pr/")) return "open-pr" as const;
	if (normalized.includes("push")) return "push" as const;
	return "create-pr" as const;
}

/** i18n key under namespace `common` (use with `t(commitTranslationKey(...))`). */
export function commitTranslationKey(mode: CommitMode, status: CommitButtonStatus): string {
	if (mode === "merged") {
		return "commit.merged";
	}
	if (mode === "closed") {
		return "commit.closed";
	}

	switch (mode) {
		case "create-pr":
		case "open-pr":
		case "commit-and-push":
		case "push":
		case "fix":
		case "resolve-conflicts":
		case "merge":
			return `commit.modes.${mode}.${status}`;
		default:
			return "commit.default";
	}
}

export function commitModeClassName(mode: CommitMode) {
	switch (mode) {
		case "fix":
		case "closed":
			return "bg-[var(--workspace-pr-closed-accent)] text-white hover:bg-[var(--workspace-pr-closed-accent)]";
		case "resolve-conflicts":
			return "bg-[var(--workspace-pr-conflicts-accent)] text-white hover:bg-[var(--workspace-pr-conflicts-accent)]";
		case "merge":
		case "open-pr":
			return "bg-[var(--workspace-pr-open-accent)] text-white hover:bg-[var(--workspace-pr-open-accent)]";
		case "merged":
			return "bg-[var(--workspace-pr-merged-accent)] text-white hover:bg-[var(--workspace-pr-merged-accent)]";
		default:
			return "bg-background text-foreground hover:bg-accent/60";
	}
}
