/**
 * One file's change for the phone: `GET /api/v1/mobile/workspaces/:id/patch`
 * gives the git patch against HEAD; `…/file` gives the file itself.
 */
export type WorkspacePatch = { path: string; patch: string; truncated: boolean };
export type WorkspaceFile = { path: string; text: string; truncated: boolean; binary: boolean; size: number };

export type PatchLine =
	| { kind: "hunk"; text: string }
	| { kind: "add" | "del" | "context"; text: string; oldLine: number | null; newLine: number | null }
	| { kind: "note"; text: string };

/**
 * The lines worth showing from a unified diff: hunk headers, then each line
 * with its old and new numbers. File headers are dropped; "\ No newline"
 * and binary notices become notes.
 */
export function parsePatch(patch: string): PatchLine[] {
	const out: PatchLine[] = [];
	let oldLine = 0;
	let newLine = 0;
	let inHunk = false;
	for (const raw of patch.split("\n")) {
		const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)$/.exec(raw);
		if (hunk) {
			oldLine = Number(hunk[1]);
			newLine = Number(hunk[2]);
			inHunk = true;
			out.push({ kind: "hunk", text: raw });
			continue;
		}
		if (!inHunk) {
			if (/^Binary files /.test(raw)) out.push({ kind: "note", text: "Arquivo binário" });
			continue;
		}
		if (raw.startsWith("+")) {
			out.push({ kind: "add", text: raw.slice(1), oldLine: null, newLine: newLine++ });
		} else if (raw.startsWith("-")) {
			out.push({ kind: "del", text: raw.slice(1), oldLine: oldLine++, newLine: null });
		} else if (raw.startsWith(" ")) {
			out.push({ kind: "context", text: raw.slice(1), oldLine: oldLine++, newLine: newLine++ });
		} else if (raw.startsWith("\\")) {
			out.push({ kind: "note", text: "Sem quebra de linha no fim do arquivo" });
		} else if (raw.startsWith("diff --git")) {
			inHunk = false;
		}
	}
	return out;
}

/** Five blocks for a file's +/− at a glance, like GitHub's diffstat. */
export function diffBlocks(additions: number, deletions: number): Array<"add" | "del" | "none"> {
	const total = additions + deletions;
	if (total === 0) return ["none", "none", "none", "none", "none"];
	let add = Math.round((additions / total) * 5);
	let del = 5 - add;
	// A side that changed always shows, even when the other dwarfs it.
	if (additions > 0 && add === 0) [add, del] = [1, 4];
	if (deletions > 0 && del === 0) [add, del] = [4, 1];
	return [...Array(add).fill("add"), ...Array(del).fill("del")];
}
