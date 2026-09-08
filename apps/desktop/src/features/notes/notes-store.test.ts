import { afterEach, describe, expect, it, vi } from "vitest";
import { NotesStore } from "./notes-store";
import { notePrompt, type ProjectNote } from "./notes-api";

const note: ProjectNote = {
	id: "idea",
	projectId: "project",
	projectName: "Project",
	title: "Devices",
	content: "List devices",
	contextSnapshot: "Saved conversation",
	sourceSessionId: "session",
	sourceWorkspaceId: "workspace",
	sourceTaskTitle: "Login",
	implementationTaskId: null,
	status: "open",
	color: "amber",
	pinned: false,
	revision: 1,
	createdAt: "t0",
	updatedAt: "t0",
	completedAt: null,
};
function deferred<T>() {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((r) => {
		resolve = r;
	});
	return { promise, resolve };
}
function fixture() {
	const api = {
		list: vi.fn(async () => [{ ...note }]),
		create: vi.fn(async () => ({ ...note })),
		update: vi.fn(async (value: ProjectNote) => ({
			...value,
			revision: value.revision + 1,
		})),
		delete: vi.fn(async (_ids: string[]) => {}),
	};
	const storage = {
		getItem: vi.fn(() => null as string | null),
		setItem: vi.fn(),
	};
	return { api, storage, store: new NotesStore(api, storage) };
}
afterEach(() => {
	vi.useRealTimers();
});

describe("durable project notes", () => {
	it("an old library refresh cannot hide a newly created note", async () => {
		const { store, api } = fixture();
		await store.load();
		const pending = deferred<ProjectNote[]>();
		api.list.mockImplementationOnce(() => pending.promise);
		const refresh = store.load();
		api.create.mockResolvedValueOnce({ ...note, id: "new-note" });
		await store.create(note);
		pending.resolve([note]);
		await refresh;
		expect(store.getSnapshot().notes.map((note) => note.id)).toContain(
			"new-note",
		);
	});
	it("serializes edits while an earlier save is delayed and preserves the newest text", async () => {
		vi.useFakeTimers();
		const { store, api } = fixture();
		await store.load();
		const first = deferred<ProjectNote>();
		api.update.mockImplementationOnce(() => first.promise);
		store.edit(note.id, { content: "First edit" });
		const flush = store.flush(note.id);
		store.edit(note.id, { content: "Second edit", title: "Revoke devices" });
		expect(api.update).toHaveBeenCalledTimes(1);
		first.resolve({ ...note, content: "First edit", revision: 2 });
		await flush;
		expect(api.update).toHaveBeenLastCalledWith(
			expect.objectContaining({
				content: "Second edit",
				title: "Revoke devices",
				revision: 2,
			}),
		);
		expect(store.getSnapshot().notes[0]?.content).toBe("Second edit");
		expect(store.getSnapshot().saving).toEqual([]);
	});
	it("retains edits on failure and retries against the latest revision only on request", async () => {
		vi.useFakeTimers();
		const { store, api, storage } = fixture();
		await store.load();
		api.update.mockRejectedValueOnce(new Error("note_conflict"));
		store.edit(note.id, { content: "Keep this idea" });
		await expect(store.flush(note.id)).rejects.toThrow("note_conflict");
		expect(store.getSnapshot().failed).toContain(note.id);
		expect(storage.setItem).toHaveBeenLastCalledWith(
			"dcc.notes.pending.v1",
			expect.stringContaining("Keep this idea"),
		);
		api.list.mockResolvedValueOnce([{ ...note, revision: 8 }]);
		await store.retry(note.id);
		expect(api.update).toHaveBeenLastCalledWith(
			expect.objectContaining({ revision: 8, content: "Keep this idea" }),
		);
		expect(store.getSnapshot().failed).toEqual([]);
	});
	it("refreshing while a write is running does not revert the editor", async () => {
		vi.useFakeTimers();
		const { store, api } = fixture();
		await store.load();
		const pending = deferred<ProjectNote>();
		api.update.mockImplementationOnce(() => pending.promise);
		store.edit(note.id, { content: "Unsaved newest text" });
		const flush = store.flush(note.id);
		await store.load();
		expect(store.getSnapshot().notes[0]?.content).toBe("Unsaved newest text");
		pending.resolve({ ...note, content: "Unsaved newest text", revision: 2 });
		await flush;
	});
	it("deletion cancels queued text writes instead of recreating the note", async () => {
		vi.useFakeTimers();
		const { store, api } = fixture();
		await store.load();
		store.edit(note.id, { content: "Queued edit" });
		await store.remove([note.id]);
		await vi.advanceTimersByTimeAsync(1_000);
		expect(api.update).not.toHaveBeenCalled();
		expect(store.getSnapshot().notes).toEqual([]);
	});
	it("restores a text recovery draft without executing a task or completing the note", async () => {
		const { store, api, storage } = fixture();
		storage.getItem.mockReturnValue(
			JSON.stringify({
				idea: {
					title: "Recovered",
					content: "Recovered body",
					status: "completed",
					implementationTaskId: "unexpected",
				},
			}),
		);
		await store.load();
		await store.flushAll();
		expect(api.update).toHaveBeenCalledWith(
			expect.objectContaining({
				title: "Recovered",
				content: "Recovered body",
				status: "open",
				implementationTaskId: null,
			}),
		);
		expect(api.create).not.toHaveBeenCalled();
	});
	it("can build a new task draft using preserved context after the source is deleted", () => {
		const prompt = notePrompt({
			...note,
			sourceSessionId: null,
			sourceWorkspaceId: null,
		});
		expect(prompt).toContain("List devices");
		expect(prompt).toContain("Saved conversation");
		expect(prompt).toContain("Login");
	});
});
