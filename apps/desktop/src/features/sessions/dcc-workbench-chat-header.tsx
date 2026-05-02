import { CircleStop, Command, RefreshCw, TerminalSquare } from "lucide-react";
import { memo, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { AppUpdateButton } from "@/features/updater";
import type { AppUpdateInfo } from "@/features/updater";
import type { DccRuntimeSessionSnapshot } from "./workbench-types";
import {
	canAbortRun,
	canResumeSession,
} from "./session-chrome-state";
import { sessionStateLabel } from "@/i18n/session-state-label";

export type DccWorkbenchChatHeaderProps = {
	threadTitle: string;
	projectBadgeLabel: string | null;
	modelBadgeLabel: string | null;
	isGitRepo: boolean;
	pathCaption: string | null;
	sessionSnapshot: DccRuntimeSessionSnapshot | null;
	pendingPrompt: string | null;
	terminalAvailable: boolean;
	terminalOpen: boolean;
	onToggleTerminal: () => void;
	onOpenCommandPalette: () => void;
	onResumeSession: () => void;
	onAbortSession: () => void;
	updateInfo: AppUpdateInfo;
	isInstallingUpdate: boolean;
	onInstallUpdate: () => void;
};

/** Chat column top bar — compact toolbar cloned from reference shells: titles left, icon cluster right. */

export const DccWorkbenchChatHeader = memo(function DccWorkbenchChatHeader({
	threadTitle,
	projectBadgeLabel,
	modelBadgeLabel,
	isGitRepo,
	pathCaption,
	sessionSnapshot,
	pendingPrompt,
	terminalAvailable,
	terminalOpen,
	onToggleTerminal,
	onOpenCommandPalette,
	onResumeSession,
	onAbortSession,
	updateInfo,
	isInstallingUpdate,
	onInstallUpdate,
}: DccWorkbenchChatHeaderProps) {
	const { t } = useTranslation("common");
	const showProjectBadge = Boolean(projectBadgeLabel);
	const resumeOk = canResumeSession(sessionSnapshot);
	const abortOk = canAbortRun(sessionSnapshot, pendingPrompt);
	const sessionStateDisplay = useMemo(() => {
		const raw = sessionSnapshot?.state ?? "idle";
		return sessionStateLabel(raw, t);
	}, [sessionSnapshot?.state, t]);

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
				{modelBadgeLabel ? (
					<Badge variant="secondary" className="min-w-0 shrink overflow-hidden">
						<span className="min-w-0 truncate">{modelBadgeLabel}</span>
					</Badge>
				) : null}
				{showProjectBadge && !isGitRepo ? (
					<Badge variant="outline" className="shrink-0 text-[10px] text-amber-700">
						{t("workbench.noGit")}
					</Badge>
				) : null}
				{pathCaption ? (
					<span className="hidden max-w-[16rem] truncate text-[11px] text-muted-foreground md:inline">
						{pathCaption}
					</span>
				) : null}
			</div>
			<div className="flex shrink-0 items-center justify-end gap-1.5 @3xl/header-actions:gap-2">
				<div className="hidden max-w-[5.5rem] items-center gap-1 truncate tabular-nums text-[11px] text-muted-foreground sm:flex">
					<span className="truncate">{sessionStateDisplay}</span>
				</div>
				<div
					className="flex items-center gap-0.5 rounded-md border border-border/50 bg-muted/25 p-0.5"
					role="toolbar"
					aria-label={t("workbench.sessionControlsAria")}
				>
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon"
								className="h-7 w-7 shrink-0 [&_svg]:size-3.5"
								aria-label={t("workbench.resumeAria")}
								onClick={onResumeSession}
								disabled={!sessionSnapshot || !resumeOk}
							>
								<RefreshCw strokeWidth={2} />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="bottom">
							{!sessionSnapshot
								? t("workbench.resumeTooltipNone")
								: resumeOk
									? t("workbench.resumeTooltipOk")
									: t("workbench.resumeTooltipActive")}
						</TooltipContent>
					</Tooltip>
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon"
								className="h-7 w-7 shrink-0 text-destructive hover:bg-destructive/10 hover:text-destructive [&_svg]:size-3.5"
								aria-label={t("workbench.abortAria")}
								onClick={onAbortSession}
								disabled={!sessionSnapshot || !abortOk}
							>
								<CircleStop className="size-3.5" strokeWidth={2} />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="bottom">
							{!sessionSnapshot
								? t("workbench.abortTooltipNone")
								: abortOk
									? t("workbench.abortTooltipOk")
									: t("workbench.abortTooltipNoTurn")}
						</TooltipContent>
					</Tooltip>
				</div>
				<AppUpdateButton
					update={updateInfo}
					installing={isInstallingUpdate}
					onInstallNow={onInstallUpdate}
				/>
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
							aria-label={t("workbench.terminalAria")}
							aria-pressed={terminalOpen}
							disabled={!terminalAvailable}
							onClick={onToggleTerminal}
						>
							<TerminalSquare className="size-4 opacity-90" strokeWidth={2} />
						</Button>
					</TooltipTrigger>
					<TooltipContent side="bottom">
						{!terminalAvailable
							? t("workbench.terminalUnavailable")
							: terminalOpen
								? t("workbench.terminalHide")
								: t("workbench.terminalToggle")}
					</TooltipContent>
				</Tooltip>
				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							type="button"
							variant="ghost"
							size="icon"
							className="size-8 shrink-0 text-muted-foreground hover:text-foreground [&_svg]:size-3.5"
							aria-label={t("workbench.commandPaletteAria")}
							onClick={onOpenCommandPalette}
						>
							<Command strokeWidth={2} />
						</Button>
					</TooltipTrigger>
					<TooltipContent side="bottom">
						{t("workbench.commandPaletteTooltip")}
					</TooltipContent>
				</Tooltip>
			</div>
		</div>
	);
});
