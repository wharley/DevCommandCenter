import { useState } from "react";
import type { CoreEvent, ProviderCatalog } from "@dcc/contracts";
import { WorkspaceTerminalDrawer } from "@/features/terminal";
import { WorkspacePanel } from "@/features/panel";
import type { AppUpdateInfo } from "@/features/updater";
import type { RuntimeSessionSnapshot } from "./workbench-types";

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
	sessionSnapshot: RuntimeSessionSnapshot | null;
	sessionEvents: CoreEvent[];
	pendingPrompt: string | null;
	onSelectProvider: (providerId: string) => void;
	onSelectModel: (modelId: string) => void;
	onStartSession: () => void;
	onSubmitPrompt: (prompt: string) => Promise<void>;
	onResumeSession: () => void;
	onAbortSession: () => void;
	onOpenCommandPalette: () => void;
	updateInfo: AppUpdateInfo;
	isInstallingUpdate: boolean;
	onInstallUpdate: () => void;
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
	sessionSnapshot,
	sessionEvents,
	pendingPrompt,
	onSelectProvider,
	onSelectModel,
	onStartSession,
	onSubmitPrompt,
	onResumeSession,
	onAbortSession,
	onOpenCommandPalette,
	updateInfo,
	isInstallingUpdate,
	onInstallUpdate,
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
				sessionSnapshot={sessionSnapshot}
				sessionEvents={sessionEvents}
				pendingPrompt={pendingPrompt}
				onSelectProvider={onSelectProvider}
				onSelectModel={onSelectModel}
				onStartSession={onStartSession}
				onSubmitPrompt={onSubmitPrompt}
				onResumeSession={onResumeSession}
				onAbortSession={onAbortSession}
				onOpenCommandPalette={onOpenCommandPalette}
				updateInfo={updateInfo}
				isInstallingUpdate={isInstallingUpdate}
				onInstallUpdate={onInstallUpdate}
				terminalAvailable={Boolean(workspacePath)}
				terminalOpen={terminalDrawerOpen}
				onToggleTerminal={() =>
					setTerminalDrawerOpen((current) => !current)
				}
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
