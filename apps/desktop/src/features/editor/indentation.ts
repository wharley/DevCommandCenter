/** How a file is indented: tabs or a number of spaces, and how wide a tab is. */
export type FileIndentation = {
	useTabs: boolean;
	/** Columns per indent level; also the rendered width of a tab. */
	size: number;
};

export const DEFAULT_FILE_INDENTATION: FileIndentation = { useTabs: false, size: 2 };

type EditorConfigSection = { glob: string; props: Map<string, string> };

/** The sections of an `.editorconfig`, in file order (later ones win). */
export function parseEditorConfig(text: string): EditorConfigSection[] {
	const sections: EditorConfigSection[] = [];
	let current: EditorConfigSection | null = null;
	for (const rawLine of text.split(/\r?\n/)) {
		const line = rawLine.trim();
		if (!line || line.startsWith("#") || line.startsWith(";")) continue;
		const header = /^\[(.+)\]$/.exec(line);
		if (header) {
			current = { glob: header[1]!.trim(), props: new Map() };
			sections.push(current);
			continue;
		}
		const separator = line.indexOf("=");
		if (!current || separator === -1) continue;
		current.props.set(
			line.slice(0, separator).trim().toLowerCase(),
			line.slice(separator + 1).trim().toLowerCase(),
		);
	}
	return sections;
}

function escapeRegExp(value: string): string {
	return value.replace(/[.+^$()|\\]/g, "\\$&");
}

function globToRegExpSource(glob: string): string {
	let source = "";
	for (let index = 0; index < glob.length; index += 1) {
		const char = glob[index]!;
		if (char === "*") {
			if (glob[index + 1] === "*") {
				source += ".*";
				index += 1;
			} else {
				source += "[^/]*";
			}
		} else if (char === "?") {
			source += "[^/]";
		} else if (char === "{") {
			const close = glob.indexOf("}", index);
			if (close === -1) {
				source += "\\{";
				continue;
			}
			const options = glob.slice(index + 1, close).split(",");
			source += `(?:${options.map(globToRegExpSource).join("|")})`;
			index = close;
		} else if (char === "[") {
			const close = glob.indexOf("]", index);
			if (close === -1) {
				source += "\\[";
				continue;
			}
			const body = glob.slice(index + 1, close).replace(/^!/, "^");
			source += `[${body}]`;
			index = close;
		} else {
			source += escapeRegExp(char);
		}
	}
	return source;
}

/**
 * EditorConfig matching: a glob without `/` matches the file name at any
 * depth; one with `/` is anchored at the `.editorconfig` folder.
 */
export function matchesEditorConfigGlob(glob: string, relativePath: string): boolean {
	const anchored = glob.includes("/");
	const pattern = anchored ? glob.replace(/^\//, "") : glob;
	const source = globToRegExpSource(pattern);
	const regex = new RegExp(anchored ? `^${source}$` : `(?:^|/)${source}$`);
	return regex.test(relativePath);
}

function positiveInt(value: string | undefined): number | null {
	if (!value) return null;
	const parsed = Number.parseInt(value, 10);
	return Number.isFinite(parsed) && parsed > 0 && parsed <= 16 ? parsed : null;
}

function fromEditorConfig(
	sections: EditorConfigSection[],
	relativePath: string,
): Partial<FileIndentation> {
	const props = new Map<string, string>();
	for (const section of sections) {
		if (!matchesEditorConfigGlob(section.glob, relativePath)) continue;
		for (const [key, value] of section.props) props.set(key, value);
	}
	const result: Partial<FileIndentation> = {};
	const style = props.get("indent_style");
	if (style === "tab") result.useTabs = true;
	else if (style === "space") result.useTabs = false;
	const tabWidth = positiveInt(props.get("tab_width"));
	const rawIndentSize = props.get("indent_size");
	const indentSize = rawIndentSize === "tab" ? tabWidth : positiveInt(rawIndentSize);
	// A tab is drawn tab_width wide; spaces step by indent_size. Each falls back
	// to the other, as the EditorConfig spec says.
	const size = result.useTabs ? (tabWidth ?? indentSize) : (indentSize ?? tabWidth);
	if (size !== null) result.size = size;
	return result;
}

/** What the file itself does: tabs or spaces, and the usual step between levels. */
export function detectIndentation(content: string): Partial<FileIndentation> {
	let tabLines = 0;
	let spaceLines = 0;
	const steps = new Map<number, number>();
	let previousSpaces = 0;
	const lines = content.split(/\r?\n/, 2_000);
	for (const line of lines) {
		if (!line.trim()) continue;
		if (line.startsWith("\t")) {
			tabLines += 1;
			continue;
		}
		const spaces = /^ */.exec(line)![0].length;
		// ` * ` continues a block comment; it is not an indent level.
		if (spaces > 0 && !/^ +\*/.test(line)) spaceLines += 1;
		const step = spaces - previousSpaces;
		if (step === 2 || step === 4 || step === 8) {
			steps.set(step, (steps.get(step) ?? 0) + 1);
		}
		previousSpaces = spaces;
	}
	if (tabLines === 0 && spaceLines === 0) return {};
	if (tabLines > spaceLines) return { useTabs: true };
	let best: number | null = null;
	for (const [step, count] of steps) {
		if (best === null || count > (steps.get(best) ?? 0)) best = step;
	}
	return best === null ? { useTabs: false } : { useTabs: false, size: best };
}

/**
 * The project's rule wins (`.editorconfig`), the file fills what it leaves
 * open, and two spaces cover the rest.
 */
export function resolveFileIndentation(
	editorConfig: string | null,
	relativePath: string,
	content: string,
): FileIndentation {
	const configured = editorConfig
		? fromEditorConfig(parseEditorConfig(editorConfig), relativePath)
		: {};
	const detected = detectIndentation(content);
	const useTabs = configured.useTabs ?? detected.useTabs ?? DEFAULT_FILE_INDENTATION.useTabs;
	const size =
		configured.size ??
		(useTabs ? 4 : detected.size) ??
		DEFAULT_FILE_INDENTATION.size;
	return { useTabs, size };
}

export function indentUnit(indentation: FileIndentation): string {
	return indentation.useTabs ? "\t" : " ".repeat(indentation.size);
}

const OPENERS: Record<string, string> = { "{": "}", "(": ")", "[": "]" };

/**
 * What Enter inserts at a caret: the line's own indentation, one level more
 * after an opening bracket, and the closing bracket pushed to its own line
 * when it sits right after the caret.
 */
export function lineBreakInsertion(
	lineText: string,
	character: number,
	unit: string,
	eol: string,
): { text: string; caretLineOffset: number; caretCharacter: number } {
	const before = lineText.slice(0, character);
	const after = lineText.slice(character);
	const base = /^[ \t]*/.exec(before)![0];
	const lastChar = before.trimEnd().at(-1) ?? "";
	const closer = OPENERS[lastChar];
	if (!closer) {
		return { text: eol + base, caretLineOffset: 1, caretCharacter: base.length };
	}
	const inner = base + unit;
	if (after.trimStart().startsWith(closer)) {
		return {
			text: eol + inner + eol + base,
			caretLineOffset: 1,
			caretCharacter: inner.length,
		};
	}
	return { text: eol + inner, caretLineOffset: 1, caretCharacter: inner.length };
}
