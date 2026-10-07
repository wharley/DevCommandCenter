import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Delegation } from "@dcc/contracts";
import { DelegationCard } from "./DelegationCard";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => ({ delegations: [] })) }));
vi.mock("./AssistantProse", () => ({
	AssistantProse: ({ content }: { content: string }) => <div data-prose>{content}</div>,
}));
vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) =>
			options?.agent ? `${key}:${String(options.agent)}` : key,
	}),
}));

function record(overrides: Partial<Delegation>): Delegation {
	return {
		id: "d-1",
		parentSessionId: "parent",
		parentTurnId: null,
		childSessionId: "child",
		workspaceId: "w",
		targetProviderId: "claude_code",
		targetModelId: "claude-sonnet-5-5",
		mode: "implement",
		status: "running",
		prompt: "Delegated implement task\n\nInstruction:\nFix stale replies\n\nGit context:\n- x",
		contextPolicy: { type: "minimal" },
		budget: { turnLimit: 1, timeoutSeconds: 900, allowFileEdits: true, approvalPolicy: null },
		resultSummary: null,
		touchedFiles: [],
		diffSummary: null,
		validationSummary: null,
		createdAt: "2026-10-07T12:00:00Z",
		updatedAt: "2026-10-07T12:00:00Z",
		origin: "agent",
		instruction: "Fix stale replies",
		startedAt: "2026-10-07T12:00:00Z",
		...overrides,
	} as Delegation;
}

describe("DelegationCard", () => {
	let container: HTMLDivElement;
	let root: Root;

	beforeEach(() => {
		(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
		container = document.createElement("div");
		document.body.append(container);
		root = createRoot(container);
	});

	afterEach(() => {
		act(() => root.unmount());
		container.remove();
	});

	async function render(
		delegation: Delegation,
		props: Partial<Parameters<typeof DelegationCard>[0]> = {},
	) {
		const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
		client.setQueryData(["delegations", "w"], [delegation]);
		await act(async () => {
			root.render(
				<QueryClientProvider client={client}>
					<DelegationCard
						delegation={{ id: delegation.id, phase: "running", childSessionId: "child" }}
						fallbackContent=""
						workspaceId="w"
						providers={[]}
						onSelectSession={() => {}}
						onReviewDelegation={() => {}}
						{...props}
					/>
				</QueryClientProvider>,
			);
		});
	}

	it("shows the agent's robot walking while the child works, under the task's title", async () => {
		await render(record({ status: "running" }), { childTitle: "Corrigir respostas antigas" });
		expect(container.textContent).toContain("Corrigir respostas antigas");
		expect(container.querySelector('[data-agent][data-state="working"]')).not.toBeNull();
		expect(container.textContent).toContain("delegation.card.working:claude-sonnet-5-5");
		expect(container.querySelector("[data-prose]")).toBeNull();
	});

	it("collapses a long report and opens a changed file in the Inspector", async () => {
		const onReviewDelegation = vi.fn();
		await render(
			record({
				status: "review_pending",
				resultSummary: `## O que mudou\n\n${"detail ".repeat(80)}`,
				touchedFiles: ["Sources/App/Companion.swift", "Package.swift"],
			}),
			{
				onReviewDelegation,
				onReviewDelegationFile: onReviewDelegation,
				verification: [
					{ command: "swift build", ok: true },
					{ command: "swift test", ok: true },
				],
			},
		);
		expect(container.querySelector('[data-agent][data-state="idle"]')).not.toBeNull();
		expect(container.textContent).toContain("delegation.card.showReport");
		expect(container.textContent).toContain("delegation.card.verified");
		expect(container.textContent).toContain("✓ swift test");

		const chip = [...container.querySelectorAll("button")].find(
			(button) => button.getAttribute("title") === "Sources/App/Companion.swift",
		);
		expect(chip?.textContent).toContain("Companion.swift");
		await act(async () => chip?.click());
		expect(onReviewDelegation).toHaveBeenCalledWith("d-1", "Sources/App/Companion.swift");
	});
});
