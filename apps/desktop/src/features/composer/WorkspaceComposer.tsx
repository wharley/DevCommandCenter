import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
	ArrowUp,
	ChevronDown,
	ClipboardList,
	Plus,
	Square,
	SquareTerminal,
	Zap,
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
import type { ProviderCatalog } from "@dcc/contracts";
import type { RuntimeSessionSnapshot } from "@/features/sessions/workbench-types";
import { canAbortRun } from "@/features/sessions/session-chrome-state";
import { ComposerPlanFollowUpBanner } from "./ComposerPlanFollowUpBanner";
import { getProviderUnhealthyReason } from "@/features/providers/provider-selection.logic";
import { ContextBar } from "./ContextBar";
import { ComposerProviderModelMenu } from "./ComposerProviderModelMenu";
import { ContextUsageRing } from "./ContextUsageRing";
import { EffortBrainIcon } from "./EffortBrainIcon";
import { UsageStatsIndicator } from "./UsageStatsIndicator";
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
import type { ComposerSubmittedTurn } from "./composer-turn";
import {
	clearDraft,
	loadEffortSelection,
	saveEffortSelection,
} from "./draftStorage";
import { appendComposerText, readComposerPrompt, setEditorText } from "./editorOps";
import type {
	OpenTerminalRequest,
	TerminalScopeTarget,
} from "@/features/terminal/terminal-scope";
import {
	getTerminalRuntimeId,
	type TerminalTab,
	useProjectTerminals,
} from "@/features/terminal/terminal-tabs-store";
import {
	getTerminalSnapshot,
	subscribeTerminalStore,
	type TerminalStatus,
} from "@/features/terminal/terminal-store";

type WorkspaceComposerProps = {
	draftKey: string;
	disabled: boolean;
	providerChoices: ProviderCatalog["providers"];
	selectedProviderId: string | null;
	selectedModelId: string | null;
	sessionSnapshot: RuntimeSessionSnapshot | null;
	pendingPrompt: string | null;
	/** Text to append to the draft (e.g. a diff annotation); nonce re-fires it. */
	prefill?: { text: string; nonce: number } | null;
	workspacePath: string | null;
	workspaceBranch: string | null;
	showPlanFollowUpPrompt: boolean;
	planTitle: string | null;
	planMarkdown: string | null;
	onSelectProvider: (providerId: string) => void;
	onSelectModel: (modelId: string) => void;
	onSubmitPrompt: (turn: ComposerSubmittedTurn) => Promise<void>;
	onAbortSession: () => void;
	onOpenPlanSidebar: () => void;
	onImplementPlanInNewThread: (input: {
		planMarkdown: string;
		planTitle: string | null;
	}) => void;
	terminalScopes?: TerminalScopeTarget[];
	onOpenTerminal?: (request: OpenTerminalRequest) => void;
};

type TerminalMenuRow = {
	tab: TerminalTab;
	status: TerminalStatus | "ready";
};

function buildTerminalMenuRows(
	scopeKey: string,
	tabs: TerminalTab[],
): TerminalMenuRow[] {
	return tabs.map((tab) => ({
		tab,
		status:
			getTerminalSnapshot(getTerminalRuntimeId(scopeKey, tab.id))?.status ??
			"ready",
	}));
}

function statusDotClass(status: TerminalStatus | "ready") {
	if (status === "running") return "bg-emerald-500";
	if (status === "starting") return "bg-amber-400";
	if (status === "error") return "bg-destructive";
	if (status === "exited") return "bg-muted-foreground";
	return "bg-muted-foreground/50";
}

