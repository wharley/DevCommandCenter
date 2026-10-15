import { describe, expect, it } from "vitest";
import {
	getStreamdownLinkKind,
	isCodeFileReference,
	isLocalFileHref,
	unwrapFileLinkTargets,
} from "./streamdown-link-presentation";

describe("getStreamdownLinkKind", () => {
	it("distinguishes web, local and default links", () => {
		expect(getStreamdownLinkKind("http://example.com", null)).toBe("external");
		expect(getStreamdownLinkKind("https://example.com", null)).toBe("external");
		expect(getStreamdownLinkKind("file:///tmp/notes.md", null)).toBe("file");
		expect(getStreamdownLinkKind("/tmp/main.ts:14:2", null)).toBe("file");
		expect(getStreamdownLinkKind("C:/work/main.ts", null)).toBe("file");
		expect(getStreamdownLinkKind("C:\\work\\main.ts", null)).toBe("file");
		expect(getStreamdownLinkKind("#section", null)).toBe("default");
	});

	it("prioritizes workspace file references", () => {
		expect(
			getStreamdownLinkKind("src/components/App.tsx", {
				path: "src/components/App.tsx",
				line: null,
				column: null,
			}),
		).toBe("workspace-file");
	});
});

describe("isLocalFileHref", () => {
	it("accepts Windows absolute paths with either separator", () => {
		expect(isLocalFileHref("C:/work/main.ts")).toBe(true);
		expect(isLocalFileHref("C:\\work\\main.ts")).toBe(true);
	});
});

describe("isCodeFileReference", () => {
	it("recognizes TypeScript references with positions", () => {
		expect(isCodeFileReference("src/components/App.ts:42:8")).toBe(true);
		expect(isCodeFileReference("src/components/App.tsx#L42")).toBe(true);
	});

	it("does not mark non-code files as code", () => {
		expect(isCodeFileReference("docs/overview.md")).toBe(false);
	});
});

describe("unwrapFileLinkTargets", () => {
	it("turns file URLs in links into plain local paths", () => {
		expect(unwrapFileLinkTargets("[`src/preco.js`](file:///Users/me/repo/src/preco.js)")).toBe(
			"[`src/preco.js`](/Users/me/repo/src/preco.js)",
		);
		expect(unwrapFileLinkTargets("[a](file://localhost/tmp/a.ts) and [b](<file:///tmp/b c.ts>)")).toBe(
			"[a](/tmp/a.ts) and [b](</tmp/b c.ts>)",
		);
		expect(unwrapFileLinkTargets("[w](file:///C:/work/main.ts)")).toBe("[w](/C:/work/main.ts)");
	});

	it("leaves everything else alone", () => {
		const text = "see file:///tmp/a.ts and [site](https://example.com) or `](file://x)`";
		expect(unwrapFileLinkTargets("[site](https://example.com)")).toBe("[site](https://example.com)");
		expect(unwrapFileLinkTargets("plain file:///tmp/a.ts text")).toBe("plain file:///tmp/a.ts text");
		expect(unwrapFileLinkTargets(text)).toBe(text);
	});
});
