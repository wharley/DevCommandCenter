import { useDeferredValue, useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { FileCode, FolderGit2, History, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
	CommandDialog,
	CommandEmpty,
	CommandGroup,
	CommandInput,
	CommandItem,
	CommandList,
	CommandSeparator,
	CommandShortcut,
} from "../../components/ui/command";
import type { SessionSearchResult, WorkspaceSessionSummary } from "@dcc/contracts";
import { rankQuickOpenFiles } from "@/features/editor/file-quick-open";
import {
	getOpenPreferredEditorShortcutKeys,
	getFocusComposerShortcutKeys,
	getInspectorCodeModeShortcutKeys,
	getInspectorGitModeShortcutKeys,
	getPrimaryShortcutModifier,
	getToggleTerminalShortcutKeys,
} from "@/features/shortcuts/shortcut-utils";
import { sessionSearchQueryOptions } from "@/features/sessions/session-search-query";
import { listGitTrackedFiles } from "@/lib/workspace-api";
import type { WorkspaceSummary } from "./types";
import type { WorkbenchCommand } from "./workbench-command";
import {
	groupPaletteSessions,
	filterPaletteSessionsByMetadata,
	matchesPaletteAction,
	recentPaletteSessions,
	rankPaletteWorkspaces,
	resolvePaletteSessionSearch,
	type PaletteAction,
} from "./workspace-command-palette.logic";

const SESSION_SEARCH_DEBOUNCE_MS = 180;

type WorkspaceCommandPaletteProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	workspaces: WorkspaceSummary[];
	selectedWorkspaceId: string | null;
	onSelectWorkspace: (workspaceId: string) => void;
	onCreateWorkspace: () => void;
	onCloneWorkspace: () => void;
	onOpenSettings: () => void;
	onOpenOnboarding: () => void;
	onOpenShortcuts: () => void;
	onOpenSkills: () => void;
	onOpenUsage: () => void;
	workspaceRoot: string | null;
	recentSessions: WorkspaceSessionSummary[];
	selectedWorkspace: WorkspaceSummary | null;
	queryScope?: string;
	onSelectSession: (result: SessionSearchResult) => void;
	onSelectFile: (input: { path: string; name: string }) => void;
	/** Present only when the active workspace has a session to delegate from. */
	onDelegate?: () => void;
	onRunWorkbenchCommand?: (command: WorkbenchCommand) => void;
};

