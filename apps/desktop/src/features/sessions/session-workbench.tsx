import { useState } from "react";
import type { CoreEvent, ProviderCatalog } from "@dcc/contracts";
import { WorkspaceTerminalDrawer } from "@/features/terminal";
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
	terminalWorkspacePath?: string | null;
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
	terminalWorkspacePath,
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
	const [terminalDrawerOpen, setTerminalDrawerOpen] = useState(false);
	const sessionState = sessionSnapshot?.state ?? "idle";
	const sessionId = sessionSnapshot?.sessionId ?? null;

	return (
		<div className="@container/header-actions flex min-h-0 min-w-0 flex-1 flex-col overflow-x-hidden bg-background">
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
			/>

			<WorkspaceTerminalDrawer
				open={terminalDrawerOpen}
				onOpenChange={setTerminalDrawerOpen}
				workspaceId={workspaceId}
				workspaceName={workspaceName}
				workspaceBranch={workspaceBranch}
				workspacePath={terminalWorkspacePath ?? workspacePath}
				providerLabel={selectedProviderLabel}
				sessionState={sessionState}
				sessionId={sessionId}
			/>
		</div>
	);
}
