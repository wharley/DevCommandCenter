/** Cross-platform basename without importing Node `path`. */
export function pathBasename(filePath: string): string {
	const normalized = filePath.replace(/\\/g, "/");
	const i = normalized.lastIndexOf("/");
	return i >= 0 ? normalized.slice(i + 1) : normalized;
}

/** If `absolutePath` is under `workspaceRoot`, return repo-relative posix path; else unchanged. */
export function pathRelativeToWorkspace(
	workspaceRoot: string | null,
	absolutePath: string,
): string {
	if (!workspaceRoot) {
		return absolutePath;
	}
	const root = workspaceRoot.replace(/\\/g, "/").replace(/\/$/, "");
	const p = absolutePath.replace(/\\/g, "/");
	const prefix = `${root}/`;
	if (p === root) {
		return "";
	}
	if (p.startsWith(prefix)) {
		return p.slice(prefix.length);
	}
	return absolutePath;
}
