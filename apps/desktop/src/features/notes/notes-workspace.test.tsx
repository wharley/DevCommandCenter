import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { NotesWorkspace, type NotesScope } from "./notes-workspace";
import { NotesStore } from "./notes-store";
import { notePrompt, type ProjectNote } from "./notes-api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, values?: { project?: string }) =>
			values?.project ? `${key}: ${values.project}` : key,
	}),
}));
vi.mock("sonner", () => ({ toast: { info: vi.fn(), success: vi.fn(), error: vi.fn() } }));

const note: ProjectNote = {
	id: "idea", projectId: "project-a", projectName: "Project A",
	title: "Idea", content: "Project A context", contextSnapshot: "Saved context",
	sourceSessionId: null, sourceWorkspaceId: null, sourceTaskTitle: "",
	implementationTaskId: null, status: "open", color: "amber", pinned: true,
	revision: 1, createdAt: "t0", updatedAt: "t0", completedAt: null,
};
const scope: NotesScope = {
	projectId: "project-a", projectName: "Project A", workspaceId: "task-a",
	sessionId: "session-a", taskTitle: "Task A",
};
let container: HTMLDivElement;
let root: Root;
let store: NotesStore;
const onUse = vi.fn();
const onCreateTask = vi.fn(async () => "new-task");

async function render(destination: NotesScope | null, open = false) {
	await act(async () => {
		root.render(<NotesWorkspace
			controller={{ store, ...store.getSnapshot() }}
			open={open} onOpenChange={() => {}} scope={destination}
			projects={[{ id: note.projectId, name: note.projectName }]}
			onUse={onUse} onCreateTask={onCreateTask}
			completionTaskIds={[]} onCompletionHandled={() => {}}
		/>);
	});
}
function button(label: string) {
	const result = container.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
	expect(result).not.toBeNull();
	return result!;
}
beforeEach(async () => {
	vi.clearAllMocks();
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	vi.stubGlobal("CSS", { escape: (value: string) => value });
	vi.stubGlobal("matchMedia", vi.fn(() => ({
		matches: true, addListener() {}, removeListener() {},
		addEventListener() {}, removeEventListener() {},
	})));
	localStorage.clear();
	store = new NotesStore({
		list: async () => [{ ...note }], create: async () => ({ ...note }),
		update: async (input) => ({ ...note, ...input, revision: 2 }), delete: async () => {},
	});
	await store.load();
	container = document.createElement("div");
	document.body.append(container);
	root = createRoot(container);
});
afterEach(async () => {
	await act(async () => root.unmount());
	// Let the dialog's scheduled focus restoration finish before removing browser shims.
	await act(async () => { await new Promise((resolve) => setTimeout(resolve, 30)); });
	container.remove();
	vi.unstubAllGlobals();
});

describe("note composer project isolation", () => {
	it("blocks a note opened from the library over another project and explains the destination", async () => {
		const other = { ...scope, projectId: "project-b" };
		await render(other, true);
		await act(async () => {
			document.querySelector<HTMLSelectElement>("select")!.value = "all";
			document.querySelector("select")!.dispatchEvent(new Event("change", { bubbles: true }));
		});
		await act(async () => document.querySelector<HTMLButtonElement>(".note-preview")!.click());
		await render(other);
		const use = button("notes.useHere");
		expect(use.disabled).toBe(true);
		expect(document.getElementById(use.getAttribute("aria-describedby")!)?.textContent)
			.toBe("notes.useRequiresProjectTask: Project A");
		await act(async () => use.click());
		expect(onUse).not.toHaveBeenCalled();
		const create = container.querySelector<HTMLButtonElement>(".note-primary-action")!;
		expect(create.disabled).toBe(false);
		await act(async () => create.click());
		expect(onCreateTask).toHaveBeenCalledWith(expect.objectContaining({ projectId: "project-a" }));
	});
	it.each([null, { ...scope, workspaceId: null }])("blocks without a selected task (%j)", async (destination) => {
		await render(destination);
		expect(button("notes.useHere").disabled).toBe(true);
		expect(container.querySelector(".note-use-hint")?.textContent).toContain("Project A");
	});
	it("allows a different task in the same project, including a new conversation", async () => {
		await render({ ...scope, workspaceId: "another-task", sessionId: null });
		expect(button("notes.useHere").disabled).toBe(false);
		expect(container.querySelector(".note-use-hint")).toBeNull();
		await act(async () => button("notes.useHere").click());
		expect(onUse).toHaveBeenCalledExactlyOnceWith(notePrompt(note));
	});
	it.each([
		{ ...scope, projectId: "project-b" },
		{ ...scope, workspaceId: "task-b" },
		{ ...scope, sessionId: "session-b" },
		null,
	])("rechecks the destination after saving (%j)", async (destination) => {
		let finishSave!: () => void;
		vi.spyOn(store, "flush").mockImplementationOnce(() => new Promise<void>((resolve) => { finishSave = resolve; }));
		await render(scope);
		await act(async () => button("notes.useHere").click());
		await render(destination);
		await act(async () => finishSave());
		expect(onUse).not.toHaveBeenCalled();
		expect(toast.info).toHaveBeenCalled();
	});
});
