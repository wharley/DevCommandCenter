// @vitest-environment jsdom
import type {
	WorkspaceReviewer,
	WorkspaceReviewerCandidate,
	WorkspaceReviewStateOutput,
} from "@dcc/contracts";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
	workspaceReviewState: vi.fn(),
	workspaceRequestReviewers: vi.fn(),
	workspaceReviewerCandidates: vi.fn(),
}));
vi.mock("@/lib/workspace-api", () => api);
vi.mock("@/lib/shell-api", () => ({ openExternal: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

const { groupReviewerCandidates } = await import("./request-review-picker");
const { WorkspaceReviewStateSection, reviewerRowAction } = await import(
	"./workspace-review-state-section"
);

function candidate(login: string, suggested = false): WorkspaceReviewerCandidate {
	return { login, name: null, avatarUrl: null, suggested };
}

function reviewer(overrides: Partial<WorkspaceReviewer>): WorkspaceReviewer {
	return {
		login: "alice",
		name: null,
		avatarUrl: null,
		htmlUrl: null,
		state: "pending",
		isTeam: false,
		...overrides,
	};
}

function reviewState(reviewers: WorkspaceReviewer[]): WorkspaceReviewStateOutput {
	return {
		provider: "github",
		host: "github.com",
		number: 12,
		url: "https://github.com/acme/app/pull/12",
		reviewState: "pending",
		reviewers,
		reviewersAvailable: true,
		approvalsAvailable: true,
		approvalsRequired: null,
		approvalsReceived: 0,
		approvalsLeft: null,
		approved: null,
		mergeable: "mergeable",
		mergeStateStatus: null,
		hasConflicts: false,
		behindBy: 0,
		discussionsResolved: true,
		draft: false,
	};
}

describe("groupReviewerCandidates", () => {
	it("splits suggested people from the rest and hides current reviewers", () => {
		const groups = groupReviewerCandidates(
			[candidate("carol", true), candidate("Alice", true), candidate("bob")],
			["alice"],
		);
		expect(groups.suggested.map((entry) => entry.login)).toEqual(["carol"]);
		expect(groups.others.map((entry) => entry.login)).toEqual(["bob"]);
	});
});

describe("reviewerRowAction", () => {
	it("offers to withdraw a pending request on both forges", () => {
		expect(reviewerRowAction(reviewer({}), "github")).toBe("remove");
		expect(reviewerRowAction(reviewer({}), "gitlab")).toBe("remove");
	});

	it("offers to ask again only on GitHub", () => {
		expect(reviewerRowAction(reviewer({ state: "approved" }), "github")).toBe(
			"rerequest",
		);
		expect(
			reviewerRowAction(reviewer({ state: "changes_requested" }), "gitlab"),
		).toBeNull();
	});

	it("offers nothing for teams or reviewers without a login", () => {
		expect(reviewerRowAction(reviewer({ isTeam: true }), "github")).toBeNull();
		expect(reviewerRowAction(reviewer({ login: null }), "github")).toBeNull();
	});
});

describe("WorkspaceReviewStateSection reviewers", () => {
	let container: HTMLDivElement;
	let root: Root;

	beforeEach(() => {
		vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
		container = document.createElement("div");
		document.body.append(container);
		root = createRoot(container);
		api.workspaceRequestReviewers.mockResolvedValue({ requested: [], removed: [] });
	});
	afterEach(async () => {
		await act(async () => root.unmount());
		container.remove();
		vi.clearAllMocks();
	});

	async function renderSection(reviewers: WorkspaceReviewer[]) {
		api.workspaceReviewState.mockResolvedValue(reviewState(reviewers));
		const client = new QueryClient({
			defaultOptions: { queries: { retry: false } },
		});
		await act(async () =>
			root.render(
				<QueryClientProvider client={client}>
					<WorkspaceReviewStateSection
						workspaceRoot="/repo"
						branch="feature"
						forgeLogin={null}
						enabled
					/>
				</QueryClientProvider>,
			),
		);
		const toggle = await vi.waitFor(() => {
			const found = [...container.querySelectorAll("button")].find((entry) =>
				entry.textContent?.includes("inspector.reviewState.title"),
			);
			if (!found) throw new Error("review state not rendered yet");
			return found;
		});
		await act(async () => toggle.click());
	}

	function buttonByLabel(label: string) {
		return container.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
	}

	it("shows the request-review button next to the reviewers", async () => {
		await renderSection([]);
		expect(container.querySelector("[data-request-review-trigger]")).not.toBeNull();
		expect(container.textContent).toContain("inspector.reviewState.noReviewers");
	});

	it("asks someone who already reviewed again", async () => {
		await renderSection([reviewer({ login: "bob", state: "approved" })]);
		await act(async () =>
			buttonByLabel("inspector.reviewState.rerequestReview")!.click(),
		);
		await vi.waitFor(() =>
			expect(api.workspaceRequestReviewers).toHaveBeenCalled(),
		);
		expect(api.workspaceRequestReviewers).toHaveBeenCalledWith({
			workspaceRoot: "/repo",
			number: 12,
			add: ["bob"],
			remove: [],
			forgeLogin: null,
		});
	});

	it("withdraws a pending request", async () => {
		await renderSection([reviewer({ login: "carol" })]);
		await act(async () =>
			buttonByLabel("inspector.reviewState.removeReviewRequest")!.click(),
		);
		await vi.waitFor(() =>
			expect(api.workspaceRequestReviewers).toHaveBeenCalled(),
		);
		expect(api.workspaceRequestReviewers).toHaveBeenCalledWith(
			expect.objectContaining({ add: [], remove: ["carol"] }),
		);
	});

	it("offers no action on a team", async () => {
		await renderSection([reviewer({ login: "acme/platform", isTeam: true })]);
		expect(buttonByLabel("inspector.reviewState.removeReviewRequest")).toBeNull();
		expect(buttonByLabel("inspector.reviewState.rerequestReview")).toBeNull();
	});
});
