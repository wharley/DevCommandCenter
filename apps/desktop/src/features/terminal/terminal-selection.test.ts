import { describe, expect, it } from "vitest";
import {
	limitTerminalSelection,
	resolveTerminalAgentContent,
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
});
