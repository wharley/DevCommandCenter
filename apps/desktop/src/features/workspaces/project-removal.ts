export type ProjectRemovalInput = {
	repositoryId: string;
	workspaceIds: string[];
};

type ProjectRemovalActions = {
	deleteRepository: (repositoryId: string) => Promise<void>;
	removeWorkspaceCaches: (workspaceIds: string[]) => void;
	refreshRepositories: () => Promise<unknown>;
	refreshWorkspaces: () => Promise<unknown>;
};

export async function removeProjectFromDcc(
	input: ProjectRemovalInput,
	actions: ProjectRemovalActions,
) {
	await actions.deleteRepository(input.repositoryId);
	actions.removeWorkspaceCaches([...new Set(input.workspaceIds)]);
	await Promise.all([actions.refreshRepositories(), actions.refreshWorkspaces()]);
}
