import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_EFFORT_LEVEL } from "./effort";
import {
	loadApprovalPolicy,
	loadDirectResponse,
	loadEffortSelection,
	saveApprovalPolicy,
	saveDirectResponse,
	saveEffortSelection,
} from "./draftStorage";

function createLocalStorageStub() {
	const store = new Map<string, string>();
	return {
		getItem: (key: string) => store.get(key) ?? null,
		setItem: (key: string, value: string) => void store.set(key, value),
		removeItem: (key: string) => void store.delete(key),
	};
}

describe("response style persistence", () => {
	beforeEach(() => {
		vi.stubGlobal("window", { localStorage: createLocalStorageStub() });
	});

	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it("defaults to the standard response style", () => {
		expect(loadDirectResponse()).toBe(false);
	});

	it("persists the direct response style globally", () => {
		saveDirectResponse(true);
		expect(loadDirectResponse()).toBe(true);

		saveDirectResponse(false);
		expect(loadDirectResponse()).toBe(false);
	});
});

describe("effort selection persistence", () => {
	beforeEach(() => {
		vi.stubGlobal("window", { localStorage: createLocalStorageStub() });
	});

	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it("returns the default selection when nothing is stored", () => {
		expect(loadEffortSelection("dcc.workspace.composer.effort.alpha")).toEqual({
			effort: DEFAULT_EFFORT_LEVEL,
			ultrathink: false,
		});
	});

	it("round-trips a saved selection", () => {
		saveEffortSelection("dcc.workspace.composer.effort.alpha", {
			effort: "high",
			ultrathink: true,
		});

		expect(loadEffortSelection("dcc.workspace.composer.effort.alpha")).toEqual({
			effort: "high",
			ultrathink: true,
		});
	});

	it("keeps selections isolated per workspace key", () => {
		saveEffortSelection("dcc.workspace.composer.effort.alpha", {
			effort: "high",
			ultrathink: false,
		});

		expect(loadEffortSelection("dcc.workspace.composer.effort.beta")).toEqual({
			effort: DEFAULT_EFFORT_LEVEL,
			ultrathink: false,
		});
	});

	it("falls back to defaults for malformed stored data", () => {
		window.localStorage.setItem(
			"dcc.workspace.composer.effort.alpha",
			"not-json",
		);

		expect(loadEffortSelection("dcc.workspace.composer.effort.alpha")).toEqual({
			effort: DEFAULT_EFFORT_LEVEL,
			ultrathink: false,
		});
	});
});

describe("approval policy persistence", () => {
	beforeEach(() => {
		vi.stubGlobal("window", { localStorage: createLocalStorageStub() });
	});

	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it("defaults to auto when the provider supports it", () => {
		expect(loadApprovalPolicy("approval.codex", ["ask", "auto", "full_access"])).toBe(
			"auto",
		);
	});

	it("keeps a saved selection only while the provider supports it", () => {
		saveApprovalPolicy("approval.claude", "full_access");
		expect(
			loadApprovalPolicy("approval.claude", ["ask", "auto", "full_access"]),
		).toBe("full_access");
		expect(loadApprovalPolicy("approval.claude", ["ask", "auto"])).toBe("auto");
	});

	it("returns null for provider-managed permissions", () => {
		expect(loadApprovalPolicy("approval.cursor", [])).toBeNull();
	});
});
