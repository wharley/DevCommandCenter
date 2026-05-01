import { Folder } from "lucide-react";
import type { DccWorkspaceRailGroup } from "./workspace-rail-projection";
import type { DccWorkspaceRailRow } from "./workspace-rail-projection";
import type { WorkspaceSummary } from "./types";
import { cn } from "@/lib/utils";

export const ARCHIVED_SECTION_ID = "dccArchived";

/** Branch / workflow chroma aligned to shell tokens (maps DCC coarse status → rail hints). */
export type WorkspaceRailBranchTone =
	| "working"
	| "open"
	| "merged"
	| "inactive";

export const workspaceRailBranchToneClasses: Record<
	WorkspaceRailBranchTone,
	string
> = {
	working: "text-[var(--workspace-branch-status-working)]",
	open: "text-[var(--workspace-branch-status-open)]",
	merged: "text-[var(--workspace-branch-status-merged)]",
	inactive: "text-[var(--workspace-branch-status-inactive)]",
};

export function branchToneFromWorkspace(
	workspace: Pick<WorkspaceSummary, "status">,
): WorkspaceRailBranchTone {
	if (workspace.status === "archived") {
		return "inactive";
	}
	if (workspace.status === "initializing") {
		return "working";
	}
	if (workspace.status === "setup_pending") {
		return "open";
	}
	if (workspace.status === "ready") {
		return "merged";
	}
	return "inactive";
}

export function humanizeWorkspaceBranchLabel(branch: string): string {
	const slug = branch.includes("/") ? branch.slice(branch.indexOf("/") + 1) : branch;
	return slug
		.replace(/[-_]+/g, " ")
		.replace(/\b\w/g, (c) => c.toUpperCase());
}

export function findSelectedRailSectionId(
	selectedWorkspaceId: string | null | undefined,
	groups: DccWorkspaceRailGroup[],
	archivedRows: DccWorkspaceRailRow[],
): string | null {
	if (!selectedWorkspaceId) {
		return null;
	}

	for (const group of groups) {
		if (group.rows.some((row) => row.id === selectedWorkspaceId)) {
			return group.id;
		}
	}

	if (archivedRows.some((row) => row.id === selectedWorkspaceId)) {
		return ARCHIVED_SECTION_ID;
	}

	return null;
}

export function initialsFromWorkspaceLabel(label: string): string {
	if (!label.trim()) {
		return "WS";
	}

	const parts = label
		.split(/[^A-Za-z0-9]+/)
		.map((part) => part.trim())
		.filter(Boolean);

	if (parts.length >= 2) {
		return parts
			.slice(0, 2)
			.map((part) => part[0]?.toUpperCase() ?? "")
			.join("");
	}

	const alphanumeric = Array.from(label).filter((character) =>
		/[A-Za-z0-9]/.test(character),
	);

	return alphanumeric.slice(0, 2).join("").toUpperCase() || "WS";
}

export function ProjectGroupGlyph({ className }: { className?: string }) {
	return (
		<Folder
			className={cn("size-[14px] shrink-0 text-muted-foreground", className)}
			strokeWidth={2}
			aria-hidden
		/>
	);
}
