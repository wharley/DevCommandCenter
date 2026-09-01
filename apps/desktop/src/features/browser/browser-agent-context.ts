import type { BrowserAgentContext } from "./browser-api";

const BROWSER_PAGE_TAG = "browser_page";
export const MAX_FORMATTED_BROWSER_CONTEXT_CHARS = 15_000;

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

function escapeBrowserAgentContextWithin(value: string, maxChars: number) {
	let escaped = "";
	let truncated = false;
	for (const character of value) {
		const next = character === "<" ? "&lt;" : character;
		if (escaped.length + next.length > maxChars) {
			truncated = true;
			break;
		}
		escaped += next;
	}
	return { escaped, truncated };
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
		semanticMap: string;
		mapId: string;
		generation: string;
		visibleElements: string;
		name: string;
		destination: string;
		states: string;
		noVisibleElements: string;
	},
) {
	const source = context.selectionOnly ? labels.selection : labels.visibleText;
	const semanticItems = context.semanticMap.items.map((item) => {
		const states = [
			item.disabled === undefined ? null : `disabled=${item.disabled}`,
			item.checked === undefined ? null : `checked=${item.checked}`,
			item.selected === undefined ? null : `selected=${item.selected}`,
			item.expanded === undefined ? null : `expanded=${item.expanded}`,
			item.pressed === undefined ? null : `pressed=${item.pressed}`,
		].filter(Boolean).join(", ");
		return [
			`- [${escapeBrowserAgentContext(item.reference)}] ${escapeBrowserAgentContext(item.role)}${item.level ? ` level=${item.level}` : ""}`,
			`${labels.name}: ${escapeBrowserAgentContext(item.name)}`,
			item.destination
				? `${labels.destination}: ${escapeBrowserAgentContext(item.destination)}`
				: null,
			states ? `${labels.states}: ${states}` : null,
		].filter(Boolean).join(" | ");
	});
	let includedItems = semanticItems;
	let mapTruncated = context.semanticMap.truncated;
	let textForEnvelope = escapeBrowserAgentContext(context.text);
	let frontendTruncated = false;
	const buildEnvelope = () => [
		labels.prompt,
		"",
		`<${BROWSER_PAGE_TAG}>`,
		`${labels.url}: ${escapeBrowserAgentContext(context.url)}`,
		`${labels.title}: ${escapeBrowserAgentContext(context.title ?? "")}`,
		`${labels.source}: ${source}`,
		`${labels.truncated}: ${context.truncated || mapTruncated || frontendTruncated ? labels.yes : labels.no}`,
		"---",
		textForEnvelope,
		`</${BROWSER_PAGE_TAG}>`,
		"",
		"<browser_semantic_map>",
		`${labels.semanticMap}: ${mapTruncated ? labels.yes : labels.no}`,
		`${labels.mapId}: ${escapeBrowserAgentContext(context.semanticMap.mapId)}`,
		`${labels.generation}: ${context.semanticMap.generation}`,
		`${labels.visibleElements}:`,
		...(includedItems.length > 0 ? includedItems : [`- ${labels.noVisibleElements}`]),
		"</browser_semantic_map>",
	].join("\n");

	let envelope = buildEnvelope();
	while (envelope.length > MAX_FORMATTED_BROWSER_CONTEXT_CHARS && includedItems.length > 0) {
		includedItems = includedItems.slice(0, -1);
		mapTruncated = true;
		envelope = buildEnvelope();
	}
	if (envelope.length > MAX_FORMATTED_BROWSER_CONTEXT_CHARS) {
		const textBeforeLimit = textForEnvelope;
		textForEnvelope = "";
		frontendTruncated = true;
		const overhead = buildEnvelope().length;
		const bounded = escapeBrowserAgentContextWithin(
			context.text,
			Math.max(0, MAX_FORMATTED_BROWSER_CONTEXT_CHARS - overhead),
		);
		textForEnvelope = bounded.escaped;
		frontendTruncated = frontendTruncated || bounded.truncated || textBeforeLimit !== bounded.escaped;
		envelope = buildEnvelope();
	}
	return envelope;
}
