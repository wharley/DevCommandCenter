import type { BrowserAgentContext } from "./browser-api";

const BROWSER_PAGE_TAG = "browser_page";

export type BrowserScope = {
	workspaceId: string;
	sessionId: string | null;
};

export function isBrowserContextForScope(
	context: Pick<BrowserAgentContext, "workspaceId" | "sessionId">,
	scope: BrowserScope,
) {
	return (
		context.workspaceId === scope.workspaceId &&
		context.sessionId === scope.sessionId
	);
}

export function isBrowserPrefillForScope(
	prefill: BrowserScope | null,
	scope: BrowserScope,
) {
	return prefill !== null && isBrowserContextForScope(prefill, scope);
}

/** Keeps remote page text inside the composer context envelope. */
export function escapeBrowserAgentContext(value: string) {
	return value.replaceAll("<", "&lt;");
}

export function formatBrowserAgentContext(
	context: BrowserAgentContext,
	labels: {
		prompt: string;
		url: string;
		title: string;
		source: string;
		truncated: string;
		selection: string;
		visibleText: string;
		yes: string;
		no: string;
	},
) {
	const source = context.selectionOnly ? labels.selection : labels.visibleText;
	return [
		labels.prompt,
		"",
		`<${BROWSER_PAGE_TAG}>`,
		`${labels.url}: ${escapeBrowserAgentContext(context.url)}`,
		`${labels.title}: ${escapeBrowserAgentContext(context.title ?? "")}`,
		`${labels.source}: ${source}`,
		`${labels.truncated}: ${context.truncated ? labels.yes : labels.no}`,
		"---",
		escapeBrowserAgentContext(context.text),
		`</${BROWSER_PAGE_TAG}>`,
	].join("\n");
}
