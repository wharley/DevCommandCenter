import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_EFFORT_LEVEL } from "./effort";
import { loadEffortSelection, saveEffortSelection } from "./draftStorage";

function createLocalStorageStub() {
	const store = new Map<string, string>();
	return {
		getItem: (key: string) => store.get(key) ?? null,
		setItem: (key: string, value: string) => void store.set(key, value),
		removeItem: (key: string) => void store.delete(key),
	};
}

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
