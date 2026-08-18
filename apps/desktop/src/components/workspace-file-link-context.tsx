import { createContext, useContext, useMemo, type ReactNode } from "react";
import type { WorkspaceFileReference } from "./workspace-file-reference";

export type WorkspaceFileLinkContextValue = {
	workspaceRoot: string | null;
	onOpenFile: (reference: WorkspaceFileReference) => void;
	getTitle?: (reference: WorkspaceFileReference) => string;
};

const WorkspaceFileLinkContext = createContext<WorkspaceFileLinkContextValue | null>(null);

export function WorkspaceFileLinkProvider({
	workspaceRoot,
	onOpenFile,
	getTitle,
	children,
}: WorkspaceFileLinkContextValue & { children: ReactNode }) {
	const value = useMemo(
		() => ({ workspaceRoot, onOpenFile, getTitle }),
		[workspaceRoot, onOpenFile, getTitle],
	);
	return (
		<WorkspaceFileLinkContext.Provider value={value}>
			{children}
		</WorkspaceFileLinkContext.Provider>
	);
}

export function useWorkspaceFileLink() {
	return useContext(WorkspaceFileLinkContext);
}
