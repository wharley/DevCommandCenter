import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
	ArrowUp,
	AlertTriangle,
	ChevronDown,
	ChevronUp,
	ClipboardList,
	CornerUpRight,
	Plus,
	SlidersHorizontal,
	Square,
} from "lucide-react";
import type { LexicalEditor } from "lexical";
import { LexicalComposer } from "@lexical/react/LexicalComposer";
import { ContentEditable } from "@lexical/react/LexicalContentEditable";
import { HistoryPlugin } from "@lexical/react/LexicalHistoryPlugin";
import { LexicalErrorBoundary } from "@lexical/react/LexicalErrorBoundary";
import { PlainTextPlugin } from "@lexical/react/LexicalPlainTextPlugin";
import { Button } from "@/components/ui/button";
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
import { pathBasename } from "@/lib/path-basename";
import { cn } from "@/lib/utils";
import type { ProviderCatalog, ProviderRuntimeConfig } from "@dcc/contracts";
import type { RuntimeSessionSnapshot } from "@/features/sessions/workbench-types";
import { canAbortRun } from "@/features/sessions/session-chrome-state";
import { ComposerPlanFollowUpBanner } from "./ComposerPlanFollowUpBanner";
import { getProviderUnhealthyReason } from "@/features/providers/provider-selection.logic";
import { ContextBar } from "./ContextBar";
import { ComposerProviderModelMenu } from "./ComposerProviderModelMenu";
import { EffortBrainIcon } from "./EffortBrainIcon";
import { ComposerButton } from "./ComposerButton";
import {
	clampEffort,
	DEFAULT_EFFORT_LEVELS,
	getEffortDisplay,
	resolveEffectiveEffort,
} from "./effort";
import {
	buildComposerContextDirectories,
	buildSpecDraftPrompt,
	composerToolbarTriggerClassName,
	getComposerDraftKey,
	getComposerEffortKey,
	isComposerSubmitEnabled,
	isSendDisabled,
	isSteerDisabled,
	resolvePlanModeState,
	setPlanModeState,
} from "./WorkspaceComposer.logic";
import { DEFAULT_SLASH_COMMANDS } from "./default-slash-commands";
import {
	AddDirTypeaheadPlugin,
	type AddDirPickEntry,
} from "./editor/add-dir/add-dir-typeahead-plugin";
import { $insertAddDirTrigger } from "./editor/add-dir/insert";
import { AddDirTriggerNode } from "./editor/add-dir/trigger-node";
import { FileBadgeNode } from "./editor/file-badge-node";
import { ImageBadgeNode } from "./editor/image-badge-node";
import { PastedSnippetBadgeNode } from "./editor/pasted-snippet-badge-node";
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
import { workspaceChildDirsQueryOptions } from "./workspace-child-dirs-query";
import type {
	ComposerDelegationRequest,
	ComposerSubmittedTurn,
} from "./composer-turn";
import { delegationTargetsFor } from "@/features/sessions/delegation-targets";
import {
	clearDraft,
	loadEffortSelection,
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

type WorkspaceComposerProps = {
	draftKey: string;
	disabled: boolean;
	providerChoices: ProviderCatalog["providers"];
	selectedProviderId: string | null;
	selectedModelId: string | null;
	selectedProviderRuntime: ProviderRuntimeConfig | null;
	sessionSnapshot: RuntimeSessionSnapshot | null;
	pendingPrompt: string | null;
	/** Text to append to the draft (e.g. a diff annotation); nonce re-fires it. */
	prefill?: { text: string; nonce: number } | null;
	workspacePath: string | null;
	workspaceBranch: string | null;
	showPlanFollowUpPrompt: boolean;
	planTitle: string | null;
	planNeedsInput: boolean;
	planApproved: boolean;
	onSelectProvider: (providerId: string) => void;
	onSelectModel: (modelId: string) => void;
	onSubmitPrompt: (turn: ComposerSubmittedTurn) => Promise<void>;
	/** Absent when the surface cannot delegate (no parent session yet). */
	onDelegatePrompt?: (request: ComposerDelegationRequest) => Promise<void>;
	/** Increment to open the delegate menu from outside (header, command palette). */
	openDelegateMenuSignal?: number;
	onAbortSession: () => void;
	onReviewPlan: () => void;
};

export function WorkspaceComposer({
	draftKey,
	disabled,
	providerChoices,
	selectedProviderId,
	selectedModelId,
	selectedProviderRuntime,
	sessionSnapshot,
	pendingPrompt,
	prefill,
	workspacePath,
	workspaceBranch,
	showPlanFollowUpPrompt,
	planTitle,
	planNeedsInput,
	planApproved,
	onSelectProvider,
	onSelectModel,
	onSubmitPrompt,
	onDelegatePrompt,
	openDelegateMenuSignal,
	onAbortSession,
	onReviewPlan,
}: WorkspaceComposerProps) {
	const { t } = useTranslation("common");
	const [hasContent, setHasContent] = useState(false);
	const [isSubmitting, setIsSubmitting] = useState(false);
	const [isFastMode, setIsFastMode] = useState(true);
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
	const [contextDirectories, setContextDirectories] = useState(() =>
		buildComposerContextDirectories({ workspacePath, workspaceBranch }),
	);
	const composerDraftKey = useMemo(() => getComposerDraftKey(draftKey), [draftKey]);
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

	// Resolve the model within the selected provider — model IDs like "auto"
	// are not unique across providers (droid and cursor both expose "auto").
	const selectedModel = useMemo(() => {
		if (!selectedModelId || !selectedProvider) return null;
		return (
			selectedProvider.models.find((model) => model.id === selectedModelId) ??
			null
		);
	}, [selectedProvider, selectedModelId]);

	const availableEffortLevels = useMemo(
		() => selectedModel?.effortLevels ?? DEFAULT_EFFORT_LEVELS,
		[selectedModel],
	);
	const sessionId = sessionSnapshot?.sessionId ?? null;
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
		async (rawPrompt: string) => {
			const effectiveEffort = resolveEffectiveEffort({
				selectedEffort: effort,
				supportedEfforts: availableEffortLevels,
				ultrathinkSelected,
				rawPrompt,
			});
			await onSubmitPrompt({
				rawPrompt,
				envelope: {
					planMode: isPlanMode,
					effort: effectiveEffort,
					fastMode: isFastMode,
				},
			});
		},
		[availableEffortLevels, effort, isFastMode, isPlanMode, onSubmitPrompt, ultrathinkSelected],
	);

	const handleSubmitDraft = useCallback(async () => {
		if (isSubmitting) {
			return;
		}

		const editor = editorRef.current;
		if (!editor) {
			return;
		}

		const prompt = readComposerPrompt(editor).trim();
		if (prompt.length === 0) {
			return;
		}

		setIsSubmitting(true);
		try {
			await submitFromComposer(prompt);
			clearDraft(composerDraftKey);
			setEditorText(editor, "");
		} finally {
			setIsSubmitting(false);
		}
	}, [composerDraftKey, isSubmitting, submitFromComposer]);

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
		async (targetProviderIds: string[]) => {
			if (!onDelegatePrompt || isSubmitting || targetProviderIds.length === 0) {
				return;
			}
			const editor = editorRef.current;
			if (!editor) {
				return;
			}
			const rawPrompt = readComposerPrompt(editor).trim();
			if (rawPrompt.length === 0) {
				return;
			}
			setSendMenuOpen(false);
			setIsSubmitting(true);
			try {
				await onDelegatePrompt({
					rawPrompt,
					targetProviderIds,
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
			} catch {
				// Delegation failures are already surfaced as a toast upstream. Swallow
				// the rejection so it does not go unhandled, and leave the draft in
				// place — the dirty-worktree preflight rejects a perfectly good
				// instruction that the user should be able to retry after committing.
			} finally {
				setIsSubmitting(false);
			}
		},
		[
			availableEffortLevels,
			composerDraftKey,
			delegateAllowFileEdits,
			effort,
			isFastMode,
			isSubmitting,
			onDelegatePrompt,
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

	// Append externally-supplied context (e.g. a diff annotation) to the draft.
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
		appendComposerText(editor, prefill.text);
		editor.focus();
	}, [prefill]);

	const lexicalInitialConfig = useMemo(
		() => ({
			namespace: "WorkspaceComposer",
			editable: true,
			onError(error: Error) {
				throw error;
			},
			nodes: [
				AddDirTriggerNode,
				FileBadgeNode,
				ImageBadgeNode,
				PastedSnippetBadgeNode,
			],
			theme: {
				paragraph: "min-h-[1.25rem]",
			},
		}),
		[],
	);

	const childDirsQuery = useQuery(workspaceChildDirsQueryOptions(workspacePath));

	const appendContextDirectory = useCallback((dirPath: string) => {
		setContextDirectories((prev) => {
			if (prev.some((d) => d.path === dirPath)) {
				return prev;
			}
			return [
				...prev,
				{
					id: `ctx-${dirPath}`,
					label: pathBasename(dirPath) || dirPath,
					path: dirPath,
				},
			];
		});
	}, []);

	const handleAddDirPick = useCallback(
		(entry: AddDirPickEntry) => {
			void (async () => {
				if (entry.kind === "browse") {
					const { open: openDialog } = await import("@tauri-apps/plugin-dialog");
					const selected = await openDialog({
						directory: true,
						multiple: false,
						title: "Add directory to context",
					});
					const pickedPath = Array.isArray(selected)
						? selected[0] ?? ""
						: selected ?? "";
					if (pickedPath) {
						appendContextDirectory(pickedPath);
					}
					return;
				}
				appendContextDirectory(entry.candidate.absolutePath);
			})();
		},
		[appendContextDirectory],
	);

	const insertSpecDraftPrompt = useCallback(
		(editor: LexicalEditor) => {
			setEditorText(editor, buildSpecDraftPrompt({ workspaceBranch }));
		},
		[workspaceBranch],
	);
	const openContextPicker = useCallback(() => {
		const editor = editorRef.current;
		if (!editor) return;
		$insertAddDirTrigger(editor, null);
		editor.focus();
	}, []);

	useEffect(
		() =>
			subscribeWorkbenchCommand((command) => {
				switch (command) {
					case "composer.focus":
						editorRef.current?.focus();
						return;
					case "composer.addContext":
						openContextPicker();
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
		[openContextPicker, togglePlanMode],
	);

	const inputDisabled = disabled;
	const toolbarDisabled = disabled;
	const hasProvider = Boolean(selectedProviderId);
	const turnCount = sessionSnapshot?.turnCount ?? 0;
	const accountUsageQuery = useProviderAccountUsage(
		selectedProviderId,
		selectedProviderRuntime,
	);
	const activeTurnId = sessionSnapshot?.activeTurnId ?? null;
	useEffect(() => {
		if (
			turnCount > 0 &&
			activeTurnId === null &&
			supportsProviderAccountUsage(selectedProviderId)
		) {
			void accountUsageQuery.refetch();
		}
	}, [
		accountUsageQuery.refetch,
		activeTurnId,
		selectedProviderId,
		turnCount,
	]);
	const accountUsageWindow = mostConstrainedUsageWindow(accountUsageQuery.data);
	const accountUsageAlert = providerUsageSeverity(accountUsageWindow);

	const canStopRun =
		canAbortRun(sessionSnapshot, pendingPrompt) || isSubmitting;

	const submitEnabled = isComposerSubmitEnabled({
		disabled: toolbarDisabled,
		hasProvider,
		hasContent,
	});
	const sendDisabled = isSendDisabled(submitEnabled, isSubmitting);
	const steerDisabled = isSteerDisabled(submitEnabled, isSubmitting);
	const submitDisabledForPlugin = !submitEnabled;

	const planActive = isPlanMode;
	const placeholder = planActive
		? t("composer.placeholder.plan")
		: t("composer.placeholder.default");

	const selectedEffortId = ultrathinkSelected ? "ultrathink" : effort;
	const effortLabel = getEffortDisplay(selectedEffortId).label;
	const translatedEffortLabel = t(`composer.effort.${selectedEffortId}`, {
		defaultValue: effortLabel,
	});
	const extraContextDirectories = contextDirectories.filter(
		(directory) =>
			directory.id !== "workspace-path" && directory.id !== "workspace-branch",
	);

	useEffect(() => {
		setContextDirectories(
			buildComposerContextDirectories({ workspacePath, workspaceBranch }),
		);
	}, [workspaceBranch, workspacePath]);

	return (
		<div
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
			{extraContextDirectories.length > 0 ? (
				<ContextBar
					directories={extraContextDirectories}
					disabled={inputDisabled}
					onRemove={(directoryId) => {
						setContextDirectories((current) =>
							current.filter((directory) => directory.id !== directoryId),
						);
					}}
				/>
			) : null}
			{selectedProviderBlockReason ? (
				<div className="mt-2 rounded-2xl border border-destructive/20 bg-destructive/10 px-3 py-2 text-[12px] leading-5 text-destructive">
					{selectedProviderBlockReason}
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
					commands={DEFAULT_SLASH_COMMANDS}
					popupAnchorRef={composerRootRef}
					clientActionHandlers={{
						"add-dir": $insertAddDirTrigger,
						spec: insertSpecDraftPrompt,
					}}
				/>
				<AddDirTypeaheadPlugin
					candidates={childDirsQuery.data ?? []}
					linkedDirectoryPaths={contextDirectories.map((d) => d.path)}
					onPick={handleAddDirPick}
					popupAnchorRef={composerRootRef}
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
				<EditorRefPlugin editorRef={editorRef} />
				<EditablePlugin disabled={inputDisabled} />
				<DraftPersistencePlugin draftKey={composerDraftKey} />
				<HasContentPlugin onChange={setHasContent} />
			</LexicalComposer>

			<div className="mt-2.5 flex items-end justify-between gap-3">
				<div className="flex min-w-0 items-center gap-1.5 overflow-hidden">
					<ComposerProviderModelMenu
						providers={providerChoices}
						selectedProviderId={selectedProviderId}
						selectedModelId={selectedModelId}
						accountUsage={accountUsageQuery.data}
						isAccountUsageFetching={accountUsageQuery.isFetching}
						hasAccountUsageError={accountUsageQuery.isError}
						onRefreshAccountUsage={() => {
							if (supportsProviderAccountUsage(selectedProviderId)) {
								void accountUsageQuery.refetch();
							}
						}}
						onSelectProvider={onSelectProvider}
						onSelectModel={onSelectModel}
						disabled={toolbarDisabled}
					/>
					<ComposerButton
						type="button"
						aria-label={t("composer.controls.addContext")}
						disabled={toolbarDisabled}
						className="h-7 shrink-0 gap-1 px-1.5 text-[var(--dcc-daily-meta-size)] text-muted-foreground"
						onClick={openContextPicker}
					>
						<Plus className="size-[13px]" strokeWidth={1.8} />
						<span className="dcc-composer-context-label">
							{extraContextDirectories.length > 0
								? t("composer.controls.contextCount", {
										count: extraContextDirectories.length,
									})
								: t("composer.controls.context")}
						</span>
					</ComposerButton>
					<ComposerButton
						type="button"
						aria-label={t("composer.controls.planMode")}
						disabled={toolbarDisabled}
						className={cn(
							"h-7 gap-1 px-1.5 text-[var(--dcc-daily-meta-size)]",
							isPlanMode
								? "text-[color:var(--plan)] hover:text-[color:var(--plan)]"
								: "text-muted-foreground/70 hover:text-muted-foreground/70",
						)}
						onClick={togglePlanMode}
					>
						<ClipboardList className="size-[13px]" strokeWidth={1.8} />
						<span className="dcc-composer-plan-label">{t("composer.controls.plan")}</span>
					</ComposerButton>
				</div>

				<div className="flex shrink-0 items-center gap-1.5">
					<DropdownMenu
						open={executionMenuOpen}
						onOpenChange={(open) => {
							setExecutionMenuOpen(open);
							if (open) recordUxMetric("advanced_composer_control_used");
						}}
					>
						<DropdownMenuTrigger
							type="button"
							aria-label={t("composer.execution.open")}
							disabled={toolbarDisabled}
							className={cn(
								`flex h-7 items-center gap-1.5 ${composerToolbarTriggerClassName}`,
								"max-w-[9rem] text-muted-foreground",
								toolbarDisabled &&
									"cursor-not-allowed opacity-45 hover:bg-transparent hover:text-muted-foreground",
							)}
						>
							<SlidersHorizontal className="size-[13px] shrink-0" strokeWidth={1.8} />
							<span className="dcc-composer-execution-label truncate text-[var(--dcc-daily-meta-size)]">
								{isFastMode
									? t("composer.execution.fast")
									: t("composer.execution.standard")}
								{" · "}
								{translatedEffortLabel}
							</span>
							<ChevronDown className="size-3 shrink-0 opacity-40" strokeWidth={2} />
						</DropdownMenuTrigger>
						<DropdownMenuContent
							side="top"
							align="end"
							sideOffset={4}
							className="w-64"
						>
							<DropdownMenuLabel>{t("composer.execution.title")}</DropdownMenuLabel>
							<DropdownMenuItem
								className="flex items-center justify-between gap-3"
								onClick={() => setIsFastMode((current) => !current)}
							>
								<div>
									<div>{t("composer.execution.fastMode")}</div>
									<div className="text-[12px] text-muted-foreground">
										{t("composer.execution.fastModeHint")}
									</div>
								</div>
								<span className="text-[12px] text-foreground">
									{isFastMode ? "✓" : ""}
								</span>
							</DropdownMenuItem>
							<DropdownMenuSeparator />
							<DropdownMenuGroup>
								<DropdownMenuLabel>{t("composer.execution.effort")}</DropdownMenuLabel>
								{availableEffortLevels.map((id) => {
									const display = getEffortDisplay(id);
									return (
										<DropdownMenuItem
											key={id}
											className="flex items-center justify-between gap-3"
											onClick={() =>
												updateEffortSelection({ effort: id, ultrathink: false })
											}
										>
											<div className="flex items-center gap-2.5">
												<EffortBrainIcon level={display.icon} />
												<span>
													{t(`composer.effort.${id}`, { defaultValue: display.label })}
												</span>
											</div>
											{id === effort && !ultrathinkSelected ? "✓" : null}
										</DropdownMenuItem>
									);
								})}
								<DropdownMenuItem
									className="flex items-center justify-between gap-3"
									onClick={() => updateEffortSelection({ effort, ultrathink: true })}
								>
									<div className="flex items-center gap-2.5">
										<EffortBrainIcon level="max" />
										<span>{t("composer.effort.ultrathink")}</span>
									</div>
									{ultrathinkSelected ? "✓" : null}
								</DropdownMenuItem>
							</DropdownMenuGroup>
						</DropdownMenuContent>
					</DropdownMenu>
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
							{hasContent ? (
								<Button
									type="button"
									variant="outline"
									size="icon"
									aria-label={t("composer.controls.steer")}
									disabled={steerDisabled}
									className="rounded-[9px]"
									onClick={() => {
										void handleSubmitDraft();
									}}
								>
									<ArrowUp className="size-[15px]" strokeWidth={2.2} />
								</Button>
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
										<ChevronUp className="size-3.5" strokeWidth={2} />
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
											{delegateTargets.map((target) => {
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
															void submitDelegation([target.id]);
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
		</div>
	);
}
