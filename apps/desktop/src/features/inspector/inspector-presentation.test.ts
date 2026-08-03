import { describe, expect, it } from "vitest";
import { shouldCollapseContextualInspector } from "./inspector-presentation";

describe("shouldCollapseContextualInspector", () => {
	it("collapses an inspector opened for a contextual workflow", () => {
		expect(shouldCollapseContextualInspector("contextual", false)).toBe(true);
	});

	it("keeps an inspector the user pinned open", () => {
		expect(shouldCollapseContextualInspector("pinned", false)).toBe(false);
	});

	it("does not request another collapse when it is already closed", () => {
		expect(shouldCollapseContextualInspector("contextual", true)).toBe(false);
	});
});
