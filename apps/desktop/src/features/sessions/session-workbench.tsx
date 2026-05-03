import { useState } from "react";
import type { CoreEvent, ProviderCatalog } from "@dcc/contracts";
import { WorkspaceTerminalDrawer } from "@/features/terminal";
import { WorkspacePanel } from "@/features/panel";
import type { AppUpdateInfo } from "@/features/updater";
import type { ComposerSubmittedTurn } from "@/features/composer/composer-turn";
import type { RuntimeSessionSnapshot } from "./workbench-types";
import type { WorkspaceGitPreviewSelection } from "@/features/inspector/workspace-git-file-preview";
import type { WorkspaceSessionSummary } from "@dcc/contracts";

export type { RuntimeSessionSnapshot } from "./workbench-types";

type SessionWorkbenchProps = {
	workspaceId: string;
	workspaceName: string;
	workspaceBranch: string;
	workspacePath: string | null;
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
	onSubmitPrompt: (turn: ComposerSubmittedTurn) => Promise<void>;
	onResumeSession: () => void;
	onAbortSession: () => void;
	updateInfo: AppUpdateInfo;
	isInstallingUpdate: boolean;
	onInstallUpdate: () => void;
	editorSelection: WorkspaceGitPreviewSelection | null;
	onCloseEditor: () => void;
};

export function SessionWorkbench({
	workspaceId,
	workspaceName,
	workspaceBranch,
	workspacePath,
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
	onSubmitPrompt,
	onResumeSession,
	onAbortSession,
	updateInfo,
	isInstallingUpdate,
	onInstallUpdate,
	editorSelection,
	onCloseEditor,
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
				onSubmitPrompt={onSubmitPrompt}
				onResumeSession={onResumeSession}
				onAbortSession={onAbortSession}
				updateInfo={updateInfo}
				isInstallingUpdate={isInstallingUpdate}
				onInstallUpdate={onInstallUpdate}
				editorSelection={editorSelection}
				onCloseEditor={onCloseEditor}
			/>

			<WorkspaceTerminalDrawer
				open={terminalDrawerOpen}
				onOpenChange={setTerminalDrawerOpen}
				workspaceId={workspaceId}
				workspaceName={workspaceName}
				workspaceBranch={workspaceBranch}
				workspacePath={workspacePath}
				providerLabel={selectedProviderLabel}
				sessionState={sessionState}
				sessionId={sessionId}
			/>
		</div>
	);
}
