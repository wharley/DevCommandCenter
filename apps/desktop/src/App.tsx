import {
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
	useSyncExternalStore,
	type KeyboardEventHandler,
	type MouseEventHandler,
} from "react";
import { useTranslation } from "react-i18next";
import { useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Toaster } from "sonner";
import type {
	CoreEvent,
	Delegation,
	DelegationContextPolicy,
	MissionSpecEntry,
	PullRequestHubItem,
	ProviderCatalog,
	Repository,
	SessionEventRecord,
	SessionSearchResult,
	WorkspaceRemoteBranchDeletionTarget,
	WorkspaceSessionSummary,
} from "@dcc/contracts";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { openInEditor } from "@/lib/shell-api";
import { cn } from "@/lib/utils";
import {
	MAX_INSPECTOR_WIDTH,
	MAX_SIDEBAR_WIDTH,
	MIN_INSPECTOR_WIDTH,
	MIN_SIDEBAR_WIDTH,
	SIDEBAR_RESIZE_HIT_AREA,
} from "./shell/layout";
import { useShellPanels } from "./shell/hooks/useShellPanels";
import { useZoom } from "./shell/use-zoom";
import {
	WorkspacesSidebar,
	WorkspaceCommandPalette,
	CreateWorkspaceDialog,
	notifyWorkspaceCreationResult,
	useWorkspacesPanel,
	type ExistingRepositoryContext,
} from "./features/workspaces";
import {
	WorkspaceInspectorSidebar,
	type WorkspaceInspectorMode,
} from "./features/inspector";
import { SettingsDialog } from "./features/settings";
import { SkillsDialog, getTotalSkillContextCount } from "./features/skills";
import { compileSkills, detectSkillContext } from "./lib/skills-api";
import { OnboardingWizard } from "./features/onboarding";
import { ShortcutCheatsheetDialog } from "./features/shortcuts";
import {
	isOpenPreferredEditorShortcut,
	isCommandPaletteShortcut,
	isFocusComposerShortcut,
	isQuickOpenShortcut,
	isToggleTerminalShortcut,
	isWorkspaceSearchShortcut,
	shouldIgnoreGlobalShortcutTarget,
} from "./features/shortcuts/shortcut-utils";
import { FileQuickOpen } from "./features/editor/file-quick-open";
import { WorkspaceSearch } from "./features/editor/workspace-search";
import { useDockUnreadBadge } from "./features/dock-badge/useDockUnreadBadge";
import { useAppUpdate } from "./features/updater";
import {
	SessionWorkbench,
	type RuntimeSessionSnapshot,
} from "./features/sessions/session-workbench";
import { SessionSearchDialog } from "./features/sessions/session-search-dialog";
import { WorkspaceBootstrapState } from "./features/panel/WorkspaceBootstrapState";
import { PullRequestsHub } from "./features/pull-requests/pull-requests-hub";
import { useSessionEventFeed } from "./features/sessions/use-session-event-feed";
import {
	workspaceSessionSnapshotFromSummary,
	workspaceSessionsQueryOptions,
} from "./features/sessions/workspace-sessions-query";
import { FALLBACK_PROVIDER_CATALOG } from "./lib/fallback-provider-catalog";
import { daemonListCombs } from "./lib/daemon-api";
import { listProviders } from "./lib/provider-api";
import {
	compileMissionSpecContext,
	deleteRepository,
	listMissionSpecs,
	listRepositories,
	listWorkspaceBundles,
	listWorkspaces,
	updateRepositoryIdentity,
	workspaceGitBranchDiff,
	workspaceGitStatus,
	workspacePrStatus,
	workspacePrepareDelegationWorktree,
	workspaceRemoveDelegationWorktree,
} from "./lib/workspace-api";
import { repositoryDisplayName } from "./features/workspaces/repository-display-name";
import {
	abortRun,
	applyTaskTitle,
	closeSession,
	loadSessionThreadEvents,
	queueTurn,
	resumeSession,
	restoreSession,
	sendTurn,
	startThread,
	steerTurn,
} from "./lib/session-api";
import {
	completeDelegation,
	createDelegation,
	failDelegation,
	getDelegation,
	listDelegations,
	startDelegation,
} from "./lib/delegation-api";
import { useAppearance } from "./components/theme-provider";
import {
	dispatchWorkbenchCommand,
	type WorkbenchCommand,
} from "./features/workspaces/workbench-command";
import { recordUxMetric } from "./lib/ux-metrics";
import {
	SELECTED_PROVIDER_STORAGE_KEY,
	SELECTED_MODEL_STORAGE_KEY,
	getProviderUnhealthyReason,
	getSessionComposerSelection,
	resolveSelectedProviderId,
	resolveSelectedModelId,
	setSessionComposerSelection,
} from "./features/providers/provider-selection.logic";
import type {
	ComposerDelegationRequest,
	ComposerSubmittedTurn,
} from "./features/composer/composer-turn";
import { resolveDelegationDefaults } from "./features/sessions/delegation-defaults";
import {
	canRerunDelegation,
	rerunMode,
} from "./features/sessions/delegation-decisions";
import type { ManualDelegationRequest } from "./features/sessions/delegation-request";
import type { AgentInitiatedDelegationRequest } from "./features/sessions/agent-delegation-request";
import { buildMissionSpecFilename } from "./features/composer/WorkspaceComposer.logic";
import {
	daemonCombToWorkspaceSummary,
	workspaceToSummary,
} from "./features/workspaces/use-workspaces";
import { deriveTaskTitle, isAutomaticTaskTitle } from "./features/workspaces/task-title";
import { removeProjectFromDcc } from "./features/workspaces/project-removal";
import type { WorkspaceSummary } from "./features/workspaces/types";
import {
	deliverMultiWorkspace,
	resolveMultiWorkspaceDeliveryState,
	type MultiWorkspaceDeliveryResult,
} from "./features/workspaces/multi-workspace-delivery";
import {
	canAbortRun,
	canResumeSession,
} from "./features/sessions/session-chrome-state";
import {
	isSessionArchived,
	isSessionEmpty,
	nextVisibleSessionIdAfterClose,
	shouldCreateReplacementSession,
	visibleSessions,
} from "./features/sessions/session-close";
import { getStoredPreferredEditor } from "./features/sessions/workspace-editor-preferences";
import type { WorkspaceGitPreviewSelection } from "./features/inspector/workspace-git-file-preview";
import { WORKSPACE_GIT_STATUS_QUERY_KEY } from "./features/inspector/use-workspace-git-status";
import { WORKSPACE_PR_STATUS_QUERY_KEY } from "./features/inspector/use-workspace-pr-status";
import { WORKSPACE_FORGE_CONTEXT_QUERY_KEY } from "./features/inspector/use-workspace-forge-context";
import { WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY } from "./features/inspector/use-workspace-git-branch-diff";
import {
	buildPlanFromSpecPrompt,
	buildPlanImplementationPrompt,
	buildPlanImplementationThreadTitle,
} from "./features/panel/plan-content";
import type { WorkspaceSurfaceSelection } from "./features/panel/workspace-surface";
import {
	buildMissionContinueCriterionPrompt,
	buildMissionReanchorPrompt,
	buildMissionValidationPrompt,
	type MissionResumeCriterion,
} from "./features/spec/mission-spec-content";
import { derivePlanFollowUpState } from "./features/panel/plan-follow-up";
import { projectWorkspaceMessages } from "./features/panel/thread-projection";
import {
	waitForAgentResolutionTurn,
	type AgentResolutionRunRequest,
	type AgentResolutionRunResult,
} from "./features/merge/agent-conflict-resolution";
import { resolveInitialOnboardingOpen } from "./lib/dev-onboarding-override";
import {
	clearProviderRuntimeDraft,
	draftToProviderRuntimeConfig,
	getProviderRuntimeDraft,
	readProviderRuntimeSettings,
	setProviderRuntimeDraft,
	type ProviderRuntimeSettings,
	writeProviderRuntimeSettings,
} from "./features/providers/provider-runtime-settings";
const ONBOARDING_COMPLETE_KEY = "dcc.onboarding.complete";
const EMPTY_WORKSPACES: WorkspaceSummary[] = [];
const LOCAL_BACKEND_CACHE_KEY = "local";

function isCompactCommandPrompt(prompt: string) {
	return /^\/compact(?:\s+.*)?$/i.test(prompt.trim());
}

async function compileMissionSpecContextBestEffort({
	workspaceRoot,
	specRelativePath,
}: {
	workspaceRoot: string;
	specRelativePath: string;
}) {
	try {
		await compileMissionSpecContext({
			workspaceRoot,
			specRelativePath,
		});
		return {
			ok: true as const,
			errorMessage: null,
		};
	} catch (error) {
		const errorMessage =
			error instanceof Error
				? error.message
				: typeof error === "string"
					? error
					: String(error);
		console.warn("[dcc] mission spec context compile failed:", error);
		return {
			ok: false as const,
			errorMessage,
		};
	}
}

function getWorkspaceSessionsCacheKey(scope: string, workspaceId: string) {
	return ["workspaceSessions", scope, workspaceId] as const;
}

type MissionSpecAutoCompileTrigger =
	| "reanchor"
	| "continue"
	| "post_compact"
	| "setup_reopen";

type MissionSpecAutoCompileFailure = {
	workspaceRoot: string;
	specRelativePath: string;
	trigger: MissionSpecAutoCompileTrigger;
	consecutiveFailures: number;
	lastError: string;
	lastAttemptAt: string;
};

function getMissionSpecAutoCompileFailureKey(
	workspaceRoot: string,
	specRelativePath: string,
) {
	return `${workspaceRoot.trim()}::${specRelativePath.trim()}`;
}

type PendingSessionClose = {
	sessionId: string;
	title: string;
	deleteHistory: boolean;
	requiresAbort: boolean;
};

type PendingSessionNavigation = {
	sessionId: string;
	workspaceId: string;
};

type WorkspaceComposerPrefillRequest = {
	workspaceId: string;
	text: string;
	nonce: number;
	mode?: "append" | "replace";
};

type DelegationChildBinding = {
	delegationId: string;
	childSessionId: string;
	parentSessionId: string;
	workspaceId: string;
	workspacePath: string | null;
	cleanupWorkspacePath?: string | null;
	reviewRequired: boolean;
	finalized: boolean;
};

function truncateDelegationContext(value: string, maxLength: number) {
	const trimmed = value.trim();
	if (trimmed.length <= maxLength) {
		return trimmed;
	}
	return `${trimmed.slice(0, maxLength).trimEnd()}\n\n[truncated]`;
}

function formatDelegationChangeList(
	label: string,
	changes: Array<{
		path: string;
		status: string;
		insertions: number;
		deletions: number;
	}>,
) {
	if (changes.length === 0) {
		return [`${label}: none`];
	}
	return [
		`${label}:`,
		...changes
			.slice(0, 80)
			.map(
				(change) =>
					`- ${change.status} ${change.path} (+${change.insertions}/-${change.deletions})`,
			),
		changes.length > 80 ? `- ... ${changes.length - 80} more file(s)` : "",
	].filter(Boolean);
}

async function summarizeSessionForDelegation(sessionId: string) {
	const events = await loadSessionThreadEvents(sessionId);
	const messages = projectWorkspaceMessages(events, [], sessionId, null);
	const lastAssistant = [...messages]
		.reverse()
		.find((message) => message.role === "assistant" && message.content.trim());
	return lastAssistant
		? truncateDelegationContext(lastAssistant.content, 1600)
		: "Delegated session completed without assistant text.";
}

async function summarizeDelegationValidation(sessionId: string) {
	const events = await loadSessionThreadEvents(sessionId);
	const commands = events
		.filter((event) => event.kind.type === "turn_tool_call_started")
		.map((event) => {
			if (event.kind.type !== "turn_tool_call_started") {
				return null;
			}
			return event.kind.command || event.kind.action;
		})
		.filter((value): value is string => Boolean(value?.trim()));
	const uniqueCommands = Array.from(new Set(commands));
	const validationCommands = uniqueCommands.filter((command) =>
		/\b(test|check|lint|typecheck|vitest|jest|cargo|pnpm|npm|yarn)\b/i.test(
			command,
		),
	);

	if (uniqueCommands.length === 0) {
		return "No tool commands observed in the delegated session.";
	}

	const lines = [
		"Observed child-session commands:",
		...uniqueCommands.slice(0, 12).map((command) => `- ${command}`),
	];
	if (uniqueCommands.length > 12) {
		lines.push(`- ... ${uniqueCommands.length - 12} more command(s)`);
	}
	lines.push(
		"",
		validationCommands.length > 0
			? `Validation-like commands: ${validationCommands.slice(0, 6).join("; ")}`
			: "Validation-like commands: none observed",
	);
	return lines.join("\n");
}

async function collectDelegationDiffArtifact(workspacePath: string | null) {
	if (!workspacePath) {
		return {
			touchedFiles: [] as string[],
			diffSummary: null as string | null,
		};
	}

	try {
		const [status, branchDiff] = await Promise.all([
			workspaceGitStatus({ workspaceRoot: workspacePath }),
			workspaceGitBranchDiff({ workspaceRoot: workspacePath }),
		]);
		const entries = [
			...status.staged,
			...status.unstaged,
			...branchDiff.changes,
		];
		const touchedFiles = Array.from(new Set(entries.map((entry) => entry.path))).sort();
		const additions = entries.reduce((sum, entry) => sum + entry.insertions, 0);
		const deletions = entries.reduce((sum, entry) => sum + entry.deletions, 0);
		const diffSummary =
			touchedFiles.length > 0
				? `${touchedFiles.length} file(s), +${additions}/-${deletions}`
				: "No changed files detected.";
		return { touchedFiles, diffSummary };
	} catch (error) {
		return {
			touchedFiles: [] as string[],
			diffSummary: `Diff unavailable: ${
				error instanceof Error ? error.message : String(error)
			}`,
		};
	}
}

async function buildManualDelegationPrompt({
	request,
	workspaceName,
	workspaceBranch,
	workspacePath,
	parentSessionId,
	parentSessionTitle,
	liveSessionEvents,
}: {
	request: ManualDelegationRequest;
	workspaceName: string;
	workspaceBranch: string;
	workspacePath: string | null;
	parentSessionId: string;
	parentSessionTitle: string;
	liveSessionEvents: CoreEvent[];
}) {
	const isImplementation = request.mode === "implement";
	const lines = [
		`Delegated ${request.mode} task from Dev Command Center.`,
		"",
		"Scope:",
		...(isImplementation
			? [
					"- File edits are allowed for this delegated implementation.",
					"- DCC only starts implementation delegations from a clean worktree. Treat the current HEAD as the checkpoint baseline.",
					"- Inspect the current git status before editing and avoid overwriting unrelated work.",
					"- Do not commit, push, delete branches, reset history, or run destructive commands.",
					"- Run focused validation where practical and report the exact commands/results.",
					"- Stop after implementation; Dev Command Center will require human review before marking this delegation complete.",
				]
			: [
					"- Work read-only. Do not edit files, run destructive commands, or apply patches.",
					"- Return concise findings, risks, and recommended next steps.",
				]),
		"",
		"Workspace:",
		`- Name: ${workspaceName}`,
		`- Branch: ${workspaceBranch || "unknown"}`,
		`- Path: ${workspacePath ?? "unknown"}`,
		`- Parent session: ${parentSessionTitle} (${parentSessionId})`,
		"",
		"Instruction:",
		request.instruction,
	];

	if (
		workspacePath &&
		(request.contextPolicy.type === "review_current_diff" ||
			request.contextPolicy.type === "full_reanchor")
	) {
		try {
			const [status, branchDiff] = await Promise.all([
				workspaceGitStatus({ workspaceRoot: workspacePath }),
				workspaceGitBranchDiff({ workspaceRoot: workspacePath }),
			]);
			lines.push(
				"",
				"Git context:",
				`- Current branch: ${status.currentBranch ?? "unknown"}`,
				`- Base branch: ${branchDiff.baseBranch ?? "unknown"}`,
				`- Conflicts: ${status.conflictCount}`,
				...formatDelegationChangeList("Staged changes", status.staged),
				...formatDelegationChangeList("Unstaged changes", status.unstaged),
				...formatDelegationChangeList("Branch diff", branchDiff.changes),
			);
		} catch (error) {
			lines.push(
				"",
				"Git context:",
				`- unavailable: ${error instanceof Error ? error.message : String(error)}`,
			);
		}
	}

	if (workspacePath && request.contextPolicy.type === "full_reanchor") {
		try {
			const specs = await listMissionSpecs({ workspaceRoot: workspacePath });
			const spec = specs.specs[0] ?? null;
			if (spec) {
				lines.push(
					"",
					"Mission spec:",
					truncateDelegationContext(spec.content, 2400),
				);
			}
		} catch (error) {
			lines.push(
				"",
				"Mission spec:",
				`- unavailable: ${error instanceof Error ? error.message : String(error)}`,
			);
		}

		try {
			const historyEvents = await loadSessionThreadEvents(parentSessionId);
			const messages = projectWorkspaceMessages(
				historyEvents,
				liveSessionEvents,
				parentSessionId,
				null,
			);
			const summary = messages
				.slice(-8)
				.map((message) => `${message.label}: ${message.content.trim()}`)
				.filter((line) => line.trim().length > 0)
				.join("\n\n");
			if (summary.trim()) {
				lines.push(
					"",
					"Recent parent session context:",
					truncateDelegationContext(summary, 2400),
				);
			}
		} catch (error) {
			lines.push(
				"",
				"Recent parent session context:",
				`- unavailable: ${error instanceof Error ? error.message : String(error)}`,
			);
		}
	}

	return lines.join("\n");
}

async function assertImplementationDelegationWorkspaceReady(workspacePath: string | null) {
	if (!workspacePath) {
		throw new Error("Implementation delegation requires a local worktree.");
	}
	const status = await workspaceGitStatus({ workspaceRoot: workspacePath });
	if (status.conflictCount > 0) {
		throw new Error(
			`Resolve ${status.conflictCount} conflict${status.conflictCount === 1 ? "" : "s"} before starting an implementation delegation.`,
		);
	}
	const changedFiles = [...status.staged, ...status.unstaged];
	if (changedFiles.length === 0) {
		return;
	}
	const preview = changedFiles
		.slice(0, 3)
		.map((entry) => entry.path)
		.join(", ");
	const suffix = changedFiles.length > 3 ? `, +${changedFiles.length - 3} more` : "";
	throw new Error(
		`Commit, stash, or discard existing worktree changes before starting an implementation delegation (${changedFiles.length} changed: ${preview}${suffix}).`,
	);
}

