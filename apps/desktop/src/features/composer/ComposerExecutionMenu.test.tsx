import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderCatalog } from "@dcc/contracts";
import { FALLBACK_PROVIDER_CATALOG } from "@/lib/fallback-provider-catalog";
import { DCC_OPEN_SETTINGS_EVENT } from "@/features/settings/settings-navigation";
import { ComposerExecutionMenu } from "./ComposerExecutionMenu";
import { getModelFavorites, saveModelFavorites } from "./model-favorites";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string, options?: { model?: string; provider?: string }) =>
		options?.model ? `${key}: ${options.model}` : options?.provider ? `${key}: ${options.provider}` : key }),
}));

const high = { providerId: "codex", modelId: "gpt-6-astra", effort: "high" };
const extraHigh = { ...high, effort: "xhigh" };
const IS_MAC = /mac/i.test(navigator.platform);
let container: HTMLDivElement;
let root: Root;
const selected = vi.fn();

function Harness({
	disabled = false,
	compact = false,
	providers = FALLBACK_PROVIDER_CATALOG.providers,
	initialProvider = "cursor",
	initialModel = "auto",
}: {
	disabled?: boolean;
	compact?: boolean;
	providers?: ProviderCatalog["providers"];
	initialProvider?: string;
	initialModel?: string;
}) {
	const [open, setOpen] = useState(true);
	const [providerId, setProviderId] = useState(initialProvider);
	const [modelId, setModelId] = useState(initialModel);
	const [effort, setEffort] = useState("medium");
	const [direct, setDirect] = useState(false);
	return <ComposerExecutionMenu compact={compact} open={open} onOpenChange={setOpen}
		providers={providers}
		selectedProviderId={providerId} selectedModelId={modelId}
		availableEffortLevels={["low", "medium", "high", "xhigh"]} selectedEffortId={effort}
		directResponse={direct} disabled={disabled}
		onSelectProvider={(value) => { selected("provider", value); setProviderId(value); setModelId("default"); }}
		onSelectModel={(value) => { selected("model", value); setModelId(value); }}
		onSelectEffort={(value) => { selected("effort", value); setEffort(value); }}
		onSelectUltrathink={() => { selected("effort", "ultrathink"); setEffort("ultrathink"); }}
		onSetDirectResponse={(value) => { selected("direct", value); setDirect(value); }}
	/>;
}

const options = () => [...document.querySelectorAll<HTMLElement>('[role="option"]')];
function option(text: string) {
	const element = options().find((entry) => entry.textContent?.includes(text));
	expect(element, text).toBeDefined();
	return element!;
}
function tab(label: string) {
	const element = [...document.querySelectorAll<HTMLElement>('[role="tab"]')]
		.find((entry) => entry.getAttribute("aria-label") === label);
	expect(element, label).toBeDefined();
	return element!;
}
async function search(text: string) {
	const input = document.querySelector<HTMLInputElement>('input[role="combobox"]')!;
	await act(async () => {
		const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
		setter.call(input, text);
		input.dispatchEvent(new Event("input", { bubbles: true }));
	});
	return input;
}
const providerLabel = (id: string) =>
	FALLBACK_PROVIDER_CATALOG.providers.find((provider) => provider.id === id)!.label;

beforeEach(() => {
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
	Element.prototype.scrollIntoView = vi.fn();
	selected.mockClear();
	saveModelFavorites([extraHigh, high]);
	container = document.createElement("div");
	document.body.append(container);
	root = createRoot(container);
});

afterEach(async () => {
	await act(async () => root.unmount());
	container.remove();
	vi.unstubAllGlobals();
});

