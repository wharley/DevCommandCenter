import { describe, expect, it } from "vitest";
import { skillsErrorMessage } from "./skills-error";

describe("skillsErrorMessage", () => {
	it("preserves Tauri string rejections", () => {
		expect(skillsErrorMessage("workspace mapping is ambiguous", "fallback")).toBe(
			"workspace mapping is ambiguous",
		);
	});

	it("uses Error messages and falls back for empty unknown errors", () => {
		expect(skillsErrorMessage(new Error("compile failed"), "fallback")).toBe(
			"compile failed",
		);
		expect(skillsErrorMessage({ message: "save failed", code: "SAVE" }, "fallback")).toBe(
			"save failed",
		);
		expect(skillsErrorMessage("   ", "fallback")).toBe("fallback");
		expect(skillsErrorMessage({ reason: "nope" }, "fallback")).toBe("fallback");
	});
});
