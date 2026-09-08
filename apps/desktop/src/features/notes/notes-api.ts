import { invoke } from "@tauri-apps/api/core";

export type NoteColor = "amber" | "mint" | "violet" | "sky";
export type ProjectNote = {
	id: string;
	projectId: string;
	projectName: string;
	title: string;
	content: string;
	contextSnapshot: string;
	sourceSessionId: string | null;
	sourceWorkspaceId: string | null;
	sourceTaskTitle: string;
	implementationTaskId: string | null;
	status: "open" | "completed";
	color: NoteColor;
	pinned: boolean;
	revision: number;
	createdAt: string;
	updatedAt: string;
	completedAt: string | null;
};
export type NotePatch = Partial<
	Pick<
		ProjectNote,
		"title" | "content" | "status" | "color" | "pinned" | "implementationTaskId"
	>
>;
export type CreateNote = Pick<
	ProjectNote,
	| "projectId"
	| "projectName"
	| "title"
	| "content"
	| "contextSnapshot"
	| "sourceSessionId"
	| "sourceWorkspaceId"
	| "sourceTaskTitle"
>;
export const notesApi = {
	list: () => invoke<ProjectNote[]>("list_project_notes"),
	create: (input: CreateNote) =>
		invoke<ProjectNote>("create_project_note", { input }),
	update: (note: ProjectNote) =>
		invoke<ProjectNote>("update_project_note", {
			input: {
				id: note.id,
				revision: note.revision,
				title: note.title,
				content: note.content,
				status: note.status,
				color: note.color,
				pinned: note.pinned,
				implementationTaskId: note.implementationTaskId,
			},
		}),
	delete: (ids: string[]) => invoke<void>("delete_project_notes", { ids }),
};

export function notePrompt(note: ProjectNote) {
	return [
		note.title,
		note.content,
		note.contextSnapshot
			? `---\n${note.sourceTaskTitle}\n<context_snapshot>\n${note.contextSnapshot}\n</context_snapshot>`
			: "",
	]
		.filter(Boolean)
		.join("\n\n");
}