export function WorkspaceCommandPalette({
	open,
	onOpenChange,
	workspaces,
	selectedWorkspaceId,
	onSelectWorkspace,
	onCreateWorkspace,
	onCloneWorkspace,
	onOpenSettings,
	onOpenOnboarding,
	onOpenShortcuts,
	onOpenSkills,
	onOpenUsage,
	workspaceRoot,
	recentSessions,
	selectedWorkspace,
	queryScope = "local",
	onSelectSession,
	onSelectFile,
	onDelegate,
	onRunWorkbenchCommand,
}: WorkspaceCommandPaletteProps) {
	const { t } = useTranslation("common");
	const [query, setQuery] = useState("");
	const deferredQuery = useDeferredValue(query.trim());
	const sessionSearch = resolvePaletteSessionSearch(query);
	const debouncedSessionSearchQuery = useDebouncedValue(
		sessionSearch.enabled ? sessionSearch.query : "",
		SESSION_SEARCH_DEBOUNCE_MS,
	);
	const shouldSearchSessionContent =
		sessionSearch.enabled &&
		debouncedSessionSearchQuery === sessionSearch.query;

	useEffect(() => {
		if (!open) setQuery("");
	}, [open]);

	const sessionResultsQuery = useQuery(
		sessionSearchQueryOptions(
			open && shouldSearchSessionContent ? debouncedSessionSearchQuery : null,
			{ scope: queryScope },
		),
	);
	const filesQuery = useQuery({
		queryKey: ["gitTrackedFiles", workspaceRoot ?? ""],
		queryFn: async () => {
			if (!workspaceRoot) return [] as string[];
			const result = await listGitTrackedFiles({ workspaceRoot });
			return result.paths;
		},
		enabled: open && Boolean(workspaceRoot),
		staleTime: 30_000,
		refetchOnWindowFocus: false,
	});

	const paletteWorkspaces = useMemo(
		() => rankPaletteWorkspaces(workspaces, deferredQuery, selectedWorkspaceId),
		[deferredQuery, selectedWorkspaceId, workspaces],
	);
	const paletteSessions = useMemo(
		() =>
			shouldSearchSessionContent
				? sessionResultsQuery.data ?? []
				: sessionSearch.isExplicit
					? []
					: filterPaletteSessionsByMetadata(
							recentPaletteSessions(recentSessions, selectedWorkspace),
							deferredQuery,
						),
		[
			recentSessions,
			selectedWorkspace,
			sessionResultsQuery.data,
			sessionSearch.isExplicit,
			shouldSearchSessionContent,
		],
	);
	const sessionGroups = useMemo(
		() => groupPaletteSessions(paletteSessions, selectedWorkspaceId),
		[paletteSessions, selectedWorkspaceId],
	);
	const files = useMemo(
		() => rankQuickOpenFiles(filesQuery.data ?? [], deferredQuery),
		[deferredQuery, filesQuery.data],
	);
	const workbenchCommands = useMemo<Array<{ command: WorkbenchCommand; label: string; keywords: string; shortcut?: string }>>(
		() => [
			{ command: "composer.focus", label: t("commandPalette.workbench.focusComposer"), keywords: t("commandPalette.keywords.focusComposer"), shortcut: getFocusComposerShortcutKeys().join("+") },
			{ command: "composer.execution", label: t("commandPalette.workbench.execution"), keywords: t("commandPalette.keywords.execution") },
			{ command: "composer.togglePlan", label: t("commandPalette.workbench.togglePlan"), keywords: t("commandPalette.keywords.plan") },
			{ command: "terminal.openWorktree", label: t("commandPalette.workbench.terminalWorktree"), keywords: t("commandPalette.keywords.worktreeTerminal"), shortcut: getToggleTerminalShortcutKeys().join("+") },
			{ command: "terminal.openProject", label: t("commandPalette.workbench.terminalProject"), keywords: t("commandPalette.keywords.projectTerminal") },
			{ command: "terminal.newWorktree", label: t("commandPalette.workbench.newTerminal"), keywords: t("commandPalette.keywords.newTerminal") },
			{ command: "inspector.changes", label: t("commandPalette.workbench.inspectorChanges"), keywords: t("commandPalette.keywords.inspectorChanges"), shortcut: getInspectorGitModeShortcutKeys().join("+") },
			{ command: "inspector.files", label: t("commandPalette.workbench.inspectorFiles"), keywords: t("commandPalette.keywords.inspectorFiles"), shortcut: getInspectorCodeModeShortcutKeys().join("+") },
			{ command: "inspector.activity", label: t("commandPalette.workbench.inspectorActivity"), keywords: t("commandPalette.keywords.activity") },
			{ command: "inspector.details", label: t("commandPalette.workbench.inspectorDetails"), keywords: t("commandPalette.keywords.details") },
		],
		[t],
	);
	const actions = useMemo<Array<PaletteAction & { id: string; shortcut?: string; onSelect: () => void }>>(
		() => [
			...(selectedWorkspaceId && onRunWorkbenchCommand ? workbenchCommands.map((item) => ({
				id: item.command,
				label: item.label,
				keywords: item.keywords,
				shortcut: item.shortcut,
				onSelect: () => onRunWorkbenchCommand(item.command),
			})) : []),
			...(onDelegate
				? [{ id: "delegate", label: t("commandPalette.delegate"), keywords: t("commandPalette.keywords.delegate"), onSelect: onDelegate }]
				: []),
			{ id: "open-project", label: t("commandPalette.openProject"), keywords: t("commandPalette.keywords.openProject"), shortcut: getOpenPreferredEditorShortcutKeys().join("+"), onSelect: onCreateWorkspace },
			{ id: "clone", label: t("commandPalette.cloneFromUrl"), keywords: t("commandPalette.keywords.clone"), onSelect: onCloneWorkspace },
			{ id: "settings", label: t("commandPalette.openSettings"), keywords: t("commandPalette.keywords.settings"), shortcut: `${getPrimaryShortcutModifier()}+,`, onSelect: onOpenSettings },
			{ id: "skills", label: t("commandPalette.manageSkills"), keywords: t("commandPalette.keywords.skills"), onSelect: onOpenSkills },
			{ id: "usage", label: t("commandPalette.openUsage"), keywords: t("commandPalette.keywords.usage"), onSelect: onOpenUsage },
			{ id: "onboarding", label: t("commandPalette.openOnboarding"), keywords: t("commandPalette.keywords.onboarding"), onSelect: onOpenOnboarding },
			{ id: "shortcuts", label: t("commandPalette.keyboardShortcuts"), keywords: t("commandPalette.keywords.shortcuts"), onSelect: onOpenShortcuts },
		],
		[
			onCloneWorkspace,
			onCreateWorkspace,
			onDelegate,
			onOpenOnboarding,
			onOpenSettings,
			onOpenShortcuts,
			onOpenSkills,
			onOpenUsage,
			onRunWorkbenchCommand,
			selectedWorkspaceId,
			t,
			workbenchCommands,
		],
	);
	const visibleActions = useMemo(
		() => actions.filter((action) => matchesPaletteAction(action, deferredQuery)),
		[actions, deferredQuery],
	);

	return (
		<CommandDialog
			open={open}
			onOpenChange={onOpenChange}
			title={t("commandPalette.title")}
			description={t("commandPalette.description")}
			shouldFilter={false}
		>
			<div className="border-b border-border/60 bg-muted/20 px-4 py-3">
				<p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
					{t("commandPalette.title")}
				</p>
				<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
					{t("commandPalette.description")}
				</p>
			</div>
			<CommandInput value={query} onValueChange={setQuery} placeholder={t("commandPalette.searchPlaceholder")} />
			<CommandList className="max-h-[min(58vh,32rem)]">
				<CommandEmpty>{t("commandPalette.empty")}</CommandEmpty>
				{visibleActions.length > 0 ? (
					<CommandGroup heading={t("commandPalette.actions")}>
						{visibleActions.map((action) => (
							<CommandItem
								key={action.id}
								value={`${action.label} ${action.keywords}`}
								onSelect={() => {
									onOpenChange(false);
									action.onSelect();
								}}
								className="items-start gap-3 py-2"
							>
								<div className="min-w-0 flex-1">
									<div className="truncate font-medium">{action.label}</div>
									<p className="truncate text-xs text-muted-foreground">{t("commandPalette.actionDestination")}</p>
								</div>
								{action.shortcut ? <CommandShortcut>{action.shortcut}</CommandShortcut> : null}
							</CommandItem>
						))}
						</CommandGroup>
				) : null}
				{visibleActions.length > 0 && paletteWorkspaces.length > 0 ? <CommandSeparator /> : null}
				{paletteWorkspaces.length > 0 ? (
					<>
						<CommandGroup heading={t("commandPalette.workspacesAndProjects")}>
							{paletteWorkspaces.map((workspace) => (
								<CommandItem
									key={workspace.id}
									value={`${workspace.name} ${workspace.branch} ${workspace.id}`}
									onSelect={() => {
										onOpenChange(false);
										onSelectWorkspace(workspace.id);
									}}
									className="items-start gap-3 py-2"
								>
									<FolderGit2 className="mt-0.5 size-4 text-muted-foreground" strokeWidth={1.8} />
									<div className="min-w-0 flex-1">
										<div className="truncate font-medium">{workspace.name}</div>
										<p className="truncate text-xs text-muted-foreground">{workspace.branch}</p>
									</div>
									{workspace.id === selectedWorkspaceId ? <CommandShortcut>{t("commandPalette.selected")}</CommandShortcut> : null}
								</CommandItem>
							))}
						</CommandGroup>
						<CommandSeparator />
					</>
				) : null}
				{shouldSearchSessionContent && sessionResultsQuery.isFetching ? <div className="flex items-center gap-2 px-4 py-2 text-xs text-muted-foreground"><Search className="size-3.5 animate-pulse" strokeWidth={1.8} />{t("commandPalette.loadingSessions")}</div> : null}
				{sessionGroups.currentWorkspace.length > 0 ? <CommandGroup heading={t("commandPalette.currentWorkspaceSessions")}>
					{sessionGroups.currentWorkspace.map((result) => <SessionResultItem key={result.sessionId} result={result} showSnippet={shouldSearchSessionContent} onSelect={() => { onOpenChange(false); onSelectSession(result); }} />)}
				</CommandGroup> : null}
				{sessionGroups.otherWorkspaces.length > 0 ? <CommandGroup heading={t("commandPalette.recentSessions")}>
					{sessionGroups.otherWorkspaces.map((result) => <SessionResultItem key={result.sessionId} result={result} showSnippet={shouldSearchSessionContent} onSelect={() => { onOpenChange(false); onSelectSession(result); }} />)}
				</CommandGroup> : null}
				{(sessionGroups.currentWorkspace.length > 0 || sessionGroups.otherWorkspaces.length > 0) ? <CommandSeparator /> : null}
				{workspaceRoot && files.length > 0 ? <CommandGroup heading={t("commandPalette.workspaceFiles")}>
					{files.map((path) => <CommandItem key={path} value={path} onSelect={() => { onOpenChange(false); onSelectFile({ path, name: basename(path) }); }} className="items-start gap-3 py-2">
						<FileCode className="mt-0.5 size-4 text-muted-foreground" strokeWidth={1.8} />
						<div className="min-w-0 flex-1"><div className="truncate font-medium">{basename(path)}</div><p className="truncate font-mono text-[11px] text-muted-foreground">{dirname(path) || t("commandPalette.workspaceRoot")}</p></div>
					</CommandItem>)}
				</CommandGroup> : null}
			</CommandList>
		</CommandDialog>
	);
}

