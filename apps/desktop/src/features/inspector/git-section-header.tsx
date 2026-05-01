import type { CommitMode } from "@/features/commit/WorkspaceCommitButton.logic";
import { WorkspaceCommitButton } from "@/features/commit";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

export type GitSectionHeaderProps = {
	workspaceDisplayName: string;
	commitMode: CommitMode;
	/** Git status query refetch — bottom shimmer while syncing. */
	isRefreshing?: boolean;
	onCommit?: () => Promise<void> | void;
	className?: string;
};

export function GitSectionHeader({
	workspaceDisplayName,
	commitMode,
	isRefreshing = false,
	onCommit,
	className,
}: GitSectionHeaderProps) {
	return (
		<div
			className={cn(
				"relative flex items-center justify-between gap-3 border-b border-border/40 px-3 py-2",
				className,
			)}
		>
			<div className="min-w-0">
				<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
					Git
				</p>
				<p
					className="truncate text-[13px] font-medium leading-tight"
					title={workspaceDisplayName}
				>
					{workspaceDisplayName}
				</p>
			</div>
			<div className="flex shrink-0 items-center gap-2">
				<Badge variant="outline" className="h-7 px-2.5 text-[11px] font-normal">
					{commitMode}
				</Badge>
				<WorkspaceCommitButton mode={commitMode} onCommit={onCommit} />
			</div>
			{isRefreshing ? (
				<div
					data-testid="git-header-shimmer"
					aria-hidden="true"
					className="pointer-events-none absolute inset-x-0 bottom-0 h-px motion-safe:animate-[shine_2s_infinite_linear]"
					style={{
						backgroundImage:
							"linear-gradient(90deg, transparent 0%, transparent 35%, color-mix(in oklch, var(--color-primary) 50%, transparent) 50%, transparent 65%, transparent 100%)",
						backgroundSize: "300% 100%",
					}}
				/>
			) : (
				<div className="pointer-events-none absolute inset-x-0 bottom-0 h-px bg-[linear-gradient(90deg,transparent,color-mix(in_oklch,var(--foreground)_28%,transparent),transparent)] bg-[length:200%_100%] motion-safe:animate-shine" />
			)}
		</div>
	);
}
