import { describe, expect, it } from "vitest";
import { projectColorId, projectIconId } from "./project-identity";

describe("project visual identity", () => {
	it("keeps supported values", () => {
		expect(projectIconId("rocket")).toBe("rocket");
		expect(projectColorId("violet")).toBe("violet");
	});

	it("falls back safely for missing or unsupported values", () => {
		expect(projectIconId("custom-svg")).toBe("folder");
		expect(projectColorId("transparent")).toBe("slate");
		expect(projectIconId(null)).toBe("folder");
		expect(projectColorId(null)).toBe("slate");
	});
});
