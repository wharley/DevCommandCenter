import { describe, expect, it } from "vitest";
import {
	buildProviderHandoffContext,
	mergeProviderHandoffToolInstructions,
	shouldCreateProviderHandoff,
} from "./provider-handoff-context";

const session = { providerId: "claude_code", turnCount: 2 };
const messages = [
	{ role: "user" as const, content: "Earlier request" },
	{ role: "assistant" as const, content: "Earlier answer" },
];

describe("provider handoff context", () => {
	it("only detects a provider switch with useful durable history", () => {
		expect(
			shouldCreateProviderHandoff({
				session,
				destinationProviderId: "codex",
				messages,
			}),
		).toBe(true);
		expect(
			shouldCreateProviderHandoff({
				session,
				destinationProviderId: "claude_code",
				messages,
			}),
		).toBe(false);
		expect(
			shouldCreateProviderHandoff({
				session,
				destinationProviderId: "codex",
				messages,
				forceNewSession: true,
			}),
		).toBe(false);
		expect(
			shouldCreateProviderHandoff({
				session,
				destinationProviderId: "codex",
				messages,
				targetSessionId: "child",
			}),
		).toBe(false);
		expect(
			shouldCreateProviderHandoff({
				session: { providerId: "claude_code", turnCount: 2 },
				destinationProviderId: "codex",
				messages: [{ role: "assistant", content: "", streaming: true }],
			}),
		).toBe(false);
	});

	it("keeps recent messages ordered, omits empty/incomplete content, and stays bounded", () => {
		const context = buildProviderHandoffContext({
			sourceProviderId: "claude_code",
			destinationProviderId: "codex",
			workspaceName: "demo",
			branch: "dcc/context",
			recentMessages: [
				...Array.from({ length: 10 }, (_, index) => ({
					role: "user" as const,
					content: `message ${index}`,
				})),
				{ role: "assistant", content: "   " },
				{ role: "assistant", content: "partial", streaming: true },
			],
		});
		expect(context.length).toBeLessThanOrEqual(12_000);
		expect(context).toContain("message 2");
		expect(context).toContain("message 9");
		expect(context.indexOf("message 2")).toBeLessThan(context.indexOf("message 9"));
		expect(context).not.toContain("partial");
		expect(context).not.toContain("Assistant: ");
	});

	it("keeps the newest messages when verbose recent context exceeds its budget", () => {
		const context = buildProviderHandoffContext({
			sourceProviderId: "claude_code",
			destinationProviderId: "codex",
			recentMessages: Array.from({ length: 8 }, (_, index) => ({
				role: index % 2 === 0 ? ("user" as const) : ("assistant" as const),
				content: `marker-${index} ${"x".repeat(1_700)}`,
			})),
		});

		expect(context.length).toBeLessThanOrEqual(12_000);
		expect(context).toContain("marker-7");
		expect(context).not.toContain("marker-0");
	});

	it("includes workspace artifacts without raw patches and excludes the current prompt", () => {
		const context = buildProviderHandoffContext({
			sourceProviderId: "claude_code",
			destinationProviderId: "codex",
			workspacePath: "/tmp/demo",
			git: {
				currentBranch: "dcc/context",
				baseBranch: "main",
				staged: [{ path: "src/a.ts", status: "M", insertions: 3, deletions: 1 }],
			},
			missionSpec: "Ship the context handoff",
			activePlan: "1. Add bounded packet",
			currentPrompt: "Continue this task",
			recentMessages: [
				{ role: "user", content: "Continue this task" },
				{ role: "assistant", content: "Previous decision" },
			],
		});
		expect(context).toContain("src/a.ts (+3/-1)");
		expect(context).toContain("Ship the context handoff");
		expect(context).toContain("Previous decision");
		expect(context).not.toContain("Continue this task");
		expect(context).not.toContain("diff --git");
	});

	it("merges existing tool instructions exactly once", () => {
		expect(mergeProviderHandoffToolInstructions("Existing", "Handoff")).toBe(
			"Existing\n\nHandoff",
		);
		expect(mergeProviderHandoffToolInstructions(null, "Handoff")).toBe("Handoff");
		expect(mergeProviderHandoffToolInstructions("Existing", null)).toBe("Existing");
	});
});
