import { beforeEach, describe, expect, it, vi } from "vitest";

const terminalApi = vi.hoisted(() => ({
	getDefaultShell: vi.fn(),
	getOrCreateTerminalByOwner: vi.fn(),
	getTerminalBackendScope: vi.fn(() => "test"),
	listenTerminalExit: vi.fn(),
	listenTerminalOutput: vi.fn(),
	resizeTerminal: vi.fn(),
}));

vi.mock("@/lib/terminal-api", () => ({
	getDefaultShell: terminalApi.getDefaultShell,
	getOrCreateTerminalByOwner: terminalApi.getOrCreateTerminalByOwner,
	getTerminalBackendScope: terminalApi.getTerminalBackendScope,
	killTerminal: vi.fn(),
	listenTerminalExit: terminalApi.listenTerminalExit,
	listenTerminalOutput: terminalApi.listenTerminalOutput,
	resizeTerminal: terminalApi.resizeTerminal,
	writeTerminalStdin: vi.fn(),
}));

describe("terminal sizing during PTY startup", () => {
	beforeEach(() => {
		vi.resetModules();
		terminalApi.getDefaultShell.mockResolvedValue({ shell: "/bin/zsh" });
		terminalApi.getOrCreateTerminalByOwner.mockResolvedValue({
			ptyId: "pty-1",
			existing: false,
			session: { status: "running", lastExitCode: null },
			chunks: [],
			truncated: false,
		});
		terminalApi.listenTerminalExit.mockResolvedValue(() => {});
		terminalApi.listenTerminalOutput.mockResolvedValue(() => {});
		terminalApi.resizeTerminal.mockReset();
	});

	it("applies the view size captured before the PTY is ready", async () => {
		const { ensureTerminal, resizeTerminalView } = await import("./terminal-store");

		resizeTerminalView("terminal-1", 94, 18);
		expect(terminalApi.resizeTerminal).not.toHaveBeenCalled();

		await ensureTerminal("terminal-1", "/workspace", {
			title: "Terminal",
			workspaceName: "Workspace",
			workspaceBranch: "main",
			providerLabel: null,
			sessionState: "idle",
			sessionId: null,
		});

		expect(terminalApi.resizeTerminal).toHaveBeenCalledWith("pty-1", 94, 18);
	});
});
