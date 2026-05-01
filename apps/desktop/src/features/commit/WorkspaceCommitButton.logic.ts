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
	return "commit-and-push" as const;
}

export function commitLabel(mode: CommitMode, status: CommitButtonStatus) {
	if (mode === "merged") return "Merged";
	if (mode === "closed") return "Closed";

	switch (mode) {
		case "create-pr":
			return { idle: "Create PR", busy: "Creating PR...", done: "PR Created", error: "Retry" }[status];
		case "open-pr":
			return { idle: "Open PR", busy: "Opening...", done: "Opened", error: "Retry" }[status];
		case "commit-and-push":
			return { idle: "Commit & Push", busy: "Committing...", done: "Pushed", error: "Retry" }[status];
		case "push":
			return { idle: "Push", busy: "Pushing...", done: "Pushed", error: "Retry" }[status];
		case "fix":
			return { idle: "Fix CI", busy: "Fixing...", done: "Fixed", error: "Retry" }[status];
		case "resolve-conflicts":
			return { idle: "Resolve", busy: "Resolving...", done: "Resolved", error: "Retry" }[status];
		case "merge":
			return { idle: "Merge", busy: "Merging...", done: "Merged", error: "Retry" }[status];
		default:
			return "Commit";
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
