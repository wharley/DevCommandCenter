import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { toast } from "sonner";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import i18n from "@/i18n/config";
import * as api from "@/lib/dcc-storage-api";
import { DccDataSection } from "./dcc-data-section";

vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
vi.mock("@/lib/dcc-storage-api", async (importOriginal) => ({
	...(await importOriginal<typeof api>()),
	readDccStorageUsage: vi.fn(),
	compactDccStorage: vi.fn(),
}));

const MB = 1024 * 1024;
const before: api.DccStorageUsage = {
	databaseBytes: 514 * MB,
	reclaimableBytes: 50 * MB,
	reclaimsOnRestart: false,
	browserCacheBytes: 692 * MB,
	appWebviewBytes: 720 * MB,
};
let root: Root;
let host: HTMLDivElement;

beforeEach(async () => {
	vi.clearAllMocks();
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	await i18n.changeLanguage("pt-BR");
	vi.mocked(api.readDccStorageUsage).mockResolvedValue(before);
	host = document.createElement("div");
	document.body.append(host);
	root = createRoot(host);
});
afterEach(async () => {
	await act(async () => root.unmount());
	host.remove();
	vi.unstubAllGlobals();
});

const mount = async (disabled = false) => {
	await act(async () => root.render(<DccDataSection open disabled={disabled} />));
};
const compactButton = () =>
	[...host.querySelectorAll("button")].find((button) => button.textContent === "Compactar")!;

it("shows what DCC keeps on disk", async () => {
	await mount();
	expect(host.textContent).toContain("Banco 514 MB (50 MB recuperáveis)");
	expect(host.textContent).toContain("cache do navegador 692 MB");
});

it("compacts and reports what left the disk", async () => {
	vi.mocked(api.compactDccStorage).mockResolvedValue({
		...before,
		databaseBytes: 130 * MB,
		reclaimableBytes: 0,
		browserCacheBytes: 0,
	});
	await mount();

	await act(async () => compactButton().click());

	expect(toast.success).toHaveBeenCalledWith("Liberados 1,1 GB.");
	expect(host.textContent).toContain("Banco 130 MB");
});

it("says the rest leaves the disk when DCC reopens", async () => {
	vi.mocked(api.compactDccStorage).mockResolvedValue({
		...before,
		reclaimableBytes: 300 * MB,
		reclaimsOnRestart: true,
		browserCacheBytes: 0,
	});
	await mount();

	await act(async () => compactButton().click());

	expect(toast.success).toHaveBeenCalledWith(
		"Liberados 692 MB; mais 300 MB saem do disco quando o DCC reabrir.",
	);
});

it("waits while tasks are being deleted", async () => {
	await mount(true);
	expect(compactButton().disabled).toBe(true);
});
