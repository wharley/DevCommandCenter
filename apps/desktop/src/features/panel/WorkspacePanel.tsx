import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { WorkspaceEditorSurface } from "@/features/editor/WorkspaceEditorSurface";
import { WorkspaceMissionSpecSurface } from "@/features/editor/WorkspaceMissionSpecSurface";
import { DccWorkbenchChatHeader } from "@/features/sessions/dcc-workbench-chat-header";
import { ActiveThreadViewport } from "./ActiveThreadViewport";
import { WorkspaceComposer } from "@/features/composer";
import { sessionThreadHistoryQueryOptions } from "@/features/sessions/session-thread-history";
import type { ComposerSubmittedTurn } from "@/features/composer/composer-turn";
import type { AppUpdateInfo } from "@/features/updater";
import type { RuntimeSessionSnapshot } from "@/features/sessions/workbench-types";
import { projectWorkspaceMessages } from "./thread-projection";
import type { ProviderCatalog, CoreEvent } from "@dcc/contracts";
import { derivePlanFollowUpState } from "./plan-follow-up";
import { useWorkspaceMissionSpecs } from "@/features/inspector/use-workspace-mission-specs";
import { buildMissionSpecFilename } from "@/features/composer/WorkspaceComposer.logic";
import {
	computeMissionSpecHash,
	parseMissionValidationPersistence,
} from "@/features/spec/mission-spec-content";
import type { WorkspaceSurfaceSelection } from "./workspace-surface";

type WorkspacePanelProps = {
	workspaceId: string;
	workspaceName: string;
	workspaceBranch: string;
	workspacePath: string | null;
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

export function WorkspacePanel({
	workspaceId,
	workspaceName,
	workspaceBranch,
	workspacePath,
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
}: WorkspacePanelProps) {
	const effectiveSessionId = selectedSessionId ?? sessions[0]?.session.id ?? null;
	const threadHistoryQuery = useQuery(
		sessionThreadHistoryQueryOptions(effectiveSessionId, {
			scope: sessionQueryScope,
			refetchInterval: false,
		}),
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
	const planFollowUpState = useMemo(
		() => derivePlanFollowUpState(messages),
		[messages],
	);
	const activePlanMessage = planFollowUpState.activePlanMessage;
	const latestPlanMessage = planFollowUpState.latestPlanMessage;
	const activePlanTitle =
		activePlanMessage?.plan?.title ?? (activePlanMessage ? "Plan" : null);
	const activePlanMarkdown =
		activePlanMessage?.plan?.markdown ?? activePlanMessage?.content ?? null;
	const showPlanFollowUpPrompt = planFollowUpState.showPlanFollowUpPrompt;
	const missionSpecsQuery = useWorkspaceMissionSpecs(workspacePath);
	const missionSpecs = missionSpecsQuery.data?.specs ?? [];
	const preferredSpecName = buildMissionSpecFilename(workspaceBranch);
	const activeMissionSpec =
		missionSpecs.find((spec) => spec.name === preferredSpecName) ??
		missionSpecs[0] ??
		null;
	const activeMissionSpecRelativePath = activeMissionSpec?.relativePath ?? null;
	const activeMissionSpecHash = activeMissionSpec
		? computeMissionSpecHash(activeMissionSpec.content)
		: null;
	const autoSaveMissionValidation =
		activeMissionSpec != null &&
		parseMissionValidationPersistence(activeMissionSpec.content) === "auto";

	return surfaceSelection ? (
		surfaceSelection.kind === "git-diff" ? (
			<WorkspaceEditorSurface
				workspaceRoot={workspacePath}
				selection={surfaceSelection.file}
				onClose={onCloseSurface}
			/>
		) : (
			<WorkspaceMissionSpecSurface
				spec={surfaceSelection.spec}
				onClose={onCloseSurface}
			/>
		)
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
					onCloseSession={onCloseSession}
					onRestoreSession={onRestoreSession}
					onOpenSessionSearch={onOpenSessionSearch}
					onResumeSession={onResumeSession}
					onAbortSession={onAbortSession}
					sessionActionSessionId={sessionActionSessionId}
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
					planMessageId={latestPlanMessage?.id ?? null}
					sessionId={effectiveSessionId}
					activeMissionSpecRelativePath={activeMissionSpecRelativePath}
					activeMissionSpecHash={activeMissionSpecHash}
					autoSaveMissionValidation={autoSaveMissionValidation}
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
