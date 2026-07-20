import { useQuery } from "@tanstack/react-query";
import type {
	SessionEventRecord,
	WorkspaceGitConflictEntry,
	WorkspaceGitConflictSide,
	WorkspaceGitValidationReport,
} from "@dcc/contracts";
import type { TFunction } from "i18next";
import {
	AlertTriangle,
	Check,
	ChevronDown,
	ChevronLeft,
	ChevronRight,
	ChevronUp,
	ExternalLink,
	FileWarning,
	GitMerge,
	ListChecks,
	Loader2,
	RotateCcw,
	Square,
	Sparkles,
	Trash2,
	X,
	XCircle,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	workspaceGitAbortMerge,
	workspaceGitAcceptConflict,
	workspaceGitCompleteMerge,
	workspaceGitConflictState,
	workspaceGitMarkConflictResolved,
	workspaceGitSyncBase,
	workspaceGitValidationConfig,
	writeWorkspaceFile,
} from "@/lib/workspace-api";
import { cn } from "@/lib/utils";
import {
	type FileEditorHandle,
	WorkspaceFileEditor,
} from "@/features/editor/WorkspaceFileSurface";
import {
	applyMergeConflictResolution,
	applyMergeConflictReplacement,
	hasMergeConflictMarkerFragments,
	parseMergeConflictHunks,
	type MergeConflictResolution,
} from "./merge-conflict-hunks";
import {
	AgentConflictResolutionError,
	buildAgentConflictResolutionPrompt,
	createAgentResolutionToken,
	parseAgentConflictResolution,
	validateAgentConflictResolutionSuggestion,
	type AgentResolutionRunRequest,
	type AgentResolutionRunResult,
} from "./agent-conflict-resolution";

const CONFLICT_STATE_QUERY_KEY = "workspaceGitConflictState";

type Props = {
	workspaceRoot: string;
	currentWorkspaceLabel?: string | null;
	baseBranch: string | null;
	forgeLogin: string | null;
	onClose: () => void;
	onStateChanged: () => Promise<void> | void;
	onResolveWithAgent: (
		request: AgentResolutionRunRequest,
	) => Promise<AgentResolutionRunResult>;
	onOpenAgentSession: (sessionId: string) => void;
};

type AppliedAgentSuggestion = {
	path: string;
	explanation: string;
	sessionId: string;
	scope: "hunk" | "file";
};

type AgentRunState = {
	sessionId: string | null;
	turnId: string | null;
	events: SessionEventRecord[];
	status: "analyzing" | "cancelling" | "completed" | "failed";
	expanded: boolean;
	error: string | null;
};

function errorMessage(error: unknown) {
	return error instanceof Error ? error.message : String(error);
}

function agentResolutionErrorMessage(
	t: TFunction<"common">,
	error: AgentConflictResolutionError,
) {
	switch (error.code) {
		case "contract":
			return t("mergeConflict.errors.agentContract");
		case "invalid":
			return t("mergeConflict.errors.agentInvalid");
		case "incomplete":
			return t("mergeConflict.errors.agentIncomplete");
		case "result-invalid":
			return t("mergeConflict.errors.agentResultInvalid");
		case "empty-result":
			return t("mergeConflict.errors.agentEmptyResult");
	}
}

