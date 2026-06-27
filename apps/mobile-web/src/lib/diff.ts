/**
 * Client-side parsing for `POST /api/v1/diffs/bundle`. The daemon returns one
 * entry per worktree with raw git output (porcelain status + `diff --stat`);
 * we fold it into a glanceable "what changed" summary. There's no per-file
 * patch text on this endpoint — the file list + line counts are the mobile
 * altitude anyway.
 */

export type BundleEntry = {
	worktreePath: string;
	branch: string | null;
	status: string | null;
	stat: string | null;
	nameStatus: string | null;
	error: string | null;
};

export type FileChange = {
	path: string;
	/** Single-letter bucket used for the status chip + color. */
	code: "A" | "M" | "D" | "R" | "?";
	label: string;
};

export type WorktreeDiff = {
	worktreePath: string;
	branch: string | null;
	files: FileChange[];
	insertions: number;
	deletions: number;
	error: string | null;
	/** True when nothing differs from HEAD and the tree is clean. */
	clean: boolean;
};

const CODE_LABEL: Record<FileChange["code"], string> = {
	A: "novo",
	M: "modificado",
	D: "removido",
	R: "renomeado",
	"?": "não rastreado",
};

function bucket(index: string, worktree: string): FileChange["code"] {
	if (index === "?" || worktree === "?") return "?";
	const flags = `${index}${worktree}`;
	if (flags.includes("D")) return "D";
	if (flags.includes("R")) return "R";
	if (flags.includes("A")) return "A";
	return "M";
}

/** Parse `git status --short --untracked-files=all` into a file list. */
export function parsePorcelain(status: string | null): FileChange[] {
	if (!status) return [];
	const files: FileChange[] = [];
	for (const raw of status.split("\n")) {
		if (raw.length < 3) continue;
		const index = raw[0] ?? " ";
		const worktree = raw[1] ?? " ";
		let path = raw.slice(3).trim();
		// Renames render as "old -> new"; keep the destination.
		const arrow = path.indexOf(" -> ");
		if (arrow !== -1) path = path.slice(arrow + 4);
		// Strip git's quoting of paths with special characters.
		if (path.startsWith('"') && path.endsWith('"')) path = path.slice(1, -1);
		if (!path) continue;
		const code = bucket(index, worktree);
		files.push({ path, code, label: CODE_LABEL[code] });
	}
	return files;
}

/** Pull the totals from the trailing summary line of `git diff --stat`. */
export function parseStat(stat: string | null): {
	insertions: number;
	deletions: number;
} {
	if (!stat) return { insertions: 0, deletions: 0 };
	const ins = /(\d+) insertions?\(\+\)/.exec(stat);
	const del = /(\d+) deletions?\(-\)/.exec(stat);
	return {
		insertions: ins ? Number(ins[1]) : 0,
		deletions: del ? Number(del[1]) : 0,
	};
}

export function foldEntry(entry: BundleEntry): WorktreeDiff {
	const files = parsePorcelain(entry.status);
	const { insertions, deletions } = parseStat(entry.stat);
	return {
		worktreePath: entry.worktreePath,
		branch: entry.branch?.trim() || null,
		files,
		insertions,
		deletions,
		error: entry.error,
		clean: files.length === 0 && insertions === 0 && deletions === 0,
	};
}

/** Index a bundle array by worktreePath for O(1) card lookup. */
export function indexBundle(bundle: BundleEntry[]): Map<string, WorktreeDiff> {
	const map = new Map<string, WorktreeDiff>();
	for (const entry of bundle) {
		map.set(entry.worktreePath, foldEntry(entry));
	}
	return map;
}
