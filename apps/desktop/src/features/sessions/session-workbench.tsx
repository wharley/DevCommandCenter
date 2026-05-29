import { useCallback, useState } from "react";
import type { CoreEvent, ProviderCatalog } from "@dcc/contracts";
import { WorkspaceTerminalDrawer } from "@/features/terminal";
import { ensureTerminal as ensureTerminalTab } from "@/features/terminal/terminal-tabs-store";
import { WorkspacePanel } from "@/features/panel";
import type { WorkspaceSurfaceSelection } from "@/features/panel/workspace-surface";
import type { AppUpdateInfo } from "@/features/updater";
import type { ComposerSubmittedTurn } from "@/features/composer/composer-turn";
import type { RuntimeSessionSnapshot } from "./workbench-types";
import type { WorkspaceSessionSummary } from "@dcc/contracts";

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
	sessionQueryScope?: string;
	selectedProviderLabel: string | null;
	selectedModelLabel: string | null;
	selectedProviderId: string | null;
	selectedModelId: string | null;
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
	onSubmitPrompt: (turn: ComposerSubmittedTurn) => Promise<void>;
	onResumeSession: () => void;
	onAbortSession: () => void;
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
};

export function SessionWorkbench({
	workspaceId,
	workspaceName,
	workspaceBranch,
	workspacePath,
	projectId,
	terminalRootPath,
	sessionQueryScope = "local",
	selectedProviderLabel,
	selectedModelLabel,
	selectedProviderId,
	selectedModelId,
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
	sessionActionSessionId,
	updateInfo,
	isInstallingUpdate,
	onInstallUpdate,
	surfaceSelection,
	onCloseSurface,
	onOpenPlanSidebar,
	onImplementPlanInNewThread,
}: SessionWorkbenchProps) {
	const [terminalOpen, setTerminalOpen] = useState(false);
	const [terminalExpanded, setTerminalExpanded] = useState(false);
	const sessionState = sessionSnapshot?.state ?? "idle";
	const sessionId = sessionSnapshot?.sessionId ?? null;
	const terminalProjectKey = projectId ?? workspaceId;

	const handleToggleTerminal = useCallback(() => {
		setTerminalOpen((current) => {
			const next = !current;
			if (next) {
				ensureTerminalTab(terminalProjectKey);
			} else {
				// Reopening should always come back as a split, not full-screen.
				setTerminalExpanded(false);
			}
			return next;
		});
	}, [terminalProjectKey]);

	const handleTerminalOpenChange = useCallback((next: boolean) => {
		setTerminalOpen(next);
		if (!next) {
			setTerminalExpanded(false);
		}
	}, []);

	// Full-bleed terminal takeover — hide the chat column entirely.
	const chatHidden = terminalOpen && terminalExpanded;

	return (
		<div className="flex min-h-0 min-w-0 flex-1 flex-row overflow-hidden bg-background">
			{!chatHidden ? (
				<div className="@container/header-actions flex min-h-0 min-w-0 flex-1 flex-col overflow-x-hidden">
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
						sessionActionSessionId={sessionActionSessionId}
						updateInfo={updateInfo}
						isInstallingUpdate={isInstallingUpdate}
						onInstallUpdate={onInstallUpdate}
						surfaceSelection={surfaceSelection}
						onCloseSurface={onCloseSurface}
						onOpenPlanSidebar={onOpenPlanSidebar}
						onImplementPlanInNewThread={onImplementPlanInNewThread}
						onOpenTerminal={handleToggleTerminal}
					/>
				</div>
			) : null}

			{terminalOpen ? (
				<WorkspaceTerminalDrawer
					open={terminalOpen}
					onOpenChange={handleTerminalOpenChange}
					expanded={terminalExpanded}
					onExpandedChange={setTerminalExpanded}
					projectKey={terminalProjectKey}
					rootPath={terminalRootPath ?? workspacePath}
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
