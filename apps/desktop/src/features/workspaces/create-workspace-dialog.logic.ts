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

/**
 * Orders the projects of a multi-project task the way the person picked them.
 * The first one is the primary: where the task lives in the sidebar, where the
 * agent starts and whose project instructions it loads. The agent can still
 * change any of the selected projects.
 */
export function orderMultiProjectSelection<Repository extends { id: string }>(
	repositories: Repository[],
	selectedIds: string[],
): Repository[] {
	return selectedIds.flatMap((id) => {
		const repository = repositories.find((candidate) => candidate.id === id);
		return repository ? [repository] : [];
	});
}

export function promoteMultiProjectPrimary(selectedIds: string[], id: string): string[] {
	if (!selectedIds.includes(id)) return selectedIds;
	return [id, ...selectedIds.filter((candidate) => candidate !== id)];
}

/** Option id of a linked folder that is not a DCC project yet. */
export function linkedFolderOptionId(path: string) {
	return `linked:${normalizeWorkspaceRoot(path)}`;
}

/**
 * The linked folders of every project, as options for a multi-project task:
 * a folder that is already a project uses that project; any other folder
 * becomes an option of its own, after the projects.
 */
export function withLinkedFolderOptions<
	Option extends { id: string; rootPath: string },
>(
	options: readonly Option[],
	linkedFoldersByRepositoryId: Record<string, string[]>,
	folderOption: (path: string) => Option,
): Option[] {
	const known = new Set(options.map((option) => normalizeWorkspaceRoot(option.rootPath)));
	const extra: Option[] = [];
	for (const paths of Object.values(linkedFoldersByRepositoryId)) {
		for (const path of paths) {
			const root = normalizeWorkspaceRoot(path);
			if (known.has(root)) continue;
			known.add(root);
			extra.push(folderOption(path));
		}
	}
	return [...options, ...extra];
}

/**
 * Picks a project for a multi-project task. The first pick is the primary and
 * brings its linked folders along, already selected.
 */
export function selectMultiProject<Option extends { id: string; rootPath: string }>(
	selectedIds: string[],
	optionId: string,
	options: readonly Option[],
	linkedFoldersByRepositoryId: Record<string, string[]>,
): string[] {
	if (selectedIds.includes(optionId)) return selectedIds;
	if (selectedIds.length > 0) return [...selectedIds, optionId];
	const linkedIds = (linkedFoldersByRepositoryId[optionId] ?? []).flatMap((path) => {
		const root = normalizeWorkspaceRoot(path);
		const option = options.find(
			(candidate) => normalizeWorkspaceRoot(candidate.rootPath) === root,
		);
		return option && option.id !== optionId ? [option.id] : [];
	});
	return [optionId, ...linkedIds];
}