function buildDelegateTaskToolInstructions(
	providers: ProviderCatalog["providers"],
	currentProviderId: string | null,
) {
	const targets = providers.filter(
		(provider) =>
			provider.id !== currentProviderId &&
			provider.capabilities.canBeDelegationTarget &&
			provider.capabilities.supportsReadOnlyDelegation,
	);
	if (targets.length === 0) {
		return "";
	}

	return [
		"",
		"Dev Command Center tool: delegate_task",
		"You may ask the human to delegate a bounded subtask to another provider by emitting a DCC permission request.",
		"Use it only when another provider can provide materially useful review, explanation, or implementation help.",
		"Emit exactly this JSON event through the provider permission channel:",
		JSON.stringify({
			type: "dcc_permission_request",
			request_id: "delegate-task-short-id",
			tool_name: "delegate_task",
			title: "Delegate task",
			description: "One sentence explaining why delegation is useful.",
			command: JSON.stringify({
				instruction: "Specific task for the delegated provider.",
				mode: "review",
				contextPolicy: "review_current_diff",
				targetProviderId: targets[0]?.id ?? null,
			}),
		}),
		"Allowed modes: review, explain, implement. Use implement only when file edits are necessary; DCC will require human review before completion.",
		`Available delegation targets: ${targets
			.map((provider) => `${provider.id} (${provider.label})`)
			.join(", ")}.`,
	].join("\n");
}

function resolveDelegateTaskToolInstructions({
	provider,
	providers,
}: {
	provider: ProviderCatalog["providers"][number] | null | undefined;
	providers: ProviderCatalog["providers"];
}) {
	if (!provider?.capabilities.canRequestDelegation) {
		return null;
	}
	const instructions = buildDelegateTaskToolInstructions(providers, provider.id);
	return instructions || null;
}

function ResizeSeparator({
	side,
	widthAt,
	ariaLabel,
	ariaMin,
	ariaMax,
	ariaNow,
	isActive,
	onMouseDown,
	onKeyDown,
}: {
	side: "left" | "right";
	widthAt: number;
	ariaLabel: string;
	ariaMin: number;
	ariaMax: number;
	ariaNow: number;
	isActive: boolean;
	onMouseDown: MouseEventHandler<HTMLDivElement>;
	onKeyDown: KeyboardEventHandler<HTMLDivElement>;
}) {
	return (
		<div
			role="separator"
			tabIndex={-1}
			aria-label={ariaLabel}
			aria-orientation="vertical"
			aria-valuemin={ariaMin}
			aria-valuemax={ariaMax}
			aria-valuenow={ariaNow}
			onMouseDown={onMouseDown}
			onKeyDown={onKeyDown}
			className="group absolute inset-y-0 z-30 cursor-ew-resize touch-none outline-none transition-[width,background-color,box-shadow]"
			style={{
				[side === "left" ? "left" : "right"]:
					side === "left"
						? `${Math.max(0, widthAt - SIDEBAR_RESIZE_HIT_AREA / 2)}px`
						: `${Math.max(0, widthAt - SIDEBAR_RESIZE_HIT_AREA)}px`,
				width: `${SIDEBAR_RESIZE_HIT_AREA}px`,
			}}
		>
			<span
				aria-hidden="true"
				className={cn(
					"pointer-events-none absolute inset-y-0 left-1/2 -translate-x-1/2",
					isActive
						? "w-[2px] bg-foreground/80 shadow-[0_0_12px_rgba(0,0,0,0.12)] dark:shadow-[0_0_12px_rgba(255,255,255,0.16)]"
						: "w-px bg-border group-hover:w-[2px] group-hover:bg-muted-foreground/75 group-focus-visible:w-[2px] group-focus-visible:bg-muted-foreground/75",
				)}
			/>
		</div>
	);
}

function getCoreEventSessionId(event: CoreEvent): string | null {
	if ("sessionStarted" in event && event.sessionStarted) {
		return event.sessionStarted.session_id;
	}
	if ("sessionCompleted" in event && event.sessionCompleted) {
		return event.sessionCompleted.session_id;
	}
	if ("sessionAborted" in event && event.sessionAborted) {
		return event.sessionAborted.session_id;
	}
	if ("sessionResumed" in event && event.sessionResumed) {
		return event.sessionResumed.session_id;
	}
	if (
		"sessionMcpRuntimeStatusChanged" in event &&
		event.sessionMcpRuntimeStatusChanged
	) {
		return event.sessionMcpRuntimeStatusChanged.session_id;
	}
	if ("sessionTurnStarted" in event && event.sessionTurnStarted) {
		return event.sessionTurnStarted.session_id;
	}
	if ("sessionTurnSteered" in event && event.sessionTurnSteered) {
		return event.sessionTurnSteered.session_id;
	}
	if ("sessionTurnQueued" in event && event.sessionTurnQueued) {
		return event.sessionTurnQueued.session_id;
	}
	if ("sessionQueuedTurnRemoved" in event && event.sessionQueuedTurnRemoved) {
		return event.sessionQueuedTurnRemoved.session_id;
	}
	if ("sessionTurnQueueReordered" in event && event.sessionTurnQueueReordered) {
		return event.sessionTurnQueueReordered.session_id;
	}
	if ("sessionQueuedTurnDispatched" in event && event.sessionQueuedTurnDispatched) {
		return event.sessionQueuedTurnDispatched.session_id;
	}
	if ("sessionTurnDelta" in event && event.sessionTurnDelta) {
		return event.sessionTurnDelta.session_id;
	}
	if ("sessionTurnReasoningStarted" in event && event.sessionTurnReasoningStarted) {
		return event.sessionTurnReasoningStarted.session_id;
	}
	if ("sessionTurnReasoningDelta" in event && event.sessionTurnReasoningDelta) {
		return event.sessionTurnReasoningDelta.session_id;
	}
	if ("sessionTurnReasoningCompleted" in event && event.sessionTurnReasoningCompleted) {
		return event.sessionTurnReasoningCompleted.session_id;
	}
	if ("sessionTurnToolCallStarted" in event && event.sessionTurnToolCallStarted) {
		return event.sessionTurnToolCallStarted.session_id;
	}
	if ("sessionTurnToolCallDelta" in event && event.sessionTurnToolCallDelta) {
		return event.sessionTurnToolCallDelta.session_id;
	}
	if ("sessionTurnToolCallCompleted" in event && event.sessionTurnToolCallCompleted) {
		return event.sessionTurnToolCallCompleted.session_id;
	}
	if ("sessionTurnToolCallFailed" in event && event.sessionTurnToolCallFailed) {
		return event.sessionTurnToolCallFailed.session_id;
	}
	if ("sessionTurnCompleted" in event && event.sessionTurnCompleted) {
		return event.sessionTurnCompleted.session_id;
	}
	if ("sessionTurnAborted" in event && event.sessionTurnAborted) {
		return event.sessionTurnAborted.session_id;
	}
	if ("sessionCheckpointCreated" in event && event.sessionCheckpointCreated) {
		return event.sessionCheckpointCreated.session_id;
	}
	if ("sessionPlanApproved" in event && event.sessionPlanApproved) {
		return event.sessionPlanApproved.session_id;
	}
	if ("sessionPlanHandedOff" in event && event.sessionPlanHandedOff) {
		return event.sessionPlanHandedOff.session_id;
	}
	return null;
}

function applyCoreEventToSnapshot(
	snapshot: RuntimeSessionSnapshot,
	event: CoreEvent,
): RuntimeSessionSnapshot {
	if (getCoreEventSessionId(event) !== snapshot.sessionId) {
		return snapshot;
	}

	if ("sessionTurnCompleted" in event && event.sessionTurnCompleted) {
		return { ...snapshot, activeTurnId: null, lastTurnState: "completed" };
	}
	if ("sessionTurnAborted" in event && event.sessionTurnAborted) {
		return { ...snapshot, activeTurnId: null, lastTurnState: "aborted" };
	}
	if ("sessionAborted" in event && event.sessionAborted) {
		return { ...snapshot, state: "aborted", activeTurnId: null };
	}
	if ("sessionResumed" in event && event.sessionResumed) {
		return { ...snapshot, state: "active" };
	}
	if ("sessionTurnStarted" in event && event.sessionTurnStarted) {
		return {
			...snapshot,
			activeTurnId: event.sessionTurnStarted.turn_id,
			lastTurnPrompt: event.sessionTurnStarted.prompt,
			lastTurnState: "running",
		};
	}
	if ("sessionCompleted" in event && event.sessionCompleted) {
		return { ...snapshot, state: "completed", activeTurnId: null };
	}
	return snapshot;
}

