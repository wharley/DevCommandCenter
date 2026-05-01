import { cva } from "class-variance-authority";
import { GitBranch } from "lucide-react";
import { memo } from "react";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import type { WorkspaceSummary } from "./types";
import {
	branchToneFromWorkspace,
	humanizeWorkspaceBranchLabel,
	initialsFromWorkspaceLabel,
	workspaceRailBranchToneClasses,
} from "./workspace-rail-shared";

const rowVariants = cva(
	"group/dccRailRow relative flex h-[30px] select-none cursor-pointer items-center gap-2 rounded-md px-2.5 text-[13px]",
	{
		variants: {
			active: {
				true: "workspace-row-selected text-foreground",
				false: "text-foreground/80 hover:bg-accent/60",
			},
		},
		defaultVariants: {
			active: false,
		},
	},
);

export type WorkspaceRailRowProps = {
	workspace: WorkspaceSummary;
	selected: boolean;
	onSelect?: (workspaceId: string) => void;
};

function WorkspaceRailAvatar({
	title,
	subtitle,
}: {
	title: string;
	subtitle: string;
}) {
	const initials = initialsFromWorkspaceLabel(subtitle || title);

	return (
		<span
			aria-hidden
			className="flex size-[16px] shrink-0 items-center justify-center rounded-[5px] border border-transparent bg-accent/65 text-[8px] font-semibold uppercase text-foreground ring-1 ring-border/50"
		>
			{initials}
		</span>
	);
}

function LifecycleChip({ status }: { status: WorkspaceSummary["status"] }) {
	if (status === "archived") {
		return null;
	}
	const label =
		status === "ready"
			? "Ready"
			: status === "setup_pending"
				? "Review"
				: status === "initializing"
					? "Running"
					: null;
	if (!label) {
		return null;
	}
	return (
		<Badge variant="outline" className="h-3.5 min-w-0 px-1 text-[9px] font-normal leading-none">
			<span className="truncate">{label}</span>
		</Badge>
	);
}

export const WorkspaceRailRowItem = memo(
	function WorkspaceRailRowItem({
		workspace,
		selected,
		onSelect,
	}: WorkspaceRailRowProps) {
		const branchTone = branchToneFromWorkspace(workspace);
		const displayTitle = workspace.branch
			? humanizeWorkspaceBranchLabel(workspace.branch)
			: workspace.name;

		return (
			<div className="pl-2">
				<div
					role="button"
					tabIndex={0}
					aria-current={selected ? "location" : undefined}
					aria-label={`Open workspace ${displayTitle}`}
					data-active={selected ? "true" : "false"}
					data-workspace-id={workspace.id}
					onClick={() => {
						onSelect?.(workspace.id);
					}}
					onKeyDown={(event) => {
						if (event.key === "Enter" || event.key === " ") {
							event.preventDefault();
							onSelect?.(workspace.id);
						}
					}}
					className={cn(
						rowVariants({ active: selected }),
						"w-full text-left focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring/50",
						workspace.status === "archived" && !selected && "opacity-50",
					)}
				>
					<div className="flex min-w-0 flex-1 items-center gap-2">
						<WorkspaceRailAvatar title={displayTitle} subtitle={workspace.name} />
						<div className="row-content-fade flex min-w-0 flex-1 items-center gap-2">
							<GitBranch
								className={cn(
									"size-[13px] shrink-0",
									workspaceRailBranchToneClasses[branchTone],
								)}
								strokeWidth={1.9}
								aria-hidden
							/>
							<span
								className={cn(
									"min-w-0 truncate leading-tight",
									selected ? "font-medium text-foreground" : "font-medium",
								)}
							>
								{displayTitle}
							</span>
							<LifecycleChip status={workspace.status} />
						</div>
					</div>
				</div>
			</div>
		);
	},
	(previous, next) =>
		previous.selected === next.selected &&
		previous.workspace === next.workspace,
);
