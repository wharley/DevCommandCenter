import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	ArrowUp,
	ArrowDown,
	AlertTriangle,
	ChevronDown,
	ClipboardList,
	CornerUpRight,
	GitFork,
	ListPlus,
	Paperclip,
	Pencil,
	Play,
	Square,
	X,
} from "lucide-react";
import type { LexicalEditor } from "lexical";
import { LexicalComposer } from "@lexical/react/LexicalComposer";
import { ContentEditable } from "@lexical/react/LexicalContentEditable";
import { HistoryPlugin } from "@lexical/react/LexicalHistoryPlugin";
import { LexicalErrorBoundary } from "@lexical/react/LexicalErrorBoundary";
import { PlainTextPlugin } from "@lexical/react/LexicalPlainTextPlugin";
import { Button } from "@/components/ui/button";
import {
	DebugEvidenceTray,
	type DebugEvidenceController,
} from "./DebugEvidenceTray";
import {
	buildDebugEvidencePrompt,
	summarizeDebugEvidence,
} from "@/features/sessions/debug-evidence";
import { SessionObjectiveControl } from "@/features/sessions/SessionObjectiveControl";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuGroup,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { isImageFilePath } from "@/lib/is-image-path";
import { pathRelativeToWorkspace } from "@/lib/path-basename";
import type {
	ProviderApprovalPolicy,
	ProviderCatalog,
	ProviderRuntimeConfig,
	QueuedTurn,
	WorkspaceSetupReport,
} from "@dcc/contracts";
import type { RuntimeSessionSnapshot } from "@/features/sessions/workbench-types";
import { canAbortRun } from "@/features/sessions/session-chrome-state";
import { ComposerPlanFollowUpBanner } from "./ComposerPlanFollowUpBanner";
import { ExecutionContextRail } from "./ExecutionContextRail";
import { getProviderUnhealthyReason } from "@/features/providers/provider-selection.logic";
import { ComposerApprovalPolicyMenu } from "./ComposerApprovalPolicyMenu";
import { ComposerExecutionMenu } from "./ComposerExecutionMenu";
import { ComposerButton } from "./ComposerButton";
import {
	clampEffort,
	DEFAULT_EFFORT_LEVELS,
	resolveEffectiveEffort,
} from "./effort";
import {
	buildSpecDraftPrompt,
	getComposerApprovalPolicyKey,
	getComposerConversationDraftKey,
	getComposerDraftKey,
	getComposerEffortKey,
	isComposerSubmitEnabled,
	isSendDisabled,
	resolvePlanModeState,
	setPlanModeState,
	submitComposerDraftOptimistically,
} from "./WorkspaceComposer.logic";
import { DEFAULT_SLASH_COMMANDS } from "./default-slash-commands";
import { $createFileBadgeNode, FileBadgeNode } from "./editor/file-badge-node";
import { $createImageBadgeNode, ImageBadgeNode } from "./editor/image-badge-node";
import { PastedSnippetBadgeNode } from "./editor/pasted-snippet-badge-node";
import { $appendNodesToComposerEnd } from "./editor/append-to-end";
import { AutoResizePlugin } from "./editor/plugins/AutoResizePlugin";
import { EditorRefPlugin } from "./editor/plugins/EditorRefPlugin";
import { DraftPersistencePlugin } from "./editor/plugins/DraftPersistencePlugin";
import { CompositionGuardPlugin } from "./editor/plugins/CompositionGuardPlugin";
import { DropFilePlugin } from "./editor/plugins/drop-file-plugin";
import { EditablePlugin } from "./editor/plugins/EditablePlugin";
import { FileMentionPlugin } from "./editor/plugins/file-mention-plugin";
import { HasContentPlugin } from "./editor/plugins/HasContentPlugin";
import { PasteImagePlugin } from "./editor/plugins/PasteImagePlugin";
import { SlashCommandPlugin } from "./editor/plugins/slash-command-plugin";
import { SubmitPlugin } from "./editor/plugins/SubmitPlugin";
import type {
	ComposerDelegationRequest,
	ComposerSubmittedTurn,
} from "./composer-turn";
import { delegationTargetsFor } from "@/features/sessions/delegation-targets";
import { DelegationTargetItems } from "@/features/sessions/DelegationTargetItems";
import {
	clearDraft,
	loadApprovalPolicy,
	loadDirectResponse,
	loadEffortSelection,
	saveApprovalPolicy,
	saveDirectResponse,
	saveEffortSelection,
} from "./draftStorage";
import { appendComposerText, readComposerPrompt, setEditorText } from "./editorOps";
import { subscribeWorkbenchCommand } from "@/features/workspaces/workbench-command";
import { recordUxMetric } from "@/lib/ux-metrics";
import {
	mostConstrainedUsageWindow,
	providerUsageSeverity,
	supportsProviderAccountUsage,
	useProviderAccountUsage,
} from "@/features/providers/provider-account-usage";
import {
	dispatchNextQueuedTurn,
	loadTurnQueue,
	removeQueuedTurn,
	reorderTurnQueue,
} from "@/lib/session-api";

