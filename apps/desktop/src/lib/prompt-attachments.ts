export type PromptPart =
	| { kind: "text"; value: string }
	| { kind: "attachment"; path: string }
	| { kind: "mention"; path: string };

// Workspace files mentioned with `@` are serialized as `@relative/path.ext`.
// An extension is required, so `@someone` or `@types/node` stay plain text.
const MENTION_PATTERN =
	/(^|[\s(])@((?![/\\])[^\s@<>`"']*?[^\s@<>`"'/.]\.[a-z\d]{1,12})(?=$|[\s)]|[,.;:!?](?:$|\s))/gi;

/** Ranges of fenced code blocks and inline code spans, where `@` is literal. */
function codeRanges(content: string): Array<[number, number]> {
	const ranges: Array<[number, number]> = [];
	const fence = /^ {0,3}(`{3,}|~{3,})[^\n]*\n[\s\S]*?(?:\n {0,3}\1[`~]*[ \t]*(?=\n|$)|(?![\s\S]))/gm;
	for (const match of content.matchAll(fence)) {
		const start = match.index ?? 0;
		ranges.push([start, start + match[0].length]);
	}
	const inline = /(`+)[^`\n][\s\S]*?\1/g;
	for (const match of content.matchAll(inline)) {
		const start = match.index ?? 0;
		if (!ranges.some(([from, to]) => start >= from && start < to)) {
			ranges.push([start, start + match[0].length]);
		}
	}
	return ranges;
}

function workspaceMentions(content: string) {
	const ranges = codeRanges(content);
	const mentions: Array<{ start: number; end: number; path: string }> = [];
	for (const match of content.matchAll(MENTION_PATTERN)) {
		const start = (match.index ?? 0) + match[1].length;
		if (ranges.some(([from, to]) => start >= from && start < to)) continue;
		mentions.push({ start, end: start + 1 + match[2].length, path: match[2] });
	}
	return mentions;
}

/**
 * Composer attachments are serialized into the prompt as `@/absolute/path`,
 * workspace file mentions as `@relative/path`. Splits a sent prompt back into
 * text, attachments and mentions, for the timeline and for putting a prompt
 * back into the composer as badges.
 */
export function promptParts(content: string): PromptPart[] {
	const tokens: Array<{ start: number; end: number; part: PromptPart }> = [];
	// Paths can contain spaces, so consume through the filename extension instead
	// of stopping at the first whitespace before the rest of the prompt.
	const attachmentPattern =
		/@((?:\/|[A-Za-z]:[\\/]|\\\\)[^\r\n<>]*?\.[a-z\d]{1,12})(?=$|[\s)]|[,.;!?](?:$|\s))/gi;
	for (const match of content.matchAll(attachmentPattern)) {
		const start = match.index ?? 0;
		const path = match[1];
		tokens.push({ start, end: start + 1 + path.length, part: { kind: "attachment", path } });
	}
	for (const { start, end, path } of workspaceMentions(content)) {
		if (tokens.some((token) => start < token.end && end > token.start)) continue;
		tokens.push({ start, end, part: { kind: "mention", path } });
	}
	tokens.sort((a, b) => a.start - b.start);

	const parts: PromptPart[] = [];
	let cursor = 0;
	for (const { start, end, part } of tokens) {
		if (start > cursor) parts.push({ kind: "text", value: content.slice(cursor, start) });
		parts.push(part);
		cursor = end;
	}
	if (cursor < content.length) parts.push({ kind: "text", value: content.slice(cursor) });
	return parts.length ? parts : [{ kind: "text", value: content }];
}
