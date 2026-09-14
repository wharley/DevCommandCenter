import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { BrowserSessionImportDialog } from "./browser-session-import-dialog";

const mocks = vi.hoisted(() => ({ profiles: vi.fn(), import: vi.fn(), toast: vi.fn() }));
vi.mock("./browser-api", () => ({
	listBrowserSessionImportProfiles: mocks.profiles,
	importBrowserSession: mocks.import,
}));
vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string, values?: Record<string, string>) => values ? `${key}:${Object.values(values).join(":")}` : key }),
}));
vi.mock("sonner", () => ({ toast: { success: mocks.toast } }));

let root: Root;
let container: HTMLDivElement;
const onOpenChange = vi.fn();
const onImported = vi.fn();

beforeEach(() => {
	(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
	vi.clearAllMocks();
	mocks.profiles.mockResolvedValue({ supported: true, profiles: [{ id: "chrome-default", browser: "Chrome", name: "Default" }] });
	mocks.import.mockResolvedValue({ supported: true, imported: 1, skipped: 0 });
	container = document.createElement("div"); document.body.append(container); root = createRoot(container);
});
afterEach(async () => { await act(async () => root.unmount()); container.remove(); });
const button = (text: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find((item) => item.textContent?.includes(text))!;

async function render(url = "https://example.com/settings") {
	await act(async () => root.render(<BrowserSessionImportDialog open onOpenChange={onOpenChange} workspaceId="workspace" sessionId="session" lifecycleToken={7} currentUrl={url} onImported={onImported} />));
	await act(async () => { await Promise.resolve(); });
}

describe("BrowserSessionImportDialog", () => {
	it("shows metadata only and imports the explicitly selected profile for the current site", async () => {
		await render();
		expect(document.body.textContent).toContain("Chrome");
		expect(document.body.textContent).toContain("Default");
		await act(async () => button("Chrome").click());
		await act(async () => button("browser.sessions.import").click());
		expect(mocks.import).toHaveBeenCalledWith({ workspaceId: "workspace", sessionId: "session", lifecycleToken: 7, profileId: "chrome-default", currentUrl: "https://example.com/settings" });
		expect(onImported).toHaveBeenCalledOnce();
		expect(onOpenChange).toHaveBeenCalledWith(false);
	});

	it("keeps import unavailable until an HTTP(S) site is open", async () => {
		await render("not a URL");
		await act(async () => button("Chrome").click());
		expect(button("browser.sessions.import").disabled).toBe(true);
		expect(mocks.import).not.toHaveBeenCalled();
	});

	it("ignores an import result after the browser scope changes", async () => {
		let resolveImport!: (value: { supported: boolean; imported: number; skipped: number }) => void;
		mocks.import.mockReturnValueOnce(new Promise((resolve) => { resolveImport = resolve; }));
		await render();
		await act(async () => button("Chrome").click());
		await act(async () => button("browser.sessions.import").click());
		await act(async () => root.render(<BrowserSessionImportDialog open onOpenChange={onOpenChange} workspaceId="workspace" sessionId="session" lifecycleToken={8} currentUrl="https://example.com/other" onImported={onImported} />));
		await act(async () => resolveImport({ supported: true, imported: 1, skipped: 0 }));
		expect(onImported).not.toHaveBeenCalled();
		expect(onOpenChange).not.toHaveBeenCalledWith(false);
	});

	it("does not let an old import close a newly reopened dialog", async () => {
		let resolveImport!: (value: { supported: boolean; imported: number; skipped: number }) => void;
		mocks.import.mockReturnValueOnce(new Promise((resolve) => { resolveImport = resolve; }));
		await render();
		await act(async () => button("Chrome").click());
		await act(async () => button("browser.sessions.import").click());
		await act(async () => root.render(<BrowserSessionImportDialog open={false} onOpenChange={onOpenChange} workspaceId="workspace" sessionId="session" lifecycleToken={7} currentUrl="https://example.com/settings" onImported={onImported} />));
		await act(async () => root.render(<BrowserSessionImportDialog open onOpenChange={onOpenChange} workspaceId="workspace" sessionId="session" lifecycleToken={7} currentUrl="https://example.com/settings" onImported={onImported} />));
		await act(async () => resolveImport({ supported: true, imported: 1, skipped: 0 }));
		expect(onImported).not.toHaveBeenCalled();
		expect(onOpenChange).not.toHaveBeenCalledWith(false);
	});

	it("reports imported and skipped counts before reloading", async () => {
		mocks.import.mockResolvedValueOnce({ supported: true, imported: 1, skipped: 2 });
		await render();
		await act(async () => button("Chrome").click());
		await act(async () => button("browser.sessions.import").click());
		expect(mocks.toast).toHaveBeenCalledWith("browser.sessions.result:1:2");
		expect(onImported).toHaveBeenCalledOnce();
	});

	it("shows the backend message when an explicit import skips every session", async () => {
		mocks.import.mockResolvedValueOnce({ supported: true, imported: 0, skipped: 1, message: "Profile is unavailable" });
		await render();
		await act(async () => button("Chrome").click());
		await act(async () => button("browser.sessions.import").click());
		expect(document.body.textContent).toContain("Profile is unavailable");
		expect(onImported).not.toHaveBeenCalled();
	});
});
