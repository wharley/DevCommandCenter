import { beforeEach, describe, expect, it, vi } from "vitest";

const terminalApi = vi.hoisted(() => ({
	getDefaultShell: vi.fn(),
	getOrCreateTerminalByOwner: vi.fn(),
	getTerminalBackendScope: vi.fn(() => "test"),
	killTerminal: vi.fn(),
	listenTerminalExit: vi.fn(),
	listenTerminalOutput: vi.fn(),
	listTerminalRuntimeActivity: vi.fn(),
	resizeTerminal: vi.fn(),
	writeTerminalStdin: vi.fn(),
}));

vi.mock("@/lib/terminal-api", () => ({
	getDefaultShell: terminalApi.getDefaultShell,
	getOrCreateTerminalByOwner: terminalApi.getOrCreateTerminalByOwner,
	getTerminalBackendScope: terminalApi.getTerminalBackendScope,
	killTerminal: terminalApi.killTerminal,
	listenTerminalExit: terminalApi.listenTerminalExit,
	listenTerminalOutput: terminalApi.listenTerminalOutput,
	listTerminalRuntimeActivity: terminalApi.listTerminalRuntimeActivity,
	resizeTerminal: terminalApi.resizeTerminal,
	writeTerminalStdin: terminalApi.writeTerminalStdin,
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
		terminalApi.listTerminalRuntimeActivity.mockResolvedValue([]);
		terminalApi.resizeTerminal.mockReset();
		terminalApi.killTerminal.mockResolvedValue({ ok: true });
		terminalApi.writeTerminalStdin.mockResolvedValue({ ok: true });
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

	it("restores backlog and produces a bounded plain-text agent excerpt", async () => {
		terminalApi.getOrCreateTerminalByOwner.mockResolvedValueOnce({
			ptyId: "pty-backlog",
			existing: true,
			session: { status: "running", lastExitCode: null },
			chunks: [
				"\x1b[32mready\x1b[0m\r\n",
				"\x1b[31mfailed test\x1b[0m\r\n",
			],
			truncated: false,
		});
		const { ensureTerminal, getTerminalContextExcerpt } = await import(
			"./terminal-store"
		);

		await ensureTerminal("terminal-backlog", "/workspace", {
			title: "Tests",
			workspaceName: "Workspace",
			workspaceBranch: "dcc/task",
			providerLabel: null,
			sessionState: "idle",
			sessionId: null,
		});

		expect(getTerminalContextExcerpt("terminal-backlog")).toBe(
			"ready\nfailed test",
		);
	});

	it("terminates only terminals owned by the completed workspace", async () => {
		terminalApi.getOrCreateTerminalByOwner.mockImplementation(
			async (ownerKey: string) => ({
				ptyId: `pty-${ownerKey}`,
				existing: false,
				session: { status: "running", lastExitCode: null },
				chunks: [],
				truncated: false,
			}),
		);
		const { ensureTerminal, terminateWorkspaceTerminals } = await import(
			"./terminal-store"
		);
		const context = {
			title: "Terminal",
			workspaceName: "Workspace",
			workspaceBranch: "dcc/task",
			providerLabel: null,
			sessionState: "idle",
			sessionId: null,
		};

		await ensureTerminal("worktree:workspace-a:tab-1", "/a", context);
		await ensureTerminal("worktree:workspace-b:tab-1", "/b", context);
		expect(await terminateWorkspaceTerminals(["workspace-a"])).toBe(1);

		expect(terminalApi.killTerminal).toHaveBeenCalledTimes(1);
		expect(terminalApi.killTerminal).toHaveBeenCalledWith(
			"pty-terminal:worktree:workspace-a:tab-1",
		);
	});
});
