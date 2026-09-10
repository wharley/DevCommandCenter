import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { TurnReviewSummary } from "@dcc/contracts";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { loadLastTurnReview, loadTurnReviewFileDiff } from "@/lib/session-api";
import { TurnReviewTimelineCard } from "./turn-review-timeline-card";

vi.mock("@/lib/session-api", () => ({
	loadLastTurnReview: vi.fn(),
	loadTurnReviewFileDiff: vi.fn(),
}));
vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("@/features/editor/WorkspaceChangesDiffLoader", () => ({
	WorkspacePatchDiffLoader: ({
		path,
		patch,
	}: {
		path: string;
		patch: string;
	}) => <pre data-patch={path}>{patch}</pre>,
}));

const target = {
	sessionId: "session",
	workspaceId: "workspace",
	turnId: "older-turn",
};
const summary: TurnReviewSummary = {
	...target,
	snapshotId: "older-snapshot",
	state: "available",
	compatibility: "diverged",
	baseFingerprint: null,
	resultFingerprint: null,
	files: ["src/first.ts", "src/second.ts"].map((path) => ({
		path,
		status: "M",
		insertions: 3,
		deletions: 1,
		previewUnavailable: false,
	})),
	insertions: 6,
	deletions: 2,
	diffTruncated: false,
	excludedPreexistingUntrackedCount: 0,
	observedValidations: [],
	turnOutcome: "completed",
	outcomeReason: null,
	error: null,
	completedAt: "2026-09-10T18:00:00Z",
	guardedUndo: null,
	activeUndo: null,
};
let container: HTMLDivElement;
let root: Root;
let client: QueryClient;
const onReview = vi.fn();
const settle = () => new Promise((resolve) => setTimeout(resolve, 20));
async function render() {
	await act(async () =>
		root.render(
			<QueryClientProvider client={client}>
				<TurnReviewTimelineCard target={target} onReview={onReview} />
			</QueryClientProvider>,
		),
	);
	await act(settle);
}
async function click(label: string) {
	const button = [...container.querySelectorAll("button")].find(
		(item) => item.textContent === label || item.title === label,
	);
	expect(button, label).toBeTruthy();
	await act(async () => button!.click());
	await act(settle);
}
beforeEach(() => {
	vi.resetAllMocks();
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
	container = document.createElement("div");
	document.body.append(container);
	root = createRoot(container);
	vi.mocked(loadLastTurnReview).mockResolvedValue(summary);
	vi.mocked(loadTurnReviewFileDiff).mockImplementation(
		async (snapshotId, path) => ({
			snapshotId,
			path,
			diff: `patch for ${snapshotId}/${path}`,
			previewUnavailable: false,
		}),
	);
});
afterEach(async () => {
	await act(async () => root.unmount());
	client.clear();
	container.remove();
	vi.unstubAllGlobals();
});

describe("timeline turn review", () => {
	it("starts closed, loads only the selected patch, and keeps one file open", async () => {
		await render();
		expect(loadLastTurnReview).toHaveBeenCalledWith(
			"session",
			"workspace",
			"older-turn",
		);
		expect(container.querySelectorAll(".dcc-turn-review-file")).toHaveLength(0);
		await click("turnReview.timeline.showFiles");
		expect(
			container.querySelectorAll(
				".dcc-turn-review-file-trigger[aria-expanded=false]",
			),
		).toHaveLength(2);
		expect(loadTurnReviewFileDiff).not.toHaveBeenCalled();
		await click("src/first.ts");
		expect(loadTurnReviewFileDiff).toHaveBeenCalledExactlyOnceWith(
			"older-snapshot",
			"src/first.ts",
		);
		await click("src/second.ts");
		expect(container.querySelectorAll("[data-patch]")).toHaveLength(1);
		expect(
			container.querySelector("[data-patch]")?.getAttribute("data-patch"),
		).toBe("src/second.ts");
		await click("src/second.ts");
		expect(container.querySelector("[data-patch]")).toBeNull();
	});
	it("opens the inspector with the historical turn and selected file, then resets selection when collapsed", async () => {
		await render();
		await click("turnReview.timeline.showFiles");
		await click("src/second.ts");
		await click("turnReview.timeline.review");
		expect(onReview).toHaveBeenLastCalledWith({
			...target,
			filePath: "src/second.ts",
		});
		await click("turnReview.timeline.hideFiles");
		await click("turnReview.timeline.review");
		expect(onReview).toHaveBeenLastCalledWith({
			...target,
			filePath: undefined,
		});
		await click("turnReview.timeline.showFiles");
		expect(container.querySelector("[data-patch]")).toBeNull();
	});
	it.each([
		{ turnId: "latest-turn" },
		{ workspaceId: "another-workspace" },
		{ sessionId: "another-session" },
		{ files: [] },
		{ state: "collecting" },
	])(
		"does not display an unrelated or unfinished review (%j)",
		async (override) => {
			vi.mocked(loadLastTurnReview).mockResolvedValue({
				...summary,
				...override,
			});
			await render();
			expect(container.querySelector("section")).toBeNull();
		},
	);
	it("explains unavailable previews without fetching a patch", async () => {
		vi.mocked(loadLastTurnReview).mockResolvedValue({
			...summary,
			files: [{ ...summary.files[0]!, previewUnavailable: true }],
		});
		await render();
		await click("turnReview.timeline.showFiles");
		await click("src/first.ts");
		expect(container.textContent).toContain("turnReview.previewUnavailable");
		expect(loadTurnReviewFileDiff).not.toHaveBeenCalled();
	});
	it("retries a failed patch without losing its selected turn", async () => {
		vi.mocked(loadTurnReviewFileDiff).mockRejectedValueOnce(
			new Error("temporary read failure"),
		);
		await render();
		await click("turnReview.timeline.showFiles");
		await click("src/first.ts");
		expect(container.textContent).toContain("turnReview.diffFailed");
		await click("turnReview.timeline.retry");
		expect(container.querySelector("[data-patch]")?.textContent).toContain(
			"older-snapshot",
		);
	});
});
