import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import i18n from "@/i18n/config";
import * as api from "@/lib/keep-awake-api";
import { KeepAwakePreference } from "./keep-awake-preference";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@/lib/keep-awake-api", async (importOriginal) => ({
	...(await importOriginal<typeof api>()),
	readKeepAwake: vi.fn(),
	setKeepAwakeMode: vi.fn(),
}));

let root: Root;
let host: HTMLDivElement;
let changed: () => void;
const stop = vi.fn();
const off: api.KeepAwakeStatus = { supported: true, mode: "off", active: null };

beforeEach(async () => {
	vi.clearAllMocks();
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	await i18n.changeLanguage("pt-BR");
	vi.mocked(listen).mockImplementation(async (_name, callback) => {
		changed = () => (callback as (event: unknown) => void)({});
		return stop;
	});
	vi.mocked(api.readKeepAwake).mockResolvedValue(off);
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
	await act(async () => root.render(<KeepAwakePreference />));
};
const option = (label: string) =>
	[...host.querySelectorAll("button")].find((button) => button.textContent === label)!;

it("offers the three modes, defaults to off and says the lid still sleeps", async () => {
	await mount();
	expect(option("Desligado").getAttribute("data-state")).toBe("on");
	expect(option("Sempre")).toBeDefined();
	expect(option("Durante agentes")).toBeDefined();
	expect(host.textContent).toContain("Com a tampa fechada o macOS dorme mesmo assim");
	expect(host.textContent).not.toContain("Mantendo acordado agora");
});

it("saves the chosen mode and shows why the Mac is being held", async () => {
	vi.mocked(api.setKeepAwakeMode).mockResolvedValue({
		supported: true,
		mode: "during_agents",
		active: { reason: { kind: "agentsWorking", count: 2 }, label: "2 agentes trabalhando" },
	});
	await mount();
	await act(async () => option("Durante agentes").click());
	expect(api.setKeepAwakeMode).toHaveBeenCalledWith("during_agents");
	expect(option("Durante agentes").getAttribute("data-state")).toBe("on");
	expect(host.textContent).toContain("Mantendo acordado agora: 2 agentes trabalhando");
});

it("follows backend changes and goes idle when the agents stop", async () => {
	vi.mocked(api.readKeepAwake).mockResolvedValueOnce({
		supported: true,
		mode: "during_agents",
		active: { reason: { kind: "agentsWorking", count: 1 }, label: "1 agente trabalhando" },
	});
	await mount();
	expect(host.textContent).toContain("1 agente trabalhando");
	vi.mocked(api.readKeepAwake).mockResolvedValueOnce({ supported: true, mode: "during_agents", active: null });
	await act(async () => changed());
	expect(host.textContent).toContain("Nenhum agente trabalhando agora");
	await act(async () => root.unmount());
	expect(stop).toHaveBeenCalledTimes(1);
	root = createRoot(host);
});

it("is hidden where the platform cannot keep the computer awake", async () => {
	vi.mocked(api.readKeepAwake).mockResolvedValue({ ...off, supported: false });
	await mount();
	expect(host.querySelector('[data-testid="keep-awake-preference"]')).toBeNull();
});

it("reports a failed save without changing the mode", async () => {
	vi.mocked(api.setKeepAwakeMode).mockRejectedValue(new Error("disco cheio"));
	await mount();
	await act(async () => option("Sempre").click());
	expect(host.querySelector('[role="alert"]')?.textContent).toContain("disco cheio");
	expect(option("Desligado").getAttribute("data-state")).toBe("on");
});
