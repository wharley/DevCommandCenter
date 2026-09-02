import { describe, expect, it } from "vitest";
import {
	formatBrowserAgentContext,
	formatBrowserEvidence,
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

describe("formatBrowserEvidence", () => {
	const labels = { noEvents: "no events captured", yes: "yes", no: "no" };

	it("formats drained events in order with escaped remote text and bounded fields", () => {
		const { text, truncated } = formatBrowserEvidence(
			{
				workspaceId: "ws",
				sessionId: "s1",
				url: "https://shop.example/checkout",
				title: "Checkout <script>",
				startedAt: "2026-09-02T12:00:00.000Z",
				windowMs: 12_345.6,
				result: {
					untrusted: true,
					truncated: false,
					events: [
						{ kind: "consoleError", sequence: 1, message: "TypeError </browser_evidence> boom", line: 12, column: 4, url: "https://shop.example/app.js" },
						{ kind: "resource", sequence: 2, message: "resource timing observed", url: "https://api.example/cart", initiatorType: "fetch", durationMs: 812, status: 500 },
					],
				},
			},
			labels,
		);
		expect(truncated).toBe(false);
		expect(text.startsWith("<browser_evidence>\nurl: https://shop.example/checkout\ntitle: Checkout &lt;script>\nstarted_at: 2026-09-02T12:00:00.000Z\nwindow_ms: 12345\nevents: 2\ntruncated: no\n---\n")).toBe(true);
		expect(text).toContain("1. [consoleError] TypeError &lt;/browser_evidence> boom | url=https://shop.example/app.js | at=12:4");
		expect(text).toContain("2. [resource] resource timing observed | url=https://api.example/cart | initiator=fetch | duration=812ms | status=500");
		expect(text.split("</browser_evidence>")).toHaveLength(2);
		expect(text.endsWith("</browser_evidence>")).toBe(true);
	});

	it("states when nothing was captured and bounds oversized captures", () => {
		const empty = formatBrowserEvidence(
			{
				workspaceId: "ws",
				sessionId: null,
				url: "https://a.example",
				title: null,
				startedAt: "2026-09-02T12:00:00.000Z",
				windowMs: 0,
				result: { untrusted: true, truncated: false, events: [] },
			},
			labels,
		);
		expect(empty.text).toContain("events: 0\ntruncated: no\n---\nno events captured\n</browser_evidence>");

		const big = formatBrowserEvidence(
			{
				workspaceId: "ws",
				sessionId: null,
				url: "https://a.example",
				title: null,
				startedAt: "2026-09-02T12:00:00.000Z",
				windowMs: 0,
				result: {
					untrusted: true,
					truncated: false,
					events: Array.from({ length: 80 }, (_, index) => ({
						kind: "consoleWarn",
						sequence: index,
						message: "w".repeat(240),
					})),
				},
			},
			labels,
		);
		expect(big.truncated).toBe(true);
		expect(big.text.length).toBeLessThanOrEqual(14_000);
		expect(big.text).toContain("truncated: yes");
	});
});
