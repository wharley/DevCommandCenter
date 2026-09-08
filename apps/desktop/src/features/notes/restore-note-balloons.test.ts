import { describe, expect, it } from "vitest";
import type { ProjectNote } from "./notes-api";
import {
	readNotePositions,
	NOTE_POSITION_KEY,
	restoreOpenTaskNotes,
} from "./restore-note-balloons";

const position = { x: 180, y: 120, minimized: true };
const note = (
	id: string,
	overrides: Partial<ProjectNote> = {},
): ProjectNote => ({
	id,
	projectId: "project",
	projectName: "Project",
	title: id,
	content: "",
	contextSnapshot: "",
	sourceWorkspaceId: "task-a",
	sourceSessionId: "session",
	sourceTaskTitle: "Task A",
	implementationTaskId: null,
	status: "open",
	color: "amber",
	pinned: false,
	revision: 1,
	createdAt: "2026-09-08",
	updatedAt: "2026-09-08",
	completedAt: null,
	...overrides,
});
const restore = (
	notes: ProjectNote[],
	extra: Partial<Parameters<typeof restoreOpenTaskNotes>[0]> = {},
) =>
	restoreOpenTaskNotes({
		notes,
		scope: { projectId: "project", workspaceId: "task-a" },
		current: {},
		saved: { a: position },
		dismissed: new Set(),
		changedScope: true,
		...extra,
	});

describe("task note restoration", () => {
	it("restores source and implementation notes, excluding completed, unrelated and deleted-source notes", () => {
		const result = restore([
			note("a"),
			note("linked", {
				sourceWorkspaceId: "elsewhere",
				implementationTaskId: "task-a",
			}),
			note("done", { status: "completed" }),
			note("elsewhere", { sourceWorkspaceId: "task-b" }),
			note("deleted", { sourceWorkspaceId: null }),
		]);
		expect(Object.keys(result)).toEqual(["a", "linked"]);
		expect(result.a).toEqual(position);
	});
	it("dismisses for the current visit and restores on return while retaining pinned notes across tasks", () => {
		const notes = [
			note("a"),
			note("pin", { pinned: true, projectId: "another-project" }),
		];
		expect(Object.keys(restore(notes, { dismissed: new Set(["a"]) }))).toEqual([
			"pin",
		]);
		const elsewhere = restore(notes, {
			scope: { projectId: "project", workspaceId: "task-b" },
			current: { a: position, pin: position },
		});
		expect(Object.keys(elsewhere)).toEqual(["pin"]);
		expect(Object.keys(restore(notes, { current: elsewhere }))).toEqual([
			"pin",
			"a",
		]);
	});
	it("keeps manual library previews during the visit and limits restoration to four balloons", () => {
		const manual = note("manual", { status: "completed" });
		const notes = [
			manual,
			...Array.from({ length: 6 }, (_, i) => note(String(i))),
		];
		const result = restore(notes, {
			changedScope: false,
			current: { manual: position },
		});
		expect(result.manual).toBe(position);
		expect(Object.keys(result)).toHaveLength(4);
		expect(restore(notes, { current: result }).manual).toBeUndefined();
	});
	it("restores valid saved coordinates and minimized state, ignoring malformed storage", () => {
		localStorage.setItem(
			NOTE_POSITION_KEY,
			JSON.stringify({ a: position, broken: null, invalid: { x: "no", y: 2 } }),
		);
		expect(readNotePositions()).toEqual({ a: position });
		localStorage.setItem(NOTE_POSITION_KEY, "null");
		expect(readNotePositions()).toEqual({});
		localStorage.removeItem(NOTE_POSITION_KEY);
	});
});
