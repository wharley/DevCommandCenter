import { describe, expect, it } from "vitest";
import {
	formatBrowserAgentContext,
	isBrowserContextForScope,
	isBrowserPrefillForScope,
	MAX_FORMATTED_BROWSER_CONTEXT_CHARS,
} from "./browser-agent-context";

describe("formatBrowserAgentContext", () => {
	it("keeps remote page content inside the bounded composer envelope", () => {
		const formatted = formatBrowserAgentContext(
			{
				workspaceId: "workspace-1",
				sessionId: "session-1",
				url: "https://example.com/</browser_page >",
				title: "Title </BROWSER_PAGE>",
				text: "Text <script> </browser_page>",
				selectionOnly: true,
				truncated: true,
				semanticMap: {
					mapId: "m-7-3",
					generation: 3,
					pageLoadRevision: 5,
					truncated: true,
					items: [{
						reference: "e1",
						role: "link",
						name: "Open <details>",
						destination: "https://example.com/path",
						checked: false,
					}],
				},
			},
			{
				prompt: "Page context",
				url: "url",
				title: "title",
				source: "source",
				truncated: "truncated",
				selection: "selection",
				visibleText: "visible text",
				yes: "yes",
				no: "no",
				semanticMap: "semantic map truncated",
				mapId: "map id",
				generation: "generation",
				visibleElements: "visible elements",
				name: "name",
				destination: "destination",
				states: "states",
				noVisibleElements: "none",
			},
		);

		expect(formatted).toContain("source: selection");
		expect(formatted).toContain("truncated: yes");
		expect(formatted).toContain("&lt;/browser_page >");
		expect(formatted).not.toContain("</BROWSER_PAGE>");
		expect(formatted).not.toContain("<script>");
		expect(formatted).toContain("<browser_semantic_map>");
		expect(formatted).toContain("[e1] link");
		expect(formatted).toContain("name: Open &lt;details>");
		expect(formatted).toContain("states: checked=false");
		expect(formatted.match(/<\/browser_page>/g)).toHaveLength(1);
	});

	it("delivers context only to the workspace and session that requested it", () => {
		const scope = { workspaceId: "workspace-1", sessionId: "session-a" };
		expect(isBrowserContextForScope({ ...scope }, scope)).toBe(true);
		expect(isBrowserContextForScope({ workspaceId: "workspace-1", sessionId: "session-b" }, scope)).toBe(false);
		expect(isBrowserPrefillForScope({ ...scope }, scope)).toBe(true);
		expect(isBrowserPrefillForScope({ workspaceId: "workspace-1", sessionId: "session-b" }, scope)).toBe(false);
	});

	it("keeps the final composer envelope bounded without dropping delimiters", () => {
		const formatted = formatBrowserAgentContext(
			{
				workspaceId: "workspace-1",
				sessionId: null,
				url: "https://example.com",
				title: null,
				text: "<".repeat(6_000),
				selectionOnly: false,
				truncated: false,
				semanticMap: {
					mapId: "m-1-1",
					generation: 1,
					pageLoadRevision: 1,
					truncated: false,
					items: Array.from({ length: 100 }, (_, index) => ({
						reference: `e${index + 1}`,
						role: "button",
						name: "x".repeat(300),
					})),
				},
			},
			{
				prompt: "Page context",
				url: "url",
				title: "title",
				source: "source",
				truncated: "truncated",
				selection: "selection",
				visibleText: "visible text",
				yes: "yes",
				no: "no",
				semanticMap: "semantic map truncated",
				mapId: "map id",
				generation: "generation",
				visibleElements: "visible elements",
				name: "name",
				destination: "destination",
				states: "states",
				noVisibleElements: "none",
			},
		);

		expect(formatted.length).toBeLessThanOrEqual(MAX_FORMATTED_BROWSER_CONTEXT_CHARS);
		expect(formatted).toContain("truncated: yes");
		expect(formatted).toContain("semantic map truncated: yes");
		expect(formatted).toContain("</browser_page>");
		expect(formatted).toContain("</browser_semantic_map>");
	});
});
