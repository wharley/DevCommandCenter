import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import type { CodeViewProps } from "@pierre/diffs/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import WorkspacePatchDiff from "./WorkspacePatchDiff";

let viewProps: CodeViewProps<undefined>;
vi.mock("@pierre/diffs/react", () => ({
	CodeView: (props: CodeViewProps<undefined>) => {
		viewProps = props;
		return <div data-testid="code-view" />;
	},
}));
vi.mock("@/components/theme-provider", () => ({ useAppearance: () => ({ theme: "dark" }) }));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));

const patch = `diff --git a/test.ts b/test.ts
--- a/test.ts
+++ b/test.ts
@@ -19,1 +19,2 @@
-old test
+new test
+assertion
`;
let container: HTMLDivElement;
let root: Root;
const addToChat = vi.fn();

beforeEach(() => {
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	addToChat.mockClear();
	vi.stubGlobal("CSSStyleSheet", class { replaceSync() {} });
	container = document.createElement("div");
	document.body.append(container);
	root = createRoot(container);
});
afterEach(async () => {
	await act(async () => root.unmount());
	container.remove();
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
});

describe("patch selection actions", () => {
	it("waits for an explicit click before adding selected lines and clears the action afterward", async () => {
		await act(async () => root.render(<WorkspacePatchDiff path="test.ts" patch={patch} onAddToChat={addToChat} />));
		await act(async () => viewProps.onSelectedLinesChange?.({ id: "dcc-workspace-patch", range: { start: 19, end: 20, side: "additions" } }));
		expect(addToChat).not.toHaveBeenCalled();
		const button = container.querySelector("button")!;
		expect(button.textContent).toBe("turnReview.timeline.addToChat");
		await act(async () => button.click());
		expect(addToChat).toHaveBeenCalledExactlyOnceWith([
			{ path: "test.ts", side: "modified", startLine: 19, endLine: 20, snippet: "new test\nassertion" },
		]);
		expect(container.querySelector("button")).toBeNull();
		expect(container.querySelector('[role="dialog"]')).toBeNull();
	});

	it.each(["native", "WebKit"])("offers the same action for a %s text selection inside the diff", async (browser) => {
		await act(async () => root.render(<WorkspacePatchDiff path="test.ts" patch={patch} onAddToChat={addToChat} />));
		const host = document.createElement("diffs-container");
		container.querySelector('[data-testid="code-view"]')!.append(host);
		const shadow = host.shadowRoot ?? host.attachShadow({ mode: "open" });
		shadow.innerHTML = '<div data-line="19" data-line-type="change-deletion">old test</div>';
		const node = shadow.firstChild!.firstChild!;
		const removeAllRanges = vi.fn();
		vi.spyOn(window, "getSelection").mockReturnValue({
			isCollapsed: browser === "WebKit",
			anchorNode: browser === "WebKit" ? host : node,
			focusNode: browser === "WebKit" ? host : node,
			getComposedRanges: browser === "WebKit" ? () => [new StaticRange({
				startContainer: node,
				startOffset: 0,
				endContainer: node,
				endOffset: 8,
			})] : undefined,
			removeAllRanges,
		} as unknown as Selection);
		await act(async () => document.dispatchEvent(new Event("selectionchange")));
		expect(addToChat).not.toHaveBeenCalled();
		await act(async () => container.querySelector("button")!.click());
		expect(addToChat).toHaveBeenCalledExactlyOnceWith([
			{ path: "test.ts", side: "original", startLine: 19, endLine: 19, snippet: "old test" },
		]);
		expect(removeAllRanges).toHaveBeenCalled();
	});

	it("discards the selection when switching files and keeps previews without a chat target read-only", async () => {
		await act(async () => root.render(<WorkspacePatchDiff path="test.ts" patch={patch} onAddToChat={addToChat} />));
		await act(async () => viewProps.onSelectedLinesChange?.({ id: "dcc-workspace-patch", range: { start: 19, end: 19 } }));
		await act(async () => root.render(<WorkspacePatchDiff path="other.ts" patch={patch} />));
		expect(container.querySelector("button")).toBeNull();
		expect(viewProps.options?.enableLineSelection).toBe(false);
		expect(viewProps.options?.enableGutterUtility).toBe(false);
		expect(addToChat).not.toHaveBeenCalled();
	});
});
