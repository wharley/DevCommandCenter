import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { composerTurnFromRaw } from "@/features/composer/composer-turn";
import { FALLBACK_PROVIDER_CATALOG } from "@/lib/fallback-provider-catalog";
import { QuickComposer } from "./QuickComposer";
import type { QuickLaunch } from "./api";

vi.mock("react-i18next", () => {
	const t = (key: string) => key;
	const i18n = { changeLanguage: vi.fn() };
	return { useTranslation: () => ({ t, i18n }) };
});
vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn(async () => () => {}),
}));
const accepted = vi.fn();
vi.mock("@/features/composer/WorkspaceComposer", () => ({
	WorkspaceComposer: (props: {
		disabled: boolean;
		onSubmitPrompt: (
			turn: ReturnType<typeof composerTurnFromRaw>,
		) => Promise<boolean>;
	}) => (
		<button
			disabled={props.disabled}
			onClick={() =>
				void props
					.onSubmitPrompt(
						composerTurnFromRaw("Fix @/tmp/capture.png", {
							planMode: true,
							effort: "high",
						}),
					)
					.then(accepted)
			}
		>
			submit-fixture
		</button>
	),
}));
let root: Root;
let host: HTMLDivElement;
let launch: QuickLaunch | null;
let failStart: boolean;
let failHide: boolean;
let taskState: string | null;
let failTaskLookup: boolean;
const ipc = vi.fn();
const button = (label: string) =>
	[...host.querySelectorAll("button")].find(
		(element) => element.textContent === label,
	)!;
beforeEach(() => {
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	localStorage.clear();
	launch = null;
	failStart = false;
	failHide = false;
	taskState = "ready";
	failTaskLookup = false;
	vi.mocked(listen).mockClear();
	accepted.mockClear();
	ipc.mockReset();
	ipc.mockImplementation((command, args) => {
		switch (command) {
			case "list_repositories":
				return {
					repositories: [
						{
							id: "repo",
							projectId: "project",
							name: "Fixture",
							rootPath: "/fixture/repo",
							baseBranch: "main",
						},
					],
				};
			case "list_workspaces":
				if (failTaskLookup) throw new Error("Unavailable history");
				return {
					workspaces: taskState ? [{ id: "workspace", state: taskState }] : [],
				};
			case "list_providers":
				return { catalog: FALLBACK_PROVIDER_CATALOG };
			case "quick_composer_status":
				return {
					supported: true,
					shortcut: null,
					shortcutError: false,
					launch,
				};
			case "quick_composer_begin":
			case "quick_composer_checkpoint":
				launch = args.launch;
				return launch;
			case "create_workspace_for_repo":
				return { workspace: { id: "workspace" } };
			case "start_thread":
				if (failStart) throw new Error("Provider unavailable");
				return { session: { id: "session" } };
			case "prepare_turn":
				return { preflight: { state: "ready" } };
			case "send_turn":
				return {};
			case "quick_composer_hide":
				if (failHide) throw new Error("Cannot hide");
				return;
			default:
				return null;
		}
	});
	mockIPC(ipc);
	host = document.createElement("div");
	document.body.append(host);
	root = createRoot(host);
});
afterEach(async () => {
	await act(async () => root.unmount());
	host.remove();
	clearMocks();
	vi.unstubAllGlobals();
});
it.each(["localDirect", "protectedWorktree"] as const)(
	"routes the %s choice and composer envelope through the existing APIs",
	async (mode) => {
		await act(async () => root.render(<QuickComposer />));
		await act(async () => button(`quickComposer.${mode}`).click());
		await act(async () => button("submit-fixture").click());
		expect(ipc).toHaveBeenCalledWith("create_workspace_for_repo", {
			input: expect.objectContaining({
				projectId: "project",
				workspaceRoot: "/fixture/repo",
				isolationMode: mode,
			}),
		});
		expect(ipc).toHaveBeenCalledWith("start_thread", {
			input: expect.objectContaining({
				workspaceId: "workspace",
				projectId: "project",
			}),
		});
		expect(ipc).toHaveBeenCalledWith("send_turn", {
			input: expect.objectContaining({
				sessionId: "session",
				prompt: "Fix @/tmp/capture.png",
				planMode: true,
				effort: "high",
			}),
		});
		expect(accepted).toHaveBeenLastCalledWith(true);
		expect(launch?.phase).toBe("completed");
	},
);
it("keeps an accepted send accepted when window dismissal fails", async () => {
	failHide = true;
	await act(async () => root.render(<QuickComposer />));
	await act(async () => button("submit-fixture").click());
	expect(accepted).toHaveBeenLastCalledWith(true);
	expect(host.textContent).toContain("Cannot hide");
});
it("preserves a partial task and requires review before another request", async () => {
	failStart = true;
	await act(async () => root.render(<QuickComposer />));
	await act(async () => button("submit-fixture").click());
	expect(launch).toMatchObject({
		phase: "failed",
		workspaceId: "workspace",
		sessionId: null,
	});
	expect(accepted).toHaveBeenLastCalledWith(false);
	expect(button("submit-fixture").disabled).toBe(true);
	await act(async () => button("quickComposer.stopRecovery").click());
	expect(button("submit-fixture").disabled).toBe(false);
	expect(
		ipc.mock.calls.filter(
			([command]) => command === "create_workspace_for_repo",
		),
	).toHaveLength(1);
});

const reopen = async () => {
	const onShown = vi
		.mocked(listen)
		.mock.calls.find(([name]) => name === "quick-composer-shown")![1];
	await act(async () => {
		onShown({ event: "quick-composer-shown", id: 1, payload: null });
	});
};
it.each(["completed", "archived", null])(
	"drops stale confirmation after a task becomes %s",
	async (state) => {
		await act(async () => root.render(<QuickComposer />));
		await act(async () => button("submit-fixture").click());
		expect(host.textContent).toContain("quickComposer.started");
		taskState = state;
		await reopen();
		expect(host.textContent).not.toContain("quickComposer.started");
		expect(host.textContent).not.toContain("quickComposer.openTask");
		expect(button("submit-fixture").disabled).toBe(false);
		expect(launch?.phase).toBe("completed"); // Durable send receipt is preserved.
	},
);
it("keeps a link to an existing open task without resending on reopen", async () => {
	await act(async () => root.render(<QuickComposer />));
	await act(async () => button("submit-fixture").click());
	await reopen();
	expect(host.textContent).toContain("quickComposer.started");
	expect(
		ipc.mock.calls.filter(([command]) => command === "send_turn"),
	).toHaveLength(1);
});
it("allows new requests when history is unavailable, without claiming the old task is open", async () => {
	await act(async () => root.render(<QuickComposer />));
	await act(async () => button("submit-fixture").click());
	failTaskLookup = true;
	await reopen();
	expect(host.textContent).not.toContain("quickComposer.started");
	expect(button("submit-fixture").disabled).toBe(false);
});
