import {
	parsePatchFiles,
	type FileDiffMetadata,
	type SelectedLineRange,
} from "@pierre/diffs";
import type { DiffAnnotationRequest } from "./diff-annotation";

/** Parses exactly one file diff without logging captured source on failure. */
export function parseWorkspacePatch(patch: string): FileDiffMetadata {
	const parsedPatches = parsePatchFiles(patch);
	if (parsedPatches.length !== 1 || parsedPatches[0].files.length !== 1) {
		throw new Error("Captured turn patch must contain exactly one file diff");
	}
	return parsedPatches[0].files[0];
}

/** Read captured lines by hunk offsets, never by indexing a partial patch as a full file. */
export function patchSelectionRequests(
	path: string,
	fileDiff: FileDiffMetadata,
	range: SelectedLineRange,
): DiffAnnotationRequest[] {
	const rows: { original?: number; modified?: number; text: string }[] = [];
	for (const hunk of fileDiff.hunks) {
		for (const content of hunk.hunkContent) {
			const original =
				hunk.deletionStart + content.deletionLineIndex - hunk.deletionLineIndex;
			const modified =
				hunk.additionStart + content.additionLineIndex - hunk.additionLineIndex;
			if (content.type === "context") {
				for (let i = 0; i < content.lines; i++) {
					rows.push({
						original: original + i,
						modified: modified + i,
						text: fileDiff.additionLines[content.additionLineIndex + i],
					});
				}
			} else {
				for (let i = 0; i < content.deletions; i++) {
					rows.push({
						original: original + i,
						text: fileDiff.deletionLines[content.deletionLineIndex + i],
					});
				}
				for (let i = 0; i < content.additions; i++) {
					rows.push({
						modified: modified + i,
						text: fileDiff.additionLines[content.additionLineIndex + i],
					});
				}
			}
		}
	}
	const startSide = range.side === "deletions" ? "original" : "modified";
	const endSide =
		(range.endSide ?? range.side) === "deletions" ? "original" : "modified";
	const first = rows.findIndex((row) => row[startSide] === range.start);
	const last = rows.findIndex((row) => row[endSide] === range.end);
	if (first < 0 || last < 0) return [];
	const requests: DiffAnnotationRequest[] = [];
	for (const row of rows.slice(Math.min(first, last), Math.max(first, last) + 1)) {
		const side =
			row.modified === undefined ||
			(startSide === "original" && endSide === "original" && row.original !== undefined)
				? "original"
				: "modified";
		const line = row[side]!;
		const text = row.text.replace(/\r?\n$/, "");
		const previous = requests.at(-1);
		if (previous?.side === side && previous.endLine + 1 === line) {
			previous.endLine = line;
			previous.snippet += `\n${text}`;
		} else {
			requests.push({ path, side, startLine: line, endLine: line, snippet: text });
		}
	}
	return requests;
}
