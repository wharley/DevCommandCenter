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
	/** Folders without Git; they ride along on the primary project. */
	contextFolders?: readonly string[];
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
			...(input.contextFolders?.length ? { contextFolders: [...input.contextFolders] } : {}),
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

export type LinkedFolderIndex = {
	/** Every linked folder of each project, by repository id, in order. */
	paths: Record<string, string[]>;
	/** The ones without Git, by repository id: they join a task as context. */
	contextPaths: Record<string, string[]>;
};

export const EMPTY_LINKED_FOLDER_INDEX: LinkedFolderIndex = { paths: {}, contextPaths: {} };

export function linkedFolderIndex(
	entries: readonly { repositoryId: string; paths: string[]; contextPaths: string[] }[],
): LinkedFolderIndex {
	return {
		paths: Object.fromEntries(entries.map((entry) => [entry.repositoryId, entry.paths])),
		contextPaths: Object.fromEntries(
			entries.map((entry) => [entry.repositoryId, entry.contextPaths]),
		),
	};
}

export type LinkedTaskPlan =
	| { kind: "single"; contextFolders: string[] }
	| { kind: "bundle"; projects: CreateWorkspaceForRepoInput[] };

/**
 * How a task created in a project starts: its linked Git folders make it a
 * multi-project task; folders without Git only ride along as context on the
 * project itself.
 */
export function planLinkedTask(input: {
	primary: { projectId: string; workspaceRoot: string; baseBranch: string };
	index: LinkedFolderIndex;
	repositories: readonly Repository[];
	isolationMode: WorkspaceIsolationMode | null | undefined;
	includeLinkedFolders?: boolean;
}): LinkedTaskPlan {
	if (input.includeLinkedFolders === false) return { kind: "single", contextFolders: [] };
	const linked = linkedFoldersOf(input.primary.workspaceRoot, input.repositories, input.index.paths);
	const contextFolders = linkedFoldersOf(
		input.primary.workspaceRoot,
		input.repositories,
		input.index.contextPaths,
	);
	const projectFolders = linked.filter((path) => !contextFolders.includes(path));
	if (projectFolders.length === 0) return { kind: "single", contextFolders };
	return {
		kind: "bundle",
		projects: linkedTaskProjects({
			primary: input.primary,
			linkedFolders: projectFolders,
			contextFolders,
			repositories: input.repositories,
			isolationMode: input.isolationMode,
		}),
	};
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
