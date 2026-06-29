import {
	AlertCircle,
	CircleStop,
	Clock3,
	FileDiff,
	History,
	PanelRightClose,
	PanelRightOpen,
	Plus,
	RefreshCw,
	Search,
	X,
} from "lucide-react";
import { memo } from "react";
import { useTranslation } from "react-i18next";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { AppUpdateButton } from "@/features/updater";
import type { AppUpdateInfo } from "@/features/updater";
import { WorkspaceEditorPicker } from "./workspace-editor-picker";
import type { DccRuntimeSessionSnapshot } from "./workbench-types";
import {
	canAbortRun,
	canResumeSession,
} from "./session-chrome-state";
import { isSessionArchived, visibleSessions } from "./session-close";
import { sessionStateLabel } from "@/i18n/session-state-label";
import { cn } from "@/lib/utils";

export type DccWorkbenchChatHeaderProps = {
	threadTitle: string;
	projectBadgeLabel: string | null;
	modelBadgeLabel: string | null;
	isGitRepo: boolean;
	pathCaption: string | null;
	workspacePath: string | null;
	sessions: WorkspaceSessionSummary[];
	selectedSessionId: string | null;
	isLoadingSessions: boolean;
	sessionSnapshot: DccRuntimeSessionSnapshot | null;
	pendingPrompt: string | null;
	onSelectSession: (sessionId: string) => void;
	onStartSession: () => void;
	onCloseSession: (sessionId: string) => void;
	onRestoreSession: (sessionId: string) => void;
	onOpenSessionSearch: () => void;
	onResumeSession: () => void;
	onAbortSession: () => void;
	sessionActionSessionId: string | null;
	updateInfo: AppUpdateInfo;
	isInstallingUpdate: boolean;
	onInstallUpdate: () => void;
	/** Uncommitted git changes summary; drives the calm-mode changes pill. */
	gitChangeSummary?: {
		files: number;
		additions: number;
		deletions: number;
	} | null;
	/** Current inspector visibility — picks the open vs. close affordance. */
	inspectorCollapsed?: boolean;
	/** Toggles the inspector open/closed. */
	onToggleInspector?: () => void;
};

/** Chat column top bar with titles left and action icons right. */