describe("model picker", () => {
	it("opens on favorites and applies provider, model and effort together", async () => {
		await act(async () => root.render(<Harness />));
		expect(tab("composer.favorites.title").getAttribute("aria-selected")).toBe("true");
		expect(options().map((row) => row.textContent)).toEqual([
			expect.stringContaining("composer.effort.xhigh"),
			expect.stringContaining("composer.effort.high"),
		]);
		await act(async () => option("composer.effort.xhigh").click());
		expect(selected.mock.calls).toEqual([
			["provider", "codex"], ["model", "gpt-6-astra"], ["effort", "xhigh"],
		]);
		expect(document.querySelector('[data-model-picker-panel]')).toBeNull();
		expect(getModelFavorites()).toEqual([extraHigh, high]);
	});

	it("leaves providers switched off in Settings out of the tabs and the search", async () => {
		const providers = FALLBACK_PROVIDER_CATALOG.providers.map((provider) =>
			provider.id === "codex" ? { ...provider, enabled: false } : provider);
		await act(async () => root.render(<Harness providers={providers} />));
		const labels = [...document.querySelectorAll('[role="tab"]')].map((entry) => entry.getAttribute("aria-label"));
		expect(labels).not.toContain(providerLabel("codex"));
		expect(labels).toContain(providerLabel("cursor"));
		// Codex favorites are hidden too, so the picker opens on the current provider.
		expect(tab(providerLabel("cursor")).getAttribute("aria-selected")).toBe("true");
		await search("gpt-6");
		expect(options().some((row) => row.textContent?.includes("GPT-6"))).toBe(false);
		expect(document.body.textContent).not.toContain("settings.model.disabled");
	});

	it("searches the open tab first and then other providers", async () => {
		await act(async () => root.render(<Harness />));
		await act(async () => tab(providerLabel("cursor")).click());
		await search("luna");
		expect(document.body.textContent).toContain("composer.picker.otherProviders");
		await act(async () => option("GPT-6 Luna").click());
		expect(selected.mock.calls).toEqual([["provider", "codex"], ["model", "gpt-6-luna"]]);
	});

	it("stars a model with the current effort and unstars it", async () => {
		saveModelFavorites([]);
		await act(async () => root.render(<Harness />));
		await act(async () => tab(providerLabel("codex")).click());
		const star = () => option("GPT-6 Luna").querySelector<HTMLButtonElement>("button[aria-pressed]")!;
		await act(async () => star().click());
		expect(getModelFavorites()).toEqual([{ providerId: "codex", modelId: "gpt-6-luna", effort: "medium" }]);
		expect(selected).not.toHaveBeenCalled();
		await act(async () => star().click());
		expect(getModelFavorites()).toEqual([]);
	});

	it("picks the nth row with the platform shortcut", async () => {
		await act(async () => root.render(<Harness />));
		await act(async () => {
			window.dispatchEvent(new KeyboardEvent("keydown", {
				key: "2", metaKey: IS_MAC, ctrlKey: !IS_MAC, bubbles: true,
			}));
		});
		expect(selected.mock.calls).toEqual([
			["provider", "codex"], ["model", "gpt-6-astra"], ["effort", "high"],
		]);
	});

	it("moves effort along the slider, with ultrathink as the last stop", async () => {
		await act(async () => root.render(<Harness />));
		const slider = document.querySelector<HTMLElement>('[role="slider"]')!;
		expect(slider.getAttribute("aria-valuetext")).toBe("composer.effort.medium");
		await act(async () => slider.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })));
		expect(selected).toHaveBeenLastCalledWith("effort", "high");
		await act(async () => slider.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true })));
		expect(selected).toHaveBeenLastCalledWith("effort", "ultrathink");
		const direct = document.querySelector<HTMLButtonElement>('button[aria-label="composer.picker.directOff"]')!;
		await act(async () => direct.click());
		expect(selected).toHaveBeenLastCalledWith("direct", true);
	});

	it("sends manage providers to Settings → Providers", async () => {
		const listener = vi.fn();
		window.addEventListener(DCC_OPEN_SETTINGS_EVENT, listener);
		await act(async () => root.render(<Harness />));
		await act(async () => document.querySelector<HTMLButtonElement>('button[aria-label="composer.picker.manageProviders"]')!.click());
		expect((listener.mock.calls[0]![0] as CustomEvent).detail).toEqual({ section: "model" });
		window.removeEventListener(DCC_OPEN_SETTINGS_EVENT, listener);
	});

	it("does not pick while the composer is disabled", async () => {
		await act(async () => root.render(<Harness disabled />));
		expect(document.querySelector('[data-model-picker-panel]')).not.toBeNull();
		await act(async () => option("composer.effort.xhigh").click());
		expect(selected).not.toHaveBeenCalled();
	});

	it("opens the favorites editor, reorders by keyboard and removes a favorite", async () => {
		await act(async () => root.render(<Harness />));
		const edit = [...document.querySelectorAll<HTMLButtonElement>("button")]
			.find((button) => button.textContent === "composer.favorites.edit")!;
		await act(async () => edit.click());
		const dialog = document.querySelector<HTMLElement>('[role="dialog"]')!;
		expect(dialog).not.toBeNull();
		expect(document.querySelector('[data-model-picker-panel]')).toBeNull();
		const handle = dialog.querySelector<HTMLButtonElement>('button[aria-label^="composer.favorites.reorder:"]')!;
		await act(async () => handle.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })));
		expect(getModelFavorites()).toEqual([high, extraHigh]);
		await act(async () => dialog.querySelector<HTMLButtonElement>('button[aria-label^="composer.favorites.remove:"]')!.click());
		expect(getModelFavorites()).toEqual([extraHigh]);
	});
});

describe("compact picker", () => {
	it("shows the same picker in one dialog and closes after a pick", async () => {
		await act(async () => root.render(<Harness compact />));
		expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1);
		expect(document.querySelector('[data-model-picker-panel]')).not.toBeNull();
		await act(async () => option("composer.effort.xhigh").click());
		expect(selected.mock.calls).toEqual([["provider", "codex"], ["model", "gpt-6-astra"], ["effort", "xhigh"]]);
		expect(document.querySelector('[role="dialog"]')).toBeNull();
	});
});
