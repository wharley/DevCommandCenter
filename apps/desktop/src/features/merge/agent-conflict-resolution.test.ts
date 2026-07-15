import { describe, expect, it } from "vitest";
import {
	buildAgentConflictResolutionPrompt,
	parseAgentConflictResolution,
} from "./agent-conflict-resolution";

describe("agent conflict resolution contract", () => {
	it("includes all Git sides and marks repository text as untrusted", () => {
		const prompt = buildAgentConflictResolutionPrompt(
			{
				path: "src/example.ts",
				kind: "both-modified",
				currentRef: "feature",
				incomingRef: "main",
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
		).toThrow(/contrato/);
	});

	it("rejects malformed or incomplete suggestions", () => {
		expect(() =>
			parseAgentConflictResolution(
				'<DCC_MERGE_RESOLUTION token="abc">not-json</DCC_MERGE_RESOLUTION token="abc">',
				"abc",
			),
		).toThrow(/inválida/);
		expect(() =>
			parseAgentConflictResolution(
				'<DCC_MERGE_RESOLUTION token="abc">{"resolvedContent":"ok"}</DCC_MERGE_RESOLUTION token="abc">',
				"abc",
			),
		).toThrow(/incompleta/);
	});
});
