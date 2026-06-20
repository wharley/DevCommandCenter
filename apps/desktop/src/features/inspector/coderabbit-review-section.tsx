import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
	AlertCircle,
	CheckCircle2,
	ChevronRight,
	Clock3,
	History,
	LoaderCircle,
	Rabbit,
	RefreshCw,
	Send,
	ShieldCheck,
	Trash2,
	X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type {
	CodeRabbitFinding,
	CodeRabbitFindingSeverity,
	CodeRabbitReviewErrorKind,
	CodeRabbitReviewType,
	WorkspaceGitChangeEntry,
} from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { CodeRabbitConnectDialog } from "@/features/settings/coderabbit-connect-dialog";
import {
	invalidateCodeRabbitCliQueries,
	useCodeRabbitCliStatus,
} from "@/features/settings/coderabbit-cli-queries";
import {
	workspaceCodeRabbitDiffFingerprint,
	workspaceCodeRabbitReviewCancel,
	workspaceCodeRabbitReviewJob,
	listenCodeRabbitReviewEvents,
	workspaceCodeRabbitReviewStart,
} from "@/lib/workspace-api";
import { cn } from "@/lib/utils";
import {
	buildCodeRabbitComposerPrompt,
	buildMachineAnnotationsForPath,
	findingTitle,
	lineLabel,
} from "./coderabbit-review-format";
import type { WorkspaceGitPreviewSelection } from "./workspace-git-file-preview";
import { useStoredCodeRabbitReview } from "./use-workspace-coderabbit-review";

type CodeRabbitReviewSectionProps = {
	workspaceRoot: string;
	staged: WorkspaceGitChangeEntry[];
	unstaged: WorkspaceGitChangeEntry[];
	baseBranch?: string | null;
	onSelectPreview: (selection: WorkspaceGitPreviewSelection) => void;
	onPrefillComposer?: (text: string) => void;
};

const REVIEW_TYPES: CodeRabbitReviewType[] = ["all", "uncommitted", "committed"];
const PRIVACY_CONSENT_VERSION = "v1";

const SEVERITY_ORDER: CodeRabbitFindingSeverity[] = [
	"critical",
	"major",
	"minor",
	"trivial",
	"info",
	"unknown",
];

const SEVERITY_CLASS: Record<CodeRabbitFindingSeverity, string> = {
	critical: "text-destructive border-destructive/40 bg-destructive/10",
	major: "text-amber-700 border-amber-500/40 bg-amber-500/10 dark:text-amber-300",
	minor: "text-sky-700 border-sky-500/40 bg-sky-500/10 dark:text-sky-300",
	trivial: "text-muted-foreground border-border/60 bg-muted/30",
	info: "text-muted-foreground border-border/60 bg-muted/30",
	unknown: "text-muted-foreground border-border/60 bg-muted/30",
};

const ERROR_ACTION: Partial<
	Record<CodeRabbitReviewErrorKind, "connect" | "rerun">
> = {
	auth: "connect",
	cli_unavailable: "connect",
	permission: "connect",
	network: "rerun",
	rate_limit: "rerun",
	timeout: "rerun",
	unknown: "rerun",
};

function fileName(path: string): string {
	const index = path.lastIndexOf("/");
	return index >= 0 ? path.slice(index + 1) : path;
}

function formatReviewAge(value?: string | null): string | null {
	if (!value) {
		return null;
	}
	const timestamp = Date.parse(value);
	if (Number.isNaN(timestamp)) {
		return null;
	}
	const seconds = Math.max(0, Math.floor((Date.now() - timestamp) / 1000));
	if (seconds < 60) {
		return `${seconds}s`;
	}
	const minutes = Math.floor(seconds / 60);
	if (minutes < 60) {
		return `${minutes}m`;
	}
	const hours = Math.floor(minutes / 60);
	if (hours < 48) {
		return `${hours}h`;
	}
	return `${Math.floor(hours / 24)}d`;
}

function privacyConsentKey(workspaceRoot: string): string {
	return `dcc.workspace.coderabbit.privacy.${PRIVACY_CONSENT_VERSION}.${encodeURIComponent(workspaceRoot)}`;
}

function loadPrivacyConsent(workspaceRoot: string): boolean {
	if (typeof window === "undefined" || !workspaceRoot) {
		return false;
	}
	return window.localStorage.getItem(privacyConsentKey(workspaceRoot)) === "accepted";
}