export default function App() {
	const { t } = useTranslation("common");
	useZoom(1);
	// Remote SSH backends were removed in the Connections cleanup. Keep the
	// query keys stable (still LOCAL_BACKEND_CACHE_KEY) so cached data lives
	// across mounts; the remote branches below are unreachable now.
	const activeRemoteEnvironment = null;
	// Remote backend mode is not enabled in the current desktop runtime.
	const isRemoteBackend = false;
	const backendCacheKey = LOCAL_BACKEND_CACHE_KEY;

	const {
		handleResizeKeyDown,
		handleResizeStart,
		inspectorWidth,
		inspectorCollapsed,
		isInspectorResizing,
		isSidebarResizing,
		sidebarCollapsed,
		sidebarWidth,
		setInspectorCollapsed,
		setSidebarCollapsed,
	} = useShellPanels();
	const workspacesQuery = useQuery({
		queryKey: ["workspaces", backendCacheKey],
		queryFn: async () => {
			const result = await listWorkspaces();
			return result.workspaces.map((workspace) =>
				workspaceToSummary(
					workspace,
					result.remoteBranchDeletionTargets?.[workspace.id] ?? null,
				),
			);
		},
		staleTime: 60_000,
		refetchOnWindowFocus: false,
	});
	const repositoriesQuery = useQuery({
		queryKey: ["repositories", backendCacheKey],
		queryFn: async () => {
			if (isRemoteBackend) {
				return [];
			}
			const result = await listRepositories();
			return result.repositories;
		},
		staleTime: 60_000,
		refetchOnWindowFocus: false,
	});
	const workspaceBundlesQuery = useQuery({
		queryKey: ["workspaceBundles", backendCacheKey],
		queryFn: async () => {
			if (isRemoteBackend) {
				return [];
			}
			const result = await listWorkspaceBundles();
			return result.bundles;
		},
		staleTime: 60_000,
		refetchOnWindowFocus: false,
	});
	const workspacesFromBackend = workspacesQuery.data ?? EMPTY_WORKSPACES;
	const repositoriesFromBackend = repositoriesQuery.data ?? [];
	const workspaceBundlesFromBackend = workspaceBundlesQuery.data ?? [];
	const navigationWorkspaces = useMemo(() => {
		const secondaryWorkspaceIds = new Set(
			workspaceBundlesFromBackend.flatMap((summary) =>
				summary.members
					.map((member) => member.workspaceId)
					.filter((workspaceId) => workspaceId !== summary.bundle.primaryWorkspaceId),
			),
		);
		return workspacesFromBackend
			.filter((workspace) => !secondaryWorkspaceIds.has(workspace.id))
			.map((workspace) => {
				const bundle = workspaceBundlesFromBackend.find(
					(summary) => summary.bundle.primaryWorkspaceId === workspace.id,
				);
				if (!bundle) {
					return workspace;
				}
				const memberWorkspaceIds = bundle.members.map((member) => member.workspaceId);
				const deletableMemberWorkspaceIds = bundle.members
					.filter((member) => member.createdForBundle)
					.map((member) => member.workspaceId);
				return {
					...workspace,
					name: bundle.bundle.name,
					bundleId: bundle.bundle.id,
					additionalWorkspaceIds: memberWorkspaceIds.filter(
						(workspaceId) => workspaceId !== workspace.id,
					),
					memberWorkspaceIds,
					memberNames: memberWorkspaceIds
						.map(
							(workspaceId) =>
								workspacesFromBackend.find((candidate) => candidate.id === workspaceId)?.name,
						)
						.filter((name): name is string => Boolean(name)),
					memberProjectNames: memberWorkspaceIds
						.map((workspaceId) => {
							const member = workspacesFromBackend.find(
								(candidate) => candidate.id === workspaceId,
							);
							if (!member?.rootPath) return null;
							const repository = repositoriesFromBackend.find(
								(candidate) => candidate.rootPath === member.rootPath,
							);
							return (
								(repository ? repositoryDisplayName(repository) : null) ??
								member.rootPath.split(/[\\/]/gu).filter(Boolean).at(-1) ??
								null
							);
						})
						.filter((name): name is string => Boolean(name)),
					remoteDeletionTargets: deletableMemberWorkspaceIds.flatMap(
						(workspaceId) =>
							workspacesFromBackend.find(
								(candidate) => candidate.id === workspaceId,
							)?.remoteDeletionTargets ?? [],
					),
				};
			});
	}, [repositoriesFromBackend, workspaceBundlesFromBackend, workspacesFromBackend]);
	const {
		allWorkspaces,
		applyWorkspaceTitle,
		archiveWorkspace,
		cloneWorkspaceFromUrl,
		completeWorkspace,
		createWorkspace,
		createWorkspaceFromSourceUrl,
		createWorkspaceBundle,
		deleteWorkspace,
		filteredWorkspaces,
		isCreatingWorkspace,
		renameWorkspace,
		restoreWorkspace,
		selectedWorkspace,
		selectedWorkspaceId,
		setSelectedWorkspaceId,
	} = useWorkspacesPanel(navigationWorkspaces);
	const selectedWorkspaceAdditionalWorkspaceIds = useMemo(() => {
		if (!selectedWorkspace) {
			return [];
		}
		if (selectedWorkspace.additionalWorkspaceIds) {
			return selectedWorkspace.additionalWorkspaceIds;
		}
		const bundle = workspaceBundlesFromBackend.find(
			(summary) => summary.bundle.primaryWorkspaceId === selectedWorkspace.id,
		);
		return (
			bundle?.members
				.map((member) => member.workspaceId)
				.filter((workspaceId) => workspaceId !== selectedWorkspace.id) ?? []
		);
	}, [selectedWorkspace, workspaceBundlesFromBackend]);
	const queryClient = useQueryClient();
	const [isCommandPaletteOpen, setIsCommandPaletteOpen] = useState(false);
	const [delegateSignal, setDelegateSignal] = useState(0);
	const [reviewDelegationRequest, setReviewDelegationRequest] = useState<{
		delegationId: string;
		nonce: number;
	} | null>(null);
	const [isCreateWorkspaceOpen, setIsCreateWorkspaceOpen] = useState(false);
	const [workspaceCreationMode, setWorkspaceCreationMode] = useState<"open" | "clone">(
		"open",
	);
	const [workspaceRepositoryContext, setWorkspaceRepositoryContext] =
		useState<ExistingRepositoryContext | null>(null);
	const [isSettingsOpen, setIsSettingsOpen] = useState(false);
	const [isSkillsOpen, setIsSkillsOpen] = useState(false);
	const [globalSurface, setGlobalSurface] = useState<"pullRequests" | null>(null);
	const [isSessionSearchOpen, setIsSessionSearchOpen] = useState(false);
	const [isQuickOpenOpen, setIsQuickOpenOpen] = useState(false);
	const [isWorkspaceSearchOpen, setIsWorkspaceSearchOpen] = useState(false);
	const [isOnboardingOpen, setIsOnboardingOpen] = useState(() => {
		if (typeof window === "undefined") {
			return false;
		}

		return resolveInitialOnboardingOpen({
			hasOnboardingQuery: window.location.search.includes("onboarding=1"),
			isOnboardingCompleteInStorage:
				window.localStorage.getItem(ONBOARDING_COMPLETE_KEY) === "true",
			isDev: import.meta.env.DEV,
			viteDevOnboarding: import.meta.env.VITE_DEV_ONBOARDING,
		});
	});
	const [isShortcutSheetOpen, setIsShortcutSheetOpen] = useState(false);
	const providersQuery = useQuery({
		queryKey: ["providers", "catalog"],
		queryFn: listProviders,
		staleTime: 300_000,
		placeholderData: () => ({ catalog: FALLBACK_PROVIDER_CATALOG }),
	});
	const providerCatalog =
		providersQuery.data?.catalog ?? FALLBACK_PROVIDER_CATALOG;
	const workspaceSessionsQuery = useQuery(
		workspaceSessionsQueryOptions(selectedWorkspace?.id ?? null, {
			scope: backendCacheKey,
			refetchInterval: false,
		}),
	);
	const workspaceSessions = workspaceSessionsQuery.data ?? [];
	const delegationsQuery = useQuery({
		queryKey: ["delegations", selectedWorkspace?.id ?? null],
		queryFn: async () => {
			if (!selectedWorkspace?.id) {
				return [] as Delegation[];
			}
			const output = await listDelegations({
				workspaceId: selectedWorkspace.id,
				parentSessionId: null,
			});
			return output.delegations;
		},
		enabled: Boolean(selectedWorkspace?.id),
		staleTime: 5_000,
		refetchInterval: 10_000,
	});
	const pendingImplementationDelegationChildSessionIds = useMemo(() => {
		const ids = new Set<string>();
		for (const delegation of delegationsQuery.data ?? []) {
			if (
				delegation.mode === "implement" &&
				delegation.status === "review_pending" &&
				delegation.childSessionId
			) {
				ids.add(delegation.childSessionId);
			}
		}
		return ids;
	}, [delegationsQuery.data]);
	const [selectedProviderId, setSelectedProviderId] = useState<string | null>(() => {
		if (typeof window === "undefined") {
			return null;
		}

		return window.localStorage.getItem(SELECTED_PROVIDER_STORAGE_KEY);
	});
	const [selectedModelId, setSelectedModelId] = useState<string | null>(() => {
		if (typeof window === "undefined") {
			return null;
		}

		return window.localStorage.getItem(SELECTED_MODEL_STORAGE_KEY);
	});
	const [providerRuntimeSettings, setProviderRuntimeSettings] =
		useState<ProviderRuntimeSettings>(() => readProviderRuntimeSettings());
	const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
	const [pendingSessionClose, setPendingSessionClose] =
		useState<PendingSessionClose | null>(null);
	const [pendingSessionNavigation, setPendingSessionNavigation] =
		useState<PendingSessionNavigation | null>(null);
	const [sessionActionSessionId, setSessionActionSessionId] = useState<string | null>(null);
	const [inspectorTab, setInspectorTab] = useState<
		"activity" | "context" | "spec"
	>("activity");
	const [inspectorMode, setInspectorMode] =
		useState<WorkspaceInspectorMode>("git");
	const [sessionSnapshotsById, setSessionSnapshotsById] = useState<
		Record<string, RuntimeSessionSnapshot>
	>({});
	const [pendingPrompt, setPendingPrompt] = useState<string | null>(null);
	const [pendingPromptSessionId, setPendingPromptSessionId] = useState<
		string | null
	>(null);
	const autoCompiledWorkspaceSpecsRef = useRef<Set<string>>(new Set());
	const delegatedChildSessionsRef = useRef<Map<string, DelegationChildBinding>>(
		new Map(),
	);

	const finalizeDelegationFromChild = useCallback(
		async (
			childSessionId: string,
			status: "completed" | "failed",
			reason?: string | null,
		) => {
			const binding = delegatedChildSessionsRef.current.get(childSessionId);
			if (!binding || binding.finalized) {
				return;
			}
			binding.finalized = true;

			try {
				if (status === "completed") {
					const summary = await summarizeSessionForDelegation(childSessionId);
					const validationSummary =
						await summarizeDelegationValidation(childSessionId);
					const artifact = await collectDelegationDiffArtifact(binding.workspacePath);
					await completeDelegation({
						delegationId: binding.delegationId,
						summary,
						touchedFiles: artifact.touchedFiles,
						diffSummary: artifact.diffSummary,
						validationSummary,
						reviewRequired: binding.reviewRequired,
					});
					await queryClient.invalidateQueries({
						queryKey: ["delegations", binding.workspaceId],
					});
				} else {
					await failDelegation({
						delegationId: binding.delegationId,
						reason: reason ?? "Child session failed.",
					});
					await queryClient.invalidateQueries({
						queryKey: ["delegations", binding.workspaceId],
					});
				}
			} catch (error) {
				binding.finalized = false;
				console.error("[dcc] delegation finalization failed:", error);
			}
		},
		[queryClient],
	);

	/**
	 * Apply each live event to the snapshot of the session that owns it — not
	 * just the selected one — so background sessions (e.g. a plan implementation
	 * thread opened in another tab) keep advancing while their tab is inactive.
	 */
	const handleSessionEvent = useCallback(
		(event: CoreEvent) => {
			const eventSessionId = getCoreEventSessionId(event);
			if (!eventSessionId) {
				return;
			}
			setSessionSnapshotsById((current) => {
				const prev = current[eventSessionId];
				if (!prev) {
					return current;
				}
				const next = applyCoreEventToSnapshot(prev, event);
				if (next === prev) {
					return current;
				}
				return { ...current, [eventSessionId]: next };
			});

			if ("sessionTurnCompleted" in event && event.sessionTurnCompleted) {
				void finalizeDelegationFromChild(
					event.sessionTurnCompleted.session_id,
					"completed",
				);
			}
			if ("sessionTurnAborted" in event && event.sessionTurnAborted) {
				void finalizeDelegationFromChild(
					event.sessionTurnAborted.session_id,
					"failed",
					event.sessionTurnAborted.reason,
				);
			}
		},
		[finalizeDelegationFromChild],
	);
	const { activityEvents: sessionActivityEvents, events: sessionEvents } =
		useSessionEventFeed(handleSessionEvent);

	const [surfaceSelection, setSurfaceSelection] =
		useState<WorkspaceSurfaceSelection | null>(null);
	const inspectorBeforeMergeRef = useRef<boolean | null>(null);
	const fileOpenRequestIdRef = useRef(0);
	const [workspaceComposerPrefill, setWorkspaceComposerPrefill] =
		useState<WorkspaceComposerPrefillRequest | null>(null);
	const { theme, setTheme, density, setDensity } = useAppearance();
	const {
		update: appUpdateInfo,
		currentVersion: appCurrentVersion,
		checkError: appUpdateCheckError,
		isChecking: isCheckingUpdate,
		isInstalling: isInstallingUpdate,
		checkForUpdate,
		installUpdate,
	} = useAppUpdate();
	const providerChoices = providerCatalog.providers;
	const selectedBundleMembers = useMemo(
		() =>
			(selectedWorkspace?.memberWorkspaceIds ?? [])
				.map((workspaceId) =>
					workspacesFromBackend.find((workspace) => workspace.id === workspaceId),
				)
				.filter((workspace): workspace is NonNullable<typeof workspace> => Boolean(workspace)),
		[selectedWorkspace?.memberWorkspaceIds, workspacesFromBackend],
	);
	const bundleMemberChangeQueries = useQueries({
		queries: selectedBundleMembers.map((workspace) => {
			const workspaceRoot = workspace.worktreePath ?? workspace.rootPath ?? "";
			return {
				queryKey: ["multiWorkspaceChanges", workspace.id, workspaceRoot],
				queryFn: async () => {
					const [status, branchDiff] = await Promise.all([
						workspaceGitStatus({ workspaceRoot }),
						workspaceGitBranchDiff({ workspaceRoot }),
					]);
					const hasBranchDiff = branchDiff.changes.length > 0;
					let requestState: string | null = null;
					if (hasBranchDiff) {
						try {
							const request = await workspacePrStatus({
								workspaceRoot,
								branch: status.currentBranch,
								forgeLogin: null,
							});
							requestState = request.state?.toLowerCase() ?? null;
						} catch {
							// Keep delivery available so the coordinated action can report
							// the provider/authentication error for this repository.
						}
					}
					return resolveMultiWorkspaceDeliveryState({
						gitStatus: status,
						branchDiff,
						requestState,
					});
				},
				enabled: selectedBundleMembers.length > 1 && workspaceRoot.length > 0,
				refetchInterval: 5_000,
				staleTime: 2_000,
			};
		}),
	});
	const [selectedBundleMemberId, setSelectedBundleMemberId] = useState<string | null>(null);
	useEffect(() => {
		setSelectedBundleMemberId((current) =>
			current && selectedBundleMembers.some((workspace) => workspace.id === current)
				? current
				: selectedWorkspace?.id ?? null,
		);
	}, [selectedBundleMembers, selectedWorkspace?.id]);
	const activeWorkspace =
		selectedBundleMembers.find((workspace) => workspace.id === selectedBundleMemberId) ??
		selectedWorkspace;
	const activeProjectRootPath = activeWorkspace?.rootPath ?? null;
	const activeProjectId = activeWorkspace?.projectId ?? null;
	const canCreateTaskFromDock =
		selectedBundleMembers.length <= 1 &&
		Boolean(activeProjectRootPath && activeProjectId);
	const composerFocusSequenceRef = useRef(0);
	const [pendingComposerFocusRequest, setPendingComposerFocusRequest] = useState<{
		workspaceId: string;
		key: number;
	} | null>(null);
	const requestNewTaskComposerFocus = useCallback((workspaceId: string) => {
		composerFocusSequenceRef.current += 1;
		setPendingComposerFocusRequest({
			workspaceId,
			key: composerFocusSequenceRef.current,
		});
	}, []);
	const composerFocusRequestKey =
		pendingComposerFocusRequest &&
		pendingComposerFocusRequest.workspaceId === selectedWorkspace?.id
			? pendingComposerFocusRequest.key
			: null;
	useEffect(() => {
		if (composerFocusRequestKey === null) {
			return;
		}
		const timeout = window.setTimeout(() => {
			setPendingComposerFocusRequest((current) =>
				current?.key === composerFocusRequestKey ? null : current,
			);
		}, 1_000);
		return () => window.clearTimeout(timeout);
	}, [composerFocusRequestKey]);
	const handleCreateTaskFromDockBranch = useCallback(
		async (baseBranch: string) => {
			if (!activeProjectRootPath || !activeProjectId) {
				throw new Error(t("composer.executionDock.origin.unavailable"));
			}
			const result = await createWorkspace({
				projectId: activeProjectId,
				workspaceRoot: activeProjectRootPath,
				baseBranch,
				name: null,
			});
			notifyWorkspaceCreationResult(t, "open", result);
			void queryClient.invalidateQueries({
				queryKey: ["workspaces", backendCacheKey],
			});
			requestNewTaskComposerFocus(result.workspace.id);
		},
		[
			activeProjectId,
			activeProjectRootPath,
			backendCacheKey,
			createWorkspace,
			queryClient,
			requestNewTaskComposerFocus,
			t,
		],
	);
	const handleSelectWorkspaceSurface = useCallback(
		(workspaceId: string) => {
			setGlobalSurface(null);
			setSelectedWorkspaceId(workspaceId);
		},
		[setSelectedWorkspaceId],
	);
	const handleWorkOnPullRequest = useCallback(
		async (pullRequest: PullRequestHubItem) => {
			const result = await createWorkspaceFromSourceUrl({
				projectId: pullRequest.projectId,
				workspaceRoot: pullRequest.repositoryRoot,
				url: pullRequest.url,
				name: null,
				forgeLogin: pullRequest.forgeLogin,
			});
			notifyWorkspaceCreationResult(t, "open", result);
			await Promise.all([
				queryClient.invalidateQueries({
					queryKey: ["workspaces", backendCacheKey],
				}),
				queryClient.invalidateQueries({
					queryKey: ["pullRequestHub", "list"],
				}),
			]);
			setGlobalSurface(null);
			requestNewTaskComposerFocus(result.workspace.id);
		},
		[
			backendCacheKey,
			createWorkspaceFromSourceUrl,
			queryClient,
			requestNewTaskComposerFocus,
			t,
		],
	);
	const handleDeliverWorkspaceScope = useCallback(async (): Promise<
		MultiWorkspaceDeliveryResult[]
	> => {
		const members = selectedBundleMembers.flatMap((workspace, index) =>
			bundleMemberChangeQueries[index]?.data?.needsDelivery === true
				? [
						{
							workspaceId: workspace.id,
							name: workspace.name,
							workspaceRoot: workspace.worktreePath ?? workspace.rootPath ?? "",
						},
					]
				: [],
		);
		const invalidMember = members.find((member) => !member.workspaceRoot);
		if (invalidMember) {
			throw new Error(`Workspace sem caminho Git: ${invalidMember.name}`);
		}

		const results = await deliverMultiWorkspace(members);
		await Promise.all(
			members.flatMap((member) => [
				queryClient.invalidateQueries({
					queryKey: [WORKSPACE_GIT_STATUS_QUERY_KEY, member.workspaceRoot],
				}),
				queryClient.invalidateQueries({
					queryKey: [WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY, member.workspaceRoot],
				}),
				queryClient.invalidateQueries({
					queryKey: [WORKSPACE_PR_STATUS_QUERY_KEY, member.workspaceRoot],
				}),
				queryClient.invalidateQueries({
					queryKey: ["multiWorkspaceChanges", member.workspaceId, member.workspaceRoot],
				}),
			]),
		);
		return results;
	}, [bundleMemberChangeQueries, queryClient, selectedBundleMembers]);
	const selectedWorkspacePath =
		activeWorkspace?.worktreePath ?? activeWorkspace?.rootPath ?? null;
	const selectedLocalWorkspacePath = isRemoteBackend ? null : selectedWorkspacePath;

	// Keep compiled skill artifacts in sync with the active worktree, so a freshly
	// created worktree (or one edited elsewhere) picks up project skills without
	// needing to re-save in the Skills dialog. Idempotent on the Rust side.
	const skillsProjectRoot = selectedWorkspace?.rootPath ?? null;
	const skillContextCountQuery = useQuery({
		queryKey: ["skills", "context-count", skillsProjectRoot, selectedWorkspacePath],
		enabled: !isRemoteBackend && Boolean(skillsProjectRoot),
		queryFn: async () => {
			if (!skillsProjectRoot) {
				return 0;
			}
			const detections = await detectSkillContext(
				skillsProjectRoot,
				selectedWorkspacePath,
			);
			return getTotalSkillContextCount(detections);
		},
		staleTime: 10_000,
		refetchOnWindowFocus: false,
	});
	const skillContextCount = skillContextCountQuery.data ?? 0;
	useEffect(() => {
		if (isRemoteBackend || !skillsProjectRoot || !selectedWorkspacePath) {
			return;
		}
		void compileSkills(skillsProjectRoot, selectedWorkspacePath)
			.then(() =>
				queryClient.invalidateQueries({
					queryKey: [
						"skills",
						"context-count",
						skillsProjectRoot,
						selectedWorkspacePath,
					],
				}),
			)
			.catch(() => {
				/* background recompile; errors surface on demand in the Skills dialog */
			});
	}, [isRemoteBackend, queryClient, skillsProjectRoot, selectedWorkspacePath]);
	const [
		missionSpecAutoCompileFailuresByKey,
		setMissionSpecAutoCompileFailuresByKey,
	] = useState<Record<string, MissionSpecAutoCompileFailure>>({});
	const registerMissionSpecAutoCompileAttempt = useCallback(
		async ({
			workspaceRoot,
			specRelativePath,
			trigger,
		}: {
			workspaceRoot: string;
			specRelativePath: string;
			trigger: MissionSpecAutoCompileTrigger;
		}) => {
			const result = await compileMissionSpecContextBestEffort({
				workspaceRoot,
				specRelativePath,
			});
			const failureKey = getMissionSpecAutoCompileFailureKey(
				workspaceRoot,
				specRelativePath,
			);
			setMissionSpecAutoCompileFailuresByKey((current) => {
				if (result.ok) {
					if (!(failureKey in current)) {
						return current;
					}
					const next = { ...current };
					delete next[failureKey];
					return next;
				}

				const previous = current[failureKey];
				return {
					...current,
					[failureKey]: {
						workspaceRoot,
						specRelativePath,
						trigger,
						consecutiveFailures: (previous?.consecutiveFailures ?? 0) + 1,
						lastError: result.errorMessage ?? "unknown error",
						lastAttemptAt: new Date().toISOString(),
					},
				};
			});
			return result.ok;
		},
		[],
	);
	const clearMissionSpecAutoCompileFailure = useCallback(
		({
			workspaceRoot,
			specRelativePath,
		}: {
			workspaceRoot: string;
			specRelativePath: string;
		}) => {
			const failureKey = getMissionSpecAutoCompileFailureKey(
				workspaceRoot,
				specRelativePath,
			);
			setMissionSpecAutoCompileFailuresByKey((current) => {
				if (!(failureKey in current)) {
					return current;
				}
				const next = { ...current };
				delete next[failureKey];
				return next;
			});
		},
		[],
	);
	const missionSpecAutoCompileFailures = useMemo(
		() => Object.values(missionSpecAutoCompileFailuresByKey),
		[missionSpecAutoCompileFailuresByKey],
	);
	useEffect(() => {
		if (!selectedLocalWorkspacePath || !selectedWorkspace) {
			return;
		}

		const workspaceKey = `${selectedWorkspace.id}:${selectedWorkspace.branch}`;
		if (autoCompiledWorkspaceSpecsRef.current.has(workspaceKey)) {
			return;
		}
		autoCompiledWorkspaceSpecsRef.current.add(workspaceKey);

		let cancelled = false;
		void (async () => {
			const specs = await listMissionSpecs({
				workspaceRoot: selectedLocalWorkspacePath,
			});
			if (cancelled) {
				return;
			}
			const preferredSpecName = buildMissionSpecFilename(selectedWorkspace.branch);
			const activeSpec =
				specs.specs.find((spec) => spec.name === preferredSpecName) ??
				specs.specs[0] ??
				null;
			if (!activeSpec) {
				return;
			}
			await registerMissionSpecAutoCompileAttempt({
				workspaceRoot: selectedLocalWorkspacePath,
				specRelativePath: activeSpec.relativePath,
				trigger: "setup_reopen",
			});
			if (!cancelled) {
				await queryClient.invalidateQueries({
					queryKey: ["missionSpecContextStatus", selectedLocalWorkspacePath],
				});
			}
		})().catch((error) => {
			console.warn("[dcc] workspace reopen spec context compile failed:", error);
		});

		return () => {
			cancelled = true;
		};
	}, [
		queryClient,
		registerMissionSpecAutoCompileAttempt,
		selectedLocalWorkspacePath,
		selectedWorkspace,
	]);
	const showRemoteUnsupported = useCallback(
		(kind: "sessions" | "workspaces") => {
			toast.info(
				kind === "sessions"
					? t("remoteMode.sessionsUnavailable")
					: t("remoteMode.workspaceActionsUnavailable"),
			);
		},
		[t],
	);
	const selectedProvider = useMemo(
		() =>
			providerChoices.find((provider) => provider.id === selectedProviderId) ??
			providerChoices[0] ??
			null,
		[providerChoices, selectedProviderId],
	);
	const selectedModel = useMemo(
		() =>
			selectedProvider?.models.find((model) => model.id === selectedModelId) ??
			selectedProvider?.models.find((model) => model.recommended) ??
			selectedProvider?.models[0] ??
			null,
		[selectedModelId, selectedProvider],
	);
	const selectedProviderBlockReason = useMemo(
		() => {
			const healthReason = getProviderUnhealthyReason(selectedProvider);
			if (healthReason) {
				return healthReason;
			}
			if (
				selectedProvider &&
				(selectedWorkspace?.additionalWorkspaceIds?.length ?? 0) > 0 &&
				selectedProvider.capabilities.supportsMultiRoot !== true
			) {
				return t("workspaceScope.providerUnsupported", {
					provider: selectedProvider.label,
				});
			}
			return null;
		},
		[selectedProvider, selectedWorkspace?.additionalWorkspaceIds?.length, t],
	);
	const selectedProviderRuntime = useMemo(
		() =>
			draftToProviderRuntimeConfig(
				selectedProvider
					? getProviderRuntimeDraft(providerRuntimeSettings, selectedProvider.id)
					: null,
			),
		[providerRuntimeSettings, selectedProvider],
	);
	useDockUnreadBadge(allWorkspaces);
	const visibleWorkspaceSessions = useMemo(
		() => visibleSessions(workspaceSessions),
		[workspaceSessions],
	);
	const effectiveSelectedSessionId =
		selectedSessionId &&
		visibleWorkspaceSessions.some(
			(summary) => summary.session.id === selectedSessionId,
		)
			? selectedSessionId
			: (visibleWorkspaceSessions[0]?.session.id ?? null);
	const selectedSessionSummary = useMemo(
		() =>
			workspaceSessions.find(
				(session) => session.session.id === effectiveSelectedSessionId,
			) ??
			null,
		[effectiveSelectedSessionId, workspaceSessions],
	);
	const persistAutomaticTaskTitle = useCallback(
		async (workspaceId: string, sessionId: string, title: string) => {
			const titled = await applyTaskTitle({
				workspaceId,
				sessionId,
				title,
			});
			if (!titled.applied) {
				return false;
			}

			applyWorkspaceTitle(workspaceId, title);
			queryClient.setQueryData<WorkspaceSummary[]>(
				["workspaces", backendCacheKey],
				(current = []) =>
					current.map((workspace) =>
						workspace.id === workspaceId
							? { ...workspace, name: title, isAutoNamed: false }
							: workspace,
					),
			);
			queryClient.setQueryData<WorkspaceSessionSummary[]>(
				getWorkspaceSessionsCacheKey(backendCacheKey, workspaceId),
				(current = []) =>
					current.map((summary) =>
						summary.session.id === sessionId
							? { ...summary, thread: titled.thread }
							: summary,
					),
			);
			return true;
		},
		[applyWorkspaceTitle, backendCacheKey, queryClient],
	);
	const automaticTaskTitleRepairAttemptsRef = useRef(new Set<string>());
	useEffect(() => {
		if (!selectedWorkspace || !selectedSessionSummary) {
			return;
		}
		if (
			!selectedWorkspace.isAutoNamed &&
			!isAutomaticTaskTitle(selectedWorkspace.name)
		) {
			return;
		}

		const threadTitle = selectedSessionSummary.thread.title.trim();
		if (
			!threadTitle ||
			isAutomaticTaskTitle(threadTitle) ||
			threadTitle.toLocaleLowerCase() === "new session"
		) {
			return;
		}

		const repairKey = `${selectedWorkspace.id}:${selectedSessionSummary.session.id}:${threadTitle}`;
		if (automaticTaskTitleRepairAttemptsRef.current.has(repairKey)) {
			return;
		}
		automaticTaskTitleRepairAttemptsRef.current.add(repairKey);
		void persistAutomaticTaskTitle(
			selectedWorkspace.id,
			selectedSessionSummary.session.id,
			threadTitle,
		).catch((error) => {
			automaticTaskTitleRepairAttemptsRef.current.delete(repairKey);
			console.error("[dcc] automatic task title repair failed:", error);
		});
	}, [persistAutomaticTaskTitle, selectedSessionSummary, selectedWorkspace]);
	const selectedSessionSnapshot = useMemo(() => {
		if (!effectiveSelectedSessionId) {
			return null;
		}

		return (
			sessionSnapshotsById[effectiveSelectedSessionId] ??
			(selectedSessionSummary
				? workspaceSessionSnapshotFromSummary(selectedSessionSummary)
				: null)
		);
	}, [effectiveSelectedSessionId, selectedSessionSummary, sessionSnapshotsById]);
	const selectedSessionWorkspacePath = useMemo(() => {
		if (isRemoteBackend) {
			return null;
		}
		const sessionOverride =
			selectedSessionSummary?.session.workingDirectoryOverride?.trim() ?? "";
		const sessionId = selectedSessionSummary?.session.id ?? null;
		const hasPendingImplementationReview =
			sessionId !== null && pendingImplementationDelegationChildSessionIds.has(sessionId);
		return sessionOverride.length > 0 && hasPendingImplementationReview
			? sessionOverride
			: selectedLocalWorkspacePath;
	}, [
		isRemoteBackend,
		pendingImplementationDelegationChildSessionIds,
		selectedLocalWorkspacePath,
		selectedSessionSummary,
	]);
	const openGitInspector = useCallback(() => {
		recordUxMetric("diff_discovered");
		setInspectorMode("git");
		setInspectorCollapsed(false);
	}, [setInspectorCollapsed]);
	const handleReviewDelegation = useCallback(
		(delegationId: string) => {
			setInspectorMode("git");
			setInspectorCollapsed(false);
			setSurfaceSelection(null);
			setReviewDelegationRequest((current) => ({
				delegationId,
				nonce: (current?.nonce ?? 0) + 1,
			}));
		},
		[setInspectorCollapsed],
	);
	const toggleGitInspector = useCallback(() => {
		if (inspectorCollapsed) {
			recordUxMetric("diff_discovered");
			setInspectorMode("git");
			setInspectorCollapsed(false);
			return;
		}
		setInspectorCollapsed(true);
	}, [inspectorCollapsed, setInspectorCollapsed]);
	const openPlanSurface = useCallback(() => {
		setSurfaceSelection({ kind: "plan" });
		setInspectorCollapsed(true);
	}, [setInspectorCollapsed]);
	const runWorkbenchCommand = useCallback(
		(command: WorkbenchCommand) => {
			recordUxMetric("command_palette_action");
			switch (command) {
				case "inspector.changes":
					recordUxMetric("diff_discovered");
					setInspectorMode("git");
					setInspectorCollapsed(false);
					return;
				case "inspector.files":
					setInspectorMode("code");
					setInspectorCollapsed(false);
					return;
				case "inspector.activity":
					setInspectorMode("git");
					setInspectorTab("activity");
					setInspectorCollapsed(false);
					return;
				case "inspector.details":
					setInspectorMode("git");
					setInspectorTab("context");
					setInspectorCollapsed(false);
					return;
				default:
					dispatchWorkbenchCommand(command);
			}
		},
		[setInspectorCollapsed],
	);

	useEffect(() => {
		if (providerChoices.length === 0) {
			return;
		}

		setSelectedProviderId((current) => {
			return resolveSelectedProviderId(providerChoices, current);
		});
	}, [providerChoices]);

	useEffect(() => {
		if (selectedProvider) {
			setSelectedModelId((current) =>
				resolveSelectedModelId(selectedProvider, current),
			);
		}
	}, [selectedProvider]);

	useEffect(() => {
		if (typeof window === "undefined") {
			return;
		}

		if (selectedProviderId) {
			window.localStorage.setItem(
				SELECTED_PROVIDER_STORAGE_KEY,
				selectedProviderId,
			);
			return;
		}

		window.localStorage.removeItem(SELECTED_PROVIDER_STORAGE_KEY);
	}, [selectedProviderId]);

	useEffect(() => {
		if (typeof window === "undefined") {
			return;
		}

		if (selectedModelId) {
			window.localStorage.setItem(SELECTED_MODEL_STORAGE_KEY, selectedModelId);
			return;
		}

		window.localStorage.removeItem(SELECTED_MODEL_STORAGE_KEY);
	}, [selectedModelId]);

	useEffect(() => {
		writeProviderRuntimeSettings(providerRuntimeSettings);
	}, [providerRuntimeSettings]);

	useEffect(() => {
		const onKeyDown = (event: KeyboardEvent) => {
			if (isCommandPaletteShortcut(event)) {
				event.preventDefault();
				setIsCommandPaletteOpen(true);
				return;
			}
			if (!selectedWorkspacePath) {
				return;
			}
			if (isToggleTerminalShortcut(event)) {
				event.preventDefault();
				dispatchWorkbenchCommand("terminal.toggle");
				return;
			}
			if (isFocusComposerShortcut(event)) {
				event.preventDefault();
				dispatchWorkbenchCommand("composer.focus");
				return;
			}
			// Quick Open (Cmd/Ctrl+P) is a navigation chord, so it wins even when a
			// text field has focus — the modifier means it can't be literal input.
			if (isQuickOpenShortcut(event)) {
				event.preventDefault();
				setIsQuickOpenOpen(true);
				return;
			}
			if (isWorkspaceSearchShortcut(event)) {
				event.preventDefault();
				setIsWorkspaceSearchOpen(true);
				return;
			}
			if (shouldIgnoreGlobalShortcutTarget(event.target)) {
				return;
			}
			if (!isOpenPreferredEditorShortcut(event)) {
				return;
			}

			event.preventDefault();
			const preferredEditor = getStoredPreferredEditor();
			void openInEditor(selectedWorkspacePath, preferredEditor).catch((error) => {
				toast.error(
					error instanceof Error
						? error.message
						: `Failed to open workspace in ${preferredEditor}`,
				);
			});
		};

		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, [selectedWorkspacePath]);

	/** Restore provider/model for this session, else follow backend snapshot. */
	useEffect(() => {
		if (providerChoices.length === 0) {
			return;
		}
		const sessionId = selectedSessionSnapshot?.sessionId;
		if (!sessionId) {
			return;
		}

		const stored = getSessionComposerSelection(sessionId);
		if (stored) {
			const provider = providerChoices.find((p) => p.id === stored.providerId);
			const modelId = resolveSelectedModelId(provider ?? null, stored.modelId);
			if (provider && modelId) {
				setSelectedProviderId(stored.providerId);
				setSelectedModelId(modelId);
				return;
			}
		}

		const sp = selectedSessionSnapshot.providerId;
		const sm = selectedSessionSnapshot.model;
		if (sp && sm) {
			const provider = providerChoices.find((p) => p.id === sp);
			const modelId = resolveSelectedModelId(provider ?? null, sm);
			if (provider && modelId) {
				setSelectedProviderId(sp);
				setSelectedModelId(modelId);
			}
		}
	}, [
		providerChoices,
		selectedSessionSnapshot?.sessionId,
		selectedSessionSnapshot?.providerId,
		selectedSessionSnapshot?.model,
	]);

	useEffect(() => {
		const sessionId = selectedSessionSnapshot?.sessionId;
		if (!sessionId || !selectedProviderId || !selectedModelId) {
			return;
		}
		setSessionComposerSelection(sessionId, {
			providerId: selectedProviderId,
			modelId: selectedModelId,
		});
	}, [selectedSessionSnapshot?.sessionId, selectedProviderId, selectedModelId]);

	useEffect(() => {
		setSelectedSessionId(null);
		setPendingSessionClose(null);
		setSessionActionSessionId(null);
		setSessionSnapshotsById({});
		setPendingPrompt(null);
		setPendingPromptSessionId(null);
		setSurfaceSelection(null);
	}, [selectedWorkspace?.id]);

	useEffect(() => {
		setIsCommandPaletteOpen(false);
		setIsCreateWorkspaceOpen(false);
		setSelectedSessionId(null);
		setPendingSessionNavigation(null);
		setPendingSessionClose(null);
		setSessionActionSessionId(null);
		setSessionSnapshotsById({});
		setPendingPrompt(null);
		setPendingPromptSessionId(null);
		setSurfaceSelection(null);
		setWorkspaceRepositoryContext(null);
		setIsSessionSearchOpen(false);
		setIsQuickOpenOpen(false);
		setIsWorkspaceSearchOpen(false);
	}, [backendCacheKey, selectedWorkspace?.id]);

	useEffect(() => {
		if (!pendingSessionNavigation) {
			return;
		}

		if (selectedWorkspace?.id !== pendingSessionNavigation.workspaceId) {
			return;
		}

		const hasTargetSession = workspaceSessions.some(
			(summary) => summary.session.id === pendingSessionNavigation.sessionId,
		);
		if (!hasTargetSession) {
			return;
		}

		setSelectedSessionId(pendingSessionNavigation.sessionId);
		setPendingSessionNavigation(null);
	}, [pendingSessionNavigation, selectedWorkspace?.id, workspaceSessions]);

	useEffect(() => {
		if (!selectedWorkspace?.id) {
			return;
		}

		if (visibleWorkspaceSessions.length === 0) {
			setSelectedSessionId(null);
			return;
		}

		setSessionSnapshotsById((current) => {
			const next = { ...current };
			for (const summary of workspaceSessions) {
				next[summary.session.id] = workspaceSessionSnapshotFromSummary(summary);
			}
			return next;
		});

		setSelectedSessionId((current) => {
			if (
				current &&
				visibleWorkspaceSessions.some((session) => session.session.id === current)
			) {
				return current;
			}

			return visibleWorkspaceSessions[0]?.session.id ?? null;
		});
	}, [selectedWorkspace?.id, visibleWorkspaceSessions, workspaceSessions]);

	const handleStartSession = useCallback(async () => {
		if (!selectedProvider || !selectedWorkspace) {
			return;
		}
		if (selectedProviderBlockReason) {
			toast.error(selectedProviderBlockReason);
			return;
		}
		try {
			const result = await startThread({
				workspaceId: selectedWorkspace.id,
				additionalWorkspaceIds: selectedWorkspaceAdditionalWorkspaceIds,
				projectId: selectedWorkspace.projectId ?? selectedWorkspace.id,
				providerId: selectedProvider.id,
				model: selectedModel?.id ?? null,
				providerRuntime: selectedProviderRuntime,
				title: selectedWorkspace.name,
			});

			const snapshot: RuntimeSessionSnapshot = {
				sessionId: result.session.id,
				projectId: result.session.projectId,
				workspaceId: result.session.workspaceId,
				providerId: result.session.providerId,
				model: result.session.model,
				state: result.projection.state,
				turnCount: result.projection.turnCount,
				checkpointCount: result.projection.checkpointCount,
				activeTurnId: result.projection.activeTurnId ?? null,
				lastTurnPrompt: null,
				lastTurnState: result.projection.activeTurnId ? "running" : null,
			};
			setSessionSnapshotsById((current) => ({
				...current,
				[result.session.id]: snapshot,
			}));
			setSelectedSessionId(result.session.id);
			queryClient.setQueryData<WorkspaceSessionSummary[]>(
				getWorkspaceSessionsCacheKey(backendCacheKey, selectedWorkspace.id),
				(current = []) => {
					const nextSummary: WorkspaceSessionSummary = {
						session: result.session,
						thread: result.thread,
						projection: result.projection,
						lastTurnPrompt: null,
						lastTurnState: result.projection.activeTurnId ? "running" : null,
						lastTurnStartedAt: null,
						lastTurnCompletedAt: null,
					};
					return [
						nextSummary,
						...current.filter(
							(summary) => summary.session.id !== result.session.id,
						),
					];
				},
			);
		} catch (error) {
			const message =
				error instanceof Error
					? error.message
					: typeof error === "string"
						? error
						: "Failed to create chat";
			console.error("[dcc] create chat failed:", error);
			toast.error(message);
		}
	}, [
		backendCacheKey,
		selectedModel,
		selectedProvider,
		selectedProviderBlockReason,
		selectedProviderRuntime,
		selectedWorkspace,
		selectedWorkspaceAdditionalWorkspaceIds,
		queryClient,
	]);

	const handleResolveConflictWithAgent = useCallback(
		async (
			request: AgentResolutionRunRequest,
		): Promise<AgentResolutionRunResult> => {
			if (!selectedProvider || !selectedWorkspace || !selectedLocalWorkspacePath) {
				throw new Error(
					"Selecione um workspace local e um provedor disponível para usar o agente.",
				);
			}
			if (selectedProviderBlockReason) {
				throw new Error(selectedProviderBlockReason);
			}

			const started = await startThread({
				workspaceId: selectedWorkspace.id,
				additionalWorkspaceIds: selectedWorkspaceAdditionalWorkspaceIds,
				projectId: selectedWorkspace.projectId ?? selectedWorkspace.id,
				providerId: selectedProvider.id,
				model: selectedModel?.id ?? null,
				providerRuntime: selectedProviderRuntime,
				title: request.title,
			});
			const startedSnapshot: RuntimeSessionSnapshot = {
				sessionId: started.session.id,
				projectId: started.session.projectId,
				workspaceId: started.session.workspaceId,
				providerId: started.session.providerId,
				model: started.session.model,
				state: started.projection.state,
				turnCount: started.projection.turnCount,
				checkpointCount: started.projection.checkpointCount,
				activeTurnId: started.projection.activeTurnId ?? null,
				lastTurnPrompt: null,
				lastTurnState: null,
			};
			setSessionSnapshotsById((current) => ({
				...current,
				[started.session.id]: startedSnapshot,
			}));
			queryClient.setQueryData<WorkspaceSessionSummary[]>(
				getWorkspaceSessionsCacheKey(backendCacheKey, selectedWorkspace.id),
				(current = []) => [
					{
						session: started.session,
						thread: started.thread,
						projection: started.projection,
						lastTurnPrompt: null,
						lastTurnState: null,
						lastTurnStartedAt: null,
						lastTurnCompletedAt: null,
					},
					...current.filter(
						(summary) => summary.session.id !== started.session.id,
					),
				],
			);

			try {
				const result = await sendTurn({
					sessionId: started.session.id,
					prompt: request.prompt,
					toolInstructions: [
						"This is a read-only Git conflict analysis request.",
						"Do not edit files or run state-changing commands.",
						"Never run git add, commit, merge, reset, checkout, clean, or push.",
						"Return only the exact response envelope requested by the user prompt.",
					].join("\n"),
					providerId: selectedProvider.id,
					model: selectedModel?.id ?? null,
					providerRuntime: selectedProviderRuntime,
					planMode: false,
					effort: "medium",
					fastMode: false,
				});
				const resultSnapshot: RuntimeSessionSnapshot = {
					sessionId: result.session.id,
					projectId: result.session.projectId,
					workspaceId: result.session.workspaceId,
					providerId: result.session.providerId,
					model: result.session.model,
					state: result.projection.state,
					turnCount: result.projection.turnCount,
					checkpointCount: result.projection.checkpointCount,
					activeTurnId: result.projection.activeTurnId ?? null,
					lastTurnPrompt: result.turn.content,
					lastTurnState: result.turn.state,
				};
				setSessionSnapshotsById((current) => ({
					...current,
					[result.session.id]: resultSnapshot,
				}));
				queryClient.setQueryData<WorkspaceSessionSummary[]>(
					getWorkspaceSessionsCacheKey(backendCacheKey, result.session.workspaceId),
					(current = []) =>
						current.map((summary) =>
							summary.session.id === result.session.id
								? {
										...summary,
										session: result.session,
										projection: result.projection,
										lastTurnPrompt: result.turn.content,
										lastTurnState: result.turn.state,
										lastTurnStartedAt: result.turn.createdAt,
										lastTurnCompletedAt:
											result.turn.state === "running"
												? null
												: result.turn.updatedAt,
									}
								: summary,
						),
				);

				request.onStarted?.({
					sessionId: result.session.id,
					turnId: result.turn.id,
				});
				let abortRequested = false;
				let rejectAbortFailure: ((reason: unknown) => void) | null = null;
				const abortFailure = new Promise<never>((_, reject) => {
					rejectAbortFailure = reject;
				});
				const requestAbort = () => {
					if (abortRequested) return;
					abortRequested = true;
					void abortRun({
						sessionId: result.session.id,
						reason: "Cancelada pelo usuário no resolvedor de conflitos",
					}).catch((error) => rejectAbortFailure?.(error));
				};
				request.signal?.addEventListener("abort", requestAbort, { once: true });
				if (request.signal?.aborted) {
					requestAbort();
				}

				let events: SessionEventRecord[];
				try {
					events = await Promise.race([
						waitForAgentResolutionTurn(
							result.session.id,
							result.turn.id,
							{
								loadEvents: loadSessionThreadEvents,
								onEvents: (nextEvents) =>
									request.onProgress?.({
										sessionId: result.session.id,
										turnId: result.turn.id,
										events: nextEvents,
									}),
							},
						),
						abortFailure,
					]);
				} finally {
					request.signal?.removeEventListener("abort", requestAbort);
				}
				const response = [...projectWorkspaceMessages(events, [], result.session.id, null)]
					.reverse()
					.find(
						(message) =>
							message.role === "assistant" && message.content.trim().length > 0,
					);
				if (!response) {
					throw new Error("O agente concluiu sem retornar uma sugestão textual.");
				}
				return { content: response.content, sessionId: result.session.id };
			} finally {
				void queryClient.invalidateQueries({
					queryKey: getWorkspaceSessionsCacheKey(
						backendCacheKey,
						selectedWorkspace.id,
					),
				});
			}
		},
		[
			backendCacheKey,
			queryClient,
			selectedLocalWorkspacePath,
			selectedModel,
			selectedProvider,
			selectedProviderBlockReason,
			selectedProviderRuntime,
			selectedWorkspace,
			selectedWorkspaceAdditionalWorkspaceIds,
		],
	);

	const handleDelegate = useCallback(
		async (request: ManualDelegationRequest) => {
			if (!selectedWorkspace || !selectedSessionSnapshot) {
				toast.error("Select an active parent session before delegating.");
				return;
			}

			const requestedTargetProviderIds = Array.from(
				new Set(
					(request.targetProviderIds?.length
						? request.targetProviderIds
						: [request.targetProviderId]
					).filter(Boolean),
				),
			);
			const targetProviderIds =
				request.mode === "implement"
					? requestedTargetProviderIds.slice(0, 1)
					: requestedTargetProviderIds;
			if (targetProviderIds.length === 0) {
				toast.error("Delegation target provider is unavailable.");
				return;
			}

			const parentSessionId = selectedSessionSnapshot.sessionId;
			const parentTitle = selectedSessionSummary?.thread.title ?? selectedWorkspace.name;
			const allowFileEdits = request.mode === "implement";
			if (allowFileEdits) {
				try {
					await assertImplementationDelegationWorkspaceReady(selectedLocalWorkspacePath);
				} catch (error) {
					const message =
						error instanceof Error
							? error.message
							: typeof error === "string"
								? error
								: "Implementation delegation preflight failed";
					toast.error(message);
					throw error;
				}
			}
			const threadTitle = `Delegated ${request.mode}: ${parentTitle}`;
			const failures: string[] = [];
			let startedCount = 0;

			for (const targetProviderId of targetProviderIds) {
				const targetProvider = providerChoices.find(
					(provider) => provider.id === targetProviderId,
				);
				if (!targetProvider) {
					failures.push(`${targetProviderId}: unavailable`);
					continue;
				}
				const targetProviderBlockReason = getProviderUnhealthyReason(targetProvider);
				if (targetProviderBlockReason) {
					failures.push(`${targetProvider.label}: ${targetProviderBlockReason}`);
					continue;
				}
				if (
					request.mode === "implement" &&
					!targetProvider.capabilities.supportsEditDelegation
				) {
					failures.push(
						`${targetProvider.label}: provider does not support edit delegations`,
					);
					continue;
				}
				const targetModelId =
					targetProvider.id === request.targetProviderId &&
					request.targetModelId &&
					targetProvider.models.some((model) => model.id === request.targetModelId)
						? request.targetModelId
						: (targetProvider.models.find((model) => model.recommended)?.id ??
							targetProvider.models[0]?.id ??
							null);
				const targetRuntime = draftToProviderRuntimeConfig(
					getProviderRuntimeDraft(providerRuntimeSettings, targetProvider.id),
				);
				let delegationId: string | null = null;
				let childSessionId: string | null = null;
				let implementationWorktreePath: string | null = null;
				try {
					if (allowFileEdits) {
						if (!selectedLocalWorkspacePath) {
							throw new Error("Implementation delegation requires a local worktree.");
						}
						const preparedWorktree = await workspacePrepareDelegationWorktree({
							workspaceRoot: selectedLocalWorkspacePath,
							delegationKey:
								selectedSessionSnapshot.activeTurnId ?? parentSessionId,
						});
						implementationWorktreePath = preparedWorktree.worktreePath;
					}
					const delegationWorkspacePath =
						implementationWorktreePath ?? selectedLocalWorkspacePath;
					const prompt =
						request.prebuiltPrompt ??
						(await buildManualDelegationPrompt({
							request,
							workspaceName: selectedWorkspace.name,
							workspaceBranch: selectedWorkspace.branch,
							workspacePath: delegationWorkspacePath,
							parentSessionId,
							parentSessionTitle: parentTitle,
							liveSessionEvents: sessionEvents,
						}));
					const started = await startThread({
						workspaceId: selectedWorkspace.id,
						projectId: selectedWorkspace.projectId ?? selectedWorkspace.id,
						providerId: targetProvider.id,
						model: targetModelId,
						providerRuntime: targetRuntime,
						workingDirectoryOverride: implementationWorktreePath,
						title: threadTitle,
					});
					childSessionId = started.session.id;
					const startedSnapshot: RuntimeSessionSnapshot = {
						sessionId: started.session.id,
						projectId: started.session.projectId,
						workspaceId: started.session.workspaceId,
						providerId: started.session.providerId,
						model: started.session.model,
						state: started.projection.state,
						turnCount: started.projection.turnCount,
						checkpointCount: started.projection.checkpointCount,
						activeTurnId: started.projection.activeTurnId ?? null,
						lastTurnPrompt: null,
						lastTurnState: started.projection.activeTurnId ? "running" : null,
					};
					setSessionSnapshotsById((current) => ({
						...current,
						[started.session.id]: startedSnapshot,
					}));
					queryClient.setQueryData<WorkspaceSessionSummary[]>(
						getWorkspaceSessionsCacheKey(backendCacheKey, selectedWorkspace.id),
						(current = []) => {
							const nextSummary: WorkspaceSessionSummary = {
								session: started.session,
								thread: started.thread,
								projection: started.projection,
								lastTurnPrompt: null,
								lastTurnState: started.projection.activeTurnId ? "running" : null,
								lastTurnStartedAt: null,
								lastTurnCompletedAt: null,
							};
							return [
								nextSummary,
								...current.filter(
									(summary) => summary.session.id !== started.session.id,
								),
							];
						},
					);

					const created = await createDelegation({
						parentSessionId,
						parentTurnId: selectedSessionSnapshot.activeTurnId,
						childSessionId,
						workspaceId: selectedWorkspace.id,
						targetProviderId: targetProvider.id,
						mode: request.mode,
						prompt,
						contextPolicy: request.contextPolicy,
						budget: {
							turnLimit: 1,
							timeoutSeconds: 600,
							allowFileEdits,
						},
					});
					delegationId = created.delegation.id;
					delegatedChildSessionsRef.current.set(childSessionId, {
						delegationId,
						childSessionId,
						parentSessionId,
						workspaceId: selectedWorkspace.id,
						workspacePath: delegationWorkspacePath,
						cleanupWorkspacePath: implementationWorktreePath,
						reviewRequired: allowFileEdits,
						finalized: false,
					});
					await startDelegation({ delegationId });

					await sendTurn({
						sessionId: childSessionId,
						prompt,
						providerId: targetProvider.id,
						model: targetModelId,
						providerRuntime: targetRuntime,
						planMode: false,
						effort: request.effort ?? "medium",
						fastMode: request.fastMode ?? true,
					});
					startedCount += 1;
				} catch (error) {
					const message =
						error instanceof Error
							? error.message
							: typeof error === "string"
								? error
								: "Failed to start delegation";
					console.error("[dcc] delegation failed:", error);
					if (delegationId) {
						await failDelegation({
							delegationId,
							reason: message,
						}).catch((failure) => {
							console.error("[dcc] delegation failure event failed:", failure);
						});
					}
					if (childSessionId) {
						const binding = delegatedChildSessionsRef.current.get(childSessionId);
						if (binding) {
							binding.finalized = true;
						}
					}
					if (implementationWorktreePath && selectedLocalWorkspacePath) {
						await workspaceRemoveDelegationWorktree({
							workspaceRoot: selectedLocalWorkspacePath,
							worktreePath: implementationWorktreePath,
							removeBranch: true,
						}).catch((cleanupError) => {
							console.error(
								"[dcc] delegation worktree cleanup failed:",
								cleanupError,
							);
						});
					}
					failures.push(`${targetProvider.label}: ${message}`);
				}
			}

			await queryClient.invalidateQueries({
				queryKey: getWorkspaceSessionsCacheKey(backendCacheKey, selectedWorkspace.id),
			});
			await queryClient.invalidateQueries({
				queryKey: ["delegations", selectedWorkspace.id],
			});

			if (startedCount === 0) {
				const message = failures[0] ?? "Failed to start delegation";
				toast.error(message);
				throw new Error(message);
			}
			if (failures.length > 0) {
				toast.warning(
					`${startedCount} delegation${startedCount === 1 ? "" : "s"} started; ${failures.length} failed.`,
				);
				return;
			}
			toast.success(
				startedCount === 1
					? "Delegation started"
					: `${startedCount} delegations started`,
			);
		},
		[
			backendCacheKey,
			providerChoices,
			providerRuntimeSettings,
			queryClient,
			selectedLocalWorkspacePath,
			selectedSessionSnapshot,
			selectedSessionSummary,
			selectedWorkspace,
			sessionEvents,
		],
	);

	/**
	 * Composer-initiated delegation. The user only picked target(s) and whether the
	 * run may write files; mode and context policy are derived here so neither term
	 * has to appear in the UI.
	 */
	const handleComposerDelegate = useCallback(
		async (request: ComposerDelegationRequest) => {
			if (request.targetProviderIds.length === 0) {
				return;
			}
			let hasWorkingTreeChanges = false;
			if (!request.allowFileEdits && selectedLocalWorkspacePath) {
				try {
					const status = await workspaceGitStatus({
						workspaceRoot: selectedLocalWorkspacePath,
					});
					hasWorkingTreeChanges =
						status.staged.length > 0 || status.unstaged.length > 0;
				} catch (error) {
					// Status is only used to pick review-vs-explain; a failure here should
					// not block the delegation.
					console.error("[dcc] delegation git status probe failed:", error);
				}
			}
			const defaults = resolveDelegationDefaults({
				allowFileEdits: request.allowFileEdits,
				hasWorkingTreeChanges,
			});
			await handleDelegate({
				targetProviderId: request.targetProviderIds[0],
				targetProviderIds: request.targetProviderIds,
				// Null lets handleDelegate resolve each target's recommended model.
				targetModelId: null,
				mode: defaults.mode,
				contextPolicy: defaults.contextPolicy,
				instruction: request.rawPrompt,
				effort: request.effort,
				fastMode: request.fastMode,
			});
		},
		[handleDelegate, selectedLocalWorkspacePath],
	);

	/**
	 * Replays a finished delegation on a different agent. The stored prompt already
	 * carries the context that was assembled the first time, so re-sending it
	 * verbatim keeps the two runs comparable instead of quietly changing the input.
	 */
	const handleRerunDelegation = useCallback(
		async (input: { delegationId: string; targetProviderId: string }) => {
			let record: Delegation | null;
			try {
				record = (await getDelegation({ delegationId: input.delegationId }))
					.delegation;
			} catch (error) {
				toast.error(
					error instanceof Error ? error.message : "Could not load the delegation.",
				);
				return;
			}
			if (!canRerunDelegation(record)) {
				toast.error(t("delegation.card.rerunUnavailable"));
				return;
			}
			await handleDelegate({
				targetProviderId: input.targetProviderId,
				targetProviderIds: [input.targetProviderId],
				targetModelId: null,
				mode: rerunMode(record),
				contextPolicy: record.contextPolicy,
				instruction: record.prompt,
				prebuiltPrompt: record.prompt,
			});
		},
		[handleDelegate, t],
	);

	const handleAgentDelegate = useCallback(
		async (request: AgentInitiatedDelegationRequest) => {
			if (!selectedProvider?.capabilities.canRequestDelegation) {
				throw new Error("The active provider cannot request delegation.");
			}
			if (!selectedSessionSnapshot || !selectedWorkspace) {
				throw new Error("Select an active parent session before delegating.");
			}

			const needsEdit = request.mode === "implement";
			const candidates = providerChoices.filter(
				(provider) =>
					provider.capabilities.canBeDelegationTarget &&
					provider.capabilities.supportsReadOnlyDelegation &&
					(!needsEdit || provider.capabilities.supportsEditDelegation),
			);
			const targetProvider =
				(request.targetProviderId
					? candidates.find((provider) => provider.id === request.targetProviderId)
					: null) ??
				candidates.find((provider) => provider.id !== selectedProvider.id) ??
				candidates[0] ??
				null;
			if (!targetProvider) {
				throw new Error("No delegation target provider is available.");
			}

			const targetModelId =
				request.targetModelId &&
				targetProvider.models.some((model) => model.id === request.targetModelId)
					? request.targetModelId
					: (targetProvider.models.find((model) => model.recommended)?.id ??
						targetProvider.models[0]?.id ??
						null);

			await handleDelegate({
				targetProviderId: targetProvider.id,
				targetModelId,
				mode: request.mode,
				contextPolicy: request.contextPolicy,
				instruction: request.instruction,
			});
		},
		[
			handleDelegate,
			providerChoices,
			selectedProvider,
			selectedSessionSnapshot,
			selectedWorkspace,
		],
	);

	const handleImplementPlanInNewThread = useCallback(
		async (input: { planMarkdown: string; planTitle: string | null }) => {
			const planMarkdown = input.planMarkdown.trim();
			if (!planMarkdown) {
				return false;
			}
			if (!selectedProvider || !selectedWorkspace) {
				return false;
			}
			if (selectedProviderBlockReason) {
				toast.error(selectedProviderBlockReason);
				return false;
			}

			const prompt = buildPlanImplementationPrompt(planMarkdown);
			const threadTitle = buildPlanImplementationThreadTitle(
				planMarkdown,
				input.planTitle,
			);
			let startedSessionId: string | null = null;

			try {
				const started = await startThread({
					workspaceId: selectedWorkspace.id,
					additionalWorkspaceIds: selectedWorkspaceAdditionalWorkspaceIds,
					projectId: selectedWorkspace.projectId ?? selectedWorkspace.id,
					providerId: selectedProvider.id,
					model: selectedModel?.id ?? null,
					providerRuntime: selectedProviderRuntime,
					title: threadTitle,
				});
				const sessionId = started.session.id;
				startedSessionId = sessionId;
				const startedSnapshot: RuntimeSessionSnapshot = {
					sessionId: started.session.id,
					projectId: started.session.projectId,
					workspaceId: started.session.workspaceId,
					providerId: started.session.providerId,
					model: started.session.model,
					state: started.projection.state,
					turnCount: started.projection.turnCount,
					checkpointCount: started.projection.checkpointCount,
					activeTurnId: started.projection.activeTurnId ?? null,
					lastTurnPrompt: null,
					lastTurnState: started.projection.activeTurnId ? "running" : null,
				};
				setSessionSnapshotsById((current) => ({
					...current,
					[sessionId]: startedSnapshot,
				}));
				setSelectedSessionId(startedSessionId);
				queryClient.setQueryData<WorkspaceSessionSummary[]>(
					getWorkspaceSessionsCacheKey(backendCacheKey, selectedWorkspace.id),
					(current = []) => {
						const nextSummary: WorkspaceSessionSummary = {
							session: started.session,
							thread: started.thread,
							projection: started.projection,
							lastTurnPrompt: null,
							lastTurnState: started.projection.activeTurnId ? "running" : null,
							lastTurnStartedAt: null,
							lastTurnCompletedAt: null,
						};
						return [
							nextSummary,
						...current.filter((summary) => summary.session.id !== sessionId),
					];
				},
				);

				setPendingPrompt(prompt);
				setPendingPromptSessionId(startedSessionId);
				const result = await sendTurn({
					sessionId,
					prompt,
					providerId: selectedProvider.id,
					model: selectedModel?.id ?? null,
					providerRuntime: selectedProviderRuntime,
					planMode: false,
					effort: "medium",
					fastMode: true,
				});

				const resultSnapshot: RuntimeSessionSnapshot = {
					sessionId: result.session.id,
					projectId: result.session.projectId,
					workspaceId: result.session.workspaceId,
					providerId: result.session.providerId,
					model: result.session.model,
					state: result.projection.state,
					turnCount: result.projection.turnCount,
					checkpointCount: result.projection.checkpointCount,
					activeTurnId: result.projection.activeTurnId ?? null,
					lastTurnPrompt: result.turn.content,
					lastTurnState: result.turn.state,
				};
				setSessionSnapshotsById((current) => ({
					...current,
					[result.session.id]: resultSnapshot,
				}));
				queryClient.setQueryData<WorkspaceSessionSummary[]>(
					getWorkspaceSessionsCacheKey(
						backendCacheKey,
						result.session.workspaceId,
					),
					(current = []) =>
						current.map((summary) =>
							summary.session.id === result.session.id
								? {
										...summary,
										session: result.session,
										projection: result.projection,
										lastTurnPrompt: result.turn.content,
										lastTurnState: result.turn.state,
										lastTurnStartedAt: result.turn.createdAt,
										lastTurnCompletedAt:
											result.turn.state === "running"
												? null
												: result.turn.updatedAt,
									}
								: summary,
						),
				);
				return true;
			} catch (error) {
				const message =
					error instanceof Error
						? error.message
						: typeof error === "string"
							? error
							: "Failed to create implementation thread";
				console.error("[dcc] implement plan thread failed:", error);
				toast.error(message);
				return false;
			} finally {
				setPendingPrompt((current) => (current === prompt ? null : current));
				setPendingPromptSessionId((current) =>
					current === startedSessionId ? null : current,
				);
			}
		},
		[
			backendCacheKey,
			queryClient,
			selectedModel,
			selectedProvider,
			selectedProviderBlockReason,
			selectedProviderRuntime,
			selectedWorkspace,
			selectedWorkspaceAdditionalWorkspaceIds,
		],
	);

	const handleSubmitPrompt = useCallback(async (
		turn: ComposerSubmittedTurn,
		options?: { forceNewSession?: boolean; targetSessionId?: string | null },
	) => {
		const trimmedPrompt = turn.rawPrompt.trim();
		if (trimmedPrompt.length === 0) {
			return;
		}
		const automaticTaskTitle = selectedWorkspace &&
			(selectedWorkspace.isAutoNamed || isAutomaticTaskTitle(selectedWorkspace.name))
			? deriveTaskTitle(trimmedPrompt, selectedWorkspace.name)
			: null;

		const targetSessionId = options?.targetSessionId ?? null;
		const targetSessionSummary =
			targetSessionId && !options?.forceNewSession
				? workspaceSessions.find(
						(summary) => summary.session.id === targetSessionId,
					) ?? null
				: null;
		if (targetSessionId && !options?.forceNewSession && !targetSessionSummary) {
			toast.error(t("inspector.delegations.applyMissingChild"));
			return;
		}
		const targetSessionProvider =
			targetSessionSummary != null
				? providerChoices.find(
						(provider) => provider.id === targetSessionSummary.session.providerId,
					) ?? null
				: null;
		const targetProviderBlockReason = targetSessionProvider
			? getProviderUnhealthyReason(targetSessionProvider)
			: null;
		const effectiveProviderBlockReason =
			targetProviderBlockReason ?? (targetSessionSummary ? null : selectedProviderBlockReason);
		if (effectiveProviderBlockReason) {
			toast.error(effectiveProviderBlockReason);
			return;
		}
		let currentSession =
			targetSessionSummary != null
				? workspaceSessionSnapshotFromSummary(targetSessionSummary)
				: selectedSessionSnapshot;
		let currentSessionId = targetSessionSummary?.session.id ?? selectedSessionId;
		const willStartSession = Boolean(
			options?.forceNewSession || !currentSession || !currentSessionId,
		);
		if (willStartSession && selectedProvider && selectedWorkspace) {
			// Render the first user turn before `start_thread` returns. The prompt is
			// temporarily workspace-scoped and is anchored to the real session as
			// soon as the backend creates it.
			setPendingPrompt(trimmedPrompt);
			setPendingPromptSessionId(null);
		}

		try {
			// `forceNewSession` always spins up a fresh thread (used by the diff
			// annotation flow); otherwise we reuse the selected session and only
			// start one when none exists yet.
			if (options?.forceNewSession || !currentSession || !currentSessionId) {
				if (!selectedProvider || !selectedWorkspace) {
					return;
				}

				const started = await startThread({
					workspaceId: selectedWorkspace.id,
					additionalWorkspaceIds: selectedWorkspaceAdditionalWorkspaceIds,
					projectId: selectedWorkspace.projectId ?? selectedWorkspace.id,
					providerId: selectedProvider.id,
					model: selectedModel?.id ?? null,
					providerRuntime: selectedProviderRuntime,
					title: automaticTaskTitle ?? selectedWorkspace.name,
				});

				currentSession = {
					sessionId: started.session.id,
					projectId: started.session.projectId,
					workspaceId: started.session.workspaceId,
					providerId: started.session.providerId,
					model: started.session.model,
					state: started.projection.state,
					turnCount: started.projection.turnCount,
					checkpointCount: started.projection.checkpointCount,
					activeTurnId: started.projection.activeTurnId ?? null,
					lastTurnPrompt: null,
					lastTurnState: started.projection.activeTurnId ? "running" : null,
				};
				const startedSessionId = started.session.id;
				currentSessionId = startedSessionId;
				setSelectedSessionId(currentSessionId);
				const startedSnapshot = currentSession as RuntimeSessionSnapshot;
				setSessionSnapshotsById((current) => ({
					...current,
					[startedSessionId]: startedSnapshot,
				}));
				queryClient.setQueryData<WorkspaceSessionSummary[]>(
					getWorkspaceSessionsCacheKey(backendCacheKey, selectedWorkspace.id),
					(current = []) => {
						const nextSummary: WorkspaceSessionSummary = {
							session: started.session,
							thread: started.thread,
							projection: started.projection,
							lastTurnPrompt: null,
							lastTurnState: started.projection.activeTurnId ? "running" : null,
							lastTurnStartedAt: null,
							lastTurnCompletedAt: null,
						};
						return [
							nextSummary,
							...current.filter(
								(summary) => summary.session.id !== started.session.id,
							),
						];
					},
				);
			}

			if (!currentSessionId || !currentSession) {
				return;
			}

			const turnProvider = targetSessionProvider ?? selectedProvider;
			const turnModel =
				targetSessionSummary != null
					? targetSessionSummary.session.model
					: (selectedModel?.id ?? null);
			const turnProviderRuntime =
				targetSessionSummary?.session.providerRuntime ?? selectedProviderRuntime;

			if (targetSessionSummary) {
				setSelectedSessionId(currentSessionId);
			}
			setPendingPrompt(trimmedPrompt);
			setPendingPromptSessionId(currentSessionId);
			const toolInstructions = resolveDelegateTaskToolInstructions({
				provider: turnProvider,
				providers: providerChoices,
			});

			const result = await sendTurn({
				sessionId: currentSessionId,
				prompt: trimmedPrompt,
				toolInstructions,
				providerId: turnProvider?.id ?? null,
				model: turnModel,
				providerRuntime: turnProviderRuntime,
				planMode: turn.envelope.planMode,
				effort: turn.envelope.effort,
				fastMode: turn.envelope.fastMode,
			});
			recordUxMetric("first_prompt");

			if (automaticTaskTitle && selectedWorkspace) {
				try {
					await persistAutomaticTaskTitle(
						selectedWorkspace.id,
						currentSessionId,
						automaticTaskTitle,
					);
				} catch (error) {
					// Naming is UX metadata and must never discard an accepted first turn.
					console.error("[dcc] automatic task title failed:", error);
				}
			}

			const resultSnapshot: RuntimeSessionSnapshot = {
				sessionId: result.session.id,
				projectId: result.session.projectId,
				workspaceId: result.session.workspaceId,
				providerId: result.session.providerId,
				model: result.session.model,
				state: result.projection.state,
				turnCount: result.projection.turnCount,
				checkpointCount: result.projection.checkpointCount,
				activeTurnId: result.projection.activeTurnId ?? null,
				lastTurnPrompt: result.turn.content,
				lastTurnState: result.turn.state,
			};
			setSessionSnapshotsById((current) => ({
				...current,
				[result.session.id]: resultSnapshot,
			}));
			queryClient.setQueryData<WorkspaceSessionSummary[]>(
				getWorkspaceSessionsCacheKey(backendCacheKey, result.session.workspaceId),
				(current = []) =>
					current.map((summary) =>
						summary.session.id === result.session.id
							? {
									...summary,
									session: result.session,
									projection: result.projection,
									lastTurnPrompt: result.turn.content,
									lastTurnState: result.turn.state,
									lastTurnStartedAt: result.turn.createdAt,
									lastTurnCompletedAt:
										result.turn.state === "running"
											? null
											: result.turn.updatedAt,
								}
							: summary,
					),
			);

			if (
				isCompactCommandPrompt(trimmedPrompt) &&
				result.turn.state === "completed" &&
				selectedLocalWorkspacePath &&
				selectedWorkspace
			) {
				const specs = await listMissionSpecs({
					workspaceRoot: selectedLocalWorkspacePath,
				});
				const preferredSpecName = buildMissionSpecFilename(selectedWorkspace.branch);
				const activeSpec =
					specs.specs.find((spec) => spec.name === preferredSpecName) ??
					specs.specs[0] ??
					null;

				if (activeSpec) {
					await registerMissionSpecAutoCompileAttempt({
						workspaceRoot: selectedLocalWorkspacePath,
						specRelativePath: activeSpec.relativePath,
						trigger: "post_compact",
					});
					await queryClient.invalidateQueries({
						queryKey: ["missionSpecContextStatus", selectedLocalWorkspacePath],
					});

					const planMessages = projectWorkspaceMessages(
						[],
						sessionEvents,
						currentSessionId,
						null,
					);
					const activePlanState = derivePlanFollowUpState(planMessages);
					const activePlanMarkdown =
						activePlanState.activePlanMessage?.plan?.markdown ??
						activePlanState.activePlanMessage?.content ??
						null;
					const reanchorPrompt = buildMissionReanchorPrompt({
						specMarkdown: activeSpec.content,
						planMarkdown: activePlanMarkdown,
						validationJson: activeSpec.validation?.content ?? null,
					});

					setPendingPrompt(reanchorPrompt);
					setPendingPromptSessionId(currentSessionId);
					try {
						const reanchorResult = await sendTurn({
							sessionId: currentSessionId,
							prompt: reanchorPrompt,
							providerId: selectedProvider?.id ?? null,
							model: selectedModel?.id ?? null,
							providerRuntime: selectedProviderRuntime,
							planMode: false,
							effort: "medium",
							fastMode: true,
						});

						const reanchorSnapshot: RuntimeSessionSnapshot = {
							sessionId: reanchorResult.session.id,
							projectId: reanchorResult.session.projectId,
							workspaceId: reanchorResult.session.workspaceId,
							providerId: reanchorResult.session.providerId,
							model: reanchorResult.session.model,
							state: reanchorResult.projection.state,
							turnCount: reanchorResult.projection.turnCount,
							checkpointCount: reanchorResult.projection.checkpointCount,
							activeTurnId: reanchorResult.projection.activeTurnId ?? null,
							lastTurnPrompt: reanchorResult.turn.content,
							lastTurnState: reanchorResult.turn.state,
						};
						setSessionSnapshotsById((current) => ({
							...current,
							[reanchorResult.session.id]: reanchorSnapshot,
						}));
						queryClient.setQueryData<WorkspaceSessionSummary[]>(
							getWorkspaceSessionsCacheKey(
								backendCacheKey,
								reanchorResult.session.workspaceId,
							),
							(current = []) =>
								current.map((summary) =>
									summary.session.id === reanchorResult.session.id
										? {
												...summary,
												session: reanchorResult.session,
												projection: reanchorResult.projection,
												lastTurnPrompt: reanchorResult.turn.content,
												lastTurnState: reanchorResult.turn.state,
												lastTurnStartedAt: reanchorResult.turn.createdAt,
												lastTurnCompletedAt:
													reanchorResult.turn.state === "running"
														? null
														: reanchorResult.turn.updatedAt,
											}
										: summary,
								),
						);
					} finally {
						setPendingPrompt((current) =>
							current === reanchorPrompt ? null : current,
						);
						setPendingPromptSessionId((current) =>
							current === currentSessionId ? null : current,
						);
					}
				}
			}
		} catch (error) {
			const message =
				error instanceof Error
					? error.message
					: typeof error === "string"
						? error
						: "Failed to send prompt";
			console.error("[dcc] send prompt failed:", error);
			toast.error(message);
		} finally {
			setPendingPrompt((current) =>
				current === trimmedPrompt ? null : current,
			);
			setPendingPromptSessionId((current) =>
				current === currentSessionId ? null : current,
			);
		}
	}, [
			backendCacheKey,
			persistAutomaticTaskTitle,
			providerChoices,
			queryClient,
			registerMissionSpecAutoCompileAttempt,
			selectedModel,
			selectedProvider,
			selectedProviderBlockReason,
			selectedProviderRuntime,
			selectedSessionId,
			selectedSessionSnapshot,
			selectedLocalWorkspacePath,
			selectedWorkspaceAdditionalWorkspaceIds,
			selectedWorkspace,
			sessionEvents,
			t,
			workspaceSessions,
	]);

	const handleSteerPrompt = useCallback(
		async (turn: ComposerSubmittedTurn) => {
			const prompt = turn.rawPrompt.trim();
			const snapshot = selectedSessionSnapshot;
			if (!snapshot?.activeTurnId || prompt.length === 0) {
				throw new Error(t("composer.followUp.noActiveTurn"));
			}
			const provider = providerChoices.find(
				(candidate) => candidate.id === snapshot.providerId,
			);
			if (!provider?.capabilities.supportsSteering) {
				throw new Error(t("composer.followUp.steerUnsupported"));
			}

			try {
				await steerTurn({
					sessionId: snapshot.sessionId,
					prompt,
				});
				recordUxMetric("steer_prompt");
			} catch (error) {
				const message =
					error instanceof Error
						? error.message
						: t("composer.followUp.steerFailed");
				toast.error(message);
				throw error;
			}
		},
		[providerChoices, selectedSessionSnapshot, t],
	);

	const handleQueuePrompt = useCallback(
		async (turn: ComposerSubmittedTurn) => {
			const prompt = turn.rawPrompt.trim();
			const snapshot = selectedSessionSnapshot;
			if (!snapshot?.activeTurnId || prompt.length === 0) {
				throw new Error(t("composer.followUp.noActiveTurn"));
			}
			const provider = providerChoices.find(
				(candidate) => candidate.id === snapshot.providerId,
			) ?? selectedProvider;
			try {
				await queueTurn({
					turn: {
						sessionId: snapshot.sessionId,
						prompt,
						toolInstructions: resolveDelegateTaskToolInstructions({
							provider,
							providers: providerChoices,
						}),
						providerId: null,
						model: null,
						providerRuntime: null,
						planMode: turn.envelope.planMode,
						effort: turn.envelope.effort,
						fastMode: turn.envelope.fastMode,
					},
				});
				recordUxMetric("queue_prompt");
			} catch (error) {
				const message =
					error instanceof Error
						? error.message
						: t("composer.followUp.queueFailed");
				toast.error(message);
				throw error;
			}
		},
		[providerChoices, selectedProvider, selectedSessionSnapshot, t],
	);

	const handleGeneratePlanFromSpec = useCallback(
		(specMarkdown: string) => {
			const prompt = buildPlanFromSpecPrompt(specMarkdown);
			void handleSubmitPrompt({
				rawPrompt: prompt,
				envelope: {
					planMode: true,
					effort: "medium",
					fastMode: true,
				},
			});
		},
		[handleSubmitPrompt],
	);

	const handleValidateMissionSpec = useCallback(
		(input: {
			specRelativePath: string;
			specMarkdown: string;
			planMarkdown: string | null;
		}) => {
			const prompt = buildMissionValidationPrompt(input);
			setInspectorMode("git");
			setInspectorCollapsed(false);
			setInspectorTab("activity");
			void handleSubmitPrompt({
				rawPrompt: prompt,
				envelope: {
					planMode: false,
					effort: "medium",
					fastMode: true,
				},
			});
		},
		[handleSubmitPrompt, setInspectorCollapsed],
	);

	const handleReanchorMissionSpec = useCallback(
		async (input: {
			specRelativePath: string;
			specMarkdown: string;
			planMarkdown: string | null;
			validationJson: string | null;
		}) => {
			if (selectedLocalWorkspacePath) {
				await registerMissionSpecAutoCompileAttempt({
					workspaceRoot: selectedLocalWorkspacePath,
					specRelativePath: input.specRelativePath,
					trigger: "reanchor",
				});
				await queryClient.invalidateQueries({
					queryKey: ["missionSpecContextStatus", selectedLocalWorkspacePath],
				});
			}

			const prompt = buildMissionReanchorPrompt(input);
			setInspectorMode("git");
			setInspectorCollapsed(false);
			setInspectorTab("activity");
			void handleSubmitPrompt({
				rawPrompt: prompt,
				envelope: {
					planMode: false,
					effort: "medium",
					fastMode: true,
				},
			});
		},
		[
			handleSubmitPrompt,
			queryClient,
			registerMissionSpecAutoCompileAttempt,
			selectedLocalWorkspacePath,
			setInspectorCollapsed,
		],
	);

	const handleContinueMissionCriterion = useCallback(
		async (input: {
			specRelativePath: string;
			specMarkdown: string;
			planMarkdown: string | null;
			validationJson: string | null;
			criterion: MissionResumeCriterion;
		}) => {
			if (selectedLocalWorkspacePath) {
				await registerMissionSpecAutoCompileAttempt({
					workspaceRoot: selectedLocalWorkspacePath,
					specRelativePath: input.specRelativePath,
					trigger: "continue",
				});
				await queryClient.invalidateQueries({
					queryKey: ["missionSpecContextStatus", selectedLocalWorkspacePath],
				});
			}

			const prompt = buildMissionContinueCriterionPrompt(input);
			setInspectorMode("git");
			setInspectorCollapsed(false);
			setInspectorTab("activity");
			void handleSubmitPrompt({
				rawPrompt: prompt,
				envelope: {
					planMode: false,
					effort: "medium",
					fastMode: true,
				},
			});
		},
		[
			handleSubmitPrompt,
			queryClient,
			registerMissionSpecAutoCompileAttempt,
			selectedLocalWorkspacePath,
			setInspectorCollapsed,
		],
	);

	const handleSelectProvider = useCallback(
		(providerId: string) => {
			setSelectedProviderId(providerId);
			const provider = providerChoices.find((candidate) => candidate.id === providerId);
			setSelectedModelId(resolveSelectedModelId(provider ?? null, null));
		},
		[providerChoices],
	);

	const handleSelectModel = useCallback((modelId: string) => {
		// Do NOT re-derive the provider from the model id here: model ids such
		// as "auto" are not unique across providers (droid and cursor both
		// expose "auto"), so scanning every provider would pick the wrong one.
		// The provider is already set explicitly via handleSelectProvider, which
		// the model picker always calls before onSelectModel.
		setSelectedModelId(modelId);
	}, []);

	const handleChangeProviderRuntime = useCallback(
		(providerId: string, draft: { homePath: string; shadowHomePath: string }) => {
			setProviderRuntimeSettings((current) =>
				setProviderRuntimeDraft(current, providerId, draft),
			);
		},
		[],
	);

	const handleClearProviderRuntime = useCallback((providerId: string) => {
		setProviderRuntimeSettings((current) =>
			clearProviderRuntimeDraft(current, providerId),
		);
	}, []);

	const handleSelectSession = useCallback((sessionId: string) => {
		setSelectedSessionId(sessionId);
	}, []);

	const handleOpenSessionSearch = useCallback(() => {
		setIsSessionSearchOpen(true);
	}, []);

	const handleOpenQuickOpen = useCallback(() => {
		setIsQuickOpenOpen(true);
	}, []);

	const handleSelectSessionSearchResult = useCallback(
		async (result: SessionSearchResult) => {
			try {
				setGlobalSurface(null);
				if (result.archivedAt) {
					await restoreSession({ sessionId: result.sessionId });
				}
				setPendingSessionNavigation({
					sessionId: result.sessionId,
					workspaceId: result.workspaceId,
				});
				setSelectedWorkspaceId(result.workspaceId);
				void queryClient.invalidateQueries({
					queryKey: getWorkspaceSessionsCacheKey(
						backendCacheKey,
						result.workspaceId,
					),
				});
				if (selectedWorkspace?.id === result.workspaceId) {
					setSelectedSessionId(result.sessionId);
				}
			} catch (error) {
				const message =
					error instanceof Error
						? error.message
						: typeof error === "string"
							? error
							: "Failed to open session search result";
				console.error("[dcc] open session search result failed:", error);
				toast.error(message);
			}
		},
		[
			backendCacheKey,
			queryClient,
			selectedWorkspace?.id,
			setSelectedWorkspaceId,
		],
	);

	const handleResumeSession = useCallback(async () => {
		if (!selectedSessionSnapshot || !canResumeSession(selectedSessionSnapshot)) {
			return;
		}

		const result = await resumeSession({ sessionId: selectedSessionSnapshot.sessionId });
		setSessionSnapshotsById((current) => {
			const prev = current[selectedSessionSnapshot.sessionId];
			if (!prev) {
				return current;
			}

			return {
				...current,
				[selectedSessionSnapshot.sessionId]: {
					...prev,
					state: result.projection.state,
					turnCount: result.projection.turnCount,
					checkpointCount: result.projection.checkpointCount,
					activeTurnId: result.projection.activeTurnId ?? null,
				},
			};
		});
		queryClient.setQueryData<WorkspaceSessionSummary[]>(
			getWorkspaceSessionsCacheKey(
				backendCacheKey,
				selectedSessionSnapshot.workspaceId,
			),
			(current = []) =>
				current.map((summary) =>
					summary.session.id === selectedSessionSnapshot.sessionId
						? {
								...summary,
								session: result.session,
								projection: result.projection,
							}
						: summary,
				),
		);
	}, [backendCacheKey, queryClient, selectedSessionSnapshot]);

	const handleOpenEditorFile = useCallback(
		(selection: WorkspaceGitPreviewSelection | null) => {
			setSurfaceSelection(
				selection
					? {
							kind: "git-diff",
							file: selection,
					  }
					: null,
			);
		},
		[],
	);

	const handleOpenMergeConflictResolver = useCallback(
		(input: {
			workspaceRoot: string;
			baseBranch: string | null;
			forgeLogin: string | null;
		}) => {
			inspectorBeforeMergeRef.current = inspectorCollapsed;
			setInspectorCollapsed(true);
			setSurfaceSelection({ kind: "merge-conflict", ...input });
		},
		[inspectorCollapsed, setInspectorCollapsed],
	);

	const handleMergeConflictStateChanged = useCallback(async (workspaceRoot: string) => {
		const root = workspaceRoot.trim();
		if (!root) return;
		await Promise.all([
			queryClient.invalidateQueries({
				queryKey: [WORKSPACE_GIT_STATUS_QUERY_KEY, root],
			}),
			queryClient.invalidateQueries({
				queryKey: [WORKSPACE_PR_STATUS_QUERY_KEY, root],
			}),
			queryClient.invalidateQueries({
				queryKey: [WORKSPACE_FORGE_CONTEXT_QUERY_KEY, root],
			}),
			queryClient.invalidateQueries({
				queryKey: [WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY, root],
			}),
		]);
	}, [queryClient]);

	const handleOpenFileFromQuickOpen = useCallback(
		({ path, name }: { path: string; name: string }) => {
			fileOpenRequestIdRef.current += 1;
			setSurfaceSelection({
				kind: "file-edit",
				path,
				name,
				requestId: fileOpenRequestIdRef.current,
				focusLine: null,
			});
		},
		[],
	);

	const handleOpenSearchMatch = useCallback(
		({ path, line }: { path: string; line: number }) => {
			const name = path.split("/").pop() ?? path;
			fileOpenRequestIdRef.current += 1;
			setSurfaceSelection({
				kind: "file-edit",
				path,
				name,
				requestId: fileOpenRequestIdRef.current,
				focusLine: line,
			});
		},
		[],
	);

	const handlePrefillComposer = useCallback(
		(text: string) => {
			if (!selectedWorkspace || text.trim().length === 0) {
				return;
			}
			setWorkspaceComposerPrefill((previous) => ({
				workspaceId: selectedWorkspace.id,
				text,
				nonce: (previous?.nonce ?? 0) + 1,
			}));
		},
		[selectedWorkspace],
	);

	const handleOpenMissionSpec = useCallback((spec: MissionSpecEntry | null) => {
		setSurfaceSelection(
			spec
				? {
						kind: "mission-spec",
						spec,
				  }
				: null,
		);
	}, []);

	const handleCloseSurface = useCallback(() => {
		if (
			surfaceSelection?.kind === "merge-conflict" &&
			inspectorBeforeMergeRef.current === false
		) {
			setInspectorCollapsed(false);
		}
		inspectorBeforeMergeRef.current = null;
		setSurfaceSelection(null);
	}, [setInspectorCollapsed, surfaceSelection]);
	const handleOpenAgentSession = useCallback(
		(sessionId: string) => {
			setSelectedSessionId(sessionId);
			handleCloseSurface();
		},
		[handleCloseSurface],
	);

	const handleAbortSession = useCallback(async () => {
		const visiblePendingPrompt =
			pendingPromptSessionId === effectiveSelectedSessionId ? pendingPrompt : null;
		if (
			!selectedSessionSnapshot ||
			!canAbortRun(selectedSessionSnapshot, visiblePendingPrompt)
		) {
			return;
		}

		const result = await abortRun({
			sessionId: selectedSessionSnapshot.sessionId,
			reason: "Stopped from shell",
		});
		setSessionSnapshotsById((current) => {
			const prev = current[selectedSessionSnapshot.sessionId];
			if (!prev) {
				return current;
			}

			return {
				...current,
				[selectedSessionSnapshot.sessionId]: {
					...prev,
					state: result.projection.state,
					turnCount: result.projection.turnCount,
					checkpointCount: result.projection.checkpointCount,
					activeTurnId: result.projection.activeTurnId ?? null,
				},
			};
		});
		queryClient.setQueryData<WorkspaceSessionSummary[]>(
			getWorkspaceSessionsCacheKey(
				backendCacheKey,
				selectedSessionSnapshot.workspaceId,
			),
			(current = []) =>
				current.map((summary) =>
					summary.session.id === selectedSessionSnapshot.sessionId
						? {
								...summary,
								session: result.session,
								projection: result.projection,
							}
						: summary,
				),
		);
		setPendingPrompt((current) =>
			visiblePendingPrompt && current === visiblePendingPrompt ? null : current,
		);
		setPendingPromptSessionId((current) =>
			current === selectedSessionSnapshot.sessionId ? null : current,
		);
	}, [
		backendCacheKey,
		effectiveSelectedSessionId,
		pendingPrompt,
		pendingPromptSessionId,
		queryClient,
		selectedSessionSnapshot,
	]);

	const performCloseSession = useCallback(
		async (request: PendingSessionClose) => {
			if (!selectedWorkspace) {
				return;
			}

			const sessionSummary = workspaceSessions.find(
				(summary) => summary.session.id === request.sessionId,
			);
			const sessionSnapshot =
				sessionSnapshotsById[request.sessionId] ??
				(sessionSummary
					? workspaceSessionSnapshotFromSummary(sessionSummary)
					: null);
			const closePendingPrompt =
				pendingPromptSessionId === request.sessionId ? pendingPrompt : null;
			const closesSelectedSession = effectiveSelectedSessionId === request.sessionId;
			const replacementSessionId = nextVisibleSessionIdAfterClose(
				workspaceSessions,
				request.sessionId,
			);
			const shouldCreateReplacement = shouldCreateReplacementSession(
				workspaceSessions,
				request.sessionId,
			);

			setSessionActionSessionId(request.sessionId);
			try {
				if (
					request.requiresAbort &&
					sessionSnapshot &&
					canAbortRun(sessionSnapshot, closePendingPrompt)
				) {
					const aborted = await abortRun({
						sessionId: request.sessionId,
						reason: "Closed from workbench",
					});
					setSessionSnapshotsById((current) => ({
						...current,
						[request.sessionId]: {
							...(current[request.sessionId] ?? sessionSnapshot),
							state: aborted.projection.state,
							turnCount: aborted.projection.turnCount,
							checkpointCount: aborted.projection.checkpointCount,
							activeTurnId: aborted.projection.activeTurnId ?? null,
						},
					}));
					queryClient.setQueryData<WorkspaceSessionSummary[]>(
						getWorkspaceSessionsCacheKey(backendCacheKey, selectedWorkspace.id),
						(current = []) =>
							current.map((summary) =>
								summary.session.id === request.sessionId
									? {
											...summary,
											session: aborted.session,
											projection: aborted.projection,
										}
									: summary,
							),
					);
				}

				const result = await closeSession({
					sessionId: request.sessionId,
					deleteHistory: request.deleteHistory,
				});

				queryClient.setQueryData<WorkspaceSessionSummary[]>(
					getWorkspaceSessionsCacheKey(backendCacheKey, selectedWorkspace.id),
					(current = []) =>
						result.deletedHistory
							? current.filter(
									(summary) => summary.session.id !== request.sessionId,
								)
							: current.map((summary) =>
									summary.session.id === request.sessionId
										? {
												...summary,
												thread: {
													...summary.thread,
													archived_at: result.archivedAt ?? summary.thread.archived_at,
												},
											}
										: summary,
								),
				);
				setSessionSnapshotsById((current) => {
					if (!result.deletedHistory) {
						return current;
					}

					const next = { ...current };
					delete next[request.sessionId];
					return next;
				});
				setPendingPrompt((current) =>
					pendingPromptSessionId === request.sessionId ? null : current,
				);
				setPendingPromptSessionId((current) =>
					current === request.sessionId ? null : current,
				);
				setPendingSessionClose((current) =>
					current?.sessionId === request.sessionId ? null : current,
				);

				if (closesSelectedSession) {
					setSelectedSessionId(replacementSessionId);
					if (shouldCreateReplacement) {
						await handleStartSession();
					}
				}
			} catch (error) {
				const message =
					error instanceof Error
						? error.message
						: typeof error === "string"
							? error
							: "Failed to close chat";
				console.error("[dcc] close session failed:", error);
				toast.error(message);
			} finally {
				setSessionActionSessionId((current) =>
					current === request.sessionId ? null : current,
				);
			}
		},
		[
			backendCacheKey,
			effectiveSelectedSessionId,
			handleStartSession,
			pendingPrompt,
			pendingPromptSessionId,
			queryClient,
			selectedWorkspace,
			sessionSnapshotsById,
			workspaceSessions,
		],
	);

	const handleCloseSession = useCallback(
		(sessionId: string) => {
			if (sessionActionSessionId) {
				return;
			}

			const summary = workspaceSessions.find(
				(candidate) => candidate.session.id === sessionId,
			);
			if (!summary || isSessionArchived(summary)) {
				return;
			}

			const snapshot =
				sessionSnapshotsById[sessionId] ??
				workspaceSessionSnapshotFromSummary(summary);
			const runningPrompt =
				pendingPromptSessionId === sessionId ? pendingPrompt : null;
			const request: PendingSessionClose = {
				sessionId,
				title: summary.thread.title,
				deleteHistory: isSessionEmpty(summary),
				requiresAbort: canAbortRun(snapshot, runningPrompt),
			};

			if (request.requiresAbort) {
				setPendingSessionClose(request);
				return;
			}

			void performCloseSession(request);
		},
		[
			pendingPrompt,
			pendingPromptSessionId,
			performCloseSession,
			sessionActionSessionId,
			sessionSnapshotsById,
			workspaceSessions,
		],
	);

	const handleConfirmCloseSession = useCallback(() => {
		if (!pendingSessionClose) {
			return;
		}

		void performCloseSession(pendingSessionClose);
	}, [pendingSessionClose, performCloseSession]);

	const handleRestoreSession = useCallback(
		async (sessionId: string) => {
			if (!selectedWorkspace) {
				return;
			}

			const summary = workspaceSessions.find(
				(candidate) => candidate.session.id === sessionId,
			);
			if (!summary) {
				return;
			}
			if (!isSessionArchived(summary)) {
				setSelectedSessionId(sessionId);
				return;
			}

			setSessionActionSessionId(sessionId);
			try {
				await restoreSession({ sessionId });
				queryClient.setQueryData<WorkspaceSessionSummary[]>(
					getWorkspaceSessionsCacheKey(backendCacheKey, selectedWorkspace.id),
					(current = []) =>
						current.map((candidate) =>
							candidate.session.id === sessionId
								? {
										...candidate,
										thread: {
											...candidate.thread,
											archived_at: null,
										},
									}
								: candidate,
						),
				);
				setSelectedSessionId(sessionId);
			} catch (error) {
				const message =
					error instanceof Error
						? error.message
						: typeof error === "string"
							? error
							: "Failed to restore chat";
				console.error("[dcc] restore session failed:", error);
				toast.error(message);
			} finally {
				setSessionActionSessionId((current) =>
					current === sessionId ? null : current,
				);
			}
		},
		[
			backendCacheKey,
			queryClient,
			selectedWorkspace,
			workspaceSessions,
		],
	);

	const handleCompleteOnboarding = useCallback(() => {
		try {
			window.localStorage.setItem(ONBOARDING_COMPLETE_KEY, "true");
		} catch {
			/* localStorage unavailable */
		}
		setIsOnboardingOpen(false);
	}, []);

	const openWorkspaceDialog = useCallback(
		(
			mode: "open" | "clone",
			repositoryContext: ExistingRepositoryContext | null = null,
		) => {
			if (isRemoteBackend) {
				showRemoteUnsupported("workspaces");
				return;
			}
			setWorkspaceCreationMode(mode);
			setWorkspaceRepositoryContext(mode === "open" ? repositoryContext : null);
			setIsCreateWorkspaceOpen(true);
		},
		[isRemoteBackend, showRemoteUnsupported],
	);
	const handleQuickCreateTask = useCallback(
		async (input: {
			projectId: string;
			workspaceRoot: string;
			baseBranch: string;
			label: string;
		}) => {
			try {
				const result = await createWorkspace({
					projectId: input.projectId,
					workspaceRoot: input.workspaceRoot,
					baseBranch: input.baseBranch,
					name: null,
				});
				notifyWorkspaceCreationResult(t, "open", result);
				void queryClient.invalidateQueries({
					queryKey: ["workspaces", backendCacheKey],
				});
				requestNewTaskComposerFocus(result.workspace.id);
			} catch (error) {
				toast.error(t("workspaceDialog.toastCreateError"), {
					description: error instanceof Error ? error.message : String(error),
				});
			}
		},
		[backendCacheKey, createWorkspace, queryClient, requestNewTaskComposerFocus, t],
	);

	const handleWorkspaceDialogOpenChange = useCallback((open: boolean) => {
		setIsCreateWorkspaceOpen(open);
		if (!open) {
			setWorkspaceRepositoryContext(null);
		}
	}, []);
	const refreshWorkspaceCollections = useCallback(
		async () =>
			Promise.all([
				queryClient.invalidateQueries({ queryKey: ["workspaces", backendCacheKey] }),
				queryClient.invalidateQueries({
					queryKey: ["workspaceBundles", backendCacheKey],
				}),
			]),
		[backendCacheKey, queryClient],
	);
	const handleArchiveWorkspace = useCallback(
		async (workspaceId: string) => {
			await archiveWorkspace(workspaceId);
			await refreshWorkspaceCollections();
		},
		[archiveWorkspace, refreshWorkspaceCollections],
	);
	const handleRenameWorkspace = useCallback(
		async (workspaceId: string, name: string) => {
			await renameWorkspace(workspaceId, name);
			await refreshWorkspaceCollections();
		},
		[refreshWorkspaceCollections, renameWorkspace],
	);
	const handleRestoreWorkspace = useCallback(
		async (workspaceId: string) => {
			await restoreWorkspace(workspaceId);
			await refreshWorkspaceCollections();
		},
		[refreshWorkspaceCollections, restoreWorkspace],
	);
	const handleCompleteWorkspace = useCallback(
		async (workspaceId: string) => {
			await completeWorkspace(workspaceId);
			await refreshWorkspaceCollections();
		},
		[completeWorkspace, refreshWorkspaceCollections],
	);
	const handleDeleteWorkspace = useCallback(
		async (
			workspaceId: string,
			options: {
				deleteRemoteBranch?: boolean;
				expectedRemoteTarget?: WorkspaceRemoteBranchDeletionTarget | null;
				expectedRemoteTargets?: WorkspaceRemoteBranchDeletionTarget[];
			} = {},
		) => {
			const workspace = allWorkspaces.find((candidate) => candidate.id === workspaceId);
			const affectedWorkspaceIds =
				workspace?.bundleId && workspace.memberWorkspaceIds?.length
					? workspace.memberWorkspaceIds
					: [workspaceId];
			await deleteWorkspace(workspaceId, options);
			for (const affectedWorkspaceId of affectedWorkspaceIds) {
				queryClient.removeQueries({
					queryKey: getWorkspaceSessionsCacheKey(
						backendCacheKey,
						affectedWorkspaceId,
					),
				});
				queryClient.removeQueries({
					queryKey: ["multiWorkspaceChanges", affectedWorkspaceId],
				});
			}
			await refreshWorkspaceCollections();
		},
		[
			allWorkspaces,
			backendCacheKey,
			deleteWorkspace,
			queryClient,
			refreshWorkspaceCollections,
		],
	);

	const handleDeleteProject = useCallback(
		async (input: { repositoryId: string; workspaceIds: string[] }) => {
			if (isRemoteBackend) {
				showRemoteUnsupported("workspaces");
				return;
			}
			await removeProjectFromDcc(input, {
				deleteRepository,
				removeLocalState: (workspaceIds) => {
					const removedWorkspaceIds = new Set(workspaceIds);
					queryClient.setQueryData<WorkspaceSummary[]>(
						["workspaces", backendCacheKey],
						(current = []) =>
							current.filter(
								(workspace) => !removedWorkspaceIds.has(workspace.id),
							),
					);
					queryClient.setQueryData<Repository[]>(
						["repositories", backendCacheKey],
						(current = []) =>
							current.filter(
								(repository) => repository.id !== input.repositoryId,
							),
					);
					for (const workspaceId of workspaceIds) {
						queryClient.removeQueries({
							queryKey: getWorkspaceSessionsCacheKey(
								backendCacheKey,
								workspaceId,
							),
						});
						queryClient.removeQueries({
							queryKey: ["multiWorkspaceChanges", workspaceId],
						});
					}
				},
				refreshRepositories: () =>
					queryClient.invalidateQueries({
						queryKey: ["repositories", backendCacheKey],
					}),
				refreshWorkspaces: refreshWorkspaceCollections,
			});
		},
		[
			backendCacheKey,
			isRemoteBackend,
			queryClient,
			refreshWorkspaceCollections,
			showRemoteUnsupported,
		],
	);
	const handleUpdateProjectIdentity = useCallback(
		async (input: {
			repositoryId: string;
			displayName: string | null;
			icon: string | null;
			color: string | null;
		}) => {
			const updated = await updateRepositoryIdentity({
				...input,
			});
			queryClient.setQueryData<Repository[]>(
				["repositories", backendCacheKey],
				(current = []) =>
					current.map((repository) =>
						repository.id === updated.id ? updated : repository,
					),
			);
		},
		[backendCacheKey, queryClient],
	);
	const handleRemoteWorkspaceMutation = useCallback(() => {
		showRemoteUnsupported("workspaces");
	}, [showRemoteUnsupported]);

	const visiblePendingPrompt =
		effectiveSelectedSessionId === pendingPromptSessionId ||
		(!effectiveSelectedSessionId && !pendingPromptSessionId)
			? pendingPrompt
			: null;
	const sidebarRailWidth = sidebarCollapsed ? 76 : sidebarWidth;
	const hasWorkspace = Boolean(selectedWorkspace);
	const activeProjectRoot = activeWorkspace?.rootPath ?? selectedWorkspace?.rootPath ?? null;
	const activeProjectRepository = activeProjectRoot
		? repositoriesFromBackend.find(
				(repository) => repository.rootPath === activeProjectRoot,
			) ?? null
		: null;
	const activeProjectLabel = activeProjectRepository
		? repositoryDisplayName(activeProjectRepository)
		: null;
	const activeProjectIcon = activeProjectRepository?.icon ?? null;
	const activeProjectColor = activeProjectRepository?.color ?? null;

	return (
		<>
			<main
				aria-label={t("app.shellAria")}
				className="relative h-screen overflow-hidden bg-background font-sans text-foreground antialiased"
			>
				<div className="relative flex h-full min-h-0 bg-background">
					<aside
						aria-label={t("app.workspaceSidebarAria")}
						data-dcc-sidebar-root
						className="relative flex h-full shrink-0 flex-col overflow-hidden bg-sidebar"
						style={{ width: `${sidebarRailWidth}px` }}
					>
						<WorkspacesSidebar
							collapsed={sidebarCollapsed}
							isCreatingWorkspace={isCreatingWorkspace}
							showAgentStates={!isRemoteBackend}
							sessionQueryScope={backendCacheKey}
							onSelectWorkspace={handleSelectWorkspaceSurface}
							onCreateWorkspace={() => {
								setGlobalSurface(null);
								openWorkspaceDialog("open");
							}}
							onCloneWorkspace={() => {
								setGlobalSurface(null);
								openWorkspaceDialog("clone");
							}}
							onCreateWorkspaceFromProject={
								isRemoteBackend
									? undefined
									: (repository) => {
										setGlobalSurface(null);
										void handleQuickCreateTask(repository);
									}
							}
							onOpenSettings={() => setIsSettingsOpen(true)}
							onOpenSkills={() => setIsSkillsOpen(true)}
							onOpenPullRequests={() => setGlobalSurface("pullRequests")}
							pullRequestsActive={globalSurface === "pullRequests"}
							onToggleCollapsed={() => setSidebarCollapsed((value) => !value)}
							onArchiveWorkspace={
								isRemoteBackend ? handleRemoteWorkspaceMutation : handleArchiveWorkspace
							}
							onRenameWorkspace={
								isRemoteBackend ? undefined : handleRenameWorkspace
							}
							onCompleteWorkspace={
								isRemoteBackend ? undefined : handleCompleteWorkspace
							}
							onRestoreWorkspace={
								isRemoteBackend ? handleRemoteWorkspaceMutation : handleRestoreWorkspace
							}
							onDeleteWorkspace={
								isRemoteBackend ? handleRemoteWorkspaceMutation : handleDeleteWorkspace
							}
							onDeleteProject={isRemoteBackend ? undefined : handleDeleteProject}
							onUpdateProjectIdentity={
								isRemoteBackend ? undefined : handleUpdateProjectIdentity
							}
							repositories={repositoriesFromBackend}
							skillCount={skillContextCount}
							appUpdate={appUpdateInfo}
							isInstallingUpdate={isInstallingUpdate}
							onInstallUpdate={() => {
								void installUpdate();
							}}
							selectedWorkspaceId={
								globalSurface === "pullRequests" ? null : selectedWorkspaceId
							}
							workspaces={filteredWorkspaces}
						/>
					</aside>

					<ResizeSeparator
						side="left"
						widthAt={sidebarRailWidth}
						ariaLabel={t("app.resizeSidebarAria")}
						ariaMin={MIN_SIDEBAR_WIDTH}
						ariaMax={MAX_SIDEBAR_WIDTH}
						ariaNow={sidebarWidth}
						isActive={isSidebarResizing}
						onMouseDown={handleResizeStart("sidebar")}
						onKeyDown={handleResizeKeyDown("sidebar")}
					/>

					<section
						aria-label={t("app.workspacePanelAria")}
						className="relative flex min-h-0 flex-1 flex-col overflow-hidden bg-background"
					>
						{/* Keep z-index below workspace viewport so header/toolbar clicks reach buttons (Tauri drag region steals first click when on top). */}
						<div
							aria-label={t("app.workspaceDragRegionAria")}
							data-tauri-drag-region
							className="absolute inset-x-0 top-0 z-10 h-9 bg-transparent"
						/>
						<div
							aria-label={t("app.workspaceViewportAria")}
							className="relative z-20 flex min-h-0 flex-1 flex-col bg-background"
						>
							<WorkspaceCommandPalette
								open={isCommandPaletteOpen}
								onOpenChange={setIsCommandPaletteOpen}
								workspaces={allWorkspaces}
								selectedWorkspaceId={selectedWorkspaceId}
								onSelectWorkspace={handleSelectWorkspaceSurface}
								onCreateWorkspace={() => {
									setGlobalSurface(null);
									openWorkspaceDialog("open");
								}}
								onCloneWorkspace={() => {
									setGlobalSurface(null);
									openWorkspaceDialog("clone");
								}}
								onOpenSettings={() => setIsSettingsOpen(true)}
								onOpenOnboarding={() => setIsOnboardingOpen(true)}
								onOpenShortcuts={() => setIsShortcutSheetOpen(true)}
								onOpenSkills={() => setIsSkillsOpen(true)}
								onRunWorkbenchCommand={runWorkbenchCommand}
								onDelegate={
									selectedSessionSnapshot
										? () => setDelegateSignal((signal) => signal + 1)
										: undefined
								}
							/>
							<SessionSearchDialog
								open={isSessionSearchOpen}
								onOpenChange={setIsSessionSearchOpen}
								selectedWorkspaceId={selectedWorkspaceId}
								queryScope={backendCacheKey}
								onSelectResult={handleSelectSessionSearchResult}
							/>
							<FileQuickOpen
								open={isQuickOpenOpen}
								onOpenChange={setIsQuickOpenOpen}
								workspaceRoot={selectedWorkspacePath}
								onSelectFile={handleOpenFileFromQuickOpen}
							/>
							<WorkspaceSearch
								open={isWorkspaceSearchOpen}
								onOpenChange={setIsWorkspaceSearchOpen}
								workspaceRoot={selectedWorkspacePath}
								onSelectMatch={handleOpenSearchMatch}
							/>
							<CreateWorkspaceDialog
								open={isCreateWorkspaceOpen}
								mode={workspaceCreationMode}
								repositoryContext={workspaceRepositoryContext}
								onOpenChange={handleWorkspaceDialogOpenChange}
								onCreateWorkspace={async (input) => {
									const result = await createWorkspace(input);
									void queryClient.invalidateQueries({
										queryKey: ["repositories", backendCacheKey],
									});
									requestNewTaskComposerFocus(result.workspace.id);
									return result;
								}}
								onCreateWorkspaceFromSourceUrl={async (input) => {
									const result = await createWorkspaceFromSourceUrl(input);
									requestNewTaskComposerFocus(result.workspace.id);
									return result;
								}}
								onCreateWorkspaceBundle={async (input) => {
									const result = await createWorkspaceBundle(input);
									void queryClient.invalidateQueries({
										queryKey: ["workspaceBundles"],
									});
									void queryClient.invalidateQueries({ queryKey: ["workspaces"] });
									requestNewTaskComposerFocus(result.primaryWorkspace.id);
									return result;
								}}
								onCloneWorkspace={async (input) => {
									const result = await cloneWorkspaceFromUrl(input);
									void queryClient.invalidateQueries({
										queryKey: ["repositories", backendCacheKey],
									});
									requestNewTaskComposerFocus(result.workspace.id);
									return result;
								}}
								repositories={repositoriesFromBackend}
								isSubmitting={isCreatingWorkspace}
							/>
							{globalSurface === "pullRequests" ? (
								<PullRequestsHub
									onOpenWorkspace={handleSelectWorkspaceSurface}
									onWorkOnPullRequest={handleWorkOnPullRequest}
									providers={providerChoices}
									selectedProviderId={selectedProviderId}
									selectedModelId={selectedModelId}
									selectedProviderRuntime={selectedProviderRuntime}
									onSelectProvider={handleSelectProvider}
									onSelectModel={handleSelectModel}
								/>
							) : hasWorkspace && selectedWorkspace ? (
								<SessionWorkbench
									workspaceId={selectedWorkspace.id}
									workspaceName={selectedWorkspace.name}
									workspaceBranch={selectedWorkspace.branch}
									workspacePath={selectedSessionWorkspacePath}
									workspaceSetupReport={
										activeWorkspace?.setupReport ?? selectedWorkspace.setupReport ?? null
									}
									composerFocusRequestKey={composerFocusRequestKey}
									terminalWorkspaceId={activeWorkspace?.id ?? selectedWorkspace.id}
									terminalWorkspaceName={activeWorkspace?.name ?? selectedWorkspace.name}
									terminalWorkspaceBranch={activeWorkspace?.branch ?? selectedWorkspace.branch}
									projectId={activeWorkspace?.projectId ?? selectedWorkspace.projectId ?? selectedWorkspace.id}
									terminalRootPath={activeWorkspace?.rootPath ?? selectedWorkspace.rootPath ?? null}
									projectLabel={activeProjectLabel}
									projectIcon={activeProjectIcon}
									projectColor={activeProjectColor}
									terminalWorktreePath={
										selectedWorkspacePath ??
										(isRemoteBackend ? null : (activeWorkspace?.worktreePath ?? null))
									}
									workspaceScopeOptions={selectedBundleMembers.map((workspace, index) => {
										const repository = repositoriesFromBackend.find(
											(candidate) => candidate.rootPath === workspace.rootPath,
										);
										return {
											id: workspace.id,
											name: repository
												? repositoryDisplayName(repository)
												: workspace.name,
											branch: workspace.branch,
											icon: repository?.icon ?? null,
											color: repository?.color ?? null,
											hasChanges:
												bundleMemberChangeQueries[index]?.data?.hasChanges ?? null,
											needsDelivery:
												bundleMemberChangeQueries[index]?.data?.needsDelivery ?? null,
										};
									})}
									selectedWorkspaceScopeId={activeWorkspace?.id ?? selectedWorkspace.id}
									onSelectWorkspaceScope={setSelectedBundleMemberId}
									onDeliverWorkspaceScope={
										isRemoteBackend ? undefined : handleDeliverWorkspaceScope
									}
									sessionQueryScope={backendCacheKey}
									selectedProviderLabel={selectedProvider?.label ?? null}
									selectedModelLabel={selectedModel?.label ?? null}
									selectedProviderId={selectedProviderId}
									selectedModelId={selectedModelId}
									selectedProviderRuntime={selectedProviderRuntime}
									providerChoices={providerChoices}
									sessions={workspaceSessions}
									selectedSessionId={effectiveSelectedSessionId}
									isLoadingSessions={workspaceSessionsQuery.isPending}
									sessionSnapshot={selectedSessionSnapshot}
									sessionEvents={sessionEvents}
									pendingPrompt={visiblePendingPrompt}
									onSelectProvider={handleSelectProvider}
									onSelectModel={handleSelectModel}
									onStartSession={handleStartSession}
									onSelectSession={handleSelectSession}
									onCloseSession={handleCloseSession}
									onRestoreSession={handleRestoreSession}
									onOpenSessionSearch={handleOpenSessionSearch}
									onSubmitPrompt={handleSubmitPrompt}
									onSteerPrompt={handleSteerPrompt}
									onQueuePrompt={handleQueuePrompt}
									onResumeSession={handleResumeSession}
									onAbortSession={handleAbortSession}
									onDelegate={handleDelegate}
									onDelegatePrompt={handleComposerDelegate}
									onAgentDelegate={handleAgentDelegate}
									sessionActionSessionId={sessionActionSessionId}
									surfaceSelection={surfaceSelection}
									onCloseSurface={handleCloseSurface}
									onOpenPlanSurface={openPlanSurface}
									onImplementPlanInNewThread={handleImplementPlanInNewThread}
									inspectorCollapsed={inspectorCollapsed}
									onToggleInspector={toggleGitInspector}
									onReviewChanges={openGitInspector}
									onCreateTaskFromBranch={
										canCreateTaskFromDock
											? handleCreateTaskFromDockBranch
											: undefined
									}
									onReviewDelegation={handleReviewDelegation}
									onRerunDelegation={handleRerunDelegation}
									onResolveConflictWithAgent={handleResolveConflictWithAgent}
									onOpenAgentSession={handleOpenAgentSession}
									onMergeConflictStateChanged={handleMergeConflictStateChanged}
									delegateSignal={delegateSignal}
									composerPrefill={
										workspaceComposerPrefill?.workspaceId === selectedWorkspace.id
											? {
													text: workspaceComposerPrefill.text,
													nonce: workspaceComposerPrefill.nonce,
													mode: workspaceComposerPrefill.mode,
												}
											: null
									}
								/>
							) : (
								<WorkspaceBootstrapState
									selectedProviderLabel={selectedProvider?.label ?? null}
									selectedModelLabel={selectedModel?.label ?? null}
									onCreateWorkspace={() => openWorkspaceDialog("open")}
									onCloneWorkspace={() => openWorkspaceDialog("clone")}
								/>
							)}
						</div>
					</section>

					{globalSurface !== "pullRequests" && !inspectorCollapsed && (
						<>
							<ResizeSeparator
								side="right"
								widthAt={inspectorWidth}
								ariaLabel={t("app.resizeInspectorAria")}
								ariaMin={MIN_INSPECTOR_WIDTH}
								ariaMax={MAX_INSPECTOR_WIDTH}
								ariaNow={inspectorWidth}
								isActive={isInspectorResizing}
								onMouseDown={handleResizeStart("inspector")}
								onKeyDown={handleResizeKeyDown("inspector")}
							/>

							<aside
								aria-label={t("app.inspectorSidebarAria")}
								className="inspector-enter relative h-full shrink-0 overflow-hidden bg-sidebar"
								style={{ width: `${inspectorWidth}px` }}
							>
								<WorkspaceInspectorSidebar
									providerCatalog={providerCatalog}
									sessionSnapshot={selectedSessionSnapshot}
									sessionEvents={sessionEvents}
									sessionActivityEvents={sessionActivityEvents}
									currentRepository={
										selectedWorkspace?.rootPath?.trim()
											? repositoriesFromBackend.find(
													(repository) =>
														repository.rootPath.trim() ===
														selectedWorkspace.rootPath!.trim(),
												) ?? null
											: null
									}
									workspaceId={selectedWorkspace?.id ?? null}
									workspaceName={selectedWorkspace?.name ?? null}
									workspaceBranch={selectedWorkspace?.branch ?? null}
									workspacePath={selectedLocalWorkspacePath}
									sessionWorkspacePath={selectedSessionWorkspacePath}
									workspaceStatus={selectedWorkspace?.status ?? null}
									workspaceSetupReport={selectedWorkspace?.setupReport ?? null}
									selectedProviderLabel={selectedProvider?.label ?? null}
									selectedModelLabel={selectedModel?.label ?? null}
									sessionState={selectedSessionSnapshot?.state ?? "idle"}
									sessionId={selectedSessionSnapshot?.sessionId ?? null}
									selectedPreview={
										surfaceSelection?.kind === "git-diff"
											? surfaceSelection.file
											: null
									}
									onSelectPreview={handleOpenEditorFile}
									onSelectSession={handleSelectSession}
									onPrefillComposer={handlePrefillComposer}
									onOpenMergeConflictResolver={handleOpenMergeConflictResolver}
									onOpenCodeFile={handleOpenFileFromQuickOpen}
									selectedCodePath={
										surfaceSelection?.kind === "file-edit"
											? surfaceSelection.path
											: null
									}
									onOpenQuickOpen={handleOpenQuickOpen}
									onOpenMissionSpec={handleOpenMissionSpec}
									onGeneratePlanFromSpec={handleGeneratePlanFromSpec}
									onValidateMissionSpec={handleValidateMissionSpec}
									onReanchorMissionSpec={handleReanchorMissionSpec}
									onContinueMissionCriterion={handleContinueMissionCriterion}
									missionSpecAutoCompileFailures={missionSpecAutoCompileFailures}
									onClearMissionSpecAutoCompileFailure={
										clearMissionSpecAutoCompileFailure
									}
									onCompleteWorkspace={
										isRemoteBackend ? undefined : handleCompleteWorkspace
									}
									reviewDelegationRequest={reviewDelegationRequest}
									activeTab={inspectorTab}
									onTabChange={setInspectorTab}
									mode={inspectorMode}
									onModeChange={setInspectorMode}
								/>
							</aside>
						</>
					)}
				</div>
			</main>
			<Dialog
				open={pendingSessionClose != null}
				onOpenChange={(open) => {
					if (!open && !sessionActionSessionId) {
						setPendingSessionClose(null);
					}
				}}
			>
				<DialogContent showCloseButton={!sessionActionSessionId}>
					<DialogHeader>
						<DialogTitle>{t("workbench.closeSessionConfirmTitle")}</DialogTitle>
						<DialogDescription>
							{pendingSessionClose
								? pendingSessionClose.deleteHistory
									? t("workbench.closeSessionConfirmEmptyDescription", {
											title: pendingSessionClose.title,
										})
									: t("workbench.closeSessionConfirmArchivedDescription", {
											title: pendingSessionClose.title,
										})
								: null}
						</DialogDescription>
						{pendingSessionClose?.requiresAbort ? (
							<DialogDescription>
								{t("workbench.closeSessionConfirmRunningDescription")}
							</DialogDescription>
						) : null}
					</DialogHeader>
					<DialogFooter>
						<Button
							variant="outline"
							onClick={() => setPendingSessionClose(null)}
							disabled={Boolean(sessionActionSessionId)}
						>
							{t("workbench.closeSessionCancel")}
						</Button>
						<Button
							variant="destructive"
							onClick={handleConfirmCloseSession}
							disabled={Boolean(sessionActionSessionId)}
						>
							{t("workbench.closeSessionConfirm")}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
			<SettingsDialog
				open={isSettingsOpen}
				onOpenChange={setIsSettingsOpen}
				onOpenShortcuts={() => setIsShortcutSheetOpen(true)}
				theme={theme}
				onThemeChange={setTheme}
				density={density}
				onDensityChange={setDensity}
				providerCatalog={providerCatalog}
				selectedProviderId={selectedProviderId}
				selectedModelId={selectedModelId}
				onSelectProvider={handleSelectProvider}
				onSelectModel={handleSelectModel}
				providerRuntimeSettings={providerRuntimeSettings}
				onChangeProviderRuntime={handleChangeProviderRuntime}
				onClearProviderRuntime={handleClearProviderRuntime}
				appVersion={appCurrentVersion}
				appUpdate={appUpdateInfo}
				isCheckingUpdate={isCheckingUpdate}
				isInstallingUpdate={isInstallingUpdate}
				updateCheckError={appUpdateCheckError}
				onCheckForUpdate={() => {
					void checkForUpdate();
				}}
				onInstallUpdate={() => {
					void installUpdate();
				}}
				workspaceRoot={selectedLocalWorkspacePath}
				workspaceName={selectedWorkspace?.name ?? null}
				projectId={
					selectedWorkspace?.projectId ?? selectedWorkspace?.id ?? null
				}
				sessionId={effectiveSelectedSessionId}
				sessionProviderId={
					selectedSessionSummary?.session.providerId ?? null
				}
				sessionCreatedAt={
					selectedSessionSummary?.session.createdAt ?? null
				}
			/>
			<SkillsDialog
				open={isSkillsOpen}
				onOpenChange={setIsSkillsOpen}
				onSkillsChanged={() => {
					void queryClient.invalidateQueries({
						queryKey: ["skills", "context-count"],
					});
				}}
				projectRoot={selectedWorkspace?.rootPath ?? null}
				targetRoot={selectedWorkspacePath}
			/>
			<ShortcutCheatsheetDialog
				open={isShortcutSheetOpen}
				onOpenChange={setIsShortcutSheetOpen}
			/>
			<OnboardingWizard
				open={isOnboardingOpen}
				onOpenChange={setIsOnboardingOpen}
				onComplete={handleCompleteOnboarding}
			/>
			<Toaster theme={theme} position="bottom-right" visibleToasts={6} />
		</>
	);
}
