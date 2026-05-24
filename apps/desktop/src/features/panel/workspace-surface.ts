import type { MissionSpecEntry } from "@dcc/contracts";
import type { WorkspaceGitPreviewSelection } from "@/features/inspector/workspace-git-file-preview";

export type WorkspaceSurfaceSelection =
	| {
			kind: "git-diff";
			file: WorkspaceGitPreviewSelection;
	  }
	| {
			kind: "mission-spec";
			spec: MissionSpecEntry;
	  };
