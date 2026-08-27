export function inferProjectIdFromWorkspaceRoot(workspaceRoot: string): string {
	const normalized = normalizeWorkspaceRoot(workspaceRoot);
	const lastSegment = normalized.split("/").filter(Boolean).pop() ?? "";

	return sanitizeProjectIdSegment(lastSegment);
}

export function normalizeWorkspaceRoot(workspaceRoot: string) {
	return workspaceRoot.replaceAll("\\", "/").replace(/\/+$/, "");
}

export function repositoryNameFromWorkspaceRoot(workspaceRoot: string) {
	return (
		normalizeWorkspaceRoot(workspaceRoot).split("/").filter(Boolean).pop() ??
		"Project"
	);
}

export function includePickedRepository<Repository extends { rootPath: string }>(
	repositories: readonly Repository[],
	pickedRepository: Repository | null,
): Repository[] {
	if (
		!pickedRepository ||
		repositories.some(
			(repository) =>
				normalizeWorkspaceRoot(repository.rootPath) ===
				normalizeWorkspaceRoot(pickedRepository.rootPath),
		)
	) {
		return [...repositories];
	}
	return [pickedRepository, ...repositories];
}

export type WorkspaceStart = "new" | "branch";

export function initialWorkspaceStart(hasRepositoryContext: boolean): WorkspaceStart {
	return hasRepositoryContext ? "branch" : "new";
}

export function isBranchWorkspaceSource(kind: "branch" | "pull_request"): boolean {
	return kind === "branch";
}

function sanitizeProjectIdSegment(value: string): string {
	const sanitized = value
		.trim()
		.toLowerCase()
		.replace(/[^a-z0-9._-]+/g, "-")
		.replace(/-+/g, "-")
		.replace(/^[-_.]+|[-_.]+$/g, "");

	return sanitized.length > 0 ? sanitized : "project";
}
