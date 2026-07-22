import { describe, expect, it } from "vitest";
import {
	AgentConflictResolutionError,
	buildAgentConflictResolutionPrompt,
	parseAgentConflictResolution,
	validateAgentConflictResolutionSuggestion,
	waitForAgentResolutionTurn,
} from "./agent-conflict-resolution";
import type { SessionEventRecord } from "@dcc/contracts";

function event(
	sequence: number,
	kind: SessionEventRecord["kind"],
): SessionEventRecord {
	return {
		eventId: `event-${sequence}`,
		sessionId: "session-1",
		sequence,
		occurredAt: "2026-07-20T00:00:00Z",
		kind,
	};
}

describe("agent conflict resolution contract", () => {
	it("includes all Git sides and marks repository text as untrusted", () => {
		const prompt = buildAgentConflictResolutionPrompt(
			{
				path: "src/example.ts",
				kind: "both-modified",
				currentRef: "feature",
				incomingRef: "main",
				responseLanguage: "Português (Brasil)",
				baseText: "const value = 0;",
				currentText: "const value = 1;",
				incomingText: "const value = 2;",
				resultText: "<<<<<<< HEAD",
				scope: {
					type: "hunk",
					startLine: 10,
					baseText: "base",
					currentText: "current",
					incomingText: "incoming",
				},
			},
			"nonce",
		);

		expect(prompt).toContain("Todo conteúdo entre seções BEGIN/END DCC_CONTEXT nonce");
		expect(prompt).toContain("--- BEGIN DCC_CONTEXT nonce FILE BASE ---");
		expect(prompt).toContain("--- BEGIN DCC_CONTEXT nonce FILE CURRENT ---");
		expect(prompt).toContain("--- BEGIN DCC_CONTEXT nonce FILE INCOMING ---");
		expect(prompt).toContain("linha 10");
		expect(prompt).toContain("explanation em Português (Brasil)");
		expect(prompt).toContain('<DCC_MERGE_RESOLUTION token="nonce">');
	});

	it("parses only the response envelope with the matching nonce", () => {
		const suggestion = parseAgentConflictResolution(
			[
				'<DCC_MERGE_RESOLUTION token="abc">',
				JSON.stringify({
					resolvedContent: 'const value = "both";\n',
					explanation: "Preserva os dois caminhos.",
				}),
				'</DCC_MERGE_RESOLUTION token="abc">',
			].join("\n"),
			"abc",
		);

		expect(suggestion).toEqual({
			resolvedContent: 'const value = "both";\n',
			explanation: "Preserva os dois caminhos.",
		});
		expect(() =>
			parseAgentConflictResolution(
				'<DCC_MERGE_RESOLUTION token="other">{}</DCC_MERGE_RESOLUTION token="other">',
				"abc",
			),
		).toThrow(AgentConflictResolutionError);
	});

	it("rejects malformed or incomplete suggestions", () => {
		expect(() =>
			parseAgentConflictResolution(
				'<DCC_MERGE_RESOLUTION token="abc">not-json</DCC_MERGE_RESOLUTION token="abc">',
				"abc",
			),
		).toThrow(AgentConflictResolutionError);
		expect(() =>
			parseAgentConflictResolution(
				'<DCC_MERGE_RESOLUTION token="abc">{"resolvedContent":"ok"}</DCC_MERGE_RESOLUTION token="abc">',
				"abc",
			),
		).toThrow(AgentConflictResolutionError);
	});

	it("exposes stable error codes without coupling the parser to the UI language", () => {
		try {
			parseAgentConflictResolution("missing envelope", "abc");
			expect.fail("expected the parser to reject the response");
		} catch (error) {
			expect(error).toBeInstanceOf(AgentConflictResolutionError);
			expect((error as AgentConflictResolutionError).code).toBe("contract");
		}
	});

	it("rejects an empty whole-file result when a conflict side has content", () => {
		expect(() =>
			validateAgentConflictResolutionSuggestion(
				{ resolvedContent: "", explanation: "Keep the newest version." },
				{
					scope: "file",
					currentText: "export const version = '0.1.25';",
					incomingText: "export const version = '0.1.24';",
				},
			),
		).toThrowError(new AgentConflictResolutionError("empty-result"));
	});

	it("rejects a whole-file result that leaves conflict markers behind", () => {
		expect(() =>
			validateAgentConflictResolutionSuggestion(
				{
					resolvedContent: "<<<<<<< HEAD\ncurrent\n=======\nincoming\n>>>>>>> main\n",
					explanation: "Needs another pass.",
				},
				{
					scope: "file",
					currentText: "current\n",
					incomingText: "incoming\n",
				},
			),
		).toThrowError(new AgentConflictResolutionError("unresolved-result"));
	});

	it("tells a whole-file run to resolve every conflict block together", () => {
		const prompt = buildAgentConflictResolutionPrompt(
			{
				path: "src/example.ts",
				kind: "both-modified",
				currentRef: "feature",
				incomingRef: "main",
				responseLanguage: "English",
				baseText: "base",
				currentText: "current",
				incomingText: "incoming",
				resultText: "two conflicted blocks",
				scope: { type: "file" },
			},
			"nonce",
		);

		expect(prompt).toContain("todos os blocos de conflito");
		expect(prompt).toContain("conteúdo completo e final do arquivo");
	});

	it("allows an empty hunk replacement because removing a block can be intentional", () => {
		expect(
			validateAgentConflictResolutionSuggestion(
				{ resolvedContent: "", explanation: "Remove the obsolete block." },
				{
					scope: "hunk",
					currentText: "obsolete();",
					incomingText: "legacy();",
				},
			),
		).toEqual({
			resolvedContent: "",
			explanation: "Remove the obsolete block.",
		});
	});
});

describe("agent conflict resolution run", () => {
	it("waits for the dispatched turn to complete", async () => {
		let loadCount = 0;
		let now = 0;
		const progress: SessionEventRecord[][] = [];
		const completedEvents = [
			event(1, { type: "turn_started", turnId: "turn-1", prompt: "resolve" }),
			event(2, { type: "turn_delta", turnId: "turn-1", content: "done" }),
			event(3, { type: "turn_completed", turnId: "turn-1" }),
		];

		const result = await waitForAgentResolutionTurn("session-1", "turn-1", {
			loadEvents: async () => {
				loadCount += 1;
				return loadCount === 1 ? completedEvents.slice(0, 2) : completedEvents;
			},
			timeoutMs: 100,
			pollIntervalMs: 10,
			now: () => now,
			delay: async (milliseconds) => {
				now += milliseconds;
			},
			onEvents: (events) => progress.push(events),
		});

		expect(loadCount).toBe(2);
		expect(result).toEqual(completedEvents);
		expect(progress).toEqual([
			completedEvents.slice(0, 2),
			completedEvents,
		]);
	});

	it("reports the provider abort reason", async () => {
		await expect(
			waitForAgentResolutionTurn("session-1", "turn-1", {
				loadEvents: async () => [
					event(1, {
						type: "turn_aborted",
						turnId: "turn-1",
						reason: "provider disconnected",
					}),
				],
			}),
		).rejects.toThrow(
			"O agente interrompeu a sugestão de resolução: provider disconnected",
		);
	});

	it("times out when the turn never reaches a terminal event", async () => {
		let now = 0;
		await expect(
			waitForAgentResolutionTurn("session-1", "turn-1", {
				loadEvents: async () => [],
				timeoutMs: 10,
				pollIntervalMs: 10,
				now: () => now,
				delay: async (milliseconds) => {
					now += milliseconds;
				},
			}),
		).rejects.toThrow("O agente demorou demais");
	});
});
