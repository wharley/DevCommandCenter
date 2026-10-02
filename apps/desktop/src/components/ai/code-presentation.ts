import { createContext } from "react";

/** How code blocks behave inside one surface (a user prompt, an assistant reply). */
export type CodeBlockPresentation = {
	/** Blocks taller than this start collapsed, and only these lines are in the DOM. */
	collapsedLines?: number;
	/** Re-indent single-line JSON for reading. The copied text is the re-indented one. */
	formatJson?: boolean;
	download?: boolean;
};

export const CodeBlockPresentationContext = createContext<CodeBlockPresentation>({
	download: true,
});

/** A prompt is something the person wrote: keep pasted payloads short and readable. */
export const USER_PROMPT_CODE_PRESENTATION: CodeBlockPresentation = {
	collapsedLines: 16,
	formatJson: true,
	download: false,
};

/** Parsing is synchronous, so anything larger stays as the person pasted it. */
const JSON_MAX_CHARS = 200_000;
const JSON_FENCE_MAX_CANDIDATES = 24;
const PLAIN_LANGUAGES = new Set(["", "text", "txt", "plain", "plaintext"]);

function isJson(code: string): boolean {
	const trimmed = code.trim();
	if (trimmed.length < 2 || trimmed.length > JSON_MAX_CHARS) return false;
	const first = trimmed[0];
	const last = trimmed[trimmed.length - 1];
	if (!((first === "{" && last === "}") || (first === "[" && last === "]"))) return false;
	try {
		JSON.parse(trimmed);
		return true;
	} catch {
		return false;
	}
}

/**
 * The language to highlight with, or null for plain text. A fence without a
 * language that holds valid JSON is read as JSON when `detect` is on.
 */
export function resolveCodeLanguage(
	language: string | undefined,
	code: string,
	detect = true,
): string | null {
	const normalized = (language ?? "").trim().toLowerCase();
	if (!PLAIN_LANGUAGES.has(normalized)) return normalized;
	return detect && isJson(code) ? "json" : null;
}

/**
 * Re-indents valid single-line JSON. It moves whitespace only and never
 * re-serializes, so big numbers, key order and duplicate keys stay as written.
 */
export function formatJsonForDisplay(code: string): string {
	const source = code.trim();
	if (source.includes("\n") || !isJson(source)) return code;
	const out: string[] = [];
	let depth = 0;
	const newline = () => out.push("\n", "  ".repeat(depth));
	for (let index = 0; index < source.length; index += 1) {
		const char = source[index]!;
		if (char === '"') {
			const start = index;
			index += 1;
			while (index < source.length && source[index] !== '"') {
				index += source[index] === "\\" ? 2 : 1;
			}
			out.push(source.slice(start, index + 1));
		} else if (char === "{" || char === "[") {
			let next = index + 1;
			while (next < source.length && /\s/.test(source[next]!)) next += 1;
			if (source[next] === "}" || source[next] === "]") {
				out.push(char, source[next]!);
				index = next;
			} else {
				out.push(char);
				depth += 1;
				newline();
			}
		} else if (char === "}" || char === "]") {
			depth -= 1;
			newline();
			out.push(char);
		} else if (char === ",") {
			out.push(char);
			newline();
		} else if (char === ":") {
			out.push(": ");
		} else if (!/\s/.test(char)) {
			out.push(char);
		}
	}
	return out.join("");
}

/** Last line of the JSON value that starts on `lines[start]`, or -1. */
function jsonBlockEnd(lines: string[], start: number): number {
	let depth = 0;
	let chars = 0;
	for (let lineIndex = start; lineIndex < lines.length; lineIndex += 1) {
		const line = lines[lineIndex]!;
		chars += line.length + 1;
		if (chars > JSON_MAX_CHARS) return -1;
		let inString = false;
		for (let index = 0; index < line.length; index += 1) {
			const char = line[index]!;
			if (inString) {
				if (char === "\\") index += 1;
				else if (char === '"') inString = false;
			} else if (char === '"') {
				inString = true;
			} else if (char === "{" || char === "[") {
				depth += 1;
			} else if (char === "}" || char === "]") {
				depth -= 1;
				if (depth === 0) {
					if (line.slice(index + 1).trim()) return -1;
					const candidate = lines.slice(start, lineIndex + 1).join("\n");
					// `[1]` or `[x]` on its own line is far more likely prose than a payload.
					const structured = lineIndex > start || /[{[][^]*[{[:]/.test(candidate.trim());
					return structured && isJson(candidate) ? lineIndex : -1;
				}
			}
		}
		// JSON strings cannot span lines.
		if (inString) return -1;
	}
	return -1;
}

/**
 * People paste raw JSON into the composer without a fence, and markdown then
 * folds it into one paragraph. This wraps such payloads in a ```json fence
 * for display. Text already inside a fence is left alone.
 */
export function fenceBareJson(markdown: string): string {
	if (markdown.length > JSON_MAX_CHARS || !/^[ \t]*[{[]/m.test(markdown)) return markdown;
	const lines = markdown.split("\n");
	const out: string[] = [];
	let openFence: string | null = null;
	let candidates = 0;
	let changed = false;
	for (let index = 0; index < lines.length; index += 1) {
		const line = lines[index]!;
		const fence = /^ {0,3}(`{3,}|~{3,})/.exec(line)?.[1];
		if (openFence) {
			if (fence && fence[0] === openFence[0] && fence.length >= openFence.length && line.trim() === fence) {
				openFence = null;
			}
			out.push(line);
			continue;
		}
		if (fence) {
			openFence = fence;
			out.push(line);
			continue;
		}
		if (candidates < JSON_FENCE_MAX_CANDIDATES && /^[ \t]*[{[]/.test(line)) {
			candidates += 1;
			const end = jsonBlockEnd(lines, index);
			if (end >= 0) {
				out.push("", "```json", ...lines.slice(index, end + 1), "```", "");
				index = end;
				changed = true;
				continue;
			}
		}
		out.push(line);
	}
	return changed ? out.join("\n").trim() : markdown;
}

/** Fence info string for a file's code: its extension, which highlighters accept as an alias. */
export function fenceLanguageForPath(path: string): string {
	return /\.([a-z0-9]{1,10})$/i.exec(path)?.[1]?.toLowerCase() ?? "";
}

/** `startLine=12` in a fence's info string: the file line the snippet starts at. */
export function fenceStartLine(meta: string | undefined): number | undefined {
	const value = Number.parseInt(/\bstartLine=(\d+)/.exec(meta ?? "")?.[1] ?? "", 10);
	return value >= 1 ? value : undefined;
}
