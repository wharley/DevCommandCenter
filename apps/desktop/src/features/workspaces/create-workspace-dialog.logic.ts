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

export const LAST_TASK_REPOSITORY_ROOT_STORAGE_KEY =
	"dcc-last-task-repository-root-v1";

type RepositoryRootPreferenceStorage = Pick<
	Storage,
	"getItem" | "setItem"
>;

function browserPreferenceStorage(): RepositoryRootPreferenceStorage | null {
	if (typeof window === "undefined") {
		return null;
	}
	return window.localStorage;
}

export function readLastTaskRepositoryRoot(
	storage: RepositoryRootPreferenceStorage | null = browserPreferenceStorage(),
): string | null {
	try {
		const root = storage
			?.getItem(LAST_TASK_REPOSITORY_ROOT_STORAGE_KEY)
			?.trim();
		return root ? normalizeWorkspaceRoot(root) : null;
	} catch {
		return null;
	}
}

export function rememberLastTaskRepositoryRoot(
	rootPath: string,
	storage: RepositoryRootPreferenceStorage | null = browserPreferenceStorage(),
): void {
	const normalized = normalizeWorkspaceRoot(rootPath.trim());
	if (!normalized) {
		return;
	}
	try {
		storage?.setItem(LAST_TASK_REPOSITORY_ROOT_STORAGE_KEY, normalized);
	} catch {
		// Preferences are best-effort when WebKit storage is unavailable.
	}
}

export function initialTaskRepository<Repository extends { rootPath: string }>(
	repositories: readonly Repository[],
	explicitRoot: string | null | undefined,
	rememberedRoot: string | null | undefined,
): Repository | null {
	for (const preferredRoot of [explicitRoot, rememberedRoot]) {
		if (!preferredRoot?.trim()) {
			continue;
		}
		const normalizedPreferredRoot = normalizeWorkspaceRoot(preferredRoot.trim());
		const match = repositories.find(
			(repository) =>
				normalizeWorkspaceRoot(repository.rootPath) === normalizedPreferredRoot,
		);
		if (match) {
			return match;
		}
	}

	return repositories[0] ?? null;
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
