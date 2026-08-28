import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { PrepareGuardedUndoOutput } from "@dcc/contracts";
import {
	AlertTriangle,
	CheckCircle2,
	FileDiff,
	Loader2,
	RotateCcw,
	ShieldAlert,
	X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import {
	executeGuardedUndo,
	loadLastTurnReview,
	loadTurnReviewFileDiff,
	prepareGuardedUndo,
} from "@/lib/session-api";
import { cn } from "@/lib/utils";
import {
	canPrepareGuardedUndo,
	isGuardedUndoPreviewExpired,
	reconcileTurnReviewSelection,
	resolveGuardedUndoCapture,
	resolveGuardedUndoFailureReason,
	resolveTurnReviewOutcome,
	resolveTurnReviewPreviewState,
} from "./turn-review.logic";
import { lastTurnReviewQueryKey } from "./turn-review-query";

export function TurnReviewSurface({
	sessionId,
	workspaceId,
	onClose,
}: {
	sessionId: string;
	workspaceId: string;
	onClose: () => void;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const reviewQuery = useQuery({
		queryKey: lastTurnReviewQueryKey(sessionId, workspaceId),
		queryFn: () => loadLastTurnReview(sessionId, workspaceId),
	});
	const review = reviewQuery.data ?? null;
	const [selectedPath, setSelectedPath] = useState<string | null>(null);
	useEffect(() => {
		setSelectedPath((current) =>
			reconcileTurnReviewSelection(current, review?.files ?? []),
		);
	}, [review?.files, review?.state, review?.snapshotId]);
	const selectedFile =
		review?.files.find((file) => file.path === selectedPath) ?? null;
	const diffEnabled = Boolean(
		review?.snapshotId &&
			selectedPath &&
			selectedFile &&
			!selectedFile.previewUnavailable,
	);
	const diffQuery = useQuery({
		queryKey: ["turnReviewFileDiff", review?.snapshotId, selectedPath],
		queryFn: () => loadTurnReviewFileDiff(review!.snapshotId, selectedPath!),
		enabled: diffEnabled,
	});
	const previewState = resolveTurnReviewPreviewState({
		selectedPath,
		isFetching: diffEnabled && diffQuery.isFetching,
		isError: diffEnabled && diffQuery.isError,
		diff: diffQuery.data?.diff,
	});
	const stateLabel = review ? t(`turnReview.states.${review.state}`) : "";
	const compatibilityLabel = review
		? t(`turnReview.compatibility.${review.compatibility}`)
		: "";
	const outcome = resolveTurnReviewOutcome(
		review?.turnOutcome,
		review?.outcomeReason,
	);
	const guardedUndoCapture = review?.guardedUndo ?? null;
	const activeUndo = review?.activeUndo ?? null;
	const guardedUndo = resolveGuardedUndoCapture(guardedUndoCapture);
	const undoAvailable = canPrepareGuardedUndo(guardedUndoCapture, activeUndo);
	const [undoDialogOpen, setUndoDialogOpen] = useState(false);
	const [preparedUndo, setPreparedUndo] =
		useState<Extract<PrepareGuardedUndoOutput, { status: "ready" }> | null>(null);
	const [undoOutcome, setUndoOutcome] = useState<{
		status: "completed" | "blocked" | "rolled_back" | "recovery_required";
		reasonCode?: string | null;
		operationId?: string | null;
	} | null>(null);
	const [prepareProblem, setPrepareProblem] = useState<{
		status: "blocked" | "unavailable";
		reasonCode: string;
	} | null>(null);
	const [previewExpired, setPreviewExpired] = useState(false);
	const prepareUndoMutation = useMutation({
		mutationFn: prepareGuardedUndo,
		onSuccess: (result) => {
			setUndoOutcome(null);
			setPrepareProblem(null);
			if (result.status === "ready" && result.unrelatedPathsAreNotTargets) {
				setPreparedUndo(result);
				setPreviewExpired(isGuardedUndoPreviewExpired(result.expiresAt));
				setUndoDialogOpen(true);
				return;
			}
			setPreparedUndo(null);
			if (result.status === "ready") {
				setPrepareProblem({
					status: "unavailable",
					reasonCode: "invalid_persisted_record",
				});
				return;
			}
			setPrepareProblem({
				status: result.status,
				reasonCode: result.reasonCode,
			});
		},
	});
	const refreshUndoQueries = async () => {
		await Promise.all([
			queryClient.invalidateQueries({
				queryKey: lastTurnReviewQueryKey(sessionId, workspaceId),
			}),
			queryClient.invalidateQueries({
				predicate: (query) =>
					[
						"workspaceGitStatus",
						"workspaceGitBranchDiff",
						"workspaceGitFilePreviewContent",
					].includes(String(query.queryKey[0] ?? "")),
			}),
		]);
	};
	const executeUndoMutation = useMutation({
		mutationFn: executeGuardedUndo,
		onSuccess: (result) => {
			setUndoOutcome(result);
		},
		onSettled: refreshUndoQueries,
	});
	useEffect(() => {
		if (!preparedUndo || !undoDialogOpen) return;
		const expiresAt = Date.parse(preparedUndo.expiresAt);
		if (!Number.isFinite(expiresAt)) {
			setPreviewExpired(true);
			return;
		}
		const remaining = expiresAt - Date.now();
		if (remaining <= 0) {
			setPreviewExpired(true);
			return;
		}
		const timer = window.setTimeout(() => setPreviewExpired(true), remaining);
		return () => window.clearTimeout(timer);
	}, [preparedUndo, undoDialogOpen]);
	const guardedUndoReason = t(`turnReview.guardedUndo.reasons.${guardedUndo.reason}`, {
		count: guardedUndoCapture?.fileCount ?? 0,
		expiresAt: guardedUndoCapture?.expiresAt
			? t("turnReview.guardedUndo.validUntil", {
				expiresAt: guardedUndoCapture.expiresAt,
			})
			: "",
	});
	const fingerprints = useMemo(() => {
		if (!review) return null;
		return {
			base: review.baseFingerprint?.slice(0, 12) ?? "—",
			result: review.resultFingerprint?.slice(0, 12) ?? "—",
		};
	}, [review]);
	const activeUndoReason = activeUndo
		? resolveGuardedUndoFailureReason(activeUndo.reasonCode)
		: null;
	const outcomeReason = undoOutcome
		? resolveGuardedUndoFailureReason(undoOutcome.reasonCode)
		: null;
	const prepareProblemReason = prepareProblem
		? resolveGuardedUndoFailureReason(prepareProblem.reasonCode)
		: null;
	const preparedFileCount = preparedUndo?.fileCount ?? 0;
	const closeUndoDialog = (open: boolean) => {
		if (executeUndoMutation.isPending) return;
		setUndoDialogOpen(open);
		if (!open && undoOutcome?.status !== "recovery_required") {
			setPreparedUndo(null);
			setUndoOutcome(null);
		}
	};

	return (
		<section
			className="flex h-full min-h-0 flex-col bg-background"
			aria-label={t("turnReview.title")}
		>
			<header className="flex h-12 shrink-0 items-center gap-2 border-b border-border/60 px-3">
				<FileDiff className="size-4 text-primary" />
				<div className="min-w-0 flex-1">
					<h2 className="truncate text-sm font-semibold">{t("turnReview.title")}</h2>
					<p className="truncate text-[11px] text-muted-foreground">{t("turnReview.subtitle")}</p>
				</div>
					{undoAvailable ? (
						<Button
							type="button"
							variant="outline"
							size="sm"
							className="h-8 gap-1.5"
							disabled={prepareUndoMutation.isPending}
							onClick={() => review && prepareUndoMutation.mutate(review.snapshotId)}
						>
							{prepareUndoMutation.isPending ? (
								<Loader2 className="size-3.5 animate-spin" />
							) : (
								<RotateCcw className="size-3.5" />
							)}
							{t("turnReview.guardedUndo.action")}
						</Button>
					) : null}
					<Button
					type="button"
					variant="ghost"
					size="icon-sm"
					aria-label={t("turnReview.close")}
					onClick={onClose}
				>
					<X className="size-4" />
				</Button>
			</header>
			{reviewQuery.isPending ? (
				<div className="flex flex-1 items-center justify-center gap-2 text-sm text-muted-foreground">
					<Loader2 className="size-4 animate-spin" /> {t("turnReview.loading")}
				</div>
			) : reviewQuery.isError ? (
				<div className="m-4 rounded-md border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">
					{t("turnReview.failed")}
				</div>
			) : !review ? (
				<div className="flex flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground">
					{t("turnReview.unavailable")}
				</div>
			) : (
				<>
					<div className="shrink-0 space-y-2 border-b border-border/60 p-3 text-xs">
						<div className="flex flex-wrap items-center gap-2">
							<span className="rounded-full border px-2 py-0.5 font-medium">{stateLabel}</span>
							{outcome ? <span className={cn("rounded-full border px-2 py-0.5 font-medium", outcome.outcome === "completed" ? "text-emerald-600" : "text-amber-600")}>{t(`turnReview.outcomes.${outcome.outcome}`)}</span> : null}
							<span className={cn("rounded-full border px-2 py-0.5", review.compatibility === "matches_result" ? "text-emerald-600" : "text-amber-600")}>{compatibilityLabel}</span>
							<span
								className={cn(
									"rounded-full border px-2 py-0.5",
									guardedUndo.state === "eligible"
										? "text-emerald-600"
										: guardedUndo.state === "failed"
											? "text-destructive"
											: "text-amber-600",
								)}
							>
								{t(`turnReview.guardedUndo.states.${guardedUndo.state}`)}
							</span>
							<span className="ml-auto tabular-nums text-muted-foreground">{t("turnReview.fileCount", { count: review.files.length })} · <span className="text-emerald-600">+{review.insertions}</span> <span className="text-destructive">−{review.deletions}</span></span>
						</div>
						<p className="text-muted-foreground">{t("turnReview.attributionNotice")}</p>
						{outcome?.reason ? <p className="flex gap-1.5 text-amber-600"><AlertTriangle className="mt-0.5 size-3.5 shrink-0" />{t(`turnReview.outcomeReasons.${outcome.reason}`)}</p> : null}
						{review.state === "partial" ? <p className="flex gap-1.5 text-amber-600"><AlertTriangle className="mt-0.5 size-3.5 shrink-0" />{t("turnReview.partialNotice")}</p> : null}
						{review.diffTruncated ? <p className="flex gap-1.5 text-amber-600"><AlertTriangle className="mt-0.5 size-3.5 shrink-0" />{t("turnReview.diffTruncatedNotice")}</p> : null}
						{review.compatibility === "diverged" ? <p className="flex gap-1.5 text-amber-600"><AlertTriangle className="mt-0.5 size-3.5 shrink-0" />{t("turnReview.divergedNotice")}</p> : null}
						{review.excludedPreexistingUntrackedCount > 0 ? <p className="text-amber-600">{t("turnReview.excludedUntracked", { count: review.excludedPreexistingUntrackedCount })}</p> : null}
						{review.observedValidations.length > 0 ? <p className="flex gap-1.5 text-muted-foreground"><CheckCircle2 className="mt-0.5 size-3.5 shrink-0" />{t("turnReview.observedValidations", { values: review.observedValidations.join(", ") })}</p> : null}
						{review.error ? <p className="text-destructive">{review.error}</p> : null}
							<p
							className={cn(
								"text-muted-foreground",
								guardedUndo.state === "failed" && "text-destructive",
							)}
						>
								{guardedUndoReason}
							</p>
							{activeUndo ? (
								<div
									className={cn(
										"rounded-md border p-2.5",
										activeUndo.status === "recovery_required"
											? "border-destructive/40 bg-destructive/5 text-destructive"
											: "border-amber-500/40 bg-amber-500/5 text-amber-700 dark:text-amber-400",
									)}
								>
									<p className="flex items-center gap-1.5 font-medium">
										{activeUndo.status === "recovery_required" ? (
											<ShieldAlert className="size-3.5" />
										) : (
											<Loader2 className="size-3.5 animate-spin" />
										)}
										{t(`turnReview.guardedUndo.operationStates.${activeUndo.status}`)}
									</p>
									{activeUndoReason ? (
										<p className="mt-1">
											{t(`turnReview.guardedUndo.failureReasons.${activeUndoReason}`)}
										</p>
									) : null}
									<p className="mt-1 font-mono text-[10px] opacity-75">
										{t("turnReview.guardedUndo.operationId", {
											operationId: activeUndo.operationId,
										})}
									</p>
									<Button
										type="button"
										variant="outline"
										size="xs"
										className="mt-2"
										onClick={() =>
											void queryClient.invalidateQueries({
												queryKey: lastTurnReviewQueryKey(sessionId, workspaceId),
											})
										}
									>
										{t("turnReview.guardedUndo.refreshRecovery")}
									</Button>
								</div>
							) : null}
							{prepareUndoMutation.isError ? (
								<p className="text-destructive">
									{t("turnReview.guardedUndo.prepareFailed")}
								</p>
							) : null}
							{prepareProblem && prepareProblemReason ? (
								<div className="rounded-md border border-amber-500/40 bg-amber-500/5 p-2.5 text-amber-700 dark:text-amber-400">
									<p className="font-medium">
										{t(`turnReview.guardedUndo.prepareResults.${prepareProblem.status}`)}
									</p>
									<p className="mt-1">
										{t(`turnReview.guardedUndo.failureReasons.${prepareProblemReason}`)}
									</p>
								</div>
							) : null}
							{undoOutcome && !undoDialogOpen ? (
								<p
									className={cn(
										undoOutcome.status === "completed"
											? "text-emerald-600"
											: "text-amber-600",
									)}
								>
									{t(`turnReview.guardedUndo.results.${undoOutcome.status}`)}
								</p>
							) : null}
						<details className="text-[10px] text-muted-foreground"><summary className="cursor-pointer">{t("turnReview.fingerprints")}</summary><code className="mt-1 block">{fingerprints?.base} → {fingerprints?.result}</code></details>
					</div>
					<div className="grid min-h-0 flex-1 grid-cols-[minmax(150px,38%)_1fr]">
						<div
							className="min-h-0 overflow-y-auto border-r border-border/60 p-1.5"
						>
							{review.files.length === 0 ? <p className="p-3 text-xs text-muted-foreground">{review.state === "collecting" ? t("turnReview.collectingHint") : t("turnReview.noChanges")}</p> : review.files.map((file) => (
								<button key={file.path} type="button" aria-pressed={selectedPath === file.path} onClick={() => setSelectedPath(file.path)} className={cn("flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[11px] hover:bg-muted", selectedPath === file.path && "bg-muted text-foreground")}>
									<span className="w-3 shrink-0 font-semibold text-muted-foreground">{file.status}</span><span className="min-w-0 flex-1 truncate">{file.path}</span>
								</button>
							))}
						</div>
						<div className="min-h-0 overflow-auto bg-muted/15">
							{previewState === "loading" ? <div className="flex h-full items-center justify-center"><Loader2 className="size-4 animate-spin" /></div> : previewState === "error" ? <div className="flex h-full items-center justify-center p-4 text-center text-xs text-destructive">{t("turnReview.diffFailed")}</div> : previewState === "diff" ? <pre className="min-w-max p-3 font-mono text-[10.5px] leading-5"><code>{diffQuery.data!.diff}</code></pre> : <div className="flex h-full items-center justify-center p-4 text-center text-xs text-muted-foreground">{previewState === "unavailable" ? t("turnReview.previewUnavailable") : review.state === "collecting" ? t("turnReview.collectingHint") : review.state === "no_changes" ? t("turnReview.noChanges") : t("turnReview.selectFile")}</div>}
						</div>
					</div>
				</>
			)}
			<Dialog open={undoDialogOpen} onOpenChange={closeUndoDialog}>
				<DialogContent className="flex max-h-[86vh] max-w-2xl flex-col overflow-hidden" showCloseButton={!executeUndoMutation.isPending}>
					<DialogHeader>
						<DialogTitle>{t("turnReview.guardedUndo.confirmTitle")}</DialogTitle>
						<DialogDescription>
							{preparedUndo
								? t("turnReview.guardedUndo.confirmSummary", {
									count: preparedUndo.fileCount,
									bytes: new Intl.NumberFormat(undefined, {
										style: "unit",
										unit: "byte",
										unitDisplay: "short",
									}).format(preparedUndo.totalBytes),
								})
								: null}
						</DialogDescription>
					</DialogHeader>
					{preparedUndo ? (
						<div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1 text-xs">
							<div className="rounded-md border border-amber-500/40 bg-amber-500/5 p-3 text-amber-800 dark:text-amber-300">
								<p className="flex gap-2"><AlertTriangle className="mt-0.5 size-4 shrink-0" />{t("turnReview.guardedUndo.scopeWarning")}</p>
							</div>
							<p className="text-muted-foreground">
								{t("turnReview.guardedUndo.previewExpires", { expiresAt: preparedUndo.expiresAt })}
							</p>
							<div className="space-y-2">
								{preparedUndo.files.map((file, index) => (
									<div key={`${index}:${file.displayPath}`} className="overflow-hidden rounded-md border">
										<div className="flex items-center gap-2 bg-muted/40 px-2.5 py-2">
											<span className="min-w-0 flex-1 truncate font-mono">{file.displayPath}</span>
											<span className="text-muted-foreground">{file.binary ? t("turnReview.guardedUndo.binary") : t("turnReview.guardedUndo.text")}</span>
											<span className="tabular-nums text-muted-foreground">{file.size} B</span>
										</div>
										{!file.binary && file.preview ? <pre className="max-h-52 overflow-auto p-2.5 font-mono text-[10.5px] leading-5"><code>{file.preview}</code></pre> : null}
									</div>
								))}
							</div>
							{executeUndoMutation.isPending ? (
								<div className="flex items-start gap-2 rounded-md border border-primary/30 bg-primary/5 p-3">
									<Loader2 className="mt-0.5 size-4 shrink-0 animate-spin text-primary" />
									<div><p className="font-medium">{t("turnReview.guardedUndo.executing")}</p><p className="mt-1 text-muted-foreground">{t("turnReview.guardedUndo.executingSteps")}</p></div>
								</div>
							) : null}
							{previewExpired && !undoOutcome ? <p className="text-destructive">{t("turnReview.guardedUndo.previewExpired")}</p> : null}
							{executeUndoMutation.isError ? <p className="text-destructive">{t("turnReview.guardedUndo.executeFailed")}</p> : null}
							{undoOutcome ? (
								<div className={cn("rounded-md border p-3", undoOutcome.status === "completed" ? "border-emerald-500/40 bg-emerald-500/5 text-emerald-700 dark:text-emerald-400" : undoOutcome.status === "recovery_required" ? "border-destructive/40 bg-destructive/5 text-destructive" : "border-amber-500/40 bg-amber-500/5 text-amber-700 dark:text-amber-400")}>
									<p className="font-medium">{t(`turnReview.guardedUndo.results.${undoOutcome.status}`)}</p>
									{outcomeReason ? <p className="mt-1">{t(`turnReview.guardedUndo.failureReasons.${outcomeReason}`)}</p> : null}
								</div>
							) : null}
						</div>
					) : null}
					<DialogFooter>
						<Button type="button" variant="outline" disabled={executeUndoMutation.isPending} onClick={() => closeUndoDialog(false)}>
							{undoOutcome ? t("turnReview.guardedUndo.closeResult") : t("turnReview.guardedUndo.cancel")}
						</Button>
							{preparedUndo && !undoOutcome && !executeUndoMutation.isError ? (
							<Button type="button" variant="destructive" disabled={executeUndoMutation.isPending || previewExpired} onClick={() => executeUndoMutation.mutate(preparedUndo.previewToken)}>
								{executeUndoMutation.isPending ? <Loader2 className="size-4 animate-spin" /> : <RotateCcw className="size-4" />}
								{t("turnReview.guardedUndo.confirmAction", { count: preparedFileCount })}
							</Button>
						) : null}
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</section>
	);
}