type WorkspaceComposerProps = {
	draftKey: string;
	draftSessionId: string | null;
	disabled: boolean;
	providerChoices: ProviderCatalog["providers"];
	selectedProviderId: string | null;
	selectedModelId: string | null;
	selectedProviderRuntime: ProviderRuntimeConfig | null;
	sessionSnapshot: RuntimeSessionSnapshot | null;
	turnQueueEventKey: string | null;
	pendingPrompt: string | null;
	/** External draft injection; annotations append and recovery actions replace. */
	prefill?: {
		text: string;
		nonce: number;
		mode?: "append" | "replace";
	} | null;
	focusRequestKey?: number | null;
	/** Evidence-first debugging tray; the person reviews what travels with the next message. */
	debugEvidence?: DebugEvidenceController | null;
	workspacePath: string | null;
	workspaceSetupReport?: WorkspaceSetupReport | null;
	workspaceBranch: string | null;
	projectLabel: string | null;
	projectIcon?: string | null;
	projectColor?: string | null;
	currentBranch: string | null;
	isIsolatedWorkspace: boolean;
	contextProjects?: Array<{
		id: string;
		name: string;
		branch: string;
		icon?: string | null;
		color?: string | null;
	}>;
	showPlanFollowUpPrompt: boolean;
	planTitle: string | null;
	planNeedsInput: boolean;
	planApproved: boolean;
	onSelectProvider: (providerId: string) => void;
	onSelectModel: (modelId: string) => void;
	onSubmitPrompt: (turn: ComposerSubmittedTurn) => Promise<boolean>;
	onSteerPrompt?: (turn: ComposerSubmittedTurn) => Promise<void>;
	onQueuePrompt?: (turn: ComposerSubmittedTurn) => Promise<void>;
	/** Absent when the surface cannot delegate (no parent session yet). */
	onDelegatePrompt?: (request: ComposerDelegationRequest) => Promise<void>;
	/** Increment to open the delegate menu from outside (for example, the command palette). */
	openDelegateMenuSignal?: number;
	onAbortSession: () => void;
	onReviewPlan: () => void;
	onOpenTerminal?: () => void;
};

