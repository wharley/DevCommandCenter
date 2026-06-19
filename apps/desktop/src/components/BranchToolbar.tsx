import { ChevronDown, GitBranch, GitCommitHorizontal, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

export function BranchToolbar({
	branch,
	workspacePath,
	behindOfRemoteCount = 0,
	isSyncingBase = false,
	onSyncBase,
	className,
}: {
	branch: string;
	workspacePath: string | null;
	behindOfRemoteCount?: number;
	isSyncingBase?: boolean;
	onSyncBase?: () => void;
	className?: string;
}) {
	const { t } = useTranslation("common");
	const canSyncBase = Boolean(onSyncBase);

	return (
		<div
			className={cn(
				"flex min-w-0 items-center gap-1.5 rounded-xl border border-border/60 bg-muted/20 px-2 py-1.5",
				className,
			)}
		>
			<GitBranch className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
			<div className="min-w-0 flex-1">
				<div className="flex min-w-0 items-center gap-1.5">
					<span className="truncate text-[12px] font-medium text-foreground">
						{branch || t("branchToolbar.noBranch")}
					</span>
					<Badge variant="outline" className="h-5 px-1.5 text-[10px] font-normal">
						{t("branchToolbar.activeBadge")}
					</Badge>
					{behindOfRemoteCount > 0 ? (
						<Badge variant="secondary" className="h-5 px-1.5 text-[10px] font-normal">
							{t("branchToolbar.behindBadge", { count: behindOfRemoteCount })}
						</Badge>
					) : null}
				</div>
				<p className="truncate text-[11px] text-muted-foreground">
					{workspacePath ?? t("branchToolbar.workspacePathUnavailable")}
				</p>
			</div>
			<DropdownMenu>
				<DropdownMenuTrigger asChild>
					<Button
						type="button"
						variant="ghost"
						size="icon-xs"
						className="text-muted-foreground hover:text-foreground"
						aria-label={t("branchToolbar.branchOptionsAria")}
					>
						<ChevronDown className="size-3.5" />
					</Button>
				</DropdownMenuTrigger>
				<DropdownMenuContent align="end" sideOffset={6} className="min-w-44">
					<DropdownMenuItem
						className="gap-2 text-[13px]"
						disabled={!canSyncBase || isSyncingBase}
						onSelect={() => {
							onSyncBase?.();
						}}
					>
						<RefreshCw
							className={cn("size-4", isSyncingBase && "animate-spin")}
							aria-hidden
						/>
						{isSyncingBase
							? t("branchToolbar.syncingBase")
							: t("branchToolbar.syncBase")}
					</DropdownMenuItem>
					<DropdownMenuItem className="gap-2 text-[13px]">
						<GitCommitHorizontal className="size-4" aria-hidden />
						{t("branchToolbar.openBranchDetails")}
					</DropdownMenuItem>
					<DropdownMenuItem className="gap-2 text-[13px]">
						<GitBranch className="size-4" aria-hidden />
						{t("branchToolbar.copyBranchName")}
					</DropdownMenuItem>
				</DropdownMenuContent>
			</DropdownMenu>
		</div>
	);
}