function shortRef(value: string | null | undefined, fallback: string) {
	if (!value) return fallback;
	return value.replace(/^refs\/(heads|remotes)\//, "").replace(/^remotes\//, "");
}

function conflictKindLabel(
	t: TFunction<"common">,
	entry: WorkspaceGitConflictEntry,
) {
	switch (entry.kind) {
		case "both-modified":
			return t("mergeConflict.conflictKind.bothModified");
		case "both-added":
			return t("mergeConflict.conflictKind.bothAdded");
		case "deleted-by-current":
			return t("mergeConflict.conflictKind.deletedByCurrent");
		case "deleted-by-incoming":
			return t("mergeConflict.conflictKind.deletedByIncoming");
		case "both-deleted":
			return t("mergeConflict.conflictKind.bothDeleted");
		default:
			return t("mergeConflict.conflictKind.other");
	}
}

function editableGitMode(mode: string | null) {
	return mode == null || mode.startsWith("100");
}

function compactActivityTarget(value: string | null, maxLength = 96) {
	if (!value) return null;
	const singleLine = value.replace(/\s+/g, " ").trim();
	return singleLine.length <= maxLength
		? singleLine
		: `${singleLine.slice(0, maxLength - 1)}…`;
}

function SidePreview({
	label,
	text,
	tone,
}: {
	label: string;
	text: string;
	tone: "current" | "incoming";
}) {
	return (
		<div
			className={cn(
				"flex min-w-0 flex-1 flex-col overflow-hidden rounded-xl border bg-background/80 shadow-sm",
				tone === "current"
					? "border-sky-500/20"
					: "border-amber-500/25",
			)}
		>
			<div
				className={cn(
					"flex shrink-0 items-center gap-2 border-b px-3 py-2 text-[10px] font-semibold uppercase tracking-[0.08em]",
					tone === "current"
						? "border-sky-500/15 bg-sky-500/[0.06] text-sky-700 dark:text-sky-300"
						: "border-amber-500/15 bg-amber-500/[0.07] text-amber-800 dark:text-amber-200",
				)}
			>
				<span
					className={cn(
						"size-1.5 rounded-full",
						tone === "current" ? "bg-sky-500" : "bg-amber-500",
					)}
				/>
				<span className="truncate" title={label}>{label}</span>
			</div>
			<pre className="max-h-[28vh] min-h-24 overflow-auto whitespace-pre-wrap break-words px-3 py-2.5 font-mono text-[11px] leading-5 selection:bg-primary/20">
				{text || "∅"}
			</pre>
		</div>
	);
}

export function WorkspaceMergeConflictResolver({
	workspaceRoot,
	currentWorkspaceLabel,
	baseBranch,
	forgeLogin,
	onClose,
	onStateChanged,
	onResolveWithAgent,
	onOpenAgentSession,
}: Props) {
	const { t, i18n } = useTranslation("common");
	const editorRef = useRef<FileEditorHandle | null>(null);
	const initialResultRef = useRef<string | null>(null);
	const totalConflictsRef = useRef(0);
	const agentAbortControllerRef = useRef<AbortController | null>(null);
	const [selectedPath, setSelectedPath] = useState<string | null>(null);
	const [buffer, setBuffer] = useState("");
	const [activeHunk, setActiveHunk] = useState(0);
	const [busy, setBusy] = useState<string | null>(null);
	const [agentSuggestion, setAgentSuggestion] =
		useState<AppliedAgentSuggestion | null>(null);
	const [agentRun, setAgentRun] = useState<AgentRunState | null>(null);
	const [validationReport, setValidationReport] =
		useState<WorkspaceGitValidationReport | null>(null);

	const query = useQuery({
		queryKey: [CONFLICT_STATE_QUERY_KEY, workspaceRoot],
		queryFn: () => workspaceGitConflictState({ workspaceRoot }),
		enabled: Boolean(workspaceRoot),
		staleTime: 0,
	});

	const state = query.data;
	const conflicts = state?.conflicts ?? [];
	const validationConfigQuery = useQuery({
		queryKey: ["workspaceGitValidationConfig", workspaceRoot],
		queryFn: () => workspaceGitValidationConfig({ workspaceRoot }),
		enabled: state?.operation === "merge",
		staleTime: 0,
		refetchOnWindowFocus: false,
	});
	if (conflicts.length > totalConflictsRef.current) {
		totalConflictsRef.current = conflicts.length;
	}

	const selected = useMemo(
		() => conflicts.find((entry) => entry.path === selectedPath) ?? null,
		[conflicts, selectedPath],
	);
	const hunks = useMemo(() => parseMergeConflictHunks(buffer), [buffer]);
	const hunk = hunks[Math.min(activeHunk, Math.max(hunks.length - 1, 0))] ?? null;
	const dirty = selected != null && buffer !== (initialResultRef.current ?? "");
	const currentRef = shortRef(
		state?.currentBranch,
		t("mergeConflict.currentBranchFallback"),
	);
	const currentLabel = t("mergeConflict.currentWorkspaceLabel", {
		workspace: currentWorkspaceLabel?.trim() || currentRef,
	});
	const incomingRef = shortRef(
		state?.incomingRef ?? baseBranch,
		t("mergeConflict.incomingBranchFallback"),
	);
	const incomingLabel = t("mergeConflict.incomingBranchLabel", {
		branch: incomingRef,
	});
	const suspiciousEmptyResult =
		selected != null &&
		buffer.trim().length === 0 &&
		[selected.current.text, selected.incoming.text].some(
			(value) => value != null && value.trim().length > 0,
		);
	const resultLineCount = buffer.length === 0 ? 0 : buffer.split(/\r\n|\r|\n/).length;
	const agentRunDetails = useMemo(() => {
		if (!agentRun?.turnId) {
			return {
				activities: [] as string[],
				output: "",
				pendingPermission: false,
				pendingInput: false,
			};
		}
		const events = agentRun.events.filter((event) => {
			const kind = event.kind;
			return "turnId" in kind && kind.turnId === agentRun.turnId;
		});
		const pendingPermissionIds = new Set<string>();
		const pendingInputIds = new Set<string>();
		const activities: string[] = [];
		let output = "";

		for (const event of events) {
			switch (event.kind.type) {
				case "turn_started":
					activities.push(t("mergeConflict.agent.activity.started"));
					break;
				case "turn_reasoning_started":
					activities.push(
						event.kind.label?.trim() ||
							t("mergeConflict.agent.activity.analyzing"),
					);
					break;
				case "turn_tool_call_started": {
					const target = compactActivityTarget(
						event.kind.file ?? event.kind.command,
					);
					activities.push(
						t("mergeConflict.agent.activity.tool", {
							action: event.kind.action,
							target: target ? ` · ${target}` : "",
						}),
					);
					break;
				}
				case "turn_tool_call_failed":
					activities.push(
						t("mergeConflict.agent.activity.toolFailed", {
							reason: event.kind.reason ? ` · ${event.kind.reason}` : "",
						}),
					);
					break;
				case "turn_permission_requested":
					pendingPermissionIds.add(event.kind.requestId);
					activities.push(t("mergeConflict.agent.activity.permission"));
					break;
				case "turn_permission_resolved":
					pendingPermissionIds.delete(event.kind.requestId);
					break;
				case "turn_user_input_requested":
					pendingInputIds.add(event.kind.requestId);
					activities.push(t("mergeConflict.agent.activity.input"));
					break;
				case "turn_user_input_resolved":
					pendingInputIds.delete(event.kind.requestId);
					break;
				case "turn_delta":
					output += event.kind.content;
					break;
				case "turn_completed":
					activities.push(t("mergeConflict.agent.activity.completed"));
					break;
				case "turn_aborted":
					activities.push(t("mergeConflict.agent.activity.aborted"));
					break;
			}
		}

		return {
			activities: activities.slice(-6),
			output,
			pendingPermission: pendingPermissionIds.size > 0,
			pendingInput: pendingInputIds.size > 0,
		};
	}, [agentRun, t]);

	const openEntry = useCallback((entry: WorkspaceGitConflictEntry) => {
		const result =
			entry.result.text ?? entry.current.text ?? entry.incoming.text ?? "";
		setSelectedPath(entry.path);
		setBuffer(result);
		initialResultRef.current = entry.result.text;
		setActiveHunk(0);
		setAgentSuggestion(null);
		setAgentRun(null);
	}, []);

	useEffect(() => {
		if (conflicts.length === 0) {
			setSelectedPath(null);
			return;
		}
		if (!selectedPath || !conflicts.some((entry) => entry.path === selectedPath)) {
			openEntry(conflicts[0]!);
		}
	}, [conflicts, openEntry, selectedPath]);

	useEffect(() => {
		if (activeHunk >= hunks.length) {
			setActiveHunk(Math.max(hunks.length - 1, 0));
		}
	}, [activeHunk, hunks.length]);

	const refresh = useCallback(async () => {
		await query.refetch();
		await onStateChanged();
	}, [onStateChanged, query]);

	const requestClose = useCallback(
		() => {
			if (busy) {
				toast.info(
					busy === "agent"
						? t("mergeConflict.busy.agent")
						: t("mergeConflict.busy.operation"),
				);
				return;
			}
			if (
				dirty &&
				!window.confirm(t("mergeConflict.confirm.discardEdit"))
			) {
				return;
			}
			onClose();
		},
		[busy, dirty, onClose, t],
	);

	const startMerge = useCallback(async () => {
		setBusy("start");
		let failure: unknown = null;
		try {
			await workspaceGitSyncBase({
				workspaceRoot,
				baseBranch,
				forgeLogin,
			});
		} catch (error) {
			failure = error;
		}
		const next = await query.refetch();
		await onStateChanged();
		setBusy(null);
		if ((next.data?.conflicts.length ?? 0) > 0) {
			toast.info(t("mergeConflict.toast.mergeStarted"));
			return;
		}
		if (failure) {
			toast.error(errorMessage(failure));
			return;
		}
		toast.success(t("mergeConflict.toast.updatedWithoutConflicts"));
		onClose();
	}, [
		baseBranch,
		forgeLogin,
		onClose,
		onStateChanged,
		query,
		t,
		workspaceRoot,
	]);

	const applyHunk = useCallback(
		(resolution: MergeConflictResolution) => {
			if (!hunk) return;
			const source = editorRef.current?.getValue() ?? buffer;
			const nextHunk = parseMergeConflictHunks(source).find(
				(candidate) => candidate.startOffset === hunk.startOffset,
			);
			if (!nextHunk) return;
			const next = applyMergeConflictResolution(source, nextHunk, resolution);
			editorRef.current?.setValue(next);
			setBuffer(next);
		},
		[buffer, hunk],
	);

	const resolveWithAgent = useCallback(async () => {
		if (!selected) return;
		const source = editorRef.current?.getValue() ?? buffer;
		const requestedHunk = hunk
			? parseMergeConflictHunks(source).find(
					(candidate) => candidate.startOffset === hunk.startOffset,
				)
			: null;
		if (hunk && !requestedHunk) {
			toast.warning(t("mergeConflict.toast.hunkChanged"));
			return;
		}

		const token = createAgentResolutionToken();
		const prompt = buildAgentConflictResolutionPrompt(
			{
				path: selected.path,
				kind: selected.kind,
				currentRef,
				incomingRef,
				responseLanguage:
					i18n.resolvedLanguage === "en" ? "English" : "Português (Brasil)",
				baseText: selected.base.text,
				currentText: selected.current.text,
				incomingText: selected.incoming.text,
				resultText: source,
				scope: requestedHunk
					? {
							type: "hunk",
							startLine: requestedHunk.startLine,
							baseText: requestedHunk.baseText,
							currentText: requestedHunk.currentText,
							incomingText: requestedHunk.incomingText,
						}
					: { type: "file" },
			},
			token,
		);
		const abortController = new AbortController();
		agentAbortControllerRef.current = abortController;
		setBusy("agent");
		setAgentSuggestion(null);
		setAgentRun({
			sessionId: null,
			turnId: null,
			events: [],
			status: "analyzing",
			expanded: true,
			error: null,
		});
		try {
			const result = await onResolveWithAgent({
				prompt,
				title: t("mergeConflict.agent.sessionTitle", { path: selected.path }),
				signal: abortController.signal,
				onStarted: (run) => {
					setAgentRun((current) => ({
						sessionId: run.sessionId,
						turnId: run.turnId,
						events: current?.events ?? [],
						status: current?.status ?? "analyzing",
						expanded: current?.expanded ?? true,
						error: null,
					}));
				},
				onProgress: (progress) => {
					setAgentRun((current) => ({
						sessionId: progress.sessionId,
						turnId: progress.turnId,
						events: progress.events,
						status: current?.status ?? "analyzing",
						expanded: current?.expanded ?? true,
						error: null,
					}));
				},
			});
			const suggestion = validateAgentConflictResolutionSuggestion(
				parseAgentConflictResolution(result.content, token),
				{
					scope: requestedHunk ? "hunk" : "file",
					currentText: requestedHunk
						? requestedHunk.currentText
						: selected.current.text,
					incomingText: requestedHunk
						? requestedHunk.incomingText
						: selected.incoming.text,
				},
			);
			const latest = editorRef.current?.getValue() ?? buffer;
			if (latest !== source) {
				throw new Error(t("mergeConflict.errors.agentResultChanged"));
			}
			const next = requestedHunk
				? applyMergeConflictReplacement(source, requestedHunk, suggestion.resolvedContent)
				: suggestion.resolvedContent;
			editorRef.current?.setValue(next);
			setBuffer(next);
			setAgentSuggestion({
				path: selected.path,
				explanation: suggestion.explanation,
				sessionId: result.sessionId,
				scope: requestedHunk ? "hunk" : "file",
			});
			setAgentRun((current) =>
				current
					? { ...current, status: "completed", expanded: false, error: null }
					: current,
			);
			toast.success(t("mergeConflict.toast.agentApplied"));
		} catch (error) {
			const message =
				error instanceof AgentConflictResolutionError
					? agentResolutionErrorMessage(t, error)
					: errorMessage(error);
			setAgentRun((current) =>
				current
					? { ...current, status: "failed", expanded: true, error: message }
					: current,
			);
			toast.error(
				message,
			);
		} finally {
			if (agentAbortControllerRef.current === abortController) {
				agentAbortControllerRef.current = null;
			}
			setBusy(null);
		}
	}, [
		buffer,
		currentRef,
		hunk,
		i18n.resolvedLanguage,
		incomingRef,
		onResolveWithAgent,
		selected,
		t,
	]);

	const cancelAgentRun = useCallback(() => {
		const controller = agentAbortControllerRef.current;
		if (!controller || controller.signal.aborted) return;
		setAgentRun((current) =>
			current ? { ...current, status: "cancelling" } : current,
		);
		controller.abort();
	}, []);

	const openAgentSession = useCallback(() => {
		if (!agentRun?.sessionId) return;
		if (dirty && !window.confirm(t("mergeConflict.confirm.openAgentSession"))) {
			return;
		}
		onOpenAgentSession(agentRun.sessionId);
	}, [agentRun?.sessionId, dirty, onOpenAgentSession, t]);

	const saveAndResolve = useCallback(async () => {
		if (!selected) return;
		const content = editorRef.current?.getValue() ?? buffer;
		if (parseMergeConflictHunks(content).length > 0) {
			toast.warning(t("mergeConflict.toast.resolveAllHunks"));
			return;
		}
		if (
			hasMergeConflictMarkerFragments(content) &&
			!window.confirm(t("mergeConflict.confirm.remainingMarkers"))
		) {
			return;
		}
		if (
			content.trim().length === 0 &&
			[selected.current.text, selected.incoming.text].some(
				(value) => value != null && value.trim().length > 0,
			) &&
			!window.confirm(t("mergeConflict.confirm.emptyResult"))
		) {
			return;
		}
		setBusy("save");
		try {
			if (content !== (initialResultRef.current ?? "") || !selected.result.exists) {
				const result = await writeWorkspaceFile({
					workspaceRoot,
					relativePath: selected.path,
					content,
					expectedPrevious: selected.result.exists ? initialResultRef.current : null,
				});
				if (result.conflicted) {
					throw new Error(t("mergeConflict.toast.fileChanged"));
				}
			}
			await workspaceGitMarkConflictResolved({
				workspaceRoot,
				relativePath: selected.path,
				delete: false,
			});
			toast.success(
				t("mergeConflict.toast.markedResolved", { path: selected.path }),
			);
			await refresh();
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			setBusy(null);
		}
	}, [buffer, refresh, selected, t, workspaceRoot]);

	const acceptWholeSide = useCallback(
		async (side: WorkspaceGitConflictSide) => {
			if (!selected) return;
			setBusy(side);
			try {
				await workspaceGitAcceptConflict({
					workspaceRoot,
					relativePath: selected.path,
					side,
				});
				toast.success(
					t("mergeConflict.toast.resolvedWith", {
						path: selected.path,
						branch: side === "current" ? currentLabel : incomingLabel,
					}),
				);
				await refresh();
			} catch (error) {
				toast.error(errorMessage(error));
			} finally {
				setBusy(null);
			}
		},
		[currentLabel, incomingLabel, refresh, selected, t, workspaceRoot],
	);

	const deleteResult = useCallback(async () => {
		if (!selected) return;
		if (
			!window.confirm(
				t("mergeConflict.confirm.deleteResult", { path: selected.path }),
			)
		) {
			return;
		}
		setBusy("delete");
		try {
			await workspaceGitMarkConflictResolved({
				workspaceRoot,
				relativePath: selected.path,
				delete: true,
			});
			toast.success(
				t("mergeConflict.toast.removedResolved", { path: selected.path }),
			);
			await refresh();
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			setBusy(null);
		}
	}, [refresh, selected, t, workspaceRoot]);

	const abortMerge = useCallback(async () => {
		if (
			!window.confirm(t("mergeConflict.confirm.abort"))
		) {
			return;
		}
		setBusy("abort");
		try {
			await workspaceGitAbortMerge({ workspaceRoot });
			await onStateChanged();
			toast.success(t("mergeConflict.toast.mergeAborted"));
			onClose();
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			setBusy(null);
		}
	}, [onClose, onStateChanged, t, workspaceRoot]);

	const completeMerge = useCallback(async () => {
		const commands = validationConfigQuery.data?.commands ?? [];
		if (
			commands.length > 0 &&
			!window.confirm(
				t("mergeConflict.confirm.validationCommands", {
					commands: commands.map((command) => `• ${command}`).join("\n"),
				}),
			)
		) {
			return;
		}
		setBusy("complete");
		setValidationReport(null);
		try {
			const result = await workspaceGitCompleteMerge({
				workspaceRoot,
				forgeLogin,
				validationConfigHash: validationConfigQuery.data?.configHash ?? null,
				validationCommands: validationConfigQuery.data?.commands ?? [],
			});
			setValidationReport(result.validation);
			if (!result.completed) {
				await onStateChanged();
				toast.error(t("mergeConflict.toast.validationFailed"));
				return;
			}
			await onStateChanged();
			toast.success(t("mergeConflict.toast.mergeCompleted"));
				onClose();
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			setBusy(null);
		}
	}, [
		forgeLogin,
		onClose,
		onStateChanged,
		validationConfigQuery.data?.commands,
		validationConfigQuery.data?.configHash,
		workspaceRoot,
		t,
	]);

	const total = Math.max(totalConflictsRef.current, conflicts.length);
	const resolved = Math.max(total - conflicts.length, 0);
	const unsupported = state && !["none", "merge"].includes(state.operation);
	const readyToComplete =
		state?.operation === "merge" && conflicts.length === 0;
	const textUnavailable = selected
		? selected.result.binary || selected.result.truncated ||
			!editableGitMode(selected.current.mode) ||
			!editableGitMode(selected.incoming.mode) ||
			(selected.current.text == null && selected.incoming.text == null)
		: false;

	return (
		<div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
			<header className="relative shrink-0 border-b border-border/60 bg-gradient-to-r from-amber-500/[0.08] via-background to-background px-4 py-3">
				<div className="flex min-w-0 items-center gap-3">
					<div className="flex size-10 shrink-0 items-center justify-center rounded-xl border border-amber-500/20 bg-amber-500/12 text-amber-700 shadow-sm dark:text-amber-300">
						<GitMerge className="size-5" />
					</div>
					<div className="min-w-0 flex-1">
						<h1 className="truncate text-sm font-semibold tracking-[-0.01em]">
							{t("mergeConflict.title")}
						</h1>
						<div className="mt-1 flex min-w-0 items-center gap-1.5 text-[11px] text-muted-foreground">
							<span className="max-w-40 truncate rounded-md border border-border/60 bg-background/70 px-1.5 py-0.5 font-mono text-foreground/80" title={`${currentLabel}\n${currentRef}`}>
								{currentLabel}
							</span>
							<span aria-hidden>←</span>
							<span className="max-w-40 truncate rounded-md border border-amber-500/20 bg-amber-500/8 px-1.5 py-0.5 font-mono text-amber-800 dark:text-amber-200" title={`${incomingLabel}\n${incomingRef}`}>
								{incomingLabel}
							</span>
						</div>
					</div>
					{total > 0 ? (
						<div className="hidden min-w-36 rounded-lg border border-border/50 bg-background/60 px-3 py-2 sm:block">
							<div className="flex items-center justify-between text-[10px] text-muted-foreground">
								<span>{t("mergeConflict.files")}</span>
								<span className="font-mono text-foreground">{resolved}/{total}</span>
							</div>
							<div className="mt-1.5 h-1 overflow-hidden rounded-full bg-muted">
								<div className="h-full rounded-full bg-emerald-500 transition-all" style={{ width: `${total ? (resolved / total) * 100 : 0}%` }} />
							</div>
						</div>
					) : null}
						{state?.operation === "merge" ? (
							<Button
								variant="outline"
								size="sm"
								className="gap-1.5"
								disabled={Boolean(busy)}
								onClick={() => void abortMerge()}
							>
								<RotateCcw className="size-3.5" />
								{t("mergeConflict.abort")}
							</Button>
						) : null}
						<Button
							variant="ghost"
							size="icon-sm"
							disabled={Boolean(busy)}
							onClick={requestClose}
							title={t("mergeConflict.close")}
						>
							<X className="size-4" />
							<span className="sr-only">{t("mergeConflict.close")}</span>
						</Button>
				</div>
			</header>

				{query.isPending ? (
					<div className="flex flex-1 items-center justify-center gap-2 text-muted-foreground">
						<Loader2 className="size-4 animate-spin" /> {t("mergeConflict.loadingGit")}
					</div>
				) : query.isError ? (
					<div className="flex flex-1 items-center justify-center p-8 text-destructive">
						{errorMessage(query.error)}
					</div>
				) : unsupported ? (
					<div className="flex flex-1 items-center justify-center p-8 text-center">
						<div className="max-w-md">
							<AlertTriangle className="mx-auto size-8 text-amber-500" />
							<h3 className="mt-3 font-semibold">
								{t("mergeConflict.unsupported.title", { operation: state.operation })}
							</h3>
							<p className="mt-2 text-sm text-muted-foreground">
								{t("mergeConflict.unsupported.description")}
							</p>
						</div>
					</div>
				) : state?.operation === "none" && conflicts.length === 0 ? (
					<div className="flex flex-1 items-center justify-center p-8 text-center">
						<div className="max-w-lg">
							<GitMerge className="mx-auto size-10 text-amber-500" />
							<h3 className="mt-4 text-base font-semibold">
								{t("mergeConflict.start.title", {
									incoming: incomingLabel,
									current: currentLabel,
								})}
							</h3>
							<p className="mt-2 text-sm leading-6 text-muted-foreground">
								{t("mergeConflict.start.description")}
							</p>
							<Button
								className="mt-5 gap-1.5"
								disabled={Boolean(busy)}
								onClick={() => void startMerge()}
							>
								{busy === "start" ? (
									<Loader2 className="size-4 animate-spin" />
								) : (
									<GitMerge className="size-4" />
								)}
								{t("mergeConflict.start.action")}
							</Button>
						</div>
					</div>
				) : readyToComplete ? (
					<div className="flex flex-1 items-center justify-center overflow-y-auto p-8 text-center">
						<div className="w-full max-w-2xl">
							<div className="mx-auto flex size-12 items-center justify-center rounded-full bg-emerald-500/15 text-emerald-600">
								<Check className="size-6" />
							</div>
							<h3 className="mt-4 text-base font-semibold">
								{t("mergeConflict.completion.title")}
							</h3>
							<p className="mt-2 text-sm text-muted-foreground">{t("mergeConflict.completion.description")}</p>
							{validationConfigQuery.isPending ? (
								<div className="mt-5 flex items-center justify-center gap-2 text-xs text-muted-foreground"><Loader2 className="size-3.5 animate-spin" />{t("mergeConflict.validation.loading")}</div>
							) : validationConfigQuery.isError ? (
								<div className="mt-5 rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-left text-xs text-destructive"><div className="flex items-center gap-2 font-semibold"><XCircle className="size-4" />{t("mergeConflict.validation.invalidTitle")}</div><p className="mt-1 whitespace-pre-wrap">{errorMessage(validationConfigQuery.error)}</p></div>
							) : (validationConfigQuery.data?.commands.length ?? 0) > 0 ? (
								<div className="mt-5 rounded-lg border border-border/60 bg-muted/10 p-3 text-left">
									<div className="flex items-center gap-2 text-xs font-semibold"><ListChecks className="size-4 text-violet-500" />{t("mergeConflict.validation.configuredTitle")}</div>
									<div className="mt-2 space-y-1.5">{validationConfigQuery.data?.commands.map((command, index) => <code key={`${command}-${index}`} className="block rounded bg-muted px-2 py-1.5 text-[11px]">{command}</code>)}</div>
									<p className="mt-2 text-[10px] text-muted-foreground">{t("mergeConflict.validation.timeout", { seconds: validationConfigQuery.data?.timeoutSeconds ?? 600 })}</p>
								</div>
							) : (
								<div className="mt-5 rounded-lg border border-border/60 bg-muted/10 p-3 text-xs text-muted-foreground">{t("mergeConflict.validation.empty")}</div>
							)}
							{validationReport ? (
								<div className={cn("mt-4 rounded-lg border p-3 text-left", validationReport.status === "failed" ? "border-destructive/30 bg-destructive/5" : "border-emerald-500/30 bg-emerald-500/5")}>
									<div className="flex items-center gap-2 text-xs font-semibold">{validationReport.status === "failed" ? <XCircle className="size-4 text-destructive" /> : <Check className="size-4 text-emerald-600" />}{validationReport.status === "failed" ? t("mergeConflict.validation.failed") : validationReport.status === "passed" ? t("mergeConflict.validation.passed") : t("mergeConflict.validation.notConfigured")}</div>
									<div className="mt-2 space-y-2">{validationReport.steps.map((step, index) => <div key={`${step.command}-${index}`} className="overflow-hidden rounded border border-border/50 bg-background/60"><div className="flex items-center gap-2 px-2.5 py-1.5 text-[11px]"><span className={step.success ? "text-emerald-600" : "text-destructive"}>{step.success ? "✓" : "✕"}</span><code className="min-w-0 flex-1 truncate">{step.command}</code><span className="text-muted-foreground">{t("mergeConflict.validation.stepMeta", { seconds: (step.durationMs / 1000).toFixed(1), exitCode: step.exitCode != null ? t("mergeConflict.validation.exitCode", { code: step.exitCode }) : "" })}</span></div>{step.output ? <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-words border-t border-border/40 px-2.5 py-2 font-mono text-[10px] leading-4 text-muted-foreground">{step.output}</pre> : null}</div>)}</div>
								</div>
							) : null}
							<Button
								className="mt-5 gap-1.5"
								disabled={Boolean(busy) || validationConfigQuery.isPending || validationConfigQuery.isError}
								onClick={() => void completeMerge()}
							>
								{busy === "complete" ? (
									<Loader2 className="size-4 animate-spin" />
								) : (
									<GitMerge className="size-4" />
								)}
								{(validationConfigQuery.data?.commands.length ?? 0) > 0 ? t("mergeConflict.completion.withValidation") : t("mergeConflict.completion.withoutValidation")}
							</Button>
						</div>
					</div>
				) : (
					<div className="flex min-h-0 flex-1 bg-muted/[0.025]">
						<aside className="flex w-64 shrink-0 flex-col border-r border-border/60 bg-sidebar/60 2xl:w-72">
							<div className="border-b border-border/50 bg-background/40 px-3 py-3">
								<div className="flex items-center justify-between text-xs font-medium">
									<span>{t("mergeConflict.files")}</span>
									<Badge variant="secondary">
										{resolved}/{total}
									</Badge>
								</div>
								<div className="mt-2 h-1 overflow-hidden rounded-full bg-muted">
									<div
										className="h-full bg-emerald-500 transition-all"
										style={{
											width: `${total ? (resolved / total) * 100 : 0}%`,
										}}
									/>
								</div>
							</div>
							<div className="min-h-0 flex-1 overflow-y-auto p-2">
								{conflicts.map((entry) => (
									<button
										key={entry.path}
										type="button"
										disabled={Boolean(busy)}
										onClick={() => openEntry(entry)}
										className={cn(
											"group mb-1 flex w-full items-start gap-2.5 rounded-lg border px-2.5 py-2.5 text-left transition-all",
											selected?.path === entry.path
												? "border-amber-500/25 bg-amber-500/[0.08] text-accent-foreground shadow-sm"
												: "border-transparent hover:border-border/60 hover:bg-accent/50",
										)}
									>
										<FileWarning className="mt-0.5 size-3.5 shrink-0 text-amber-500" />
										<span className="min-w-0">
											<span className="block truncate font-mono text-[11px] font-medium">
												{entry.path}
											</span>
											<span className="mt-0.5 block text-[10px] text-muted-foreground">
												{conflictKindLabel(t, entry)}
											</span>
										</span>
									</button>
								))}
							</div>
						</aside>

						{selected ? (
							<section className="flex min-w-0 flex-1 flex-col bg-background">
								<div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border/60 bg-background/95 px-4 py-2.5 shadow-[0_1px_0_hsl(var(--border)/0.15)]">
									<div className="mr-auto min-w-0"><p className="truncate font-mono text-xs font-semibold">{selected.path}</p><p className="text-[10px] text-muted-foreground">{conflictKindLabel(t, selected)}</p></div>
									<Button className="max-w-56" variant="outline" size="xs" disabled={Boolean(busy)} onClick={() => void acceptWholeSide("current")} title={t("mergeConflict.fileActions.useFileTitle", { branch: currentLabel })}><span className="min-w-0 truncate">{t("mergeConflict.fileActions.useFile", { branch: currentLabel })}</span></Button>
									<Button className="max-w-56" variant="outline" size="xs" disabled={Boolean(busy)} onClick={() => void acceptWholeSide("incoming")} title={t("mergeConflict.fileActions.useFileTitle", { branch: incomingLabel })}><span className="min-w-0 truncate">{t("mergeConflict.fileActions.useFile", { branch: incomingLabel })}</span></Button>
									<Button className="gap-1.5" variant="destructive" size="xs" disabled={Boolean(busy)} onClick={() => void deleteResult()}><Trash2 className="size-3.5" />{t("mergeConflict.fileActions.deleteResult")}</Button>
								</div>

								{textUnavailable ? (
									<div className="flex flex-1 items-center justify-center p-8 text-center"><div className="max-w-md"><FileWarning className="mx-auto size-8 text-amber-500" /><h3 className="mt-3 font-semibold">{t("mergeConflict.unavailable.title")}</h3><p className="mt-2 text-sm text-muted-foreground">{selected.result.truncated ? t("mergeConflict.unavailable.tooLarge") : t("mergeConflict.unavailable.unsupported")} {t("mergeConflict.unavailable.instruction")}</p></div></div>
								) : (
									<>
										<div className="shrink-0 border-b border-border/60 bg-gradient-to-b from-muted/30 to-muted/10 p-4">
											{hunk ? (
												<>
											<div className="mb-2 flex items-center gap-2"><Badge variant="outline">{t("mergeConflict.hunk.position", { current: activeHunk + 1, total: hunks.length })}</Badge><div className="ml-auto flex gap-1"><Button variant="ghost" size="icon-xs" disabled={activeHunk === 0} onClick={() => setActiveHunk((value) => Math.max(0, value - 1))}><ChevronLeft /></Button><Button variant="ghost" size="icon-xs" disabled={activeHunk >= hunks.length - 1} onClick={() => setActiveHunk((value) => Math.min(hunks.length - 1, value + 1))}><ChevronRight /></Button></div></div>
											<div className="flex gap-3"><SidePreview label={currentLabel} text={hunk.currentText} tone="current" /><SidePreview label={incomingLabel} text={hunk.incomingText} tone="incoming" /></div>
											<div className="mt-2 flex flex-wrap gap-1.5"><Button className="max-w-56" size="xs" variant="outline" disabled={Boolean(busy)} onClick={() => applyHunk("current")} title={t("mergeConflict.hunk.accept", { branch: currentLabel })}><span className="min-w-0 truncate">{t("mergeConflict.hunk.accept", { branch: currentLabel })}</span></Button><Button className="max-w-56" size="xs" variant="outline" disabled={Boolean(busy)} onClick={() => applyHunk("incoming")} title={t("mergeConflict.hunk.accept", { branch: incomingLabel })}><span className="min-w-0 truncate">{t("mergeConflict.hunk.accept", { branch: incomingLabel })}</span></Button><Button size="xs" disabled={Boolean(busy)} onClick={() => applyHunk("both")}>{t("mergeConflict.hunk.acceptBoth")}</Button><Button className="gap-1.5" size="xs" variant="secondary" disabled={Boolean(busy)} onClick={() => void resolveWithAgent()}>{busy === "agent" ? <Loader2 className="size-3.5 animate-spin" /> : <Sparkles className="size-3.5" />}{t("mergeConflict.hunk.resolveWithAgent")}</Button></div>
										</>
									) : (
										<div className="flex flex-wrap items-center gap-2 text-xs text-emerald-600"><Check className="size-4" /><span className="mr-auto">{t("mergeConflict.hunk.nonePending")}</span><Button className="gap-1.5" size="xs" variant="secondary" disabled={Boolean(busy)} onClick={() => void resolveWithAgent()}>{busy === "agent" ? <Loader2 className="size-3.5 animate-spin" /> : <Sparkles className="size-3.5" />}{t("mergeConflict.agent.reviewFile")}</Button></div>
									)}
									{agentRun ? (
										<div className="mt-2 overflow-hidden rounded-md border border-sky-500/25 bg-sky-500/[0.04] text-[11px]">
											<div className="flex flex-wrap items-center gap-2 px-3 py-2">
												{agentRun.status === "analyzing" ? <Loader2 className="size-3.5 animate-spin text-sky-500" /> : agentRun.status === "cancelling" ? <Loader2 className="size-3.5 animate-spin text-amber-500" /> : agentRun.status === "completed" ? <Check className="size-3.5 text-emerald-500" /> : <AlertTriangle className="size-3.5 text-destructive" />}
												<span className="mr-auto font-semibold">{t(`mergeConflict.agent.run.${agentRun.status}`)}</span>
												{agentRun.sessionId ? <Button type="button" size="xs" variant="ghost" className="gap-1.5" onClick={openAgentSession}><ExternalLink className="size-3.5" />{t("mergeConflict.agent.viewSession")}</Button> : null}
												{agentRun.status === "analyzing" || agentRun.status === "cancelling" ? <Button type="button" size="xs" variant="outline" className="gap-1.5" disabled={agentRun.status === "cancelling"} onClick={cancelAgentRun}><Square className="size-3" />{t("mergeConflict.agent.cancel")}</Button> : null}
												<Button type="button" size="icon-xs" variant="ghost" aria-label={t(agentRun.expanded ? "mergeConflict.agent.collapse" : "mergeConflict.agent.expand")} onClick={() => setAgentRun((current) => current ? { ...current, expanded: !current.expanded } : current)}>{agentRun.expanded ? <ChevronUp /> : <ChevronDown />}</Button>
											</div>
											{agentRunDetails.pendingPermission || agentRunDetails.pendingInput ? (
												<div className="flex items-start gap-2 border-t border-amber-500/20 bg-amber-500/[0.08] px-3 py-2 text-amber-800 dark:text-amber-200"><AlertTriangle className="mt-0.5 size-3.5 shrink-0" /><span>{t(agentRunDetails.pendingPermission ? "mergeConflict.agent.permissionRequired" : "mergeConflict.agent.inputRequired")}</span></div>
											) : null}
											{agentRun.expanded ? (
												<div className="grid gap-2 border-t border-border/50 p-3 lg:grid-cols-2">
													<div className="min-w-0"><p className="mb-1 font-semibold text-foreground">{t("mergeConflict.agent.activity.title")}</p><div className="max-h-28 overflow-auto rounded border bg-background/60 px-2 py-1.5 text-muted-foreground">{agentRunDetails.activities.length > 0 ? agentRunDetails.activities.map((activity, index) => <p key={`${index}-${activity}`} className="truncate" title={activity}>• {activity}</p>) : <p>{t("mergeConflict.agent.activity.waiting")}</p>}</div></div>
													<div className="min-w-0"><p className="mb-1 font-semibold text-foreground">{t("mergeConflict.agent.liveOutput")}</p><pre className="max-h-28 overflow-auto whitespace-pre-wrap break-words rounded border bg-background/60 px-2 py-1.5 font-mono text-[10px] text-muted-foreground">{agentRunDetails.output || t("mergeConflict.agent.waitingOutput")}</pre></div>
												</div>
											) : null}
											{agentRun.error ? <div className="border-t border-destructive/20 bg-destructive/[0.06] px-3 py-2 text-destructive">{agentRun.error}</div> : null}
										</div>
									) : null}
									{agentSuggestion?.path === selected.path ? (
										<div className="mt-2 rounded-md border border-violet-500/25 bg-violet-500/5 px-3 py-2 text-[11px] leading-5">
											<div className="flex items-center gap-1.5 font-semibold text-violet-700 dark:text-violet-300"><Sparkles className="size-3.5" />{t(agentSuggestion.scope === "hunk" ? "mergeConflict.agent.suggestionAppliedHunk" : "mergeConflict.agent.suggestionAppliedFile")}</div>
											<p className="mt-1 text-muted-foreground">{agentSuggestion.explanation}</p>
							<p className="mt-1 font-medium text-foreground" title={t("mergeConflict.agent.sessionLabel", { sessionId: agentSuggestion.sessionId })}>{t("mergeConflict.agent.reviewNotice")}</p>
										</div>
									) : null}
								</div>
								<div className="flex shrink-0 items-center gap-2 border-b border-border/50 bg-muted/[0.08] px-4 py-1.5 text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground"><span className={cn("size-1.5 rounded-full", suspiciousEmptyResult ? "bg-destructive" : "bg-emerald-500")} />{t("mergeConflict.result.title")}</div>
								{suspiciousEmptyResult ? <div className="flex shrink-0 items-start gap-2 border-b border-destructive/20 bg-destructive/[0.06] px-4 py-2 text-[11px] text-destructive"><AlertTriangle className="mt-0.5 size-3.5 shrink-0" /><span>{t("mergeConflict.result.emptyWarning")}</span></div> : null}
								<div className="min-h-0 flex-1"><WorkspaceFileEditor key={selected.path} ref={editorRef} path={selected.path} content={buffer} readOnly={busy === "agent"} annotateLabel="" onChange={() => setBuffer(editorRef.current?.getValue() ?? "")} /></div>
										<div className="flex shrink-0 items-center justify-between gap-3 border-t border-border/60 bg-background/95 px-4 py-2.5 shadow-[0_-8px_24px_-20px_rgba(0,0,0,0.6)]"><div className="min-w-0"><span className="block text-[11px] text-muted-foreground">{hunks.length > 0 ? t("mergeConflict.hunk.pending", { count: hunks.length }) : dirty ? t("mergeConflict.result.changed") : t("mergeConflict.result.ready")}</span><span className={cn("block text-[10px]", suspiciousEmptyResult ? "text-destructive" : "text-muted-foreground")}>{t("mergeConflict.result.stats", { lines: resultLineCount, characters: buffer.length })}</span></div><Button className="shrink-0 gap-1.5" size="sm" title={t("mergeConflict.result.saveTitle")} disabled={Boolean(busy) || hunks.length > 0} onClick={() => void saveAndResolve()}>{busy === "save" ? <Loader2 className="size-4 animate-spin" /> : <Check className="size-4" />}{t("mergeConflict.result.save")}</Button></div>
									</>
								)}
							</section>
						) : null}
					</div>
				)}
		</div>
	);
}
