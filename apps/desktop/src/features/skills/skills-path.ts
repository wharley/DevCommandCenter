/** Skills are sourced and compiled in the same checkout where the active task runs. */
export function resolveActiveSkillsCheckout(workspace: {
	rootPath?: string | null;
	worktreePath?: string | null;
}): string | null {
	const worktreePath = workspace.worktreePath?.trim();
	if (worktreePath) {
		return worktreePath;
	}
	return workspace.rootPath?.trim() || null;
}
