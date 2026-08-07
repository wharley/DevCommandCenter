import type { SelectedLineRange } from "@pierre/diffs";
import type {
	DiffAnnotationPayload,
	DiffMachineAnnotation,
} from "./diff-types";

export type WorkspaceDiffAnnotationGroup = {
	side: "deletions" | "additions";
	lineNumber: number;
	annotations: DiffMachineAnnotation[];
};

function snippetFromLines(text: string, startLine: number, endLine: number): string {
	return text
		.split("\n")
		.slice(Math.max(0, startLine - 1), Math.max(startLine, endLine))
		.join("\n");
}

export function workspaceDiffContentHash(value: string): number {
	let hash = 2166136261;
	for (let index = 0; index < value.length; index += 1) {
		hash ^= value.charCodeAt(index);
		hash = Math.imul(hash, 16777619);
	}
	return hash >>> 0;
}

export function annotationPayloadFromPierreRange(input: {
	range: SelectedLineRange;
	originalText: string;
	modifiedText: string;
	anchor: DiffAnnotationPayload["anchor"];
}): DiffAnnotationPayload {
	const { range } = input;
	const startSide = range.side ?? "additions";
	const endSide = range.endSide ?? startSide;
	// A selection that crosses the old/new columns cannot be represented by the
	// DCC annotation contract. Anchor it to the side where the gesture ended.
	const side = startSide === endSide ? startSide : endSide;
	const rawStart = startSide === side ? range.start : range.end;
	const rawEnd = endSide === side ? range.end : range.start;
	const startLine = Math.max(1, Math.min(rawStart, rawEnd));
	const endLine = Math.max(startLine, Math.max(rawStart, rawEnd));
	const sourceText = side === "deletions" ? input.originalText : input.modifiedText;

	return {
		side: side === "deletions" ? "original" : "modified",
		startLine,
		endLine,
		snippet: snippetFromLines(sourceText, startLine, endLine),
		anchor: input.anchor,
	};
}

export function groupWorkspaceDiffAnnotations(
	annotations: DiffMachineAnnotation[],
): WorkspaceDiffAnnotationGroup[] {
	const grouped = new Map<string, WorkspaceDiffAnnotationGroup>();
	for (const annotation of annotations) {
		const side = annotation.side === "original" ? "deletions" : "additions";
		const lineNumber = Math.max(1, annotation.endLine);
		const key = `${side}:${lineNumber}`;
		const existing = grouped.get(key);
		if (existing) {
			existing.annotations.push(annotation);
		} else {
			grouped.set(key, { side, lineNumber, annotations: [annotation] });
		}
	}
	return [...grouped.values()];
}

export function workspaceDiffAnnotationCss(
	annotations: DiffMachineAnnotation[],
): string {
	const originalLines = new Set<number>();
	const modifiedLines = new Set<number>();
	for (const annotation of annotations) {
		const destination =
			annotation.side === "original" ? originalLines : modifiedLines;
		const start = Math.max(1, Math.min(annotation.startLine, annotation.endLine));
		const end = Math.max(start, Math.max(annotation.startLine, annotation.endLine));
		// Defensive cap: a malformed external finding must not create megabytes of CSS.
		for (let line = start; line <= end && line < start + 500; line += 1) {
			destination.add(line);
		}
	}

	const selectors = (side: "deletions" | "additions", lines: Set<number>) =>
		[...lines]
			.flatMap((line) => [
				`[data-${side}] [data-line="${line}"]::after`,
				side === "deletions"
					? `[data-diff-type="unified"] [data-line-type="change-deletion"][data-line="${line}"]::after`
					: `[data-diff-type="unified"] [data-line="${line}"]:not([data-line-type="change-deletion"])::after`,
			])
			.join(",\n");
	const originalSelectors = selectors("deletions", originalLines);
	const modifiedSelectors = selectors("additions", modifiedLines);
	return [
		originalSelectors
			? `${originalSelectors} { box-shadow: inset 3px 0 0 var(--diffs-deletion-base); filter: brightness(.96); }`
			: "",
		modifiedSelectors
			? `${modifiedSelectors} { box-shadow: inset 3px 0 0 var(--diffs-modified-base); filter: brightness(.96); }`
			: "",
	]
		.filter(Boolean)
		.join("\n");
}