function savePrivacyConsent(workspaceRoot: string) {
	if (typeof window === "undefined" || !workspaceRoot) {
		return;
	}
	window.localStorage.setItem(privacyConsentKey(workspaceRoot), "accepted");
}

function buildSelectionForFinding(
	finding: CodeRabbitFinding,
	allFindings: CodeRabbitFinding[],
	staged: WorkspaceGitChangeEntry[],
	unstaged: WorkspaceGitChangeEntry[],
	baseBranch?: string | null,
): WorkspaceGitPreviewSelection {
	const machineAnnotations = buildMachineAnnotationsForPath(
		finding.path,
		allFindings,
	);
	const focusLine = finding.startLine ?? null;
	const stagedEntry = staged.find((entry) => entry.path === finding.path);
	if (stagedEntry) {
		return {
			group: "staged",
			path: finding.path,
			name: stagedEntry.name,
			status: stagedEntry.status,
			baseBranch: null,
			focusLine,
			machineAnnotations,
		};
	}
	const unstagedEntry = unstaged.find((entry) => entry.path === finding.path);
	if (unstagedEntry) {
		return {
			group: "unstaged",
			path: finding.path,
			name: unstagedEntry.name,
			status: unstagedEntry.status,
			baseBranch: null,
			focusLine,
			machineAnnotations,
		};
	}
	return {
		group: "committed",
		path: finding.path,
		name: fileName(finding.path),
		status: "M",
		baseBranch: baseBranch ?? null,
		focusLine,
		machineAnnotations,
	};
}

