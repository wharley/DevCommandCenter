export type MergeConflictResolution = "current" | "incoming" | "both";

export type MergeConflictHunk = {
	id: string;
	startOffset: number;
	endOffset: number;
	startLine: number;
	endLine: number;
	currentLabel: string | null;
	incomingLabel: string | null;
	currentText: string;
	baseText: string | null;
	incomingText: string;
	lineEnding: string;
};

type SourceLine = {
	start: number;
	end: number;
	body: string;
	lineEnding: string;
};

const START_MARKER = /^<{7,}(?: .*)?$/;
const BASE_MARKER = /^\|{7,}(?: .*)?$/;
const SEPARATOR_MARKER = /^={7,}$/;
const END_MARKER = /^>{7,}(?: .*)?$/;

function sourceLines(source: string): SourceLine[] {
	const lines: SourceLine[] = [];
	let start = 0;
	while (start < source.length) {
		let cursor = start;
		while (cursor < source.length && source[cursor] !== "\n" && source[cursor] !== "\r") {
			cursor += 1;
		}
		const bodyEnd = cursor;
		if (source[cursor] === "\r" && source[cursor + 1] === "\n") {
			cursor += 2;
		} else if (source[cursor] === "\r" || source[cursor] === "\n") {
			cursor += 1;
		}
		lines.push({
			start,
			end: cursor,
			body: source.slice(start, bodyEnd),
			lineEnding: source.slice(bodyEnd, cursor),
		});
		start = cursor;
	}
	return lines;
}

function markerLabel(body: string): string | null {
	const space = body.indexOf(" ");
	if (space < 0) return null;
	const label = body.slice(space + 1).trim();
	return label || null;
}

function sliceLines(source: string, lines: SourceLine[], start: number, end: number) {
	if (start >= end) return "";
	return source.slice(lines[start]!.start, lines[end - 1]!.end);
}

export function parseMergeConflictHunks(source: string): MergeConflictHunk[] {
	const lines = sourceLines(source);
	const hunks: MergeConflictHunk[] = [];

	for (let index = 0; index < lines.length; index += 1) {
		const startLine = lines[index]!;
		if (!START_MARKER.test(startLine.body)) continue;

		const currentStart = index + 1;
		let cursor = currentStart;
		while (
			cursor < lines.length &&
			!BASE_MARKER.test(lines[cursor]!.body) &&
			!SEPARATOR_MARKER.test(lines[cursor]!.body)
		) {
			cursor += 1;
		}
		if (cursor >= lines.length) continue;

		const currentEnd = cursor;
		let baseStart: number | null = null;
		let baseEnd: number | null = null;
		if (BASE_MARKER.test(lines[cursor]!.body)) {
			baseStart = cursor + 1;
			cursor = baseStart;
			while (cursor < lines.length && !SEPARATOR_MARKER.test(lines[cursor]!.body)) {
				cursor += 1;
			}
			if (cursor >= lines.length) continue;
			baseEnd = cursor;
		}

		const incomingStart = cursor + 1;
		cursor = incomingStart;
		while (cursor < lines.length && !END_MARKER.test(lines[cursor]!.body)) {
			cursor += 1;
		}
		if (cursor >= lines.length) continue;

		const endMarker = lines[cursor]!;
		hunks.push({
			id: `${startLine.start}:${endMarker.end}`,
			startOffset: startLine.start,
			endOffset: endMarker.end,
			startLine: index + 1,
			endLine: cursor + 1,
			currentLabel: markerLabel(startLine.body),
			incomingLabel: markerLabel(endMarker.body),
			currentText: sliceLines(source, lines, currentStart, currentEnd),
			baseText:
				baseStart != null && baseEnd != null
					? sliceLines(source, lines, baseStart, baseEnd)
					: null,
			incomingText: sliceLines(source, lines, incomingStart, cursor),
			lineEnding: startLine.lineEnding || "\n",
		});
		index = cursor;
	}

	return hunks;
}

export function hasMergeConflictMarkerFragments(source: string) {
	return sourceLines(source).some(
		(line) =>
			START_MARKER.test(line.body) ||
			BASE_MARKER.test(line.body) ||
			SEPARATOR_MARKER.test(line.body) ||
			END_MARKER.test(line.body),
	);
}

function combineBoth(hunk: MergeConflictHunk) {
	if (!hunk.currentText) return hunk.incomingText;
	if (!hunk.incomingText) return hunk.currentText;
	const needsSeparator =
		!hunk.currentText.endsWith("\n") &&
		!hunk.currentText.endsWith("\r") &&
		!hunk.incomingText.startsWith("\n") &&
		!hunk.incomingText.startsWith("\r");
	return `${hunk.currentText}${needsSeparator ? hunk.lineEnding : ""}${hunk.incomingText}`;
}

export function applyMergeConflictResolution(
	source: string,
	hunk: MergeConflictHunk,
	resolution: MergeConflictResolution,
) {
	const replacement =
		resolution === "current"
			? hunk.currentText
			: resolution === "incoming"
				? hunk.incomingText
				: combineBoth(hunk);
	return `${source.slice(0, hunk.startOffset)}${replacement}${source.slice(hunk.endOffset)}`;
}
