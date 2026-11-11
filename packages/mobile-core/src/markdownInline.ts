/**
 * Inline markdown the phone draws in an answer's line: links (`[label](url)`
 * and bare http(s) addresses) besides what RichText already knew, and the
 * plain one-line text of a message for previews (a pass, a wake), with the
 * marks gone and each link read by its label.
 */

export type InlinePiece =
	| { kind: "text"; text: string }
	| { kind: "code"; text: string }
	| { kind: "bold"; text: string }
	| { kind: "link"; text: string; url: string };

/** Only addresses a phone may open from an answer. */
export function safeUrl(url: string): string | null {
	const trimmed = url.trim();
	return /^(https?:\/\/|mailto:)/i.test(trimmed) ? trimmed : null;
}

/** "app.zedy.com.br/admin/27885/…" for a bare address shown in the text. */
export function shortUrl(url: string, max = 36): string {
	const bare = url.replace(/^https?:\/\//i, "").replace(/\/$/, "");
	return bare.length > max ? `${bare.slice(0, max - 1)}…` : bare;
}

const TOKEN = /(`[^`\n]+`|\*\*[^*\n]+\*\*|\[[^\]\n]+\]\([^)\s]+\)|https?:\/\/[^\s)<>\]]+)/g;

/** A line of an answer, split into what RichText draws differently. */
export function inlinePieces(line: string): InlinePiece[] {
	const pieces: InlinePiece[] = [];
	let last = 0;
	for (const match of line.matchAll(TOKEN)) {
		const token = match[0];
		const at = match.index ?? 0;
		if (at > last) pieces.push({ kind: "text", text: line.slice(last, at) });
		last = at + token.length;
		if (token.startsWith("`")) {
			pieces.push({ kind: "code", text: token.slice(1, -1) });
		} else if (token.startsWith("**")) {
			pieces.push({ kind: "bold", text: token.slice(2, -2) });
		} else if (token.startsWith("[")) {
			const split = token.indexOf("](");
			const label = token.slice(1, split);
			const url = safeUrl(token.slice(split + 2, -1));
			pieces.push(url ? { kind: "link", text: label, url } : { kind: "text", text: label });
		} else {
			// A bare address: a trailing period or comma belongs to the sentence.
			const clean = token.replace(/[.,;:!?]+$/, "");
			const url = safeUrl(clean);
			pieces.push(url ? { kind: "link", text: shortUrl(clean), url } : { kind: "text", text: clean });
			if (clean.length < token.length) pieces.push({ kind: "text", text: token.slice(clean.length) });
		}
	}
	if (last < line.length) pieces.push({ kind: "text", text: line.slice(last) });
	return pieces;
}

/** A message as one plain line: no marks, links by their label, spaces folded. */
export function plainMarkdown(text: string): string {
	return text
		.replace(/\[([^\]\n]+)\]\([^)\s]+\)/g, "$1")
		.replace(/\*\*([^*\n]+)\*\*/g, "$1")
		.replace(/`([^`\n]+)`/g, "$1")
		.replace(/https?:\/\/[^\s)<>\]]+/g, (url) => shortUrl(url))
		.replace(/\s+/g, " ")
		.trim();
}
