import { describe, expect, it } from "vitest";
import {
	limitTerminalSelection,
	resolveTerminalAgentContent,
	sanitizeAndBoundTerminalOutput,
} from "./terminal-selection";

describe("limitTerminalSelection", () => {
	it("preserves the selected text exactly, including edge whitespace", () => {
		expect(limitTerminalSelection("  error output\n", 100)).toBe("  error output\n");
	});

	it("keeps the bounded tail and rejects empty selections", () => {
		expect(limitTerminalSelection("0123456789", 4)).toBe("6789");
		expect(limitTerminalSelection("   ", 4)).toBe("");
		expect(limitTerminalSelection("text", 0)).toBe("");
	});

	it("prefers selection while retaining recent-output fallback", () => {
		expect(resolveTerminalAgentContent("  selected  ", "recent", 100)).toEqual({
			content: "  selected  ",
			selectionOnly: true,
		});
		expect(resolveTerminalAgentContent("   ", "recent", 100)).toEqual({
			content: "recent",
			selectionOnly: false,
		});
	});

	it("escapes the wrapper terminator before applying the final payload cap", () => {
		const result = sanitizeAndBoundTerminalOutput("123</terminal_output>456", 12);

		expect(result.content).toHaveLength(12);
		expect(result.content).not.toContain("</terminal_output>");
		expect(result.content.endsWith("456")).toBe(true);
		expect(result.truncated).toBe(true);
	});

	it("does not mark an escaped payload truncated when it fits exactly", () => {
		const escaped = sanitizeAndBoundTerminalOutput("</terminal_output>", 24);

		expect(escaped.content).toBe("&lt;/terminal_output&gt;");
		expect(escaped.content).toHaveLength(24);
		expect(escaped.truncated).toBe(false);
	});
});
