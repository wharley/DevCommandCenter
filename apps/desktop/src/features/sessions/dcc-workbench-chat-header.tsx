import { Globe2, History, LoaderCircle, Plus, RefreshCw, Search, SquareTerminal, TextSearch, X } from "lucide-react";
import { memo, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { WorkspaceEditorPicker } from "./workspace-editor-picker";
import type { DccRuntimeSessionSnapshot } from "./workbench-types";
import { canResumeSession } from "./session-chrome-state";
import { isSessionArchived, visibleSessions } from "./session-close";
import { sessionStateLabel } from "@/i18n/session-state-label";
import { cn } from "@/lib/utils";
import type { TerminalScopeTarget } from "@/features/terminal/terminal-scope";
import { useActiveTerminalCount, useGlobalActiveTerminalCount } from "@/features/terminal/use-active-terminal-count";
import { getToggleTerminalShortcutKeys } from "@/features/shortcuts/shortcut-utils";

export type DccWorkbenchChatHeaderProps = {
	threadTitle: string;
	projectLabel: string | null;
	workspacePath: string | null;
	sessions: WorkspaceSessionSummary[];
	selectedSessionId: string | null;
	isLoadingSessions: boolean;
	sessionSnapshot: DccRuntimeSessionSnapshot | null;
	onSelectSession: (sessionId: string) => void;
	onStartSession: () => void;
	onCloseSession: (sessionId: string) => void;
	onRestoreSession: (sessionId: string) => void;
	onOpenSessionSearch: () => void;
	/** Find in the current conversation (renderer-side). */
	onOpenThreadFind?: () => void;
	onResumeSession: () => void;
	sessionActionSessionId: string | null;
	onOpenTerminal?: () => void;
	onOpenBrowser?: () => void;
	browserOpen?: boolean;
	terminalScopes?: TerminalScopeTarget[];
	workspaceActions?: ReactNode;
};

/** Single-row workspace bar. Every visible action opens a concrete surface. */
export const DccWorkbenchChatHeader = memo(function DccWorkbenchChatHeader({
	threadTitle, projectLabel, workspacePath, sessions, selectedSessionId,
	isLoadingSessions, sessionSnapshot, onSelectSession, onStartSession,
	onCloseSession, onRestoreSession, onOpenSessionSearch, onOpenThreadFind, onResumeSession,
	sessionActionSessionId, onOpenTerminal, onOpenBrowser, browserOpen = false, terminalScopes, workspaceActions,
}: DccWorkbenchChatHeaderProps) {
	const { t } = useTranslation("common");
	const resumeOk = canResumeSession(sessionSnapshot);
	const visibleSessionList = visibleSessions(sessions);
	const archivedSessionList = sessions.filter(isSessionArchived);
	const activeTerminalCount = useActiveTerminalCount(terminalScopes);
	const globalActiveTerminalCount = useGlobalActiveTerminalCount();
	const terminalShortcut = getToggleTerminalShortcutKeys().join("+");
	const terminalLabel = globalActiveTerminalCount > activeTerminalCount
		? t("workbench.terminal.openWithBackground", { total: globalActiveTerminalCount, current: activeTerminalCount })
		: activeTerminalCount > 0
			? t("workbench.terminal.openWithActive", { count: activeTerminalCount })
			: t("workbench.terminal.open");

	return (
		<div className="@container/header-actions flex min-w-0 flex-1 items-center justify-between gap-3 overflow-hidden">
			<h2 className="min-w-0 flex-1 truncate text-[13px] font-medium text-foreground" title={projectLabel ? `${projectLabel} / ${threadTitle}` : threadTitle}>
				{threadTitle}
			</h2>
			<div className="flex shrink-0 items-center justify-end gap-1">
				<Tooltip>
					<TooltipTrigger asChild>
						<Button type="button" variant="ghost" size="icon-sm" onClick={onResumeSession} disabled={!sessionSnapshot || !resumeOk} aria-label={t("workbench.resumeAria")} className="text-muted-foreground hover:text-foreground">
							<RefreshCw className="size-3.5" strokeWidth={1.9} />
						</Button>
					</TooltipTrigger>
					<TooltipContent side="bottom">{!sessionSnapshot ? t("workbench.resumeTooltipNone") : resumeOk ? t("workbench.resumeTooltipOk") : t("workbench.resumeTooltipActive")}</TooltipContent>
				</Tooltip>
				<WorkspaceEditorPicker workspacePath={workspacePath} />
				{onOpenTerminal ? (
					<Tooltip>
						<TooltipTrigger asChild>
							<Button type="button" variant="ghost" size="icon-sm" className="relative text-muted-foreground hover:text-foreground" onClick={onOpenTerminal} aria-label={terminalLabel}>
								<SquareTerminal className="size-3.5" strokeWidth={1.8} />
								{globalActiveTerminalCount > 0 ? <span className={cn("absolute -right-1 -top-1 flex h-4 min-w-4 items-center justify-center rounded-full border border-background px-1 text-[9px] font-medium leading-none text-white", activeTerminalCount > 0 ? "bg-sky-500" : "bg-amber-500")}>{globalActiveTerminalCount}</span> : null}
							</Button>
						</TooltipTrigger>
						<TooltipContent side="bottom">{terminalLabel} · {terminalShortcut}</TooltipContent>
					</Tooltip>
				) : null}
				{onOpenBrowser ? (
					<Tooltip>
						<TooltipTrigger asChild>
							<Button type="button" variant="ghost" size="icon-sm" className={browserOpen ? "text-cyan-400 hover:text-cyan-300" : "text-muted-foreground hover:text-foreground"} onClick={onOpenBrowser} aria-label={browserOpen ? t("browser.close") : t("browser.open")}>
								<Globe2 className="size-3.5" strokeWidth={1.8} />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="bottom">{browserOpen ? t("browser.close") : t("browser.open")}</TooltipContent>
					</Tooltip>
				) : null}
				{workspaceActions}
				<Tooltip>
					<TooltipTrigger asChild><Button type="button" variant="ghost" size="icon-sm" onClick={onStartSession} aria-label={t("workbench.newSessionAria")} className="text-muted-foreground hover:text-foreground"><Plus className="size-3.5" /></Button></TooltipTrigger>
					<TooltipContent side="bottom">{t("workbench.newSessionTooltip")}</TooltipContent>
				</Tooltip>
				{onOpenThreadFind ? (
					<Tooltip>
						<TooltipTrigger asChild><Button type="button" variant="ghost" size="icon-sm" onClick={onOpenThreadFind} aria-label={t("conversation.find.open")} className="text-muted-foreground hover:text-foreground"><TextSearch className="size-3.5" /></Button></TooltipTrigger>
						<TooltipContent side="bottom">{t("conversation.find.openTooltip")}</TooltipContent>
					</Tooltip>
				) : null}
				<Tooltip>
					<TooltipTrigger asChild><Button type="button" variant="ghost" size="icon-sm" onClick={onOpenSessionSearch} aria-label={t("workbench.sessionSearch.buttonAria")} className="text-muted-foreground hover:text-foreground"><Search className="size-3.5" /></Button></TooltipTrigger>
					<TooltipContent side="bottom">{t("workbench.sessionSearch.buttonTooltip")}</TooltipContent>
				</Tooltip>
				<DropdownMenu>
					<DropdownMenuTrigger asChild><Button type="button" variant="ghost" size="icon-sm" aria-label={t("workbench.sessionHistoryAria")} className="text-muted-foreground hover:text-foreground">{isLoadingSessions ? <LoaderCircle className="size-3.5 animate-spin" /> : <History className="size-3.5" />}</Button></DropdownMenuTrigger>
					<DropdownMenuContent align="end" className="max-h-96 w-72 overscroll-contain">
						<DropdownMenuLabel>{t("workbench.sessionHistoryLabel")}</DropdownMenuLabel>
						<DropdownMenuSeparator />
						{visibleSessionList.length > 0 ? visibleSessionList.map((session) => (
							<DropdownMenuItem key={session.session.id} onSelect={() => onSelectSession(session.session.id)} className="group/session gap-2">
								<span className={cn("size-1.5 shrink-0 rounded-full", session.session.id === selectedSessionId ? "bg-emerald-500" : "bg-muted-foreground/45")} />
								<span className="min-w-0 flex-1 truncate">{session.thread.title}</span>
								<span className="text-[10px] text-muted-foreground">{sessionStateLabel(session.projection.state, t)}</span>
								<button type="button" disabled={sessionActionSessionId === session.session.id} onClick={(event) => { event.preventDefault(); event.stopPropagation(); onCloseSession(session.session.id); }} aria-label={t("workbench.closeSessionAria", { title: session.thread.title })} className="grid size-5 place-items-center rounded-sm opacity-0 hover:bg-accent group-hover/session:opacity-100 focus-visible:opacity-100"><X className="size-3" /></button>
							</DropdownMenuItem>
						)) : <DropdownMenuItem disabled>{t("workbench.noSessions")}</DropdownMenuItem>}
						{archivedSessionList.length > 0 ? <>
							<DropdownMenuSeparator />
							<DropdownMenuLabel className="text-xs text-muted-foreground">{t("workbench.archivedSessionsLabel")}</DropdownMenuLabel>
							{archivedSessionList.map((session) => <DropdownMenuItem key={session.session.id} onSelect={() => onRestoreSession(session.session.id)} className="justify-between gap-2"><span className="min-w-0 truncate">{session.thread.title}</span><span className="text-[10px] text-muted-foreground">{t("workbench.restoreSessionLabel")}</span></DropdownMenuItem>)}
						</> : null}
					</DropdownMenuContent>
				</DropdownMenu>
			</div>
		</div>
	);
});