function basename(path: string) {
	return path.split("/").pop() ?? path;
}

function dirname(path: string) {
	const slash = path.lastIndexOf("/");
	return slash === -1 ? "" : path.slice(0, slash);
}

function SessionResultItem({ result, showSnippet, onSelect }: { result: SessionSearchResult; showSnippet: boolean; onSelect: () => void }) {
	const { t } = useTranslation("common");
	const workspace = [result.workspaceName, result.workspaceBranch].filter(Boolean).join(" · ") || result.workspaceId;
	return <CommandItem value={`${result.threadTitle} ${workspace}`} onSelect={onSelect} className="items-start gap-3 py-2">
		<History className="mt-0.5 size-4 text-muted-foreground" strokeWidth={1.8} />
		<div className="min-w-0 flex-1"><div className="truncate font-medium">{result.threadTitle}</div><p className="truncate text-xs text-muted-foreground">{workspace}</p>{showSnippet && result.snippet.trim() ? <p className="mt-1 line-clamp-2 text-xs leading-relaxed text-foreground/80">{result.snippet}</p> : null}{result.archivedAt ? <p className="text-[11px] text-muted-foreground">{t("workbench.sessionSearch.archived")}</p> : null}</div>
	</CommandItem>;
}

function useDebouncedValue(value: string, delay: number) {
	const [debouncedValue, setDebouncedValue] = useState(value);

	useEffect(() => {
		const timeout = window.setTimeout(() => setDebouncedValue(value), delay);
		return () => window.clearTimeout(timeout);
	}, [delay, value]);

	return debouncedValue;
}
