/**
 * `@` to mention a file in the composer, as on the desktop: the `@` before the
 * cursor opens a list of the workspace's tracked files and a pick becomes
 * `@caminho/relativo `. Follows Paseo's `findActiveFileMention`
 * (`packages/app/src/utils/file-mention-autocomplete.ts`) with the desktop's
 * trigger rule (`matchFileMentionTrigger`): the `@` starts the text or follows
 * a space or `(`, so `ana@empresa.com` never opens the list.
 */

export type FileMention = {
	/** Index of the `@`. */
	start: number;
	/** The cursor: the mention spans `[start, end)`. */
	end: number;
	/** What follows the `@`, the filter for the list. */
	query: string;
};

const MAX_MENTION_QUERY = 200;

export function findActiveFileMention(text: string, cursor: number): FileMention | null {
	const end = Math.max(0, Math.min(cursor, text.length));
	const before = text.slice(0, end);
	const at = before.lastIndexOf("@");
	if (at < 0) return null;
	const lead = at === 0 ? "" : before[at - 1]!;
	if (lead && !/[\s(]/.test(lead)) return null;
	const query = before.slice(at + 1);
	if (/[\s@]/.test(query) || query.length > MAX_MENTION_QUERY) return null;
	return { start: at, end, query };
}

/** The text with the mention replaced by `@path ` and where the cursor goes next. */
export function applyFileMention(
	text: string,
	mention: FileMention,
	path: string,
): { text: string; cursor: number } {
	const inserted = `@${path} `;
	const after = text.slice(mention.end).replace(/^ /, "");
	return {
		text: `${text.slice(0, mention.start)}${inserted}${after}`,
		cursor: mention.start + inserted.length,
	};
}

/** `GET /api/v1/mobile/{workspaces|repositories}/:id/files?q=`. */
export type FileSearchResult = { files: Array<{ path: string }>; total: number };

export function fileSearchPath(
	scope: { workspaceId: string } | { repositoryId: string },
	query: string,
): string {
	const base =
		"workspaceId" in scope
			? `/api/v1/mobile/workspaces/${encodeURIComponent(scope.workspaceId)}/files`
			: `/api/v1/mobile/repositories/${encodeURIComponent(scope.repositoryId)}/files`;
	return `${base}?q=${encodeURIComponent(query)}`;
}

/** File name and folder, for a two-tone list row. */
export function splitPath(path: string): { name: string; dir: string } {
	const slash = path.lastIndexOf("/");
	return slash < 0 ? { name: path, dir: "" } : { name: path.slice(slash + 1), dir: path.slice(0, slash + 1) };
}

/**
 * The desktop's ranking (`filterFiles` in `file-mention-plugin.tsx`), which the
 * desktop API applies before answering; the demo filters its fictional files
 * with it. An exact name or path stands alone; then path suffix, name prefix,
 * name substring, path substring.
 */
export function filterTrackedFiles(paths: readonly string[], query: string, limit = 50): string[] {
	if (!query) return paths.slice(0, limit);
	const q = query.toLowerCase();
	const rank = (path: string) => {
		const lower = path.toLowerCase();
		const name = splitPath(lower).name;
		if (name === q || lower === q) return 5;
		if (lower.endsWith(`/${q}`)) return 4;
		if (name.startsWith(q)) return 3;
		if (name.includes(q)) return 2;
		if (lower.includes(q)) return 1;
		return 0;
	};
	const ranked = paths.map((path) => ({ path, score: rank(path) })).filter((entry) => entry.score > 0);
	const exact = ranked.filter((entry) => entry.score === 5);
	return (exact.length > 0 ? exact : ranked.sort((a, b) => b.score - a.score))
		.slice(0, limit)
		.map((entry) => entry.path);
}
