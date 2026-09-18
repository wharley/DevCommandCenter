import { BrainCircuit, Check, Cloud, CloudOff, EyeOff, Globe2, History, LoaderCircle, Pin, Plus, RefreshCw, Search, SquareTerminal, TextSearch, X } from "lucide-react";
import { memo, useMemo, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
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
import { toast } from "sonner";
import { loadAiMemoryExportStatus, loadAiMemoryRecoveredSources, loadAiMemorySidecarStatus, loadAiMemorySourceActions, saveAiMemorySourceAction } from "@/lib/session-api";

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
	const aiMemoryExportQuery = useQuery({
		queryKey: ["ai-memory-export-status", selectedSessionId],
		queryFn: () => loadAiMemoryExportStatus(selectedSessionId as string),
		enabled: Boolean(selectedSessionId),
		refetchInterval: 30_000,
	});
	const aiMemoryExportStatus = aiMemoryExportQuery.data;
	const aiMemorySidecarQuery = useQuery({
		queryKey: ["ai-memory-sidecar-status"],
		queryFn: loadAiMemorySidecarStatus,
		staleTime: 30_000,
		refetchInterval: 30_000,
	});
	const aiMemorySidecarMode = aiMemorySidecarQuery.data?.mode;
	const aiMemorySidecarActive = aiMemorySidecarMode === "managed" || aiMemorySidecarMode === "remote";
	const aiMemorySidecarUnavailable = aiMemorySidecarMode === "unavailable";
	const aiMemorySourcesQuery = useQuery({
		queryKey: ["ai-memory-recovered-sources", selectedSessionId],
		queryFn: () => loadAiMemoryRecoveredSources(selectedSessionId as string),
		enabled: Boolean(selectedSessionId),
		staleTime: 30_000,
	});
	const aiMemoryActionsQuery = useQuery({
		queryKey: ["ai-memory-source-actions"],
		queryFn: () => loadAiMemorySourceActions(),
		enabled: Boolean(selectedSessionId),
		staleTime: 30_000,
	});
	const [aiMemoryCorrection, setAiMemoryCorrection] = useState<Record<string, string>>({});
	const [aiMemoryActionSaving, setAiMemoryActionSaving] = useState<string | null>(null);
	const aiMemoryActions = useMemo(
		() => new Map((aiMemoryActionsQuery.data ?? []).map((action) => [action.sourceKey, action])),
		[aiMemoryActionsQuery.data],
	);
	const aiMemorySourceKey = (source: { path?: string | null; title?: string | null; snippet?: string | null }) =>
		[source.path ?? "", source.title ?? "", (source.snippet ?? "").slice(0, 160)].join("|");
	const aiMemoryVisibleSources = useMemo(
		() => (aiMemorySourcesQuery.data ?? [])
			.filter((source) => aiMemoryActions.get(aiMemorySourceKey(source))?.action !== "ignored")
			.sort((left, right) => {
				const leftPinned = aiMemoryActions.get(aiMemorySourceKey(left))?.action === "pinned";
				const rightPinned = aiMemoryActions.get(aiMemorySourceKey(right))?.action === "pinned";
				return Number(rightPinned) - Number(leftPinned);
			}),
		[aiMemoryActions, aiMemorySourcesQuery.data],
	);
	const saveSourceAction = async (sourceKey: string, action: "corrected" | "ignored" | "pinned", correction?: string) => {
		setAiMemoryActionSaving(sourceKey);
		try {
			await saveAiMemorySourceAction({ sourceKey, action, correction: correction ?? null });
			await aiMemoryActionsQuery.refetch();
			toast.success(t("workbench.aiMemory.actionSaved"));
		} catch (error) {
			toast.error(error instanceof Error ? error.message : t("workbench.aiMemory.actionError"));
		} finally {
			setAiMemoryActionSaving(null);
		}
	};
	const aiMemoryLabel = aiMemoryExportQuery.isFetching
		? "Verificando sincronização com ai-memory"
		: aiMemoryExportStatus?.lastError
			? `ai-memory aguardando nova tentativa (${aiMemoryExportStatus.attempts})`
			: "Sessão aguardando sincronização com ai-memory";
	const aiMemorySourcesLabel = aiMemorySourcesQuery.isLoading
		? t("workbench.aiMemory.sourcesLoading")
		: aiMemorySidecarActive
			? t("workbench.aiMemory.statusActive")
			: aiMemorySidecarUnavailable
				? t("workbench.aiMemory.statusUnavailable")
				: t("workbench.aiMemory.sourcesAria");
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
				{selectedSessionId && (aiMemoryExportQuery.isFetching || aiMemoryExportStatus) ? (
					<Tooltip>
						<TooltipTrigger asChild>
							<span className={cn("inline-flex size-7 items-center justify-center", aiMemoryExportStatus?.lastError ? "text-amber-500" : "text-muted-foreground/70")} aria-label={aiMemoryLabel}>
								{aiMemoryExportQuery.isFetching ? <LoaderCircle className="size-3.5 animate-spin" /> : aiMemoryExportStatus?.lastError ? <CloudOff className="size-3.5" /> : <Cloud className="size-3.5" />}
							</span>
						</TooltipTrigger>
						<TooltipContent side="bottom">{aiMemoryLabel}</TooltipContent>
					</Tooltip>
				) : null}
				{selectedSessionId && (aiMemorySourcesQuery.isLoading || aiMemoryVisibleSources.length > 0 || aiMemorySidecarActive || aiMemorySidecarUnavailable) ? (
					<DropdownMenu>
						<DropdownMenuTrigger asChild>
							<Button type="button" variant="ghost" size="icon-sm" title={aiMemorySourcesLabel} className={cn("relative", aiMemorySourcesQuery.isLoading ? "text-muted-foreground" : aiMemorySidecarUnavailable ? "text-amber-500 hover:text-amber-400" : aiMemorySidecarActive || aiMemoryVisibleSources.length > 0 ? "text-emerald-500 hover:text-emerald-400" : "text-muted-foreground hover:text-foreground")} aria-label={aiMemorySourcesLabel}>
								{aiMemorySourcesQuery.isLoading ? <BrainCircuit className="size-3.5 animate-pulse" /> : <BrainCircuit className="size-3.5" />}
								{aiMemoryVisibleSources.length > 0 ? <span className="absolute -right-1 -top-1 flex h-4 min-w-4 items-center justify-center rounded-full border border-background bg-emerald-500 px-1 text-[9px] font-medium leading-none text-white">{aiMemoryVisibleSources.length}</span> : null}
							</Button>
						</DropdownMenuTrigger>
						<DropdownMenuContent align="end" className="w-96 max-w-[min(92vw,24rem)]">
							<DropdownMenuLabel>{t("workbench.aiMemory.sourcesTitle")}</DropdownMenuLabel>
							<DropdownMenuSeparator />
							{aiMemorySourcesQuery.isLoading ? <div className="flex items-center gap-2 px-3 py-4 text-[11px] text-muted-foreground"><LoaderCircle className="size-3.5 animate-spin" />{t("workbench.aiMemory.sourcesLoading")}</div> : null}
							{!aiMemorySourcesQuery.isLoading && aiMemoryVisibleSources.length === 0 ? <div className="px-3 py-4 text-[11px] text-muted-foreground">{t("workbench.aiMemory.sourcesEmpty")}</div> : null}
							{!aiMemorySourcesQuery.isLoading ? aiMemoryVisibleSources.map((source, index) => {
								const sourceKey = aiMemorySourceKey(source);
								const action = aiMemoryActions.get(sourceKey);
								return (
								<div className="border-b border-border/40 px-3 py-2.5 last:border-b-0" key={`${source.path ?? source.title ?? "source"}-${index}`}>
									<p className="truncate text-[11px] font-medium text-foreground">{source.title ?? source.path ?? t("workbench.aiMemory.untitled")}</p>
									{action?.action === "pinned" ? <p className="mt-0.5 text-[10px] font-medium text-amber-600">{t("workbench.aiMemory.pinned")}</p> : null}
									{source.path && source.title ? <p className="mt-0.5 truncate font-mono text-[10px] text-muted-foreground">{source.path}</p> : null}
									<p className="mt-0.5 truncate text-[10px] text-muted-foreground">{t("workbench.aiMemory.scope", { project: projectLabel ?? "—", checkout: workspacePath ?? "—" })}</p>
									{source.createdAt ? <p className="mt-0.5 text-[10px] text-muted-foreground">{source.createdAt}</p> : null}
									{source.snippet ? <p className="mt-1 line-clamp-3 text-[11px] leading-relaxed text-muted-foreground">{source.snippet}</p> : null}
									{action?.action === "corrected" && action.correction ? <p className="mt-1 rounded bg-emerald-500/10 px-2 py-1 text-[11px] text-emerald-600">{action.correction}</p> : null}
									<div className="mt-2 flex items-center gap-1">
										<Button type="button" variant="ghost" size="icon" className="size-7" title={t("workbench.aiMemory.pin")} disabled={aiMemoryActionSaving === sourceKey} onClick={(event) => { event.preventDefault(); event.stopPropagation(); void saveSourceAction(sourceKey, "pinned"); }}><Pin className="size-3.5" /></Button>
										<Input className="h-7 flex-1 text-[11px]" value={aiMemoryCorrection[sourceKey] ?? ""} placeholder={t("workbench.aiMemory.correctPlaceholder")} onChange={(event) => setAiMemoryCorrection((current) => ({ ...current, [sourceKey]: event.target.value }))} onClick={(event) => event.stopPropagation()} />
										<Button type="button" variant="ghost" size="icon" className="size-7" title={t("workbench.aiMemory.correct")} disabled={aiMemoryActionSaving === sourceKey || !(aiMemoryCorrection[sourceKey] ?? "").trim()} onClick={(event) => { event.preventDefault(); event.stopPropagation(); void saveSourceAction(sourceKey, "corrected", aiMemoryCorrection[sourceKey]); }}><Check className="size-3.5" /></Button>
										<Button type="button" variant="ghost" size="icon" className="size-7" title={t("workbench.aiMemory.ignore")} disabled={aiMemoryActionSaving === sourceKey} onClick={(event) => { event.preventDefault(); event.stopPropagation(); void saveSourceAction(sourceKey, "ignored"); }}><EyeOff className="size-3.5" /></Button>
									</div>
								</div>
								);
							}) : null}
						</DropdownMenuContent>
					</DropdownMenu>
				) : null}
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
								<button type="button" disabled={sessionActionSessionId === session.session.id} onClick={(event) => { event.preventDefault(); event.stopPropagation(); onCloseSession(session.session.id); }} aria-label={t("workbench.closeSessionAria", { title: session.thread.title })} title={t("workbench.closeSessionTooltip")} className="grid size-5 place-items-center rounded-sm text-muted-foreground/70 hover:bg-accent hover:text-foreground focus-visible:text-foreground"><X className="size-3" /></button>
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
