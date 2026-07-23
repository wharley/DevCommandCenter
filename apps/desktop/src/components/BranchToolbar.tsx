import { ChevronDown, Copy, GitBranch, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
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
	const canCopyBranchName = Boolean(branch);
	const isBehind = behindOfRemoteCount > 0;
	// When the branch is behind its base, surface the fix as a first-class
	// inline action right next to the signal. Otherwise it stays in the overflow.
	const showInlineSync = canSyncBase && (isBehind || isSyncingBase);
	const handleCopyBranchName = async () => {
		try {
			await navigator.clipboard.writeText(branch);
			toast.success(t("branchToolbar.copyBranchNameSuccess"));
		} catch {
			toast.error(t("branchToolbar.copyBranchNameError"));
		}
	};

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
				</div>
				<p
					className="truncate text-[11px] text-muted-foreground"
					title={workspacePath ?? undefined}
				>
					{workspacePath ?? t("branchToolbar.workspacePathUnavailable")}
				</p>
			</div>
			{showInlineSync ? (
				<Button
					type="button"
					variant="secondary"
					size="sm"
					className="h-7 shrink-0 gap-1.5 rounded-lg px-2.5 text-[12px] font-medium"
					disabled={isSyncingBase}
					onClick={() => {
						onSyncBase?.();
					}}
				>
					<RefreshCw
						className={cn("size-3.5", isSyncingBase && "animate-spin")}
						aria-hidden
					/>
					{isSyncingBase
						? t("branchToolbar.syncingBase")
						: t("branchToolbar.syncBaseBehind", { count: behindOfRemoteCount })}
				</Button>
			) : null}
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
					{canSyncBase && !isBehind ? (
						<DropdownMenuItem
							className="gap-2 text-[13px]"
							disabled={isSyncingBase}
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
					) : null}
					<DropdownMenuItem
						className="gap-2 text-[13px]"
						disabled={!canCopyBranchName}
						onSelect={() => {
							void handleCopyBranchName();
						}}
					>
						<Copy className="size-4" aria-hidden />
						{t("branchToolbar.copyBranchName")}
					</DropdownMenuItem>
				</DropdownMenuContent>
			</DropdownMenu>
		</div>
	);
}