function TerminalScopeMenuGroup({
	scope,
	rows,
	onOpenTerminal,
}: {
	scope: TerminalScopeTarget | null;
	rows: TerminalMenuRow[];
	onOpenTerminal: (request: OpenTerminalRequest) => void;
}) {
	const label = scope?.label ?? "Terminal";
	const disabled = !scope || !scope.cwd;
	const kind = scope?.kind;

	return (
		<>
			<DropdownMenuLabel>{label}</DropdownMenuLabel>
			{rows.length > 0 ? (
				rows.map((row) => (
					<DropdownMenuItem
						key={row.tab.id}
						size="sm"
						disabled={disabled}
						className="justify-between gap-2"
						onSelect={() => {
							if (!kind) return;
							onOpenTerminal({ scope: kind, terminalId: row.tab.id });
						}}
					>
						<span className="flex min-w-0 items-center gap-1.5">
							<span
								className={cn(
									"size-1.5 shrink-0 rounded-full",
									statusDotClass(row.status),
								)}
							/>
							<span className="truncate">{row.tab.title}</span>
						</span>
						<span className="shrink-0 text-[11px] text-muted-foreground">
							{row.status}
						</span>
					</DropdownMenuItem>
				))
			) : (
				<DropdownMenuItem size="sm" disabled className="text-muted-foreground">
					No terminals yet
				</DropdownMenuItem>
			)}
			<DropdownMenuItem
				size="sm"
				disabled={disabled}
				onSelect={() => {
					if (!kind) return;
					onOpenTerminal({ scope: kind, createNew: true });
				}}
			>
				<Plus className="size-3.5" strokeWidth={1.8} />
				<span>New {label.toLowerCase()} terminal</span>
			</DropdownMenuItem>
			{scope?.disabledReason ? (
				<div className="px-1.5 py-1 text-[11px] leading-snug text-muted-foreground">
					{scope.disabledReason}
				</div>
			) : null}
		</>
	);
}

