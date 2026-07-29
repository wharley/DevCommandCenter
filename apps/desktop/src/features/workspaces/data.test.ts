import { describe, expect, it } from "vitest";
import { getWorkspaceTone } from "./data";

describe("getWorkspaceTone", () => {
	it("maps ready workspaces to success", () => {
		expect(getWorkspaceTone("ready")).toBe("success");
	});

	it("maps setup pending workspaces to warn", () => {
		expect(getWorkspaceTone("setup_pending")).toBe("warn");
	});

	it("maps archived workspaces to secondary", () => {
		expect(getWorkspaceTone("archived")).toBe("secondary");
	});

	it("maps completed workspaces to secondary", () => {
		expect(getWorkspaceTone("completed")).toBe("secondary");
	});

	it("keeps initializing workspaces secondary", () => {
		expect(getWorkspaceTone("initializing")).toBe("secondary");
	});
});
