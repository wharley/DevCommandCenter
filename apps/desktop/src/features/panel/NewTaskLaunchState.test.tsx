import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Repository } from "@dcc/contracts";
import { NewTaskLaunchState } from "./NewTaskLaunchState";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

const repository = {
	id: "repo-1",
	projectId: "project-1",
	name: "dcc",
	displayName: "DCC",
	rootPath: "/tmp/dcc",
	baseBranch: "main",
} as Repository;

let root: Root;
let host: HTMLDivElement;
const onSelectProject = vi.fn(async () => {});

beforeEach(() => {
	(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT =
		true;
	host = document.createElement("div");
	document.body.append(host);
	root = createRoot(host);
	act(() =>
		root.render(
			<NewTaskLaunchState
				repositories={[repository]}
				isCreating={false}
				onSelectProject={onSelectProject}
				onSelectMultiple={() => {}}
				onOpenProject={() => {}}
			/>,
		),
	);
});

afterEach(() => {
	act(() => root.unmount());
	host.remove();
	onSelectProject.mockClear();
});

const worktree = () =>
	host.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
const shortcut = () =>
	host.querySelector<HTMLButtonElement>(".dcc-project-shortcut")!;

it("starts the task in a worktree by default", async () => {
	expect(worktree().checked).toBe(true);
	await act(async () => shortcut().click());
	expect(onSelectProject).toHaveBeenCalledWith(repository, "protectedWorktree", true);
});

it("starts the task in the current checkout when worktree is unchecked", async () => {
	act(() => worktree().click());
	expect(worktree().checked).toBe(false);
	expect(host.textContent).toContain("newTask.execution.localDescription");
	await act(async () => shortcut().click());
	expect(onSelectProject).toHaveBeenCalledWith(repository, "localDirect", true);
});

it("only offers linked folders when a project links some, and can skip them", async () => {
	expect(host.textContent).not.toContain("newTask.linkedFolders");
	act(() =>
		root.render(
			<NewTaskLaunchState
				repositories={[repository]}
				isCreating={false}
				onSelectProject={onSelectProject}
				onSelectMultiple={() => {}}
				onOpenProject={() => {}}
				linkedFolderCounts={{ [repository.id]: 2 }}
			/>,
		),
	);
	expect(shortcut().textContent).toContain("+2");
	const linked = host.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[1]!;
	expect(linked.checked).toBe(true);
	act(() => linked.click());
	expect(shortcut().textContent).not.toContain("+2");
	await act(async () => shortcut().click());
	expect(onSelectProject).toHaveBeenCalledWith(repository, "protectedWorktree", false);
});