export function WorkspaceComposer({
	draftKey,
	disabled,
	providerChoices,
	selectedProviderId,
	selectedModelId,
	sessionSnapshot,
	pendingPrompt,
	prefill,
	workspacePath,
	workspaceBranch,
	showPlanFollowUpPrompt,
	planTitle,
	planMarkdown,
	onSelectProvider,
	onSelectModel,
	onSubmitPrompt,
	onAbortSession,
	onOpenPlanSidebar,
	onImplementPlanInNewThread,
	terminalScopes,
	onOpenTerminal,
}: WorkspaceComposerProps) {
	const { t } = useTranslation("common");
	const [hasContent, setHasContent] = useState(false);
	const [isSubmitting, setIsSubmitting] = useState(false);
	const [isFastMode, setIsFastMode] = useState(true);
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
	const [terminalStatusVersion, setTerminalStatusVersion] = useState(0);
	const worktreeTerminalScope =
		terminalScopes?.find((scope) => scope.kind === "worktree") ?? null;
	const projectTerminalScope =
		terminalScopes?.find((scope) => scope.kind === "project") ?? null;
	const worktreeTerminals = useProjectTerminals(
		worktreeTerminalScope?.scopeKey ?? "__missing-worktree-terminal-scope__",
	);
	const projectTerminals = useProjectTerminals(
		projectTerminalScope?.scopeKey ?? "__missing-project-terminal-scope__",
	);
	const worktreeTerminalRows = useMemo(
		() =>
			worktreeTerminalScope
				? buildTerminalMenuRows(worktreeTerminalScope.scopeKey, worktreeTerminals.tabs)
				: [],
		[terminalStatusVersion, worktreeTerminalScope, worktreeTerminals.tabs],
	);
	const projectTerminalRows = useMemo(
		() =>
			projectTerminalScope
				? buildTerminalMenuRows(projectTerminalScope.scopeKey, projectTerminals.tabs)
				: [],
		[terminalStatusVersion, projectTerminalScope, projectTerminals.tabs],
	);
	const runningTerminalCount = useMemo(
		() =>
			[...worktreeTerminalRows, ...projectTerminalRows].filter(
				(row) => row.status === "running" || row.status === "starting",
			).length,
		[projectTerminalRows, worktreeTerminalRows],
	);

	useEffect(
		() => subscribeTerminalStore(() => setTerminalStatusVersion((value) => value + 1)),
		[],
	);
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

	const inputDisabled = disabled;
	const toolbarDisabled = disabled;
	const hasProvider = Boolean(selectedProviderId);
	const turnCount = sessionSnapshot?.turnCount ?? 0;
	const checkpointCount = sessionSnapshot?.checkpointCount ?? 0;
	const contextUsageValue = Math.min(100, turnCount * 12 + checkpointCount * 8);

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
	const highlightEffort =
		ultrathinkSelected || effort === "xhigh" || effort === "max";

	useEffect(() => {
		setContextDirectories(
			buildComposerContextDirectories({ workspacePath, workspaceBranch }),
		);
	}, [workspaceBranch, workspacePath]);

	return (
		<div
			ref={composerRootRef}
			aria-label="Workspace composer"
			data-focus-scope="composer"
			className={cn(
				"relative flex flex-col rounded-2xl border border-border/40 bg-sidebar shadow-[0_-1px_8px_rgba(0,0,0,0.05),0_0_0_1px_rgba(255,255,255,0.02)]",
				inputDisabled ? "p-0" : "px-4 pb-3 pt-3",
				inputDisabled && "cursor-not-allowed opacity-60",
			)}
		>
			{showPlanFollowUpPrompt ? (
				<ComposerPlanFollowUpBanner
					planTitle={planTitle}
					onImplementInNewThread={() => {
						if (!planMarkdown) {
							return;
						}
						onImplementPlanInNewThread({
							planMarkdown,
							planTitle,
						});
					}}
					onOpenPlanSidebar={onOpenPlanSidebar}
				/>
			) : null}
			<ContextBar
				directories={contextDirectories}
				disabled={inputDisabled}
				onRemove={(directoryId) => {
					setContextDirectories((current) =>
						current.filter((directory) => directory.id !== directoryId),
					);
				}}
			/>
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
								aria-label="Workspace input"
								aria-multiline
								className={cn(
									"composer-editor min-h-[64px] max-h-[240px] resize-none overflow-x-hidden overflow-y-auto whitespace-pre-wrap break-words bg-transparent text-[14px] leading-5 tracking-[-0.01em] text-foreground outline-none",
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
				<div className="flex min-w-0 flex-wrap items-center gap-2">
					<ComposerProviderModelMenu
						providers={providerChoices}
						selectedProviderId={selectedProviderId}
						selectedModelId={selectedModelId}
						onSelectProvider={onSelectProvider}
						onSelectModel={onSelectModel}
						disabled={toolbarDisabled}
					/>
					<Tooltip>
						<TooltipTrigger asChild>
							<ComposerButton
								type="button"
								aria-label="Fast mode"
								disabled={toolbarDisabled}
								className={cn(
									"relative h-7 px-1.5",
									isFastMode
										? "text-amber-500 hover:bg-amber-500/10 hover:text-amber-500"
										: "text-muted-foreground",
									toolbarDisabled &&
										"cursor-not-allowed opacity-45 hover:bg-transparent hover:text-muted-foreground",
								)}
								onClick={() => setIsFastMode((c) => !c)}
							>
								<span className="relative block size-[14px]">
									<Zap
										className={cn(
											"absolute inset-0 z-0 size-[14px]",
											!isFastMode && "opacity-55",
										)}
										strokeWidth={1.8}
									/>
								</span>
							</ComposerButton>
						</TooltipTrigger>
						<TooltipContent side="top" sideOffset={4}>
							<span>Fast mode{isFastMode ? " (on)" : ""}</span>
						</TooltipContent>
					</Tooltip>
					<DropdownMenu>
						<DropdownMenuTrigger
							type="button"
							disabled={toolbarDisabled}
							className={cn(
								`flex h-7 items-center gap-0.5 ${composerToolbarTriggerClassName}`,
								highlightEffort
									? "bg-amber-500/8 text-amber-600 hover:bg-amber-500/12 hover:text-amber-700 dark:text-amber-400 dark:hover:text-amber-300"
									: "text-muted-foreground",
								toolbarDisabled &&
									"cursor-not-allowed opacity-45 hover:bg-transparent hover:text-muted-foreground",
							)}
						>
							<span className="capitalize">{effortLabel}</span>
							<ChevronDown
								className="size-3 text-muted-foreground/40"
								strokeWidth={2}
							/>
						</DropdownMenuTrigger>
						<DropdownMenuContent
							side="top"
							align="start"
							sideOffset={4}
							className="min-w-[11rem]"
						>
							<DropdownMenuGroup>
								<DropdownMenuLabel>Effort</DropdownMenuLabel>
								{availableEffortLevels.map((id) => {
									const display = getEffortDisplay(id);
									return (
										<DropdownMenuItem
											key={id}
											disabled={toolbarDisabled}
											className="flex items-center justify-between gap-3"
											onClick={() => {
												updateEffortSelection({ effort: id, ultrathink: false });
											}}
										>
											<div className="flex items-center gap-2.5">
												<EffortBrainIcon level={display.icon} />
												<span>{display.label}</span>
											</div>
											{id === effort && !ultrathinkSelected ? (
												<span className="text-[11px] text-foreground">✓</span>
											) : null}
										</DropdownMenuItem>
									);
								})}
								<DropdownMenuSeparator />
								<DropdownMenuItem
									disabled={toolbarDisabled}
									className="flex items-center justify-between gap-3"
									onClick={() => {
										updateEffortSelection({ effort, ultrathink: true });
									}}
								>
									<div className="flex items-center gap-2.5">
										<EffortBrainIcon level="max" />
										<span>Ultrathink</span>
									</div>
									{ultrathinkSelected ? (
										<span className="text-[11px] text-foreground">✓</span>
									) : null}
								</DropdownMenuItem>
							</DropdownMenuGroup>
						</DropdownMenuContent>
					</DropdownMenu>
					<ComposerButton
						type="button"
						aria-label="Plan mode"
						disabled={toolbarDisabled}
						className={cn(
							"h-7 gap-1 px-1.5 text-[11px]",
							isPlanMode
								? "text-[color:var(--plan)] hover:text-[color:var(--plan)]"
								: "text-muted-foreground/70 hover:text-muted-foreground/70",
						)}
						onClick={() =>
							setPlanModeByScope((current) =>
								setPlanModeState(current, {
									workspaceId: draftKey,
									sessionId,
									enabled: !isPlanMode,
								}),
							)
						}
					>
						<ClipboardList className="size-[13px]" strokeWidth={1.8} />
						<span>Plan</span>
					</ComposerButton>
					{onOpenTerminal ? (
						<DropdownMenu>
							<Tooltip>
								<TooltipTrigger asChild>
									<DropdownMenuTrigger asChild>
										<ComposerButton
											type="button"
											aria-label="Terminal"
											className="relative h-7 px-1.5 text-muted-foreground"
										>
											<SquareTerminal className="size-[14px]" strokeWidth={1.8} />
											{runningTerminalCount > 0 ? (
												<span className="absolute -right-1 -top-1 flex h-4 min-w-4 items-center justify-center rounded-full border border-background bg-emerald-500 px-1 text-[9px] font-medium leading-none text-white">
													{runningTerminalCount}
												</span>
											) : null}
										</ComposerButton>
									</DropdownMenuTrigger>
								</TooltipTrigger>
								<TooltipContent side="top" sideOffset={4}>
									<span>Terminal</span>
								</TooltipContent>
							</Tooltip>
							<DropdownMenuContent align="start" className="w-72">
								<TerminalScopeMenuGroup
									scope={worktreeTerminalScope}
									rows={worktreeTerminalRows}
									onOpenTerminal={onOpenTerminal}
								/>
								<DropdownMenuSeparator />
								<TerminalScopeMenuGroup
									scope={projectTerminalScope}
									rows={projectTerminalRows}
									onOpenTerminal={onOpenTerminal}
								/>
							</DropdownMenuContent>
						</DropdownMenu>
					) : null}
				</div>

				<div className="flex shrink-0 items-center gap-1">
					<UsageStatsIndicator
						turnCount={turnCount}
						checkpointCount={checkpointCount}
						disabled={!sessionSnapshot}
					/>
					{sessionId ? (
						<ContextUsageRing
							value={contextUsageValue}
							className="shrink-0"
						/>
					) : null}
					{canStopRun ? (
						<div className="ml-1.5 flex items-center gap-1.5">
							<Button
								type="button"
								variant="destructive"
								size="icon"
								aria-label="Stop"
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
								aria-label="Steer"
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
						<Button
							type="button"
							variant="outline"
							size="icon"
							aria-label="Send"
							disabled={sendDisabled}
							onClick={() => {
								void handleSubmitDraft();
							}}
							className="ml-1.5 rounded-[9px]"
						>
							<ArrowUp className="size-[15px]" strokeWidth={2.2} />
						</Button>
					)}
				</div>
			</div>
		</div>
	);
}
