import { Command, TerminalSquare } from "lucide-react";
import { memo } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import type { DccRuntimeSessionSnapshot } from "./workbench-types";

export type DccWorkbenchChatHeaderProps = {
	threadTitle: string;
	projectBadgeLabel: string | null;
	isGitRepo: boolean;
	pathCaption: string | null;
	sessionSnapshot: DccRuntimeSessionSnapshot | null;
	terminalAvailable: boolean;
	terminalOpen: boolean;
	onToggleTerminal: () => void;
	onOpenCommandPalette: () => void;
	onResumeSession: () => void;
	onAbortSession: () => void;
};

/** Chat column top bar — same layout affordances as the reference desktop/chat header (titles + badges + toolbar cluster). */

export const DccWorkbenchChatHeader = memo(function DccWorkbenchChatHeader({
	threadTitle,
	projectBadgeLabel,
	isGitRepo,
	pathCaption,
	sessionSnapshot,
	terminalAvailable,
	terminalOpen,
	onToggleTerminal,
	onOpenCommandPalette,
	onResumeSession,
	onAbortSession,
}: DccWorkbenchChatHeaderProps) {
	const showProjectBadge = Boolean(projectBadgeLabel);

	return (
		<div className="@container/header-actions flex min-w-0 flex-1 items-center gap-2">
			<div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden sm:gap-3">
				<h2
					className="min-w-0 shrink truncate text-sm font-medium text-foreground"
					title={threadTitle}
				>
					{threadTitle}
				</h2>
				{showProjectBadge ? (
					<Badge variant="outline" className="min-w-0 shrink overflow-hidden">
						<span className="min-w-0 truncate">{projectBadgeLabel}</span>
					</Badge>
				) : null}
				{showProjectBadge && !isGitRepo ? (
					<Badge variant="outline" className="shrink-0 text-[10px] text-amber-700">
						No Git
					</Badge>
				) : null}
				{pathCaption ? (
					<span className="hidden max-w-[16rem] truncate text-[11px] text-muted-foreground md:inline">
						{pathCaption}
					</span>
				) : null}
			</div>
			<div className="flex shrink-0 items-center justify-end gap-2 @3xl/header-actions:gap-3">
				<div className="hidden items-center gap-1 tabular-nums text-[11px] text-muted-foreground sm:flex">
					<span>{sessionSnapshot?.state ?? "idle"}</span>
				</div>
				<div className="hidden items-center gap-1 lg:flex">
					<Button
						type="button"
						variant="outline"
						size="sm"
						className="h-8 px-2.5 text-xs font-normal"
						onClick={onResumeSession}
						disabled={!sessionSnapshot}
					>
						Resume
					</Button>
					<Button
						type="button"
						variant="outline"
						size="sm"
						className="h-8 px-2.5 text-xs font-normal"
						onClick={onAbortSession}
						disabled={!sessionSnapshot}
					>
						Abort
					</Button>
				</div>
				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							type="button"
							variant="outline"
							className={
								terminalOpen
									? "h-7 min-h-7 min-w-7 shrink-0 rounded-md border-border bg-accent px-0 text-accent-foreground [&_svg]:size-3"
									: "h-7 min-h-7 min-w-7 shrink-0 rounded-md px-0 [&_svg]:size-3"
							}
							aria-label="Toggle terminal drawer"
							aria-pressed={terminalOpen}
							disabled={!terminalAvailable}
							onClick={onToggleTerminal}
						>
							<TerminalSquare className="size-4 opacity-90" strokeWidth={2} />
						</Button>
					</TooltipTrigger>
					<TooltipContent side="bottom">
						{!terminalAvailable
							? "Terminal needs a workspace path."
							: terminalOpen
								? "Hide terminal drawer (Esc)"
								: "Toggle terminal drawer"}
					</TooltipContent>
				</Tooltip>
				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							type="button"
							variant="ghost"
							size="sm"
							className="hidden gap-1.5 text-muted-foreground hover:text-foreground sm:inline-flex"
							onClick={onOpenCommandPalette}
						>
							<Command className="size-3.5" />
							<span className="text-xs">Workspaces</span>
						</Button>
					</TooltipTrigger>
					<TooltipContent side="bottom">Command palette (⌘K)</TooltipContent>
				</Tooltip>
				<Button
					type="button"
					variant="ghost"
					size="icon"
					className="inline-flex size-8 shrink-0 sm:hidden"
					aria-label="Open command palette"
					onClick={onOpenCommandPalette}
				>
					<Command className="size-3.5" />
				</Button>
			</div>
		</div>
	);
});
