// @vitest-environment jsdom
import type { WorkspacePrReviewComment } from "@dcc/contracts";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WorkspaceReviewThreadsSection } from "./workspace-review-threads-section";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: { count?: number }) =>
			options?.count != null ? `${key}:${options.count}` : key,
	}),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn() } }));

function comment(overrides: Partial<WorkspacePrReviewComment>): WorkspacePrReviewComment {
	return {
		id: "1",
		parentId: null,
		threadId: "t1",
		path: "src/app.ts",
		body: "Please rename.",
		diffHunk: null,
		htmlUrl: null,
		side: "RIGHT",
		line: 10,
		startLine: null,
		originalLine: 10,
		originalStartLine: null,
		author: { login: "alice", avatarUrl: null, htmlUrl: null },
		createdAt: null,
		updatedAt: null,
		resolvable: true,
		resolved: false,
		outdated: false,
		...overrides,
	};
}

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	container = document.createElement("div");
	document.body.append(container);
	root = createRoot(container);
});
afterEach(async () => {
	await act(async () => root.unmount());
	container.remove();
});

function button(label: string) {
	return [...container.querySelectorAll("button")].find((entry) =>
		entry.textContent?.includes(label),
	)!;
}

describe("WorkspaceReviewThreadsSection", () => {
	const comments = [
		comment({ threadId: "open", body: "Rename this." }),
		comment({ threadId: "done", id: "2", path: "src/other.ts", body: "Fixed.", resolved: true }),
	];

	it("hands only the picked conversations to the agent", async () => {
		const onAskAgent = vi.fn((_prompt: string) => true);
		await act(async () =>
			root.render(
				<WorkspaceReviewThreadsSection
					comments={comments}
					pullRequestUrl="https://github.com/acme/app/pull/7"
					onOpenThread={() => {}}
					onAskAgent={onAskAgent}
				/>,
			),
		);

		// Open conversations start picked; resolved ones do not.
		const ask = button("reviewThread.section.askAgent");
		expect(ask.textContent).toContain("reviewThread.section.askAgent:1");
		await act(async () => ask.click());
		const prompt = onAskAgent.mock.calls[0]![0];
		expect(prompt).toContain("Rename this.");
		expect(prompt).not.toContain("Fixed.");
	});

	it("opens a conversation from the list", async () => {
		const onOpenThread = vi.fn();
		await act(async () =>
			root.render(
				<WorkspaceReviewThreadsSection
					comments={comments}
					pullRequestUrl={null}
					onOpenThread={onOpenThread}
				/>,
			),
		);
		await act(async () => button("reviewThread.section.title").click());
		await act(async () => button("Rename this.").click());
		expect(onOpenThread).toHaveBeenCalledWith(expect.objectContaining({ id: "open" }));
	});
});