export const DccWorkbenchChatHeader = memo(function DccWorkbenchChatHeader({
	threadTitle,
	projectBadgeLabel,
	modelBadgeLabel,
	isGitRepo,
	pathCaption,
	workspacePath,
	sessions,
	selectedSessionId,
	isLoadingSessions,
	sessionSnapshot,
	pendingPrompt,
	onSelectSession,
	onStartSession,
	onCloseSession,
	onRestoreSession,
	onOpenSessionSearch,
	onResumeSession,
	onAbortSession,
	sessionActionSessionId,
	updateInfo,
	isInstallingUpdate,
	onInstallUpdate,
	gitChangeSummary,
	inspectorCollapsed,
	onToggleInspector,
}: DccWorkbenchChatHeaderProps) {
	const { t } = useTranslation("common");
	const showProjectBadge = Boolean(projectBadgeLabel);
	const resumeOk = canResumeSession(sessionSnapshot);
	const abortOk = canAbortRun(sessionSnapshot, pendingPrompt);
	const visibleSessionList = visibleSessions(sessions);
	const archivedSessionList = sessions.filter(isSessionArchived);
	const activeSessionId = selectedSessionId ?? visibleSessionList[0]?.session.id ?? "";

	return (
		<div className="@container/header-actions flex min-w-0 flex-1 flex-col gap-2">
			<div className="flex min-w-0 items-center justify-between gap-3 overflow-hidden sm:gap-4">
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
					<WorkspaceEditorPicker workspacePath={workspacePath} />
					{onToggleInspector ? (
						inspectorCollapsed && gitChangeSummary ? (
							<Tooltip>
								<TooltipTrigger asChild>
									<Button
										type="button"
										variant="ghost"
										size="sm"
										onClick={onToggleInspector}
										aria-label={t("workbench.changesPill.aria", {
											count: gitChangeSummary.files,
										})}
										className="h-7 shrink-0 gap-1.5 rounded-md border border-border/50 bg-muted/25 px-2 text-[12px] font-normal hover:bg-accent/60"
									>
										<FileDiff
											className="size-3.5 text-muted-foreground"
											strokeWidth={1.8}
										/>
										<span className="tabular-nums text-foreground">
											{gitChangeSummary.files}
										</span>
										{gitChangeSummary.additions > 0 ? (
											<span className="tabular-nums text-emerald-600 dark:text-emerald-400">
												+{gitChangeSummary.additions}
											</span>
										) : null}
										{gitChangeSummary.deletions > 0 ? (
											<span className="tabular-nums text-destructive">
												−{gitChangeSummary.deletions}
											</span>
										) : null}
									</Button>
								</TooltipTrigger>
								<TooltipContent side="bottom">
									{t("workbench.changesPill.tooltip")}
								</TooltipContent>
							</Tooltip>
						) : (
							<Tooltip>
								<TooltipTrigger asChild>
									<Button
										type="button"
										variant="ghost"
										size="icon-sm"
										onClick={onToggleInspector}
										aria-label={
											inspectorCollapsed
												? t("workbench.inspectorToggle.open")
												: t("workbench.inspectorToggle.close")
										}
										className="shrink-0 text-muted-foreground hover:bg-accent/60 hover:text-foreground"
									>
										{inspectorCollapsed ? (
											<PanelRightOpen className="size-3.5" strokeWidth={1.8} />
										) : (
											<PanelRightClose className="size-3.5" strokeWidth={1.8} />
										)}
									</Button>
								</TooltipTrigger>
								<TooltipContent side="bottom">
									{inspectorCollapsed
										? t("workbench.inspectorToggle.open")
										: t("workbench.inspectorToggle.close")}
								</TooltipContent>
							</Tooltip>
						)
					) : null}
				</div>
			</div>

			<div className="flex items-center gap-2">
				<div className="group/tabs-scroll relative min-w-0 flex-1">
					<div className="scrollbar-none min-w-0 flex-1 overflow-x-auto">
						{isLoadingSessions ? (
							<div className="flex h-[1.85rem] items-center gap-1.5 px-2 text-[12px] text-muted-foreground">
								<Clock3 className="size-3 animate-pulse" strokeWidth={1.8} />
								<span>{t("workbench.loadingSessions")}</span>
							</div>
						) : visibleSessionList.length > 0 ? (
							<Tabs
								value={activeSessionId}
								onValueChange={onSelectSession}
								className="min-w-max gap-0"
							>
								<TabsList
									aria-label={t("workbench.sessionTabsAria")}
									className="inline-flex min-w-full w-max justify-start self-start gap-0 rounded-none bg-transparent p-0"
								>
									{visibleSessionList.map((session) => {
										const selected = session.session.id === activeSessionId;
										const state = session.projection.state;
										const stateDotClass =
											state === "active"
												? "bg-chart-2"
												: state === "completed"
													? "bg-emerald-500"
													: state === "aborted"
														? "bg-destructive"
														: "bg-muted-foreground";

										return (
											<Tooltip key={session.session.id}>
												<TooltipTrigger asChild>
													<div className="group/tab relative min-w-[6.5rem] max-w-[14rem] shrink-0 flex-none">
														<TabsTrigger
															value={session.session.id}
															className="h-[1.85rem] w-full justify-start gap-1.5 overflow-hidden pr-7 text-[13px] text-muted-foreground data-[state=active]:text-foreground"
															title={session.thread.title}
														>
															<span className="flex min-w-0 flex-1 items-center gap-1.5">
																<span
																	className={cn(
																		"size-1.5 shrink-0 rounded-full",
																		selected ? "bg-foreground" : stateDotClass,
																	)}
																/>
																<span className="truncate font-medium">
																	{session.thread.title}
																</span>
															</span>
														</TabsTrigger>
														<button
															type="button"
															aria-label={t("workbench.closeSessionAria", {
																title: session.thread.title,
															})}
															title={t("workbench.closeSessionTooltip")}
															disabled={sessionActionSessionId === session.session.id}
															onClick={(event) => {
																event.preventDefault();
																event.stopPropagation();
																onCloseSession(session.session.id);
															}}
															className="absolute right-1 top-1/2 inline-flex size-5 -translate-y-1/2 items-center justify-center rounded-sm text-muted-foreground/80 opacity-0 transition hover:bg-accent hover:text-foreground focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 group-hover/tab:opacity-100 disabled:pointer-events-none disabled:opacity-40"
														>
															<X className="size-3" strokeWidth={2} />
														</button>
													</div>
												</TooltipTrigger>
												<TooltipContent
													side="bottom"
													sideOffset={4}
													className="flex h-[22px] items-center rounded-md px-1.5 text-[11px] leading-none"
												>
													<span className="truncate">
														{session.thread.title} ·{" "}
														{sessionStateLabel(session.projection.state, t)}
													</span>
												</TooltipContent>
											</Tooltip>
										);
									})}
								</TabsList>
							</Tabs>
						) : (
							<div className="flex h-[1.85rem] items-center gap-1.5 px-2 text-[12px] text-muted-foreground">
								<AlertCircle className="size-3" strokeWidth={1.8} />
								{t("workbench.noSessions")}
							</div>
						)}
					</div>
				</div>

				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							aria-label={t("workbench.newSessionAria")}
							onClick={onStartSession}
							variant="ghost"
							size="icon-sm"
							className="shrink-0 text-muted-foreground hover:bg-accent/60 hover:text-foreground"
						>
							<Plus className="size-3.5" strokeWidth={1.8} />
						</Button>
					</TooltipTrigger>
					<TooltipContent
						side="bottom"
						sideOffset={4}
						className="flex h-[24px] items-center gap-2 rounded-md px-2 text-[12px] leading-none"
					>
						<span>{t("workbench.newSessionTooltip")}</span>
					</TooltipContent>
				</Tooltip>

				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							aria-label={t("workbench.sessionSearch.buttonAria")}
							onClick={onOpenSessionSearch}
							variant="ghost"
							size="icon-sm"
							className="shrink-0 text-muted-foreground hover:bg-accent/60 hover:text-foreground"
						>
							<Search className="size-3.5" strokeWidth={1.8} />
						</Button>
					</TooltipTrigger>
					<TooltipContent
						side="bottom"
						sideOffset={4}
						className="flex h-[24px] items-center gap-2 rounded-md px-2 text-[12px] leading-none"
					>
						<span>{t("workbench.sessionSearch.buttonTooltip")}</span>
					</TooltipContent>
				</Tooltip>

				<DropdownMenu>
					<DropdownMenuTrigger asChild>
						<Button
							aria-label={t("workbench.sessionHistoryAria")}
							variant="ghost"
							size="icon-sm"
							className="shrink-0 text-muted-foreground hover:bg-accent/60 hover:text-foreground focus-visible:border-transparent focus-visible:ring-0"
						>
							<History className="size-3.5" strokeWidth={1.8} />
						</Button>
					</DropdownMenuTrigger>
					<DropdownMenuContent
						align="end"
						className="max-h-96 w-64 overscroll-contain"
					>
						<DropdownMenuLabel>{t("workbench.sessionHistoryLabel")}</DropdownMenuLabel>
						<DropdownMenuSeparator />
						{sessions.length > 0 ? (
							<>
								{visibleSessionList.map((session) => (
									<DropdownMenuItem
										key={session.session.id}
										onSelect={() => {
											onSelectSession(session.session.id);
										}}
										className="flex items-center justify-between gap-2"
									>
										<span className="min-w-0 truncate">{session.thread.title}</span>
										<span className="shrink-0 text-[11px] text-muted-foreground">
											{sessionStateLabel(session.projection.state, t)}
										</span>
									</DropdownMenuItem>
								))}
								{archivedSessionList.length > 0 ? (
									<>
										<DropdownMenuSeparator />
										<DropdownMenuLabel className="text-xs text-muted-foreground">
											{t("workbench.archivedSessionsLabel")}
										</DropdownMenuLabel>
										{archivedSessionList.map((session) => (
											<DropdownMenuItem
												key={session.session.id}
												onSelect={() => {
													onRestoreSession(session.session.id);
												}}
												className="flex items-center justify-between gap-2"
											>
												<span className="min-w-0 truncate">{session.thread.title}</span>
												<span className="shrink-0 text-[11px] text-muted-foreground">
													{t("workbench.restoreSessionLabel")}
												</span>
											</DropdownMenuItem>
										))}
									</>
								) : null}
							</>
						) : (
							<DropdownMenuItem disabled>
								{t("workbench.noSessions")}
							</DropdownMenuItem>
						)}
					</DropdownMenuContent>
				</DropdownMenu>
			</div>
		</div>
	);
});
