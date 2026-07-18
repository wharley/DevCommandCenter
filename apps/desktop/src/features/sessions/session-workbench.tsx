import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
	CoreEvent,
	ProviderCatalog,
	ProviderRuntimeConfig,
} from "@dcc/contracts";
import { WorkspaceTerminalDrawer } from "@/features/terminal";
import {
	addTerminal as addTerminalTab,
	ensureTerminal as ensureTerminalTab,
	setActiveTerminal as setActiveTerminalTab,
} from "@/features/terminal/terminal-tabs-store";
import { WorkspacePanel } from "@/features/panel";
import type { WorkspaceSurfaceSelection } from "@/features/panel/workspace-surface";
import type { AppUpdateInfo } from "@/features/updater";
import type { ComposerSubmittedTurn } from "@/features/composer/composer-turn";
import type {
	AgentResolutionRunRequest,
	AgentResolutionRunResult,
} from "@/features/merge/agent-conflict-resolution";
import type { RuntimeSessionSnapshot } from "./workbench-types";
import type { ManualDelegationRequest } from "./delegation-dialog";
import type { AgentInitiatedDelegationRequest } from "./agent-delegation-request";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import type {
	OpenTerminalRequest,
	TerminalScopeKind,
	TerminalScopeTarget,
} from "@/features/terminal/terminal-scope";
import {
	dispatchWorkbenchCommand,
	subscribeWorkbenchCommand,
} from "@/features/workspaces/workbench-command";
import { recordUxMetric } from "@/lib/ux-metrics";

export type { RuntimeSessionSnapshot } from "./workbench-types";

type SessionWorkbenchProps = {
	workspaceId: string;
	workspaceName: string;
	workspaceBranch: string;
	workspacePath: string | null;
	/** Project id — terminals are scoped per project. */
	projectId?: string | null;
	/** Project root (`rootPath`) — terminals open here, outside the worktree. */
	terminalRootPath?: string | null;
	/** Active mission worktree path. Unlike workspacePath, this does not fall back to rootPath. */
	terminalWorktreePath?: string | null;
	workspaceScopeOptions?: Array<{
		id: string;
		name: string;
		branch: string;
		hasChanges: boolean | null;
	}>;
	selectedWorkspaceScopeId?: string | null;
	onSelectWorkspaceScope?: (workspaceId: string) => void;
	sessionQueryScope?: string;
	selectedProviderLabel: string | null;
	selectedModelLabel: string | null;
	selectedProviderId: string | null;
	selectedModelId: string | null;
	selectedProviderRuntime: ProviderRuntimeConfig | null;
	providerChoices: ProviderCatalog["providers"];
	sessions: WorkspaceSessionSummary[];
	selectedSessionId: string | null;
	isLoadingSessions: boolean;
	sessionSnapshot: RuntimeSessionSnapshot | null;
	sessionEvents: CoreEvent[];
	pendingPrompt: string | null;
	onSelectProvider: (providerId: string) => void;
	onSelectModel: (modelId: string) => void;
	onStartSession: () => void;
	onSelectSession: (sessionId: string) => void;
	onCloseSession: (sessionId: string) => void;
	onRestoreSession: (sessionId: string) => void;
	onOpenSessionSearch: () => void;
	onSubmitPrompt: (
		turn: ComposerSubmittedTurn,
		options?: { forceNewSession?: boolean; targetSessionId?: string | null },
	) => Promise<void>;
	onResumeSession: () => void;
	onAbortSession: () => void;
	onDelegate: (request: ManualDelegationRequest) => Promise<void>;
	onAgentDelegate: (request: AgentInitiatedDelegationRequest) => Promise<void>;
	sessionActionSessionId: string | null;
	updateInfo: AppUpdateInfo;
	isInstallingUpdate: boolean;
	onInstallUpdate: () => void;
	surfaceSelection: WorkspaceSurfaceSelection | null;
	onCloseSurface: () => void;
	onOpenPlanSidebar: () => void;
	onImplementPlanInNewThread: (input: {
		planMarkdown: string;
		planTitle: string | null;
	}) => void;
	composerPrefill?: { text: string; nonce: number } | null;
	/** Current inspector visibility — picks the open vs. close affordance. */
	inspectorCollapsed?: boolean;
	/** Toggles the inspector open/closed — wired to the header control. */
	onToggleInspector?: () => void;
	/** Reveals the inspector to review the current Git changes. */
	onReviewChanges?: () => void;
	/** Opens the inspector and previews an implementation delegation diff. */
	onReviewDelegation?: (delegationId: string) => void;
	onResolveConflictWithAgent: (
		request: AgentResolutionRunRequest,
	) => Promise<AgentResolutionRunResult>;
	onMergeConflictStateChanged: (workspaceRoot: string) => Promise<void> | void;
	/** Increment to open the Delegate dialog from outside (command palette). */
	delegateSignal?: number;
};

