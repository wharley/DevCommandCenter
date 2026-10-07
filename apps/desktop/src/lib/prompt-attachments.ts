export type PromptPart = { kind: "text"; value: string } | { kind: "attachment"; path: string };

/**
 * Composer attachments are serialized into the prompt as `@/absolute/path`.
 * Splits a sent prompt back into text and attachments, for timeline chips and
 * for putting a prompt back into the composer as badges.
 */
export function promptParts(content: string): PromptPart[] {
	const parts: PromptPart[] = [];
	// Paths can contain spaces, so consume through the filename extension instead
	// of stopping at the first whitespace before the rest of the prompt.
	const attachmentPattern =
		/@((?:\/|[A-Za-z]:[\\/]|\\\\)[^\r\n<>]*?\.[a-z\d]{1,12})(?=$|[\s),.;!?])/gi;
	let cursor = 0;
	for (const match of content.matchAll(attachmentPattern)) {
		const path = match[1];
		const start = match.index ?? 0;
		if (start > cursor) parts.push({ kind: "text", value: content.slice(cursor, start) });
		parts.push({ kind: "attachment", path });
		cursor = start + 1 + path.length;
	}
	if (cursor < content.length) parts.push({ kind: "text", value: content.slice(cursor) });
	return parts.length ? parts : [{ kind: "text", value: content }];
}
