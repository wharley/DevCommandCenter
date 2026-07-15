import type { MissionSpecEntry } from "@dcc/contracts";
import type { WorkspaceGitPreviewSelection } from "@/features/inspector/workspace-git-file-preview";

export type WorkspaceSurfaceSelection =
	| {
			kind: "merge-conflict";
			workspaceRoot: string;
			baseBranch: string | null;
			forgeLogin: string | null;
	  }
	| {
			kind: "git-diff";
			file: WorkspaceGitPreviewSelection;
	  }
	| {
			kind: "mission-spec";
			spec: MissionSpecEntry;
	  }
	| {
			kind: "file-edit";
			path: string;
			name: string;
			requestId: number;
			focusLine?: number | null;
	  };
