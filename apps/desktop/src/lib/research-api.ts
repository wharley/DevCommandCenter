import { invoke } from "@tauri-apps/api/core";

/** An idea in progress: a repository under ~/dcc-ideias kept out of the project list. */
export type Idea = {
	rootPath: string;
	createdAt: string;
};

/** Creates the idea's folder, its empty IDEIA.md and the mark that hides it from the projects. */
export function createIdea() {
	return invoke<Idea>("research_create_idea");
}

export function listIdeas() {
	return invoke<Idea[]>("research_list_ideas");
}

export type IdeaVisibility = "private" | "public";

export type PublishIdeaInput = {
	rootPath: string;
	/** Repository name, also the project's folder name. */
	name: string;
	/** Folder that receives the project folder. */
	destination: string;
	visibility: IdeaVisibility;
	/** Public repository only: keep docs/IDEIA.md local, out of every commit. */
	keepIdeaLocal: boolean;
};

export type PublishedIdea = {
	rootPath: string;
	repositoryUrl: string | null;
};

/** Creates the GitHub repository, moves the folder and turns the idea into a project. */
export function publishIdea(input: PublishIdeaInput) {
	return invoke<PublishedIdea>("research_publish_idea", { input });
}

/** Deletes the idea's task, conversation, records and folder. */
export function discardIdea(rootPath: string) {
	return invoke<void>("research_discard_idea", { input: { rootPath } });
}
