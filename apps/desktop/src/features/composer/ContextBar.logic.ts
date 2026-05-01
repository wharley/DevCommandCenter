import type { ComposerContextDirectory } from "./WorkspaceComposer.logic";

export function formatContextLabel(directory: ComposerContextDirectory) {
	return `${directory.label}: ${directory.path}`;
}
