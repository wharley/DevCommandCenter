import { DccWorkbenchChatHeader } from "@/features/sessions/dcc-workbench-chat-header";
import type { RuntimeSessionSnapshot } from "@/features/sessions/workbench-types";
import type { ProviderCatalog, CoreEvent } from "@dcc/contracts";
import type { AppUpdateInfo } from "@/features/updater";
import { useQuery } from "@tanstack/react-query";
import { ActiveThreadViewport } from "./ActiveThreadViewport";
import { WorkspaceComposer } from "@/features/composer";
import { sessionThreadHistoryQueryOptions } from "@/features/sessions/session-thread-history";
import type { ComposerSubmittedTurn } from "@/features/composer/composer-turn";

type WorkspacePanelProps = {
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
	onSubmitPrompt: (turn: ComposerSubmittedTurn) => Promise<void>;
	onResumeSession: () => void;
	onAbortSession: () => void;
	onOpenCommandPalette: () => void;
	updateInfo: AppUpdateInfo;
	isInstallingUpdate: boolean;
	onInstallUpdate: () => void;
	terminalAvailable: boolean;
	terminalOpen: boolean;
	onToggleTerminal: () => void;
};

export function WorkspacePanel({
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
	terminalAvailable,
	terminalOpen,
	onToggleTerminal,
}: WorkspacePanelProps) {
	const sessionId = sessionSnapshot?.sessionId ?? null;
	const threadHistoryQuery = useQuery(
		sessionThreadHistoryQueryOptions(sessionId),
	);
	const pathCaption =
		workspacePath && workspacePath.length > 0
			? workspacePath.length > 52
				? `…${workspacePath.slice(-51)}`
				: workspacePath
			: null;
	const historyEvents = threadHistoryQuery.data ?? [];
	const hasLoaded = sessionSnapshot
		? Boolean(threadHistoryQuery.isFetched || sessionEvents.length > 0)
		: true;
	const hasEmptyThread = !sessionSnapshot;
	const sessionState = sessionSnapshot?.state ?? null;
	const lastTurnState = sessionSnapshot?.lastTurnState ?? null;
	const isGitRepo = Boolean(workspaceBranch) || Boolean(workspacePath);

	return (
		<div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
			<header
				className={[
					"border-b border-border/60 px-4 py-3",
					"pl-[calc(env(safe-area-inset-left)+1rem)] pr-[calc(env(safe-area-inset-right)+1rem)]",
				].join(" ")}
			>
				<DccWorkbenchChatHeader
					threadTitle={workspaceName}
					projectBadgeLabel={workspaceBranch || null}
					modelBadgeLabel={selectedModelLabel}
					isGitRepo={isGitRepo}
					pathCaption={pathCaption}
					sessionSnapshot={sessionSnapshot}
					terminalAvailable={terminalAvailable}
					terminalOpen={terminalOpen}
					onToggleTerminal={onToggleTerminal}
					onOpenCommandPalette={onOpenCommandPalette}
					onResumeSession={onResumeSession}
					onAbortSession={onAbortSession}
					updateInfo={updateInfo}
					isInstallingUpdate={isInstallingUpdate}
					onInstallUpdate={onInstallUpdate}
				/>
			</header>

			<div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
				<ActiveThreadViewport
					historyEvents={historyEvents}
					liveEvents={sessionEvents}
					hasLoaded={hasLoaded}
					isEmpty={hasEmptyThread}
					workspaceName={workspaceName}
					selectedProviderLabel={selectedProviderLabel}
					selectedModelLabel={selectedModelLabel}
					sessionState={sessionState}
					lastTurnState={lastTurnState}
					sessionId={sessionId}
					pendingPrompt={pendingPrompt}
					onStartSession={onStartSession}
					onSubmitPrompt={onSubmitPrompt}
				/>

				<div className="border-t border-border/60 px-3 pb-3 pt-3 sm:px-4">
					<WorkspaceComposer
						draftKey={workspaceId}
						disabled={false}
						providerChoices={providerChoices}
						selectedProviderId={selectedProviderId}
						selectedModelId={selectedModelId}
						sessionSnapshot={sessionSnapshot}
						workspacePath={workspacePath}
						workspaceBranch={workspaceBranch}
						onSelectProvider={onSelectProvider}
						onSelectModel={onSelectModel}
						onSubmitPrompt={onSubmitPrompt}
						onAbortSession={onAbortSession}
					/>
				</div>
			</div>
		</div>
	);
}
