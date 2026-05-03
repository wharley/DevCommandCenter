import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { WorkspaceEditorSurface } from "@/features/editor/WorkspaceEditorSurface";
import { DccWorkbenchChatHeader } from "@/features/sessions/dcc-workbench-chat-header";
import { ActiveThreadViewport } from "./ActiveThreadViewport";
import { WorkspaceComposer } from "@/features/composer";
import { sessionThreadHistoryQueryOptions } from "@/features/sessions/session-thread-history";
import type { ComposerSubmittedTurn } from "@/features/composer/composer-turn";
import type { WorkspaceGitPreviewSelection } from "@/features/inspector/workspace-git-file-preview";
import type { AppUpdateInfo } from "@/features/updater";
import type { RuntimeSessionSnapshot } from "@/features/sessions/workbench-types";
import { projectWorkspaceMessages } from "./thread-projection";
import type { ProviderCatalog, CoreEvent } from "@dcc/contracts";

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
	onOpenPlanSidebar: () => void;
	onImplementPlanInNewThread: (input: {
		planMarkdown: string;
		planTitle: string | null;
	}) => void;
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
	onOpenPlanSidebar,
	onImplementPlanInNewThread,
}: WorkspacePanelProps) {
	const effectiveSessionId = selectedSessionId ?? sessions[0]?.session.id ?? null;
	const threadHistoryQuery = useQuery(
		sessionThreadHistoryQueryOptions(effectiveSessionId),
	);
	const selectedSessionTitle =
		sessions.find((session) => session.session.id === effectiveSessionId)?.thread
			.title ?? workspaceName;
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
	const messages = useMemo(
		() =>
			projectWorkspaceMessages(
				historyEvents,
				sessionEvents,
				effectiveSessionId,
				pendingPrompt,
			),
		[effectiveSessionId, historyEvents, pendingPrompt, sessionEvents],
	);
	const activePlanMessage = useMemo(() => {
		const assistantMessages = messages.filter(
			(message) => message.role === "assistant" && message.content.trim().length > 0,
		);
		if (assistantMessages.length === 0) {
			return null;
		}
		if (isPlanSessionState(sessionState)) {
			return assistantMessages[assistantMessages.length - 1] ?? null;
		}
		return [...assistantMessages].reverse().find((message) => message.plan?.isPlanLike) ?? null;
	}, [messages, sessionState]);
	const activePlanTitle =
		activePlanMessage?.plan?.title ?? (activePlanMessage ? "Plan" : null);
	const activePlanMarkdown =
		activePlanMessage?.plan?.markdown ?? activePlanMessage?.content ?? null;
	const showPlanFollowUpPrompt =
		Boolean(activePlanMessage) && isPlanSessionState(sessionState);

	return editorSelection ? (
		<WorkspaceEditorSurface
			workspaceRoot={workspacePath}
			selection={editorSelection}
			onClose={onCloseEditor}
		/>
	) : (
		<div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
			<header
				className={[
					"border-b border-border/60 px-4 py-3",
					"pl-[calc(env(safe-area-inset-left)+1rem)] pr-[calc(env(safe-area-inset-right)+1rem)]",
				].join(" ")}
			>
				<DccWorkbenchChatHeader
					threadTitle={selectedSessionTitle}
					projectBadgeLabel={workspaceBranch || null}
					modelBadgeLabel={selectedModelLabel}
					isGitRepo={isGitRepo}
					pathCaption={pathCaption}
					sessions={sessions}
					selectedSessionId={selectedSessionId}
					isLoadingSessions={isLoadingSessions}
					sessionSnapshot={sessionSnapshot}
					pendingPrompt={pendingPrompt}
					onSelectSession={onSelectSession}
					onStartSession={onStartSession}
					onResumeSession={onResumeSession}
					onAbortSession={onAbortSession}
					updateInfo={updateInfo}
					isInstallingUpdate={isInstallingUpdate}
					onInstallUpdate={onInstallUpdate}
					workspacePath={workspacePath}
				/>
			</header>

			<div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
				<ActiveThreadViewport
					messages={messages}
					hasLoaded={hasLoaded}
					isEmpty={hasEmptyThread}
					workspaceName={workspaceName}
					selectedProviderLabel={selectedProviderLabel}
					selectedModelLabel={selectedModelLabel}
					sessionState={sessionState}
					lastTurnState={lastTurnState}
					pendingPrompt={pendingPrompt}
					workspacePath={workspacePath}
					planMessageId={activePlanMessage?.id ?? null}
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
						pendingPrompt={pendingPrompt}
					workspacePath={workspacePath}
					workspaceBranch={workspaceBranch}
					showPlanFollowUpPrompt={showPlanFollowUpPrompt}
					planTitle={activePlanTitle}
					planMarkdown={activePlanMarkdown}
					onSelectProvider={onSelectProvider}
					onSelectModel={onSelectModel}
					onSubmitPrompt={onSubmitPrompt}
					onAbortSession={onAbortSession}
					onOpenPlanSidebar={onOpenPlanSidebar}
					onImplementPlanInNewThread={onImplementPlanInNewThread}
				/>
				</div>
			</div>
		</div>
	);
}

function isPlanSessionState(state: string | null) {
	return (
		state === "planning" ||
		state === "plan_generated" ||
		state === "generating_code" ||
		state === "code_ready" ||
		state === "applying"
	);
}
