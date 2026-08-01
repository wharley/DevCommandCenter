export type UnifiedDiffLineKind =
	| "addition"
	| "deletion"
	| "context"
	| "hunk"
	| "meta";

export type UnifiedDiffLine = {
	id: string;
	kind: UnifiedDiffLineKind;
	content: string;
	oldLine: number | null;
	newLine: number | null;
	reviewLine: number | null;
	reviewSide: "left" | "right" | null;
};

const HUNK_HEADER = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/;

export function parseUnifiedDiff(patch: string | null | undefined): UnifiedDiffLine[] {
	if (!patch) return [];
	let oldLine = 0;
	let newLine = 0;
	let hunkIndex = -1;
	return patch.split("\n").map((raw, index) => {
		const hunk = raw.match(HUNK_HEADER);
		if (hunk) {
			oldLine = Number(hunk[1]);
			newLine = Number(hunk[3]);
			hunkIndex += 1;
			return {
				id: `hunk-${hunkIndex}-${index}`,
				kind: "hunk",
				content: raw,
				oldLine: null,
				newLine: null,
				reviewLine: null,
				reviewSide: null,
			};
		}
		if (raw.startsWith("+") && !raw.startsWith("+++")) {
			const line = newLine++;
			return {
				id: `right-${line}-${index}`,
				kind: "addition",
				content: raw.slice(1),
				oldLine: null,
				newLine: line,
				reviewLine: line,
				reviewSide: "right",
			};
		}
		if (raw.startsWith("-") && !raw.startsWith("---")) {
			const line = oldLine++;
			return {
				id: `left-${line}-${index}`,
				kind: "deletion",
				content: raw.slice(1),
				oldLine: line,
				newLine: null,
				reviewLine: line,
				reviewSide: "left",
			};
		}
		if (raw.startsWith(" ")) {
			const previous = oldLine++;
			const current = newLine++;
			return {
				id: `context-${previous}-${current}-${index}`,
				kind: "context",
				content: raw.slice(1),
				oldLine: previous,
				newLine: current,
				reviewLine: current,
				reviewSide: "right",
			};
		}
		return {
			id: `meta-${index}`,
			kind: "meta",
			content: raw,
			oldLine: null,
			newLine: null,
			reviewLine: null,
			reviewSide: null,
		};
	});
}