export function WorkspaceComposer({
	draftKey,
	draftSessionId,
	disabled,
	providerChoices,
	selectedProviderId,
	selectedModelId,
	selectedProviderRuntime,
	sessionSnapshot,
	turnQueueEventKey,
	pendingPrompt,
	prefill,
	focusRequestKey = null,
	debugEvidence = null,
	workspacePath,
	workspaceSetupReport = null,
	workspaceBranch,
	projectLabel,
	projectIcon = null,
	projectColor = null,
	currentBranch,
	isIsolatedWorkspace,
	contextProjects = [],
	showPlanFollowUpPrompt,
	planTitle,
	planNeedsInput,
	planApproved,
	onSelectProvider,
	onSelectModel,
	onSubmitPrompt,
	onSteerPrompt,
	onQueuePrompt,
	onDelegatePrompt,
	openDelegateMenuSignal,
	onAbortSession,
	onReviewPlan,
	onOpenTerminal,
}: WorkspaceComposerProps) {
	const { t } = useTranslation("common");
	const sessionId = sessionSnapshot?.sessionId ?? null;
	const [hasContent, setHasContent] = useState(false);
	const [isSubmitting, setIsSubmitting] = useState(false);
	const isSubmittingRef = useRef(false);
	const [queuedTurns, setQueuedTurns] = useState<QueuedTurn[]>([]);
	const [queueActionId, setQueueActionId] = useState<string | null>(null);
	const [isFastMode, setIsFastMode] = useState(loadDirectResponse);
	const [executionMenuOpen, setExecutionMenuOpen] = useState(false);
	const [sendMenuOpen, setSendMenuOpen] = useState(false);
	const [delegateAllowFileEdits, setDelegateAllowFileEdits] = useState(false);
	const [fanOutSelection, setFanOutSelection] = useState<string[] | null>(null);
	const lastDelegateMenuSignalRef = useRef(openDelegateMenuSignal ?? 0);
	// Effort + ultrathink are persisted per workspace so the selection survives
	// composer remounts (e.g. opening a git file then pressing Esc) instead of
	// resetting to the default — mirrors the draft persistence below.
	const [effortSelection, setEffortSelection] = useState(() =>
		loadEffortSelection(getComposerEffortKey(draftKey)),
	);
	const effort = effortSelection.effort;
	const ultrathinkSelected = effortSelection.ultrathink;
	const [planModeByScope, setPlanModeByScope] = useState<Record<string, boolean>>({});
	const composerDraftKey = useMemo(
		() => getComposerConversationDraftKey(draftKey, draftSessionId),
		[draftKey, draftSessionId],
	);
	const draftFallbackKeys = useMemo(
		() => [
			getComposerConversationDraftKey(draftKey, null),
			getComposerDraftKey(draftKey),
		],
		[draftKey],
	);
	const composerDraftKeyRef = useRef(composerDraftKey);
	composerDraftKeyRef.current = composerDraftKey;
	const composerEffortKey = useMemo(
		() => getComposerEffortKey(draftKey),
		[draftKey],
	);

	// Re-load the persisted selection when switching workspaces (the composer may
	// stay mounted while draftKey changes). Restoring here does not write back, so
	// it never clobbers another workspace's saved selection.
	useEffect(() => {
		setEffortSelection(loadEffortSelection(composerEffortKey));
	}, [composerEffortKey]);

	const updateEffortSelection = useCallback(
		(next: { effort: string; ultrathink: boolean }) => {
			setEffortSelection(next);
			saveEffortSelection(composerEffortKey, next);
		},
		[composerEffortKey],
	);
	const updateDirectResponse = useCallback((direct: boolean) => {
		setIsFastMode(direct);
		saveDirectResponse(direct);
	}, []);
	const composerRootRef = useRef<HTMLDivElement | null>(null);
	const editorRef = useRef<LexicalEditor | null>(null);
	const lastPrefillNonceRef = useRef<number | null>(null);
	const selectedProvider = useMemo(
		() =>
			providerChoices.find((provider) => provider.id === selectedProviderId) ??
			null,
		[providerChoices, selectedProviderId],
	);
	const selectedProviderBlockReason = useMemo(
		() => getProviderUnhealthyReason(selectedProvider),
		[selectedProvider],
	);
	const supportedApprovalPolicies = useMemo(
		() => selectedProvider?.capabilities.approvalPolicies ?? [],
		[selectedProvider],
	);
	const approvalPolicyKey = useMemo(
		() => getComposerApprovalPolicyKey(draftKey, selectedProviderId),
		[draftKey, selectedProviderId],
	);
	const [approvalPolicyByScope, setApprovalPolicyByScope] = useState<
		Record<string, ProviderApprovalPolicy>
	>({});
	const scopedApprovalPolicy = approvalPolicyByScope[approvalPolicyKey];
	const approvalPolicy =
		scopedApprovalPolicy && supportedApprovalPolicies.includes(scopedApprovalPolicy)
			? scopedApprovalPolicy
			: loadApprovalPolicy(approvalPolicyKey, supportedApprovalPolicies);
	const selectApprovalPolicy = useCallback(
		(policy: ProviderApprovalPolicy) => {
			if (!supportedApprovalPolicies.includes(policy)) return;
			setApprovalPolicyByScope((current) => ({
				...current,
				[approvalPolicyKey]: policy,
			}));
			saveApprovalPolicy(approvalPolicyKey, policy);
		},
		[approvalPolicyKey, supportedApprovalPolicies],
	);

	// Resolve the model within the selected provider — model IDs like "auto"
	// are not unique across providers (droid and cursor both expose "auto").
	const selectedModel = useMemo(() => {
		if (!selectedModelId || !selectedProvider) return null;
		return (
			selectedProvider.models.find((model) => model.id === selectedModelId) ??
			null
		);
	}, [selectedProvider, selectedModelId]);
	const activeTurnId = sessionSnapshot?.activeTurnId ?? null;
	const hasActiveTurn = Boolean(sessionSnapshot?.activeTurnId);
	const canSteerActiveTurn = Boolean(
		hasActiveTurn &&
			onSteerPrompt &&
			selectedProvider?.capabilities.supportsSteering,
	);
	const canQueueActiveTurn = Boolean(hasActiveTurn && onQueuePrompt);

	const refreshTurnQueue = useCallback(async () => {
		if (!sessionId) {
			setQueuedTurns([]);
			return;
		}
		try {
			setQueuedTurns(await loadTurnQueue(sessionId));
		} catch (error) {
			console.error("[dcc] failed to load turn queue:", error);
		}
	}, [sessionId]);

	useEffect(() => {
		void refreshTurnQueue();
	}, [activeTurnId, refreshTurnQueue, turnQueueEventKey]);

	const availableEffortLevels = useMemo(
		() => selectedModel?.effortLevels ?? DEFAULT_EFFORT_LEVELS,
		[selectedModel],
	);
	const isPlanMode = useMemo(
		() =>
			resolvePlanModeState(planModeByScope, {
				workspaceId: draftKey,
				sessionId,
			}),
		[planModeByScope, draftKey, sessionId],
	);
	const togglePlanMode = useCallback(() => {
		recordUxMetric("advanced_composer_control_used");
		setPlanModeByScope((current) =>
			setPlanModeState(current, {
				workspaceId: draftKey,
				sessionId,
				enabled: !isPlanMode,
			}),
		);
	}, [draftKey, isPlanMode, sessionId]);

	// Clamp effort when the model changes and no longer supports the current level.
	useEffect(() => {
		const clamped = clampEffort(effort, availableEffortLevels);
		if (clamped !== effort) {
			updateEffortSelection({ effort: clamped, ultrathink: ultrathinkSelected });
		}
	}, [availableEffortLevels, effort, ultrathinkSelected, updateEffortSelection]);

	const submitFromComposer = useCallback(
		async (
			rawPrompt: string,
			behavior: "send" | "queue" | "steer" = "send",
		) => {
			const effectiveEffort = resolveEffectiveEffort({
				selectedEffort: effort,
				supportedEfforts: availableEffortLevels,
				ultrathinkSelected,
				rawPrompt,
			});
			// Evidence is composed at the send boundary so the person can still
			// remove items until the very last moment; it is settled only after
			// the turn is actually accepted.
			const evidenceItems = debugEvidence?.items ?? [];
			const evidenceStage = debugEvidence?.stage ?? "observe";
			const prompt =
				evidenceItems.length > 0
					? buildDebugEvidencePrompt({
							message: rawPrompt,
							stage: evidenceStage,
							items: evidenceItems,
							labels: {
								stageGuidance: t(`composer.evidence.stageGuidance.${evidenceStage}`),
								trustNotice: t("composer.evidence.trustNotice"),
								defaultMessage: t("composer.evidence.defaultMessage"),
							},
						})
					: rawPrompt;
			const settleEvidence = () => {
				if (evidenceItems.length > 0) {
					debugEvidence?.onConsumed(evidenceItems.map((item) => item.id));
				}
			};
			const turn = {
				rawPrompt: prompt,
				envelope: {
					planMode: isPlanMode,
					effort: effectiveEffort,
					fastMode: isFastMode,
					approvalPolicy,
					evidence: summarizeDebugEvidence(evidenceItems, evidenceStage),
				},
			};
			if (hasActiveTurn) {
				if (behavior === "steer") {
					if (!canSteerActiveTurn || !onSteerPrompt) return false;
					await onSteerPrompt(turn);
				} else {
					if (!canQueueActiveTurn || !onQueuePrompt) return false;
					await onQueuePrompt(turn);
					await refreshTurnQueue();
				}
				settleEvidence();
				return true;
			}
			const accepted = await onSubmitPrompt(turn);
			if (accepted) settleEvidence();
			return accepted;
		},
		[
			availableEffortLevels,
			canSteerActiveTurn,
			canQueueActiveTurn,
			debugEvidence,
			effort,
			hasActiveTurn,
			approvalPolicy,
			isFastMode,
			isPlanMode,
			onSteerPrompt,
			onQueuePrompt,
			onSubmitPrompt,
			refreshTurnQueue,
			t,
			ultrathinkSelected,
		],
	);

	const handleSubmitDraft = useCallback(
		async (behavior: "send" | "queue" | "steer" = "send") => {
			if (isSubmittingRef.current) {
				return;
			}

			const editor = editorRef.current;
			if (!editor) {
				return;
			}

			const prompt = readComposerPrompt(editor).trim();
			// Evidence alone is a valid message: the stage guidance carries the ask.
			if (prompt.length === 0 && (debugEvidence?.items.length ?? 0) === 0) {
				return;
			}

			const submittedDraftKey = composerDraftKey;
			const submittedEditorState = editor.getEditorState();
			const restoreSubmittedDraft = () => {
				// A failed request must not overwrite a new draft or leak the old draft
				// into a workspace selected while the request was in flight.
				if (
					composerDraftKeyRef.current !== submittedDraftKey ||
					editorRef.current !== editor ||
					readComposerPrompt(editor).length > 0
				) {
					return;
				}
				editor.setEditorState(submittedEditorState);
			};

			// The timeline accepts the prompt optimistically, so clear the matching
			// composer state at the same boundary instead of waiting for backend and
			// metadata work to finish.
			isSubmittingRef.current = true;
			setIsSubmitting(true);
			try {
				await submitComposerDraftOptimistically({
					clearSubmittedDraft: () => {
						clearDraft(submittedDraftKey);
						setEditorText(editor, "");
					},
					submit: () => submitFromComposer(prompt, behavior),
					restoreSubmittedDraft,
				});
			} finally {
				isSubmittingRef.current = false;
				setIsSubmitting(false);
			}
		},
		[composerDraftKey, debugEvidence, submitFromComposer],
	);

	const handleRemoveQueuedTurn = useCallback(
		async (queuedTurnId: string) => {
			if (!sessionId || queueActionId) return false;
			setQueueActionId(queuedTurnId);
			try {
				setQueuedTurns(await removeQueuedTurn({ sessionId, queuedTurnId }));
				return true;
			} catch (error) {
				toast.error(t("composer.followUp.queueActionFailed"), {
					description: error instanceof Error ? error.message : undefined,
				});
				return false;
			} finally {
				setQueueActionId(null);
			}
		},
		[queueActionId, sessionId, t],
	);

	const handleEditQueuedTurn = useCallback(
		async (queuedTurn: QueuedTurn) => {
			if (!(await handleRemoveQueuedTurn(queuedTurn.id))) return;
			const editor = editorRef.current;
			if (editor) {
				setEditorText(editor, queuedTurn.prompt);
				editor.focus();
			}
		},
		[handleRemoveQueuedTurn],
	);

	const handleDispatchNextQueuedTurn = useCallback(async () => {
		if (!sessionId || queueActionId || hasActiveTurn) return;
		setQueueActionId(queuedTurns[0]?.id ?? "dispatch");
		try {
			await dispatchNextQueuedTurn(sessionId);
			await refreshTurnQueue();
		} catch (error) {
			toast.error(t("composer.followUp.dispatchFailed"), {
				description: error instanceof Error ? error.message : undefined,
			});
		} finally {
			setQueueActionId(null);
		}
	}, [
		hasActiveTurn,
		queueActionId,
		queuedTurns,
		refreshTurnQueue,
		sessionId,
		t,
	]);

	const handleMoveQueuedTurn = useCallback(
		async (index: number, direction: -1 | 1) => {
			if (!sessionId || queueActionId) return;
			const target = index + direction;
			if (target < 0 || target >= queuedTurns.length) return;
			const reordered = [...queuedTurns];
			[reordered[index], reordered[target]] = [reordered[target], reordered[index]];
			setQueueActionId(queuedTurns[index].id);
			try {
				setQueuedTurns(await reorderTurnQueue({
					sessionId,
					queuedTurnIds: reordered.map((turn) => turn.id),
				}));
			} catch (error) {
				toast.error(t("composer.followUp.queueActionFailed"), {
					description: error instanceof Error ? error.message : undefined,
				});
			} finally {
				setQueueActionId(null);
			}
		},
		[queueActionId, queuedTurns, sessionId, t],
	);

	// Delegation targets narrow when the run is allowed to write files, since edit
	// delegations land in an isolated worktree and need provider edit support. The
	// menu gate stays on the unfiltered list so toggling write access can never
	// hide the control that toggles it back.
	const canDelegate =
		Boolean(onDelegatePrompt) &&
		delegationTargetsFor(providerChoices, { allowFileEdits: false }).length > 0;
	const delegateTargets = useMemo(
		() =>
			delegationTargetsFor(providerChoices, {
				allowFileEdits: delegateAllowFileEdits,
			}),
		[delegateAllowFileEdits, providerChoices],
	);
	const fanOutTargetIds = useMemo(() => {
		if (fanOutSelection === null) {
			return null;
		}
		const availableIds = new Set(delegateTargets.map((target) => target.id));
		return fanOutSelection.filter((id) => availableIds.has(id));
	}, [delegateTargets, fanOutSelection]);

	const submitDelegation = useCallback(
		async (targetProviderIds: string[], targetModelId: string | null = null) => {
			if (
				!onDelegatePrompt ||
				isSubmittingRef.current ||
				targetProviderIds.length === 0
			) {
				return;
			}
			const editor = editorRef.current;
			if (!editor) {
				return;
			}
			const rawPrompt = readComposerPrompt(editor).trim();
			// Evidence travels with a delegation the same way it travels with a
			// turn: reviewed here, composed at the boundary, settled only once the
			// delegation actually started.
			const evidenceItems = debugEvidence?.items ?? [];
			const evidenceStage = debugEvidence?.stage ?? "observe";
			if (rawPrompt.length === 0 && evidenceItems.length === 0) {
				return;
			}
			const delegatedPrompt =
				evidenceItems.length > 0
					? buildDebugEvidencePrompt({
							message: rawPrompt,
							stage: evidenceStage,
							items: evidenceItems,
							labels: {
								stageGuidance: t(`composer.evidence.stageGuidance.${evidenceStage}`),
								trustNotice: t("composer.evidence.trustNotice"),
								defaultMessage: t("composer.evidence.defaultMessage"),
							},
						})
					: rawPrompt;
			isSubmittingRef.current = true;
			setSendMenuOpen(false);
			setIsSubmitting(true);
			try {
				await onDelegatePrompt({
					rawPrompt: delegatedPrompt,
					targetProviderIds,
					targetModelId: targetProviderIds.length === 1 ? targetModelId : null,
					// Edit delegations stay single-target, matching the backend guard.
					allowFileEdits: delegateAllowFileEdits && targetProviderIds.length === 1,
					effort: resolveEffectiveEffort({
						selectedEffort: effort,
						supportedEfforts: availableEffortLevels,
						ultrathinkSelected,
						rawPrompt,
					}),
					fastMode: isFastMode,
				});
				// Only reached when the delegation actually started.
				clearDraft(composerDraftKey);
				setEditorText(editor, "");
				setFanOutSelection(null);
				if (evidenceItems.length > 0) {
					debugEvidence?.onConsumed(evidenceItems.map((item) => item.id));
				}
			} catch {
				// Delegation failures are already surfaced as a toast upstream. Swallow
				// the rejection so it does not go unhandled, and leave the draft in
				// place — the dirty-worktree preflight rejects a perfectly good
				// instruction that the user should be able to retry after committing.
			} finally {
				isSubmittingRef.current = false;
				setIsSubmitting(false);
			}
		},
		[
			availableEffortLevels,
			composerDraftKey,
			debugEvidence,
			delegateAllowFileEdits,
			effort,
			isFastMode,
			onDelegatePrompt,
			t,
			ultrathinkSelected,
		],
	);

	// The header button and the command palette no longer open a dialog; they point
	// at the same menu that lives next to Send, so there is a single delegate path.
	useEffect(() => {
		const signal = openDelegateMenuSignal ?? 0;
		if (signal <= lastDelegateMenuSignalRef.current) {
			return;
		}
		lastDelegateMenuSignalRef.current = signal;
		if (canDelegate) {
			setSendMenuOpen(true);
			editorRef.current?.focus();
		}
	}, [canDelegate, openDelegateMenuSignal]);

	const toggleFanOutTarget = useCallback((targetId: string) => {
		setFanOutSelection((current) => {
			const selection = current ?? [];
			return selection.includes(targetId)
				? selection.filter((id) => id !== targetId)
				: [...selection, targetId];
		});
	}, []);

	// Fan-out is read-only by construction, so turning on write access collapses
	// any multi-target selection instead of silently dropping targets at submit.
	useEffect(() => {
		if (delegateAllowFileEdits) {
			setFanOutSelection(null);
		}
	}, [delegateAllowFileEdits]);

	// Inject externally-supplied context. Diff annotations append; message
	// recovery explicitly replaces the draft so stale text cannot be mixed into
	// a retry without the user noticing.
	// Keyed on nonce so the same selection can be re-sent, and so it fires once.
	useEffect(() => {
		if (!prefill || prefill.text.length === 0) {
			return;
		}
		if (lastPrefillNonceRef.current === prefill.nonce) {
			return;
		}
		const editor = editorRef.current;
		if (!editor) {
			return;
		}
		lastPrefillNonceRef.current = prefill.nonce;
		if (prefill.mode === "replace") {
			setEditorText(editor, prefill.text);
		} else {
			appendComposerText(editor, prefill.text);
		}
		editor.focus();
	}, [prefill]);

	const lexicalInitialConfig = useMemo(
		() => ({
			namespace: "WorkspaceComposer",
			editable: true,
			onError(error: Error) {
				throw error;
			},
			nodes: [FileBadgeNode, ImageBadgeNode, PastedSnippetBadgeNode],
			theme: {
				paragraph: "min-h-[1.25rem]",
			},
		}),
		[],
	);

	const insertSpecDraftPrompt = useCallback(
		(editor: LexicalEditor) => {
			setEditorText(editor, buildSpecDraftPrompt({ workspaceBranch }));
		},
		[workspaceBranch],
	);
	const clearComposerDraft = useCallback(
		(editor: LexicalEditor) => {
			clearDraft(composerDraftKey);
			setEditorText(editor, "");
		},
		[composerDraftKey],
	);
	const openAttachmentPicker = useCallback(async () => {
		const editor = editorRef.current;
		if (!editor) return;
		try {
			const { open: openDialog } = await import("@tauri-apps/plugin-dialog");
			const selected = await openDialog({
				directory: false,
				multiple: true,
				title: t("composer.attachments.pickerTitle"),
			});
			const paths = Array.isArray(selected)
				? selected
				: selected
					? [selected]
					: [];
			if (paths.length === 0) return;

			editor.update(() => {
				const nodes = paths.map((absolutePath) => {
					const storedPath = pathRelativeToWorkspace(workspacePath, absolutePath);
					return isImageFilePath(absolutePath)
						? $createImageBadgeNode(storedPath)
						: $createFileBadgeNode(storedPath);
				});
				$appendNodesToComposerEnd(...nodes);
			});
		} catch (error) {
			toast.error(t("composer.attachments.error"), {
				description: error instanceof Error ? error.message : undefined,
			});
		} finally {
			editor.focus();
		}
	}, [t, workspacePath]);
	useEffect(
		() =>
			subscribeWorkbenchCommand((command) => {
				switch (command) {
					case "composer.focus":
						editorRef.current?.focus();
						return;
					case "composer.execution":
						recordUxMetric("advanced_composer_control_used");
						setExecutionMenuOpen(true);
						return;
					case "composer.togglePlan":
						togglePlanMode();
						return;
				}
			}),
		[togglePlanMode],
	);

	const inputDisabled = disabled;
	const toolbarDisabled = disabled;
	const turnSettingsDisabled = disabled || hasActiveTurn;
	const hasProvider = Boolean(selectedProviderId);
	const turnCount = sessionSnapshot?.turnCount ?? 0;
	const accountUsageQuery = useProviderAccountUsage(
		selectedProvider,
		selectedProviderRuntime,
	);
	const accountUsageSupported = supportsProviderAccountUsage(selectedProvider);
	useEffect(() => {
		if (turnCount > 0 && activeTurnId === null && accountUsageSupported) {
			void accountUsageQuery.refetch();
		}
	}, [
		accountUsageQuery.refetch,
		accountUsageSupported,
		activeTurnId,
		selectedProviderId,
		turnCount,
	]);
	const accountUsageWindow = mostConstrainedUsageWindow(accountUsageQuery.data);
	const accountUsageAlert = providerUsageSeverity(accountUsageWindow);

	const canStopRun =
		canAbortRun(sessionSnapshot, pendingPrompt) || isSubmitting;

	// Evidence alone is sendable: the stage guidance carries the ask.
	const hasSendableContent = hasContent || (debugEvidence?.items.length ?? 0) > 0;
	const submitEnabled = isComposerSubmitEnabled({
		disabled: toolbarDisabled || (hasActiveTurn && !canQueueActiveTurn),
		hasProvider,
		hasContent: hasSendableContent,
	});
	const sendDisabled = isSendDisabled(submitEnabled, isSubmitting);
	const steerDisabled = !submitEnabled || isSubmitting;
	const submitDisabledForPlugin = sendDisabled;

	const planActive = isPlanMode;
	const placeholder = planActive
		? t("composer.placeholder.plan")
		: t("composer.placeholder.default");

	const selectedEffortId = ultrathinkSelected ? "ultrathink" : effort;
	const slashCommands = useMemo(
		() =>
			DEFAULT_SLASH_COMMANDS.map((command) => ({
				...command,
				description: t(`composer.slashCommands.${command.name}`, {
					defaultValue: command.description,
				}),
			})),
		[t],
	);
	return [
		<div
			key="composer-surface"
			ref={composerRootRef}
			aria-label={t("composer.ariaLabel")}
			data-focus-scope="composer"
			className={cn(
				"dcc-composer-surface relative flex flex-col rounded-2xl border border-border/40 bg-sidebar shadow-[var(--dcc-elevation-1)]",
				inputDisabled
					? "p-0"
					: "px-[var(--dcc-composer-padding-x)] py-[var(--dcc-composer-padding-y)]",
				inputDisabled && "cursor-not-allowed opacity-60",
			)}
		>
			{showPlanFollowUpPrompt ? (
				<ComposerPlanFollowUpBanner
					planTitle={planTitle}
					needsInput={planNeedsInput}
					approved={planApproved}
					onReviewPlan={onReviewPlan}
				/>
			) : null}
			{selectedProviderBlockReason ? (
				<div className="mt-2 rounded-2xl border border-destructive/20 bg-destructive/10 px-3 py-2 text-[12px] leading-5 text-destructive">
					{selectedProviderBlockReason}
				</div>
			) : null}
			<SessionObjectiveControl
				sessionId={sessionId}
				refreshKey={`${turnCount}:${sessionSnapshot?.state ?? "idle"}:${activeTurnId ?? ""}`}
				disabled={inputDisabled}
			/>
			{debugEvidence ? (
				<DebugEvidenceTray
					controller={debugEvidence}
					disabled={inputDisabled || isSubmitting}
				/>
			) : null}
			{queuedTurns.length > 0 ? (
				<div className="mb-2 rounded-xl border border-border/45 bg-background/30 px-2.5 py-2">
					<div className="mb-1.5 flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
						<ListPlus className="size-3" strokeWidth={2} />
						<span>{t("composer.followUp.queueTitle", { count: queuedTurns.length })}</span>
						{!hasActiveTurn ? (
							<Button
								type="button"
								variant="ghost"
								size="sm"
								className="ml-auto h-6 gap-1 px-2 text-[11px]"
								disabled={queueActionId !== null}
								onClick={() => void handleDispatchNextQueuedTurn()}
							>
								<Play className="size-3" fill="currentColor" />
								{t("composer.followUp.sendNext")}
							</Button>
						) : null}
					</div>
					<div className="max-h-36 space-y-1 overflow-y-auto">
						{queuedTurns.map((queuedTurn, index) => (
							<div
								key={queuedTurn.id}
								className="group flex min-w-0 items-center gap-1 rounded-lg bg-muted/35 px-2 py-1.5"
							>
								<span className="min-w-0 flex-1 truncate text-[12px] text-foreground/85">
									{queuedTurn.prompt}
								</span>
								<Button
									type="button"
									variant="ghost"
									size="icon"
									className="size-6"
									disabled={index === 0 || queueActionId !== null}
									aria-label={t("composer.followUp.moveUp")}
									onClick={() => void handleMoveQueuedTurn(index, -1)}
								>
									<ArrowUp className="size-3" />
								</Button>
								<Button
									type="button"
									variant="ghost"
									size="icon"
									className="size-6"
									disabled={
										index === queuedTurns.length - 1 || queueActionId !== null
									}
									aria-label={t("composer.followUp.moveDown")}
									onClick={() => void handleMoveQueuedTurn(index, 1)}
								>
									<ArrowDown className="size-3" />
								</Button>
								<Button
									type="button"
									variant="ghost"
									size="icon"
									className="size-6"
									disabled={queueActionId !== null}
									aria-label={t("composer.followUp.edit")}
									onClick={() => void handleEditQueuedTurn(queuedTurn)}
								>
									<Pencil className="size-3" />
								</Button>
								<Button
									type="button"
									variant="ghost"
									size="icon"
									className="size-6"
									disabled={queueActionId !== null}
									aria-label={t("composer.followUp.remove")}
									onClick={() => void handleRemoveQueuedTurn(queuedTurn.id)}
								>
									<X className="size-3" />
								</Button>
							</div>
						))}
					</div>
				</div>
			) : null}

			<LexicalComposer initialConfig={lexicalInitialConfig}>
				<div className="relative">
					<PlainTextPlugin
						contentEditable={
							<ContentEditable
								id="workspace-input"
								aria-label={t("composer.inputAriaLabel")}
								aria-multiline
								className={cn(
									"composer-editor min-h-[var(--dcc-composer-input-min-height)] max-h-[240px] resize-none overflow-x-hidden overflow-y-auto whitespace-pre-wrap break-words bg-transparent text-[14px] leading-5 tracking-[-0.01em] text-foreground outline-none",
								)}
							/>
						}
						placeholder={
							<div className="pointer-events-none absolute left-0 top-0 text-[14px] leading-5 tracking-[-0.01em] text-muted-foreground/70">
								{placeholder}
							</div>
						}
						ErrorBoundary={LexicalErrorBoundary}
					/>
				</div>
				<HistoryPlugin />
				<SlashCommandPlugin
					commands={slashCommands}
					popupAnchorRef={composerRootRef}
					clientActionHandlers={{
						clear: clearComposerDraft,
						spec: insertSpecDraftPrompt,
					}}
				/>
				<FileMentionPlugin
					workspaceRootPath={workspacePath}
					popupAnchorRef={composerRootRef}
				/>
				<DropFilePlugin workspaceRootPath={workspacePath} />
				<CompositionGuardPlugin />
				<PasteImagePlugin workspaceRootPath={workspacePath} />
				<SubmitPlugin
					isDisabled={submitDisabledForPlugin}
					onSubmit={handleSubmitDraft}
				/>
				<AutoResizePlugin />
				<EditorRefPlugin
					editorRef={editorRef}
					focusRequestKey={focusRequestKey}
				/>
				<EditablePlugin disabled={inputDisabled} />
				<DraftPersistencePlugin
					draftKey={composerDraftKey}
					fallbackDraftKeys={draftFallbackKeys}
				/>
				<HasContentPlugin onChange={setHasContent} />
			</LexicalComposer>

			<div className="mt-2.5 flex items-end justify-between gap-3">
				<div className="flex min-w-0 items-center gap-1.5 overflow-hidden">
					<Tooltip>
						<TooltipTrigger asChild>
							<ComposerButton
								type="button"
								aria-label={t("composer.attachments.add")}
								disabled={toolbarDisabled}
								className="w-7 shrink-0 px-0 text-muted-foreground"
								onClick={() => void openAttachmentPicker()}
							>
								<Paperclip className="size-[13px]" strokeWidth={1.8} />
							</ComposerButton>
						</TooltipTrigger>
						<TooltipContent side="top">
							{t("composer.attachments.add")}
						</TooltipContent>
					</Tooltip>
					<ComposerApprovalPolicyMenu
						providerName={selectedProvider?.label ?? null}
						supportedPolicies={supportedApprovalPolicies}
						selectedPolicy={approvalPolicy}
						disabled={turnSettingsDisabled}
						planMode={isPlanMode}
						onSelect={selectApprovalPolicy}
					/>
					<Tooltip>
						<TooltipTrigger asChild>
							<ComposerButton
								type="button"
								aria-label={t(
									isPlanMode
										? "composer.controls.planModeDisable"
										: "composer.controls.planModeEnable",
								)}
								aria-pressed={isPlanMode}
								disabled={turnSettingsDisabled}
								className={cn(
									"h-7 gap-1 px-1.5 text-[var(--dcc-daily-meta-size)]",
									isPlanMode
										? "text-[color:var(--plan)] hover:text-[color:var(--plan)]"
										: "text-muted-foreground/70 hover:text-muted-foreground/70",
								)}
								onClick={togglePlanMode}
							>
								<ClipboardList className="size-[13px]" strokeWidth={1.8} />
								<span className="dcc-composer-plan-label text-[12px] font-medium leading-4">
									{t("composer.controls.plan")}
								</span>
							</ComposerButton>
						</TooltipTrigger>
						<TooltipContent side="top">
							{t(
								isPlanMode
									? "composer.controls.planModeActiveHint"
									: "composer.controls.planModeHint",
							)}
						</TooltipContent>
					</Tooltip>
				</div>

				<div className="flex shrink-0 items-center gap-1.5">
					<ComposerExecutionMenu
						open={executionMenuOpen}
						onOpenChange={(open) => {
							setExecutionMenuOpen(open);
							if (open) recordUxMetric("advanced_composer_control_used");
						}}
						providers={providerChoices}
						selectedProviderId={selectedProviderId}
						selectedModelId={selectedModelId}
						availableEffortLevels={availableEffortLevels}
						selectedEffortId={selectedEffortId}
						directResponse={isFastMode}
						onSelectProvider={onSelectProvider}
						onSelectModel={onSelectModel}
						onSelectEffort={(id) =>
							updateEffortSelection({ effort: id, ultrathink: false })
						}
						onSelectUltrathink={() =>
							updateEffortSelection({ effort, ultrathink: true })
						}
						onSetDirectResponse={updateDirectResponse}
						accountUsage={accountUsageQuery.data}
						isAccountUsageFetching={accountUsageQuery.isFetching}
						hasAccountUsageError={accountUsageQuery.isError}
						onRefreshAccountUsage={() => {
							if (accountUsageSupported) {
								void accountUsageQuery.refetch();
							}
						}}
						disabled={turnSettingsDisabled}
					/>
					{accountUsageAlert && accountUsageWindow ? (
						<Tooltip>
							<TooltipTrigger asChild>
								<span
									className={cn(
										"flex items-center gap-1 text-[11px] font-medium tabular-nums",
										accountUsageAlert === "warning" &&
											"text-amber-600 dark:text-amber-400",
										accountUsageAlert === "critical" && "text-destructive",
									)}
								>
									<AlertTriangle className="size-3" strokeWidth={2} />
									{t("composer.accountUsage.remaining", {
										percent: Math.round(accountUsageWindow.remainingPercent),
									})}
								</span>
							</TooltipTrigger>
							<TooltipContent>
								{accountUsageWindow.isExhausted
									? t("composer.accountUsage.limitReached")
									: t("composer.accountUsage.lowLimit")}
							</TooltipContent>
						</Tooltip>
					) : null}
					{canStopRun ? (
						<div className="ml-1.5 flex items-center gap-1.5">
							<Button
								type="button"
								variant="destructive"
								size="icon"
								aria-label={t("composer.controls.stop")}
								disabled={toolbarDisabled || !canStopRun}
								className="rounded-[9px]"
								onClick={onAbortSession}
							>
								<Square className="size-3 fill-current" strokeWidth={0} />
							</Button>
							{hasSendableContent && canQueueActiveTurn ? (
								<Button
									type="button"
									variant="outline"
									size="icon"
									aria-label={t("composer.followUp.queue")}
									disabled={sendDisabled}
									className="rounded-[9px]"
									onClick={() => {
										void handleSubmitDraft("queue");
									}}
								>
									<ListPlus className="size-[15px]" strokeWidth={2.2} />
								</Button>
							) : null}
							{hasSendableContent && canSteerActiveTurn ? (
								<Tooltip>
									<TooltipTrigger asChild>
										<Button
											type="button"
											variant="ghost"
											size="icon"
											aria-label={t("composer.controls.steer")}
											disabled={steerDisabled}
											className="rounded-[9px]"
											onClick={() => void handleSubmitDraft("steer")}
										>
											<CornerUpRight
												className="size-[15px]"
												strokeWidth={2.1}
											/>
										</Button>
									</TooltipTrigger>
									<TooltipContent side="top">{t("composer.followUp.steerNow")}</TooltipContent>
								</Tooltip>
							) : null}
						</div>
					) : (
						<div className="ml-1.5 flex items-center gap-1">
							{canDelegate ? (
								<DropdownMenu
									open={sendMenuOpen}
									onOpenChange={(open) => {
										setSendMenuOpen(open);
										// Always reopen in single-target mode, so the one-click
										// path is what the user finds by default.
										if (!open) {
											setFanOutSelection(null);
										}
									}}
								>
									<DropdownMenuTrigger
										type="button"
										aria-label={t("composer.delegate.open")}
										disabled={sendDisabled}
										className={cn(
											"flex h-8 w-6 items-center justify-center rounded-[9px] text-muted-foreground transition-colors hover:bg-accent/60 hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring/50",
											sendDisabled &&
												"cursor-not-allowed opacity-45 hover:bg-transparent hover:text-muted-foreground",
										)}
									>
										<GitFork className="size-3.5" strokeWidth={2} />
									</DropdownMenuTrigger>
									<DropdownMenuContent
										side="top"
										align="end"
										sideOffset={4}
										className="w-72"
									>
										<DropdownMenuLabel>
											{t("composer.delegate.title")}
										</DropdownMenuLabel>
										<DropdownMenuGroup>
											{delegateTargets.length === 0 ? (
												<p className="px-1.5 py-1.5 text-[12px] text-muted-foreground">
													{t("composer.delegate.noEditTargets")}
												</p>
											) : null}
											{fanOutTargetIds === null ? (
												<DelegationTargetItems
													targets={delegateTargets}
													disabled={isSubmitting}
													onSelect={(selection) => {
														void submitDelegation(
															[selection.providerId],
															selection.modelId,
														);
													}}
												/>
											) : delegateTargets.map((target) => {
												const isPicked =
													fanOutTargetIds?.includes(target.id) ?? false;
												return (
													<DropdownMenuItem
														key={target.id}
														className="flex items-center justify-between gap-3"
														onSelect={(event) => {
															// Fan-out mode keeps the menu open so several
															// targets can be picked before submitting.
															if (fanOutTargetIds !== null) {
																event.preventDefault();
																toggleFanOutTarget(target.id);
																return;
															}
														}}
													>
														<span className="min-w-0 truncate">{target.label}</span>
														{fanOutTargetIds !== null ? (
															<span className="text-[12px] text-foreground">
																{isPicked ? "✓" : ""}
															</span>
														) : (
															<CornerUpRight
																className="size-3.5 shrink-0 opacity-40"
																strokeWidth={2}
															/>
														)}
													</DropdownMenuItem>
												);
											})}
										</DropdownMenuGroup>
										{fanOutTargetIds !== null ? (
											<DropdownMenuItem
												disabled={fanOutTargetIds.length === 0}
												onSelect={() => void submitDelegation(fanOutTargetIds)}
											>
												{t("composer.delegate.submitFanOut", {
													count: fanOutTargetIds.length,
												})}
											</DropdownMenuItem>
										) : delegateTargets.length > 1 ? (
											<DropdownMenuItem
												onSelect={(event) => {
													event.preventDefault();
													setFanOutSelection([]);
												}}
											>
												{t("composer.delegate.fanOut")}
											</DropdownMenuItem>
										) : null}
										<DropdownMenuSeparator />
										<DropdownMenuItem
											className="flex items-center justify-between gap-3"
											onSelect={(event) => {
												event.preventDefault();
												setDelegateAllowFileEdits((current) => !current);
											}}
										>
											<div>
												<div>{t("composer.delegate.allowEdits")}</div>
												<div className="text-[12px] text-muted-foreground">
													{t("composer.delegate.allowEditsHint")}
												</div>
											</div>
											<span className="text-[12px] text-foreground">
												{delegateAllowFileEdits ? "✓" : ""}
											</span>
										</DropdownMenuItem>
									</DropdownMenuContent>
								</DropdownMenu>
							) : null}
							<Button
								type="button"
								variant="default"
								size="icon"
								aria-label={t("composer.controls.send")}
								disabled={sendDisabled}
								onClick={() => {
									void handleSubmitDraft();
								}}
								className="rounded-[9px]"
							>
								<ArrowUp className="size-[15px]" strokeWidth={2.2} />
							</Button>
						</div>
					)}
				</div>
			</div>
		</div>,
		<ExecutionContextRail
			key="execution-context"
			projectLabel={projectLabel}
			projectIcon={projectIcon}
			projectColor={projectColor}
			baseBranch={workspaceBranch}
			currentBranch={currentBranch}
			isIsolatedWorkspace={isIsolatedWorkspace}
			contextProjects={contextProjects}
			setupReport={workspaceSetupReport}
			onOpenTerminal={onOpenTerminal}
		/>,
	];
}
