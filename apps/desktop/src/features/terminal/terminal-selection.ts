/** Return a bounded terminal selection without trimming meaningful whitespace. */
export function limitTerminalSelection(selection: string, maxChars: number) {
	if (!selection.trim() || maxChars <= 0) {
		return "";
	}
	return selection.length > maxChars ? selection.slice(-maxChars) : selection;
}

export function resolveTerminalAgentContent(
	selection: string,
	recentOutput: string,
	maxChars: number,
) {
	const boundedSelection = limitTerminalSelection(selection, maxChars);
	return {
		content: boundedSelection || recentOutput,
		selectionOnly: boundedSelection.length > 0,
	};
}

/** Escape the wrapper terminator and keep the final serialized payload bounded. */
export function sanitizeAndBoundTerminalOutput(
	content: string,
	maxChars: number,
) {
	const escaped = content.replaceAll(
		"</terminal_output>",
		"&lt;/terminal_output&gt;",
	);
	if (maxChars <= 0) {
		return { content: "", truncated: escaped.length > 0 };
	}

	// Keep the same tail-preserving behavior as terminal selection. The wrapper
	// receives the most recent output when escaping pushes it over the budget.
	let bounded = escaped.slice(-maxChars);
	// Do not leave a split UTF-16 surrogate at the payload boundary.
	if (bounded.length > 0) {
		const firstCodeUnit = bounded.charCodeAt(0);
		if (firstCodeUnit >= 0xdc00 && firstCodeUnit <= 0xdfff) {
			bounded = bounded.slice(1);
		}
		const lastCodeUnit = bounded.charCodeAt(bounded.length - 1);
		if (lastCodeUnit >= 0xd800 && lastCodeUnit <= 0xdbff) {
			bounded = bounded.slice(0, -1);
		}
	}
	return {
		content: bounded,
		truncated: bounded.length < escaped.length,
	};
}
