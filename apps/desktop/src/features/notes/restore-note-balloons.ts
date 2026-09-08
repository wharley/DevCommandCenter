import type { ProjectNote } from "./notes-api";
import type { FloatingState } from "./floating-note";

export const NOTE_POSITION_KEY = "dcc.notes.positions.v1";
export type NotePositions = Record<string, FloatingState>;

export function readNotePositions(): NotePositions {
	try {
		const stored = JSON.parse(localStorage.getItem(NOTE_POSITION_KEY) ?? "{}");
		return Object.fromEntries(
			Object.entries(stored).flatMap(([id, value]) => {
				const position = value as FloatingState | null;
				return position &&
					Number.isFinite(position.x) &&
					Number.isFinite(position.y)
					? [
							[
								id,
								{
									x: position.x,
									y: position.y,
									minimized: position.minimized === true,
								},
							],
						]
					: [];
			}),
		);
	} catch {
		return {};
	}
}

export function defaultNotePosition(index: number): FloatingState {
	return {
		x: window.innerWidth - 370 - index * 32,
		y: 85 + index * 38,
		minimized: false,
	};
}

/** Closing a balloon dismisses it for this visit; entering the task restores it. */
export function restoreOpenTaskNotes({
	notes,
	current,
	saved,
	dismissed,
	scope,
	changedScope,
}: {
	notes: ProjectNote[];
	current: NotePositions;
	saved: NotePositions;
	dismissed: ReadonlySet<string>;
	scope: { projectId: string; workspaceId: string | null } | null;
	changedScope: boolean;
}): NotePositions {
	const byId = new Map(notes.map((note) => [note.id, note]));
	const next: NotePositions = Object.fromEntries(
		Object.entries(current).filter(([id]) => {
			const note = byId.get(id);
			return note && (!changedScope || (note.pinned && note.status === "open"));
		}),
	);
	for (const note of notes) {
		if (Object.keys(next).length >= 4) break;
		if (note.status !== "open" || dismissed.has(note.id) || next[note.id])
			continue;
		const belongsToTask =
			scope?.workspaceId &&
			note.projectId === scope.projectId &&
			(note.sourceWorkspaceId === scope.workspaceId ||
				note.implementationTaskId === scope.workspaceId);
		if (!note.pinned && !belongsToTask) continue;
		next[note.id] =
			saved[note.id] ?? defaultNotePosition(Object.keys(next).length);
	}
	const ids = Object.keys(next);
	return ids.length === Object.keys(current).length &&
		ids.every((id) => next[id] === current[id])
		? current
		: next;
}