export function SessionWorkbench({
	workspaceId,
	workspaceName,
	workspaceBranch,
	workspacePath,
	projectId,
	terminalRootPath,
	terminalWorktreePath,
	workspaceScopeOptions = [],
	selectedWorkspaceScopeId = null,
	onSelectWorkspaceScope,
	sessionQueryScope = "local",
	selectedProviderLabel,
	selectedModelLabel,
	selectedProviderId,
	selectedModelId,
	selectedProviderRuntime,
	providerChoices,
	sessions,
	selectedSessionId,
	isLoadingSessions,
	sessionSnapshot,
	sessionEvents,
	pendingPrompt,
	onSelectProvider,
	onSelectModel,
	onStartSession,
	onSelectSession,
	onCloseSession,
	onRestoreSession,
	onOpenSessionSearch,
	onSubmitPrompt,
	onResumeSession,
	onAbortSession,
	onDelegate,
	onAgentDelegate,
	sessionActionSessionId,
	updateInfo,
	isInstallingUpdate,
	onInstallUpdate,
	surfaceSelection,
	onCloseSurface,
	onOpenPlanSidebar,
	onImplementPlanInNewThread,
	composerPrefill,
	inspectorCollapsed,
	onToggleInspector,
	onReviewChanges,
	onReviewDelegation,
	onResolveConflictWithAgent,
	onMergeConflictStateChanged,
	delegateSignal,
}: SessionWorkbenchProps) {
	const { t } = useTranslation("common");
	const [terminalOpen, setTerminalOpen] = useState(false);
	const [terminalExpanded, setTerminalExpanded] = useState(false);
	const [terminalScopeKind, setTerminalScopeKind] =
		useState<TerminalScopeKind>("worktree");
	const sessionState = sessionSnapshot?.state ?? "idle";
	const sessionId = sessionSnapshot?.sessionId ?? null;
	const terminalProjectKey = projectId ?? workspaceId;
	const terminalScopes: TerminalScopeTarget[] = useMemo(
		() => [
			{
				kind: "worktree",
				label: t("terminalDock.scopes.worktree"),
				scopeKey: `worktree:${workspaceId}`,
				cwd: terminalWorktreePath ?? null,
				disabledReason: terminalWorktreePath
					? null
					: t("terminalDock.scopes.noWorktreePath"),
			},
			{
				kind: "project",
				label: t("terminalDock.scopes.project"),
				scopeKey: terminalProjectKey,
				cwd: terminalRootPath ?? workspacePath,
				disabledReason:
					terminalRootPath ?? workspacePath
						? null
						: t("terminalDock.scopes.noProjectPath"),
			},
		],
		[
			t,
			terminalProjectKey,
			terminalRootPath,
			terminalWorktreePath,
			workspaceId,
			workspacePath,
		],
	);
	const activeTerminalScope =
		terminalScopes.find((scope) => scope.kind === terminalScopeKind && scope.cwd) ??
		terminalScopes.find((scope) => scope.cwd) ??
		terminalScopes[0];

	const handleOpenTerminal = useCallback(
		(request: OpenTerminalRequest = { scope: "worktree" }) => {
			const scope =
				terminalScopes.find((item) => item.kind === request.scope) ??
				terminalScopes[0];
			if (!scope.cwd) {
				return;
			}
			recordUxMetric("terminal_discovered");

			setTerminalScopeKind(scope.kind);
			if (request.terminalId) {
				setActiveTerminalTab(scope.scopeKey, request.terminalId);
			} else if (request.createNew) {
				addTerminalTab(scope.scopeKey);
			} else {
				ensureTerminalTab(scope.scopeKey);
			}
			setTerminalOpen(true);
		},
		[terminalScopes],
	);
	const handleTerminalOpenChange = useCallback((next: boolean) => {
		setTerminalOpen(next);
		if (!next) {
			setTerminalExpanded(false);
			requestAnimationFrame(() => dispatchWorkbenchCommand("composer.focus"));
		}
	}, []);

	useEffect(
		() =>
			subscribeWorkbenchCommand((command) => {
				if (command === "terminal.toggle") {
					if (terminalOpen) {
						handleTerminalOpenChange(false);
					} else {
						handleOpenTerminal({ scope: "worktree" });
					}
				} else if (command === "terminal.openWorktree") {
					handleOpenTerminal({ scope: "worktree" });
				} else if (command === "terminal.openProject") {
					handleOpenTerminal({ scope: "project" });
				} else if (command === "terminal.newWorktree") {
					handleOpenTerminal({ scope: "worktree", createNew: true });
				}
			}),
		[handleOpenTerminal, handleTerminalOpenChange, terminalOpen],
	);

	// Full-bleed terminal takeover — hide the chat column entirely.
	const chatHidden = terminalOpen && terminalExpanded;

	return (
		<div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
			{!chatHidden ? (
				<div className="@container/header-actions flex min-h-0 min-w-0 flex-1 flex-col overflow-x-hidden">
					{workspaceScopeOptions.length > 1 ? (
						<div className="flex h-9 shrink-0 items-center gap-2 border-b border-border/60 bg-muted/20 px-3">
							<span className="text-[11px] font-medium text-muted-foreground">
								{t("workspaceScope.projects")}
							</span>
							<div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
								{workspaceScopeOptions.map((workspace) => {
									const selected = workspace.id === selectedWorkspaceScopeId;
									return (
										<button
											type="button"
											key={workspace.id}
											title={`${workspace.name} · ${workspace.branch}`}
											onClick={() => onSelectWorkspaceScope?.(workspace.id)}
											className={`flex h-6 min-w-0 max-w-52 shrink-0 items-center gap-1.5 rounded-md border px-2 text-[11.5px] font-medium transition-colors ${
												selected
													? "border-border bg-background text-foreground"
													: "border-transparent text-muted-foreground hover:bg-muted/70 hover:text-foreground"
											}`}
										>
											<span
												aria-hidden
												className={`size-1.5 shrink-0 rounded-full ${
													workspace.hasChanges === true
														? "bg-amber-400"
														: workspace.hasChanges === false
															? "bg-emerald-500"
															: "bg-muted-foreground/40"
												}`}
											/>
											<span className="truncate">{workspace.name}</span>
										</button>
									);
								})}
							</div>
							<span className="ml-auto shrink-0 rounded-full bg-cyan-500/15 px-2 py-0.5 text-[10px] font-semibold text-cyan-300">
								{t("workspaceScope.authorizedCount", {
									count: workspaceScopeOptions.length,
								})}
							</span>
						</div>
					) : null}
					<WorkspacePanel
						workspaceId={workspaceId}
						workspaceName={workspaceName}
						workspaceBranch={workspaceBranch}
						workspacePath={workspacePath}
						sessionQueryScope={sessionQueryScope}
						selectedProviderLabel={selectedProviderLabel}
						selectedModelLabel={selectedModelLabel}
						selectedProviderId={selectedProviderId}
						selectedModelId={selectedModelId}
						selectedProviderRuntime={selectedProviderRuntime}
						providerChoices={providerChoices}
						sessions={sessions}
						selectedSessionId={selectedSessionId}
						isLoadingSessions={isLoadingSessions}
						sessionSnapshot={sessionSnapshot}
						sessionEvents={sessionEvents}
						pendingPrompt={pendingPrompt}
						onSelectProvider={onSelectProvider}
						onSelectModel={onSelectModel}
						onStartSession={onStartSession}
						onSelectSession={onSelectSession}
						onCloseSession={onCloseSession}
						onRestoreSession={onRestoreSession}
						onOpenSessionSearch={onOpenSessionSearch}
						onSubmitPrompt={onSubmitPrompt}
						onResumeSession={onResumeSession}
						onAbortSession={onAbortSession}
						onDelegate={onDelegate}
						onAgentDelegate={onAgentDelegate}
						sessionActionSessionId={sessionActionSessionId}
						updateInfo={updateInfo}
						isInstallingUpdate={isInstallingUpdate}
						onInstallUpdate={onInstallUpdate}
						surfaceSelection={surfaceSelection}
						onCloseSurface={onCloseSurface}
						onOpenPlanSidebar={onOpenPlanSidebar}
						onImplementPlanInNewThread={onImplementPlanInNewThread}
						terminalScopes={terminalScopes}
						onOpenTerminal={handleOpenTerminal}
						externalComposerPrefill={composerPrefill}
						inspectorCollapsed={inspectorCollapsed}
						onToggleInspector={onToggleInspector}
						onReviewChanges={onReviewChanges}
						onReviewDelegation={onReviewDelegation}
						onResolveConflictWithAgent={onResolveConflictWithAgent}
						onMergeConflictStateChanged={onMergeConflictStateChanged}
						delegateSignal={delegateSignal}
					/>
				</div>
			) : null}

			{terminalOpen ? (
				<WorkspaceTerminalDrawer
					open={terminalOpen}
					onOpenChange={handleTerminalOpenChange}
					expanded={terminalExpanded}
					onExpandedChange={setTerminalExpanded}
					scopeKey={activeTerminalScope.scopeKey}
					scopeLabel={activeTerminalScope.label}
					cwd={activeTerminalScope.cwd}
					scopes={terminalScopes}
					activeScopeKind={activeTerminalScope.kind}
					onScopeChange={(kind) => {
						if (kind !== terminalScopeKind) {
							recordUxMetric("terminal_scope_switched");
						}
						setTerminalScopeKind(kind);
					}}
					workspaceName={workspaceName}
					workspaceBranch={workspaceBranch}
					providerLabel={selectedProviderLabel}
					sessionState={sessionState}
					sessionId={sessionId}
				/>
			) : null}
		</div>
	);
}
