import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Repository } from "@dcc/contracts";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { CreateWorkspaceDialog } from "./create-workspace-dialog";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../../lib/workspace-api", () => ({
	listLocalBranches: vi.fn(async () => ({ branches: ["main"] })),
	resolveWorkspaceSourceUrl: vi.fn(),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

const dcc = {
	id: "repo-dcc",
	projectId: "dcc",
	name: "dcc",
	displayName: "DevCommandCenter",
	rootPath: "/code/dcc",
	baseBranch: "main",
} as Repository;
const lp = {
	id: "repo-lp",
	projectId: "lp",
	name: "lp",
	displayName: "DCC LP",
	rootPath: "/code/lp",
	baseBranch: "develop",
} as Repository;

let root: Root;
let host: HTMLDivElement;
const onCreateWorkspace = vi.fn(async () => ({
	workspace: { id: "single" },
	setupHints: [],
	setupReport: { status: "skipped", steps: [], message: null },
}));
const onCreateWorkspaceBundle = vi.fn(async () => ({
	workspaces: [],
	primaryWorkspace: { id: "bundle" },
}));

async function render() {
	await act(async () =>
		root.render(
			<CreateWorkspaceDialog
				open
				mode="open"
				linkedFoldersByRepositoryId={{ "repo-dcc": ["/code/lp", "/docs/specs"] }}
				contextPathsByRepositoryId={{ "repo-dcc": ["/docs/specs"] }}
				onOpenChange={() => {}}
				onCreateWorkspace={onCreateWorkspace as never}
				onCreateWorkspaceFromSourceUrl={vi.fn() as never}
				onCreateWorkspaceBundle={onCreateWorkspaceBundle as never}
				onCloneWorkspace={vi.fn() as never}
				repositories={[dcc, lp]}
				isSubmitting={false}
			/>,
		),
	);
}

const byText = (text: string) =>
	[...document.querySelectorAll<HTMLElement>("button, label")].find((element) =>
		element.textContent?.includes(text),
	)!;
const submit = async () =>
	act(async () => {
		document
			.querySelector<HTMLFormElement>("form")!
			.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
	});

beforeEach(() => {
	(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
	host = document.createElement("div");
	document.body.append(host);
	root = createRoot(host);
});

afterEach(async () => {
	await act(async () => root.unmount());
	host.remove();
	onCreateWorkspace.mockClear();
	onCreateWorkspaceBundle.mockClear();
});

it("hides other projects when starting from an existing branch or PR", async () => {
	await act(async () =>
		root.render(
			<CreateWorkspaceDialog
				open
				mode="open"
				repositoryContext={{
					projectId: "dcc",
					workspaceRoot: "/code/dcc",
					label: "DevCommandCenter",
				}}
				linkedFoldersByRepositoryId={{ "repo-dcc": ["/code/lp"] }}
				onOpenChange={() => {}}
				onCreateWorkspace={onCreateWorkspace as never}
				onCreateWorkspaceFromSourceUrl={vi.fn() as never}
				onCreateWorkspaceBundle={onCreateWorkspaceBundle as never}
				onCloneWorkspace={vi.fn() as never}
				repositories={[dcc, lp]}
				isSubmitting={false}
			/>,
		),
	);
	expect(document.body.textContent).not.toContain("workspaceDialog.alsoInTask");
	await act(async () => byText("workspaceDialog.newWorkspaceStart").click());
	expect(document.body.textContent).toContain("workspaceDialog.alsoInTask");
	expect(byText("DCC LP")).toBeTruthy();
});

it("brings the project's linked folders and creates a multi-project task", async () => {
	await render();
	expect(byText("DCC LP")).toBeTruthy();
	expect(byText("specs")).toBeTruthy();
	await submit();
	expect(onCreateWorkspaceBundle).toHaveBeenCalledWith({
		name: "sidebar.newWorkspace",
		projects: [
			expect.objectContaining({
				workspaceRoot: "/code/dcc",
				isolationMode: null,
				contextFolders: ["/docs/specs"],
			}),
			expect.objectContaining({
				workspaceRoot: "/code/lp",
				baseBranch: "develop",
				isolationMode: null,
			}),
		],
	});
	expect(onCreateWorkspace).not.toHaveBeenCalled();
});

it("stays a single-project task with its context when the other project is removed", async () => {
	await render();
	// The chip, not the project grid (which would make LP the primary).
	await act(async () =>
		document
			.querySelector<HTMLButtonElement>('button[aria-label="workspaceDialog.removeFromTask"]')!
			.click(),
	);
	await act(async () => byText("newTask.execution.worktree").click());
	await submit();
	expect(onCreateWorkspaceBundle).not.toHaveBeenCalled();
	expect(onCreateWorkspace).toHaveBeenCalledWith(
		expect.objectContaining({
			workspaceRoot: "/code/dcc",
			isolationMode: "localDirect",
			contextFolders: ["/docs/specs"],
		}),
	);
});

it("opens another project at the folder picker, with no recent project preselected", async () => {
	vi.mocked(openDialog).mockResolvedValueOnce(null);
	await act(async () =>
		root.render(
			<CreateWorkspaceDialog
				open
				mode="open"
				startWithFolderPicker
				onOpenChange={() => {}}
				onCreateWorkspace={onCreateWorkspace as never}
				onCreateWorkspaceFromSourceUrl={vi.fn() as never}
				onCreateWorkspaceBundle={onCreateWorkspaceBundle as never}
				onCloneWorkspace={vi.fn() as never}
				repositories={[dcc, lp]}
				isSubmitting={false}
			/>,
		),
	);
	expect(openDialog).toHaveBeenCalledTimes(1);
	await submit();
	expect(onCreateWorkspace).not.toHaveBeenCalled();
});
