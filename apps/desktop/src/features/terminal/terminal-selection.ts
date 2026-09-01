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
