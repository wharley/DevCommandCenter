import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import i18n from "@/i18n/config";
import * as api from "@/lib/browser-data-api";
import { BrowserDataPreference } from "./browser-data-preference";

vi.mock("@/lib/browser-data-api", () => ({
	readBrowserStorageUsage: vi.fn(),
	clearBrowserData: vi.fn(),
}));

const MB = 1024 * 1024;
let root: Root;
let host: HTMLDivElement;

beforeEach(async () => {
	vi.clearAllMocks();
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	await i18n.changeLanguage("pt-BR");
	vi.mocked(api.readBrowserStorageUsage).mockResolvedValue({
		supported: true,
		cacheBytes: 692 * MB,
		totalBytes: 710 * MB,
	});
	host = document.createElement("div");
	document.body.append(host);
	root = createRoot(host);
});
afterEach(async () => {
	await act(async () => root.unmount());
	host.remove();
	vi.unstubAllGlobals();
});

const mount = async () => {
	await act(async () => root.render(<BrowserDataPreference />));
};
const button = (label: string) =>
	[...host.querySelectorAll("button")].find((candidate) => candidate.textContent === label)!;

it("shows what the browser keeps and clears only the cache by default", async () => {
	vi.mocked(api.clearBrowserData).mockResolvedValue({
		supported: true,
		cacheBytes: 0,
		totalBytes: 18 * MB,
	});
	await mount();
	expect(host.textContent).toContain("Cache: 692 MB");

	await act(async () => button("Limpar cache").click());

	expect(api.clearBrowserData).toHaveBeenCalledWith(false);
	expect(host.textContent).toContain("o cache agora ocupa 0 B");
});

it("asks before signing the browser out of every site", async () => {
	const confirm = vi.fn().mockReturnValue(false);
	vi.stubGlobal("confirm", confirm);
	await mount();

	await act(async () => button("Limpar tudo").click());

	expect(confirm).toHaveBeenCalled();
	expect(api.clearBrowserData).not.toHaveBeenCalled();
});

it("stays hidden where the browser keeps nothing on disk", async () => {
	vi.mocked(api.readBrowserStorageUsage).mockResolvedValue({
		supported: false,
		cacheBytes: 0,
		totalBytes: 0,
	});
	await mount();
	expect(host.querySelector("[data-testid=browser-data-preference]")).toBeNull();
});