export function CodeRabbitReviewSection({
	workspaceRoot,
	staged,
	unstaged,
	baseBranch,
	onSelectPreview,
	onPrefillComposer,
}: CodeRabbitReviewSectionProps) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [open, setOpen] = useState(true);
	const [reviewType, setReviewType] = useState<CodeRabbitReviewType>("all");
	const [connectOpen, setConnectOpen] = useState(false);
	const [historyOpen, setHistoryOpen] = useState(false);
	const [privacyAccepted, setPrivacyAccepted] = useState(() =>
		loadPrivacyConsent(workspaceRoot),
	);
	const [selectedFindingIds, setSelectedFindingIds] = useState<Set<string>>(
		() => new Set(),
	);
	const [activeJobId, setActiveJobId] = useState<string | null>(null);
	const [liveJobMessage, setLiveJobMessage] = useState<string | null>(null);
	const {
		review,
		saveReview,
		clearReview,
		history,
		historyLoading,
		useReviewFromHistory,
	} = useStoredCodeRabbitReview(workspaceRoot);
	const codeRabbitStatusQuery = useCodeRabbitCliStatus(workspaceRoot, {
		includeAuthStatus: true,
	});
	const codeRabbitReady = Boolean(
		codeRabbitStatusQuery.data?.installed &&
			(codeRabbitStatusQuery.data.auth?.success ||
				codeRabbitStatusQuery.data.auth?.authenticated),
	);
	const codeRabbitMessage =
		codeRabbitStatusQuery.data?.auth?.message ??
		codeRabbitStatusQuery.data?.message ??
		t("inspector.codeRabbit.checking");

	const fingerprintQuery = useQuery({
		queryKey: [
			"workspaceCodeRabbitDiffFingerprint",
			workspaceRoot,
			review?.reviewType ?? reviewType,
			baseBranch ?? null,
			review?.fingerprint?.combinedHash ?? null,
		],
		queryFn: () =>
			workspaceCodeRabbitDiffFingerprint({
				workspaceRoot,
				reviewType: review?.reviewType ?? reviewType,
				base: baseBranch ?? null,
				baseCommit: null,
			}),
		enabled: Boolean(workspaceRoot && review),
		staleTime: 8_000,
		refetchOnWindowFocus: true,
	});

	const isStale = Boolean(
		review &&
			fingerprintQuery.data &&
			fingerprintQuery.data.combinedHash !== review.fingerprint.combinedHash,
	);

	const reviewJobQuery = useQuery({
		queryKey: ["workspaceCodeRabbitReviewJob", activeJobId],
		queryFn: () => workspaceCodeRabbitReviewJob({ jobId: activeJobId! }),
		enabled: Boolean(activeJobId),
		refetchInterval: (query) => {
			const status = query.state.data?.status;
			return status === "starting" || status === "running" ? 2_500 : false;
		},
		refetchOnWindowFocus: true,
	});
	const activeJob = reviewJobQuery.data ?? null;

	const startReview = useMutation({
		mutationFn: () =>
			workspaceCodeRabbitReviewStart({
				workspaceRoot,
				reviewType,
				base: baseBranch ?? null,
				baseCommit: null,
				light: null,
				configPaths: [],
				cliPath: null,
				timeoutSeconds: null,
			}),
		onSuccess: (next) => {
			setActiveJobId(next.jobId);
			setLiveJobMessage(null);
			toast.info(t("inspector.codeRabbit.reviewStarted"));
		},
		onError: (error) => {
			toast.error(
				error instanceof Error ? error.message : t("inspector.codeRabbit.reviewFailed"),
			);
		},
	});

	const cancelReview = useMutation({
		mutationFn: () => {
			if (!activeJobId) {
				throw new Error("No active CodeRabbit review job");
			}
			return workspaceCodeRabbitReviewCancel({ jobId: activeJobId });
		},
		onSuccess: (snapshot) => {
			if (snapshot.status === "canceled") {
				setActiveJobId(null);
				toast.info(t("inspector.codeRabbit.reviewCanceled"));
			}
		},
		onError: (error) => {
			toast.error(
				error instanceof Error ? error.message : t("inspector.codeRabbit.cancelFailed"),
			);
		},
	});

	const reviewRunning = Boolean(
		startReview.isPending ||
			activeJob?.status === "starting" ||
			activeJob?.status === "running",
	);

	const formatReviewError = useCallback(
		(kind?: CodeRabbitReviewErrorKind | null, fallback?: string | null) => {
			if (kind) {
				return t(`inspector.codeRabbit.errors.${kind}.description`);
			}
			return fallback ?? t("inspector.codeRabbit.reviewFailed");
		},
		[t],
	);

	useEffect(() => {
		setPrivacyAccepted(loadPrivacyConsent(workspaceRoot));
	}, [workspaceRoot]);

	useEffect(() => {
		if (!activeJobId) {
			setLiveJobMessage(null);
			return;
		}
		let disposed = false;
		let cleanup: (() => void) | null = null;
		void listenCodeRabbitReviewEvents((event) => {
			if (disposed || event.jobId !== activeJobId) {
				return;
			}
			if (event.message || event.status) {
				setLiveJobMessage(event.message ?? event.status ?? null);
			}
			if (
				event.eventType === "finding" ||
				event.eventType === "complete" ||
				event.eventType === "succeeded" ||
				event.eventType === "failed" ||
				event.eventType === "canceled"
			) {
				void reviewJobQuery.refetch();
			}
		})
			.then((unlisten) => {
				if (disposed) {
					void unlisten();
					return;
				}
				cleanup = unlisten;
			})
			.catch(() => {
				/* polling remains the fallback */
			});
		return () => {
			disposed = true;
			cleanup?.();
		};
	}, [activeJobId, reviewJobQuery]);

	useEffect(() => {
		if (!activeJob) {
			return;
		}
		if (activeJob.status === "succeeded" && activeJob.result) {
			saveReview(activeJob.result);
			setActiveJobId(null);
			toast.success(
				t("inspector.codeRabbit.reviewComplete", {
					count: activeJob.result.findings.length,
				}),
			);
			return;
		}
		if (activeJob.status === "failed") {
			if (activeJob.result) {
				saveReview(activeJob.result);
			}
			setActiveJobId(null);
			toast.error(
				formatReviewError(
					activeJob.errorKind ?? activeJob.result?.errorKind,
					activeJob.errors[0] ?? activeJob.result?.errors[0],
				),
			);
			return;
		}
		if (activeJob.status === "canceled") {
			setActiveJobId(null);
			toast.info(t("inspector.codeRabbit.reviewCanceled"));
		}
	}, [activeJob, formatReviewError, saveReview, t]);

	const groupedFindings = useMemo(() => {
		const grouped = new Map<CodeRabbitFindingSeverity, CodeRabbitFinding[]>();
		for (const severity of SEVERITY_ORDER) {
			grouped.set(severity, []);
		}
		for (const finding of review?.findings ?? []) {
			grouped.get(finding.severity)?.push(finding);
		}
		return SEVERITY_ORDER.map((severity) => ({
			severity,
			findings: grouped.get(severity) ?? [],
		})).filter((group) => group.findings.length > 0);
	}, [review?.findings]);

	useEffect(() => {
		const allowed = new Set((review?.findings ?? []).map((finding) => finding.id));
		setSelectedFindingIds((previous) => {
			const next = new Set<string>();
			for (const id of previous) {
				if (allowed.has(id)) {
					next.add(id);
				}
			}
			return next.size === previous.size ? previous : next;
		});
	}, [review?.findings]);

	const selectedFindings = useMemo(() => {
		return (review?.findings ?? []).filter((finding) =>
			selectedFindingIds.has(finding.id),
		);
	}, [review?.findings, selectedFindingIds]);

	const toggleFindingSelection = useCallback((findingId: string) => {
		setSelectedFindingIds((previous) => {
			const next = new Set(previous);
			if (next.has(findingId)) {
				next.delete(findingId);
			} else {
				next.add(findingId);
			}
			return next;
		});
	}, []);

	const selectAllFindings = useCallback(() => {
		setSelectedFindingIds(
			new Set((review?.findings ?? []).map((finding) => finding.id)),
		);
	}, [review?.findings]);

	const clearSelectedFindings = useCallback(() => {
		setSelectedFindingIds(new Set());
	}, []);

	const reviewAge = formatReviewAge(review?.completedAt);
	const canRun =
		!reviewRunning && Boolean(workspaceRoot) && codeRabbitReady && privacyAccepted;
	const canSendToComposer = Boolean(onPrefillComposer && selectedFindings.length > 0);
	const activeJobMessage = activeJob?.cancelRequested
		? t("inspector.codeRabbit.cancelingStatus")
		: activeJob?.status === "starting"
			? t("inspector.codeRabbit.starting")
			: liveJobMessage ?? t("inspector.codeRabbit.running");
	const reviewScopeLabel = review
		? t(`inspector.codeRabbit.reviewType.${review.reviewType}`)
		: null;
	const reviewErrorKind = review?.errorKind ?? null;
	const reviewErrorAction = reviewErrorKind ? ERROR_ACTION[reviewErrorKind] : undefined;

	const acceptPrivacy = useCallback(() => {
		savePrivacyConsent(workspaceRoot);
		setPrivacyAccepted(true);
		toast.success(t("inspector.codeRabbit.privacyAccepted"));
	}, [t, workspaceRoot]);

	const sendSelectedToComposer = useCallback(() => {
		if (!onPrefillComposer || selectedFindings.length === 0 || !review) {
			return;
		}
		onPrefillComposer(
			buildCodeRabbitComposerPrompt({
				findings: selectedFindings,
				reviewType: review.reviewType,
				completedAt: review.completedAt,
				isStale,
			}),
		);
		toast.success(
			t("inspector.codeRabbit.sentToComposer", {
				count: selectedFindings.length,
			}),
		);
		clearSelectedFindings();
	}, [
		clearSelectedFindings,
		isStale,
		onPrefillComposer,
		review,
		selectedFindings,
		t,
	]);

	return (
		<div className="border-t border-border/50 bg-background/70 font-sans">
			<div className="flex items-center gap-1 px-2 py-1.5 text-[11.5px]">
				<Button
					type="button"
					variant="ghost"
					size="xs"
					onClick={() => setOpen((value) => !value)}
					aria-expanded={open}
					className="h-auto min-w-0 flex-1 justify-start gap-1 rounded-none px-0 text-left font-semibold text-muted-foreground hover:bg-transparent hover:text-foreground"
				>
					<ChevronRight
						className={cn("size-3 shrink-0 transition-transform", open && "rotate-90")}
						strokeWidth={2}
						aria-hidden
					/>
					<Rabbit className="size-3 shrink-0" strokeWidth={1.8} />
					<span className="truncate">{t("inspector.codeRabbit.reviewTitle")}</span>
				</Button>
				{review ? (
					<Badge variant={isStale ? "outline" : "secondary"} className="h-5 px-1.5 text-[10px] font-normal">
						{isStale
							? t("inspector.codeRabbit.staleBadge")
							: t("inspector.codeRabbit.findingsCount", {
									count: review.findings.length,
								})}
					</Badge>
				) : null}
			</div>
			{open ? (
				<div className="space-y-2 px-3 pb-3">
					<div className="flex flex-wrap items-center justify-between gap-2">
						<ToggleGroup
							type="single"
							value={reviewType}
							onValueChange={(value) => {
								if (REVIEW_TYPES.includes(value as CodeRabbitReviewType)) {
									setReviewType(value as CodeRabbitReviewType);
								}
							}}
							className="h-7 justify-start rounded-md border border-border/50 bg-muted/20 p-0.5"
						>
							{REVIEW_TYPES.map((type) => (
								<ToggleGroupItem
									key={type}
									value={type}
									disabled={reviewRunning}
									className="h-6 rounded px-2 text-[10.5px] data-[state=on]:bg-background data-[state=on]:text-foreground"
								>
									{t(`inspector.codeRabbit.reviewType.${type}`)}
								</ToggleGroupItem>
							))}
						</ToggleGroup>
						<div className="flex items-center gap-1.5">
							{reviewAge ? (
								<span className="inline-flex items-center gap-1 text-[10.5px] text-muted-foreground">
									<Clock3 className="size-3" />
									{t("inspector.codeRabbit.reviewAge", { age: reviewAge })}
								</span>
							) : null}
							{reviewScopeLabel ? (
								<span className="rounded bg-muted px-1.5 py-0.5 text-[10.5px] text-muted-foreground">
									{t("inspector.codeRabbit.reviewScope", {
										scope: reviewScopeLabel,
									})}
								</span>
							) : null}
							{review ? (
								<Button
									type="button"
									variant="ghost"
									size="icon-xs"
									aria-label={t("inspector.codeRabbit.clear")}
									onClick={clearReview}
								>
									<Trash2 className="size-3.5" />
								</Button>
							) : null}
							{history.length > 0 ? (
								<Button
									type="button"
									variant="ghost"
									size="icon-xs"
									aria-label={t("inspector.codeRabbit.history")}
									onClick={() => setHistoryOpen((value) => !value)}
								>
									<History className="size-3.5" />
								</Button>
							) : null}
							<Button
								type="button"
								variant={review ? "outline" : "default"}
								size="xs"
								disabled={!canRun}
								onClick={() => startReview.mutate()}
								className="gap-1.5"
							>
								{reviewRunning ? (
									<LoaderCircle className="size-3.5 animate-spin" />
								) : (
									<RefreshCw className="size-3.5" />
								)}
								{review
									? t("inspector.codeRabbit.rerun")
									: t("inspector.codeRabbit.run")}
							</Button>
							{reviewRunning ? (
								<Button
									type="button"
									variant="outline"
									size="xs"
									disabled={cancelReview.isPending}
									onClick={() => cancelReview.mutate()}
									className="gap-1.5"
								>
									<X className="size-3.5" />
									{cancelReview.isPending
										? t("inspector.codeRabbit.canceling")
										: t("inspector.codeRabbit.cancel")}
								</Button>
							) : null}
						</div>
					</div>

					{isStale ? (
						<div className="flex items-start gap-2 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-1.5 text-[11px] text-amber-700 dark:text-amber-300">
							<AlertCircle className="mt-0.5 size-3.5 shrink-0" />
							<span>{t("inspector.codeRabbit.staleMessage")}</span>
						</div>
					) : null}

					{codeRabbitReady && !privacyAccepted && !reviewRunning ? (
						<div className="flex items-start justify-between gap-2 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-2 text-[11px] text-amber-800 dark:text-amber-200">
							<div className="flex min-w-0 gap-2">
								<ShieldCheck className="mt-0.5 size-3.5 shrink-0" />
								<div className="min-w-0">
									<div className="font-medium">
										{t("inspector.codeRabbit.privacyTitle")}
									</div>
									<div className="mt-0.5 leading-snug text-amber-700 dark:text-amber-300">
										{t("inspector.codeRabbit.privacyDescription")}
									</div>
								</div>
							</div>
							<Button
								type="button"
								variant="outline"
								size="xs"
								onClick={acceptPrivacy}
								className="shrink-0 border-amber-500/40 bg-background/70 text-amber-800 hover:bg-background dark:text-amber-200"
							>
								{t("inspector.codeRabbit.privacyAccept")}
							</Button>
						</div>
					) : null}

					{historyOpen ? (
						<div className="space-y-1 rounded-md border border-border/45 bg-muted/15 p-1.5">
							<div className="flex items-center justify-between px-1 text-[10.5px] font-semibold uppercase tracking-[0.06em] text-muted-foreground">
								<span>{t("inspector.codeRabbit.history")}</span>
								<span className="tabular-nums">
									{historyLoading
										? t("inspector.codeRabbit.historyLoading")
										: t("inspector.codeRabbit.historyCount", {
												count: history.length,
											})}
								</span>
							</div>
							{history.length === 0 ? (
								<div className="px-1 py-1 text-[11px] text-muted-foreground">
									{t("inspector.codeRabbit.historyEmpty")}
								</div>
							) : (
								<div className="max-h-40 space-y-1 overflow-y-auto">
									{history.map((entry) => (
										<button
											key={entry.reviewId}
											type="button"
											className="flex w-full min-w-0 items-center justify-between gap-2 rounded border border-border/35 bg-background/70 px-2 py-1.5 text-left text-[11px] transition-colors hover:bg-accent/60"
											onClick={() => {
												useReviewFromHistory(entry.review);
												setHistoryOpen(false);
											}}
										>
											<span className="min-w-0 flex-1 truncate">
												{t("inspector.codeRabbit.historyEntry", {
													type:
														t(
															`inspector.codeRabbit.reviewType.${entry.review.reviewType}`,
														),
													count: entry.findingsCount,
												})}
											</span>
											<span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
												{formatReviewAge(entry.savedAt) ?? "-"}
											</span>
										</button>
									))}
								</div>
							)}
						</div>
					) : null}

					{reviewRunning ? (
						<div className="flex items-center justify-between gap-2 rounded-md border border-border/50 bg-muted/20 px-2 py-2 text-[11px] text-muted-foreground">
							<span className="inline-flex min-w-0 items-center gap-2">
							<LoaderCircle className="size-3.5 animate-spin" />
								<span className="truncate">
									{activeJobMessage}
								</span>
							</span>
							{activeJob?.jobId ? (
								<span className="shrink-0 font-mono text-[10px]">
									{activeJob.jobId.slice(-8)}
								</span>
							) : null}
						</div>
					) : null}

					{!codeRabbitReady && !reviewRunning ? (
						<div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border/50 bg-muted/20 px-2 py-2 text-[11px] text-muted-foreground">
							<span>{codeRabbitMessage}</span>
							<Button
								type="button"
								variant="outline"
								size="xs"
								onClick={() => setConnectOpen(true)}
							>
								{t("settings.codeRabbit.connect")}
							</Button>
						</div>
					) : null}

					{review?.errorKind || review?.errors.length ? (
						<div className="space-y-1.5 rounded-md border border-destructive/30 bg-destructive/10 px-2 py-1.5 text-[11px] text-destructive">
							<div className="flex items-start justify-between gap-2">
								<div className="min-w-0">
									<div className="font-medium">
										{reviewErrorKind
											? t(`inspector.codeRabbit.errors.${reviewErrorKind}.title`)
											: t("inspector.codeRabbit.reviewFailed")}
									</div>
									<div className="mt-0.5 leading-snug">
										{formatReviewError(reviewErrorKind, review.errors[0])}
									</div>
								</div>
								{reviewErrorAction === "connect" ? (
									<Button
										type="button"
										variant="outline"
										size="xs"
										onClick={() => setConnectOpen(true)}
										className="shrink-0 border-destructive/35 bg-background/70 text-destructive hover:bg-background"
									>
										{t("inspector.codeRabbit.errorActionConnect")}
									</Button>
								) : null}
								{reviewErrorAction === "rerun" ? (
									<Button
										type="button"
										variant="outline"
										size="xs"
										disabled={!canRun}
										onClick={() => startReview.mutate()}
										className="shrink-0 border-destructive/35 bg-background/70 text-destructive hover:bg-background"
									>
										{t("inspector.codeRabbit.errorActionRerun")}
									</Button>
								) : null}
							</div>
							{review.errors[0] ? (
								<div className="line-clamp-2 border-t border-destructive/20 pt-1 font-mono text-[10px] opacity-80">
									{review.errors[0]}
								</div>
							) : null}
						</div>
					) : null}

					{review && review.findings.length === 0 && !review.errors.length ? (
						<div className="flex items-center gap-2 rounded-md border border-border/50 bg-muted/20 px-2 py-2 text-[11px] text-muted-foreground">
							<CheckCircle2 className="size-3.5 text-emerald-600 dark:text-emerald-400" />
							{t("inspector.codeRabbit.emptyFindings")}
						</div>
					) : null}

					{groupedFindings.length > 0 ? (
						<div className="space-y-2">
							<div className="flex flex-wrap items-center justify-between gap-1.5 rounded-md border border-border/45 bg-muted/20 px-2 py-1.5 text-[11px] text-muted-foreground">
								<span className="tabular-nums">
									{t("inspector.codeRabbit.selectedCount", {
										count: selectedFindings.length,
									})}
								</span>
								<div className="flex items-center gap-1">
									<Button
										type="button"
										variant="ghost"
										size="xs"
										onClick={selectAllFindings}
										disabled={!review?.findings.length}
										className="h-6 px-1.5 text-[10.5px]"
									>
										{t("inspector.codeRabbit.selectAll")}
									</Button>
									<Button
										type="button"
										variant="ghost"
										size="xs"
										onClick={clearSelectedFindings}
										disabled={selectedFindings.length === 0}
										className="h-6 px-1.5 text-[10.5px]"
									>
										{t("inspector.codeRabbit.clearSelection")}
									</Button>
									<Button
										type="button"
										variant="default"
										size="xs"
										onClick={sendSelectedToComposer}
										disabled={!canSendToComposer}
										className="h-6 gap-1 px-1.5 text-[10.5px]"
									>
										<Send className="size-3" />
										{t("inspector.codeRabbit.sendToComposer")}
									</Button>
								</div>
							</div>
							{groupedFindings.map((group) => (
								<div key={group.severity} className="space-y-1">
									<div className="flex items-center gap-2 text-[10.5px] font-semibold uppercase tracking-[0.06em] text-muted-foreground">
										<span
											className={cn(
												"size-2 rounded-full border",
												SEVERITY_CLASS[group.severity],
											)}
										/>
										{t(`inspector.codeRabbit.severity.${group.severity}`)}
										<span className="tabular-nums">{group.findings.length}</span>
									</div>
									<div className="space-y-1">
										{group.findings.map((finding) => {
											const selected = selectedFindingIds.has(finding.id);
											return (
												<div
													key={finding.id}
													role="button"
													tabIndex={0}
													className={cn(
														"flex w-full items-start gap-2 rounded-md border border-border/45 bg-background/80 px-2 py-1.5 text-left text-[11px] transition-colors hover:bg-accent/60",
														selected && "border-primary/45 bg-primary/5",
													)}
													onClick={() =>
														onSelectPreview(
															buildSelectionForFinding(
																finding,
																review?.findings ?? [],
																staged,
																unstaged,
																baseBranch,
															),
														)
													}
													onKeyDown={(event) => {
														if (event.key === "Enter" || event.key === " ") {
															event.preventDefault();
															onSelectPreview(
																buildSelectionForFinding(
																	finding,
																	review?.findings ?? [],
																	staged,
																	unstaged,
																	baseBranch,
																),
															);
														}
													}}
												>
													<input
														type="checkbox"
														checked={selected}
														aria-label={t("inspector.codeRabbit.selectFinding", {
															path: finding.path,
														})}
														onClick={(event) => event.stopPropagation()}
														onChange={() => toggleFindingSelection(finding.id)}
														className="mt-0.5 size-3.5 shrink-0 accent-primary"
													/>
													<div className="min-w-0 flex-1">
														<div className="flex min-w-0 items-center gap-1.5">
															<span className="min-w-0 flex-1 truncate font-mono text-foreground">
																{finding.path}
															</span>
															{lineLabel(finding) ? (
																<span className="shrink-0 rounded bg-muted px-1 py-0.5 font-mono text-[10px] text-muted-foreground">
																	{lineLabel(finding)}
																</span>
															) : null}
														</div>
														<p className="mt-1 line-clamp-2 text-[11px] leading-snug text-muted-foreground">
															{findingTitle(finding)}
														</p>
													</div>
												</div>
											);
										})}
									</div>
								</div>
							))}
						</div>
					) : null}
				</div>
			) : null}
			<CodeRabbitConnectDialog
				open={connectOpen}
				onOpenChange={setConnectOpen}
				workspaceRoot={workspaceRoot}
				onConnected={() => {
					void invalidateCodeRabbitCliQueries(queryClient, workspaceRoot);
				}}
			/>
		</div>
	);
}
