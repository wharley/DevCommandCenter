import type {
	CreateWorkspaceForRepoInput,
	Repository,
	WorkspaceBundleSummary,
	WorkspaceIsolationMode,
} from "@dcc/contracts";
import {
	inferProjectIdFromWorkspaceRoot,
	normalizeWorkspaceRoot,
	repositoryNameFromWorkspaceRoot,
} from "./create-workspace-dialog.logic";
import { repositoryDisplayName } from "./repository-display-name";

/** The folders the project at `rootPath` links, in order. */
export function linkedFoldersOf(
	rootPath: string,
	repositories: readonly Repository[],
	linkedFoldersByRepositoryId: Record<string, string[]>,
): string[] {
	const root = normalizeWorkspaceRoot(rootPath.trim());
	const repository = repositories.find(
		(candidate) => normalizeWorkspaceRoot(candidate.rootPath.trim()) === root,
	);
	return repository ? (linkedFoldersByRepositoryId[repository.id] ?? []) : [];
}

/**
 * The projects of a task created in a project that links folders: the project
 * first (the primary), then each linked folder, all in the same mode. A folder
 * that is not a DCC project yet has no known base; the backend starts it from
 * that checkout's current branch.
 */
export function linkedTaskProjects(input: {
	primary: { projectId: string; workspaceRoot: string; baseBranch: string };
	linkedFolders: readonly string[];
	repositories: readonly Repository[];
	isolationMode: WorkspaceIsolationMode | null | undefined;
}): CreateWorkspaceForRepoInput[] {
	const isolationMode: WorkspaceIsolationMode | null =
		input.isolationMode === "localDirect" ? "localDirect" : null;
	const find = (path: string) => {
		const root = normalizeWorkspaceRoot(path.trim());
		return input.repositories.find(
			(candidate) => normalizeWorkspaceRoot(candidate.rootPath.trim()) === root,
		);
	};
	const primaryRepository = find(input.primary.workspaceRoot);
	return [
		{
			projectId: input.primary.projectId,
			workspaceRoot: input.primary.workspaceRoot,
			baseBranch: input.primary.baseBranch,
			name: primaryRepository ? repositoryDisplayName(primaryRepository) : null,
			isolationMode,
		},
		...input.linkedFolders.map((path) => {
			const repository = find(path);
			return {
				projectId: repository?.projectId ?? inferProjectIdFromWorkspaceRoot(path),
				workspaceRoot: path,
				baseBranch: repository?.baseBranch ?? "",
				name: repository
					? repositoryDisplayName(repository)
					: repositoryNameFromWorkspaceRoot(path),
				isolationMode,
			};
		}),
	];
}

/** The other projects of the multi-project task whose primary is `workspaceId`. */
export function additionalWorkspaceIdsOf(
	workspaceId: string,
	bundles: readonly WorkspaceBundleSummary[],
): string[] {
	const bundle = bundles.find((summary) => summary.bundle.primaryWorkspaceId === workspaceId);
	return (
		bundle?.members
			.map((member) => member.workspaceId)
			.filter((memberId) => memberId !== workspaceId) ?? []
	);
}
