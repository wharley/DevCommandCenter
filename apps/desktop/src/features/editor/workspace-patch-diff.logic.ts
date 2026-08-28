import { parsePatchFiles, type FileDiffMetadata } from "@pierre/diffs";

/** Parses exactly one file diff without logging captured source on failure. */
export function parseWorkspacePatch(patch: string): FileDiffMetadata {
	const parsedPatches = parsePatchFiles(patch);
	if (parsedPatches.length !== 1 || parsedPatches[0].files.length !== 1) {
		throw new Error("Captured turn patch must contain exactly one file diff");
	}
	return parsedPatches[0].files[0];
}
