import type { Repository } from "@dcc/contracts";

type RepositoryIdentity = Pick<Repository, "displayName" | "name" | "rootPath">;

function pathLeaf(path: string) {
	return path
		.trim()
		.replace(/[\\/]+$/gu, "")
		.split(/[\\/]/gu)
		.filter(Boolean)
		.at(-1);
}

export function repositoryDisplayName(repository: RepositoryIdentity) {
	return (
		repository.displayName?.trim() ||
		repository.name.trim() ||
		pathLeaf(repository.rootPath) ||
		"Project"
	);
}

export function repositoryHasCustomDisplayName(repository: RepositoryIdentity) {
	return Boolean(repository.displayName?.trim());
}
