import { ChevronDown, GitBranch, GitCommitHorizontal } from "lucide-react";
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
	className,
}: {
	branch: string;
	workspacePath: string | null;
	className?: string;
}) {
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
						{branch || "No branch"}
					</span>
					<Badge variant="outline" className="h-5 px-1.5 text-[10px] font-normal">
						active
					</Badge>
				</div>
				<p className="truncate text-[11px] text-muted-foreground">
					{workspacePath ?? "Workspace path unavailable"}
				</p>
			</div>
			<DropdownMenu>
				<DropdownMenuTrigger asChild>
					<Button
						type="button"
						variant="ghost"
						size="icon-xs"
						className="text-muted-foreground hover:text-foreground"
						aria-label="Branch options"
					>
						<ChevronDown className="size-3.5" />
					</Button>
				</DropdownMenuTrigger>
				<DropdownMenuContent align="end" sideOffset={6} className="min-w-44">
					<DropdownMenuItem className="gap-2 text-[13px]">
						<GitCommitHorizontal className="size-4" aria-hidden />
						Open branch details
					</DropdownMenuItem>
					<DropdownMenuItem className="gap-2 text-[13px]">
						<GitBranch className="size-4" aria-hidden />
						Copy branch name
					</DropdownMenuItem>
				</DropdownMenuContent>
			</DropdownMenu>
		</div>
	);
}
