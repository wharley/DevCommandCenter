import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FALLBACK_PROVIDER_CATALOG } from "@/lib/fallback-provider-catalog";
import { ComposerExecutionMenu } from "./ComposerExecutionMenu";
import { getModelFavorites, saveModelFavorites } from "./model-favorites";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string, options?: { model?: string }) =>
		options?.model ? `${key}: ${options.model}` : key }),
}));

const high = { providerId: "codex", modelId: "gpt-6-astra", effort: "high" };
const extraHigh = { ...high, effort: "xhigh" };
let container: HTMLDivElement;
let root: Root;
const selected = vi.fn();

function Harness({ disabled = false, managed = false }: { disabled?: boolean; managed?: boolean }) {
	const [open, setOpen] = useState(true);
	const [providerId, setProviderId] = useState(managed ? "antigravity" : "cursor");
	const [modelId, setModelId] = useState(managed ? "default" : "auto");
	const [effort, setEffort] = useState("medium");
	return <ComposerExecutionMenu open={open} onOpenChange={setOpen}
		providers={FALLBACK_PROVIDER_CATALOG.providers}
		selectedProviderId={providerId} selectedModelId={modelId}
		availableEffortLevels={["low", "medium", "high", "xhigh"]} selectedEffortId={effort}
		directResponse={false} disabled={disabled}
		onSelectProvider={(value) => { selected("provider", value); setProviderId(value); setModelId("default"); }}
		onSelectModel={(value) => { selected("model", value); setModelId(value); }}
		onSelectEffort={(value) => { selected("effort", value); setEffort(value); }}
		onSelectUltrathink={() => { selected("effort", "ultrathink"); setEffort("ultrathink"); }}
		onSetDirectResponse={() => {}}
	/>;
}

function item(text: string) {
	const element = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')]
		.find((entry) => entry.textContent?.includes(text));
	expect(element, text).toBeDefined();
	return element!;
}

beforeEach(() => {
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
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

describe("favorite picker interactions", () => {
	it("shows saved order and applies provider, model and effort together", async () => {
		await act(async () => root.render(<Harness />));
		const rows = [...document.querySelectorAll('[role="menuitem"]')];
		expect(rows[0].textContent).toContain("composer.effort.xhigh");
		expect(rows[1].textContent).toContain("composer.effort.high");
		await act(async () => item("composer.effort.xhigh").click());
		expect(selected.mock.calls).toEqual([
			["provider", "codex"], ["model", "gpt-6-astra"], ["effort", "xhigh"],
		]);
		expect(container.textContent).toContain("composer.effort.xhigh");
		expect(document.querySelector('[role="menu"]')).toBeNull();
		expect(getModelFavorites()).toEqual([extraHigh, high]);
	});

	it("opens the editor, reorders by keyboard, edits effort and removes a favorite", async () => {
		await act(async () => root.render(<Harness />));
		await act(async () => item("composer.favorites.edit").click());
		const dialog = document.querySelector<HTMLElement>('[role="dialog"]')!;
		expect(dialog).not.toBeNull();
		expect(document.querySelector('[role="menu"]')).toBeNull();
		const handle = dialog.querySelector<HTMLButtonElement>('button[aria-label^="composer.favorites.reorder:"]')!;
		await act(async () => handle.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })));
		expect(getModelFavorites()).toEqual([high, extraHigh]);
		const effort = dialog.querySelector<HTMLSelectElement>('select[aria-label^="composer.favorites.effortFor:"]')!;
		await act(async () => {
			effort.value = "low";
			effort.dispatchEvent(new Event("change", { bubbles: true }));
		});
		expect(getModelFavorites()[0].effort).toBe("low");
		await act(async () => dialog.querySelector<HTMLButtonElement>('button[aria-label^="composer.favorites.remove:"]')!.click());
		expect(getModelFavorites()).toEqual([extraHigh]);
	});

	it("saves the current automatic model without assigning an unsupported effort", async () => {
		await act(async () => root.render(<Harness managed />));
		await act(async () => item("composer.favorites.saveCurrent").click());
		expect(getModelFavorites().at(-1)).toEqual({ providerId: "antigravity", modelId: "default", effort: null });
		expect(item("composer.favorites.saved").getAttribute("data-disabled")).not.toBeNull();
	});

	it("adds multiple efforts from the editor and restores focus when finished", async () => {
		await act(async () => root.render(<Harness />));
		await act(async () => item("composer.favorites.edit").click());
		const dialog = document.querySelector<HTMLElement>('[role="dialog"]')!;
		const form = dialog.querySelector("form")!;
		await act(async () => form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
		expect(getModelFavorites().at(-1)).toEqual({ providerId: "cursor", modelId: "auto", effort: "medium" });
		expect(form.querySelector<HTMLButtonElement>('button[type="submit"]')!.disabled).toBe(true);
		const effort = form.querySelector<HTMLSelectElement>('select[id$="-effort"]')!;
		await act(async () => {
			effort.value = "high";
			effort.dispatchEvent(new Event("change", { bubbles: true }));
		});
		await act(async () => form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
		expect(getModelFavorites().slice(-2).map((entry) => entry.effort)).toEqual(["medium", "high"]);
		expect(selected).not.toHaveBeenCalled();
		const done = [...dialog.querySelectorAll("button")].find((button) => button.textContent === "composer.favorites.done")!;
		await act(async () => done.click());
		expect(document.querySelector('[role="dialog"]')).toBeNull();
		// Radix restores focus in a deferred unmount callback.
		await act(async () => {
			await vi.waitFor(() => expect(document.activeElement).toBe(container.querySelector("button")));
		});
	});

	it("applies an ultrathink favorite through the existing effort control", async () => {
		saveModelFavorites([{ ...high, effort: "ultrathink" }]);
		await act(async () => root.render(<Harness />));
		await act(async () => item("composer.effort.ultrathink").click());
		expect(selected.mock.calls).toEqual([
			["provider", "codex"], ["model", "gpt-6-astra"], ["effort", "ultrathink"],
		]);
	});

	it.each(["disabled", "unavailable"])("does not apply a %s favorite", async (reason) => {
		if (reason === "unavailable") saveModelFavorites([{ ...high, effort: "removed-effort" }]);
		await act(async () => root.render(<Harness disabled={reason === "disabled"} />));
		const favorite = item(reason === "disabled" ? "composer.effort.xhigh" : "composer.favorites.unavailable");
		expect(favorite.getAttribute("data-disabled")).not.toBeNull();
		await act(async () => favorite.click());
		expect(selected).not.toHaveBeenCalled();
	});
});
