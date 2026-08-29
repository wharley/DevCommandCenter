import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type {
	PrepareGuardedUndoOutput,
	TurnReviewFile,
} from "@dcc/contracts";
import { getMaterialFileIcon } from "file-extension-icon-js";
import {
	AlertTriangle,
	ChevronDown,
	ChevronRight,
	Loader2,
	RotateCcw,
	ShieldAlert,
	ShieldCheck,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { WorkspacePatchDiffLoader } from "@/features/editor/WorkspaceChangesDiffLoader";
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
	resolveGuardedUndoCapture,
	resolveGuardedUndoFailureReason,
	resolveTurnReviewOutcome,
} from "./turn-review.logic";
import { lastTurnReviewQueryKey } from "./turn-review-query";
import {
	reviewCardDiffHeight,
	shouldEagerLoadReviewCard,
} from "@/features/inspector/inspector-changes-presentation";

function TurnReviewCard({
	snapshotId,
	file,
	index,
}: {
	snapshotId: string;
	file: TurnReviewFile;
	index: number;
}) {
	const { t } = useTranslation("common");
	const cardRef = useRef<HTMLElement | null>(null);
	const [open, setOpen] = useState(true);
	const [shouldLoad, setShouldLoad] = useState(() =>
		shouldEagerLoadReviewCard(index),
	);
	useEffect(() => {
		if (shouldLoad || !open || file.previewUnavailable) return;
		if (typeof IntersectionObserver === "undefined") {
			setShouldLoad(true);
			return;
		}
		const target = cardRef.current;
		if (!target) return;
		const observer = new IntersectionObserver(
			(entries) => {
				if (!entries.some((entry) => entry.isIntersecting)) return;
				setShouldLoad(true);
				observer.disconnect();
			},
			{ rootMargin: "700px 0px" },
		);
		observer.observe(target);
		return () => observer.disconnect();
	}, [file.previewUnavailable, open, shouldLoad]);
	const diffQuery = useQuery({
		queryKey: ["turnReviewFileDiff", snapshotId, file.path],
		queryFn: () => loadTurnReviewFileDiff(snapshotId, file.path),
		enabled: open && shouldLoad && !file.previewUnavailable,
	});
	const name = file.path.split("/").pop() ?? file.path;
	const folder = file.path.includes("/")
		? file.path.slice(0, file.path.lastIndexOf("/"))
		: "";

	return (
		<article
			ref={cardRef}
			className="overflow-hidden rounded-xl border border-border/60 bg-background shadow-[0_1px_2px_rgba(0,0,0,0.08)]"
		>
			<div className="flex min-h-11 items-center gap-2 border-b border-border/45 px-2.5 py-1.5">
				<button
					type="button"
					className="flex min-w-0 flex-1 items-center gap-2 rounded-md text-left outline-none focus-visible:ring-1 focus-visible:ring-ring"
					onClick={() => setOpen((value) => !value)}
					aria-expanded={open}
				>
					{open ? (
						<ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
					) : (
						<ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
					)}
					<img src={getMaterialFileIcon(name)} alt="" className="size-3.5 shrink-0" />
					<span className="min-w-0 flex-1 truncate">
						<span className="font-medium text-foreground">{name}</span>
						{folder ? (
							<span className="ml-1.5 text-[10px] text-muted-foreground">
								{folder}
							</span>
						) : null}
					</span>
				</button>
				<div className="flex shrink-0 items-center gap-1 text-[10px] tabular-nums">
					{file.insertions > 0 ? (
						<span className="text-emerald-600 dark:text-emerald-400">
							+{file.insertions}
						</span>
					) : null}
					{file.deletions > 0 ? (
						<span className="text-destructive">−{file.deletions}</span>
					) : null}
					<span className="min-w-4 text-center font-semibold text-muted-foreground">
						{file.status}
					</span>
				</div>
			</div>
			{open ? (
				<div
					className="min-h-[190px] overflow-hidden bg-background"
					style={{
						height: `${reviewCardDiffHeight(file.insertions, file.deletions)}px`,
					}}
				>
					{file.previewUnavailable ? (
						<div className="flex h-full items-center justify-center p-4 text-center text-xs text-muted-foreground">
							{t("turnReview.previewUnavailable")}
						</div>
					) : !shouldLoad || diffQuery.isFetching ? (
						<div className="flex h-full items-center justify-center">
							<Loader2 className="size-4 animate-spin text-muted-foreground" />
						</div>
					) : diffQuery.isError || !diffQuery.data?.diff ? (
						<div className="flex h-full items-center justify-center p-4 text-center text-xs text-destructive">
							{t("turnReview.diffFailed")}
						</div>
					) : (
						<WorkspacePatchDiffLoader
							path={file.path}
							patch={diffQuery.data.diff}
							className="h-full"
						/>
					)}
				</div>
			) : null}
		</article>
	);
}

export function TurnReviewSurface({
	sessionId,
	workspaceId,
}: {
	sessionId: string;
	workspaceId: string;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const reviewQuery = useQuery({
		queryKey: lastTurnReviewQueryKey(sessionId, workspaceId),
		queryFn: () => loadLastTurnReview(sessionId, workspaceId),
	});
	const review = reviewQuery.data ?? null;
	const stateLabel = review ? t(`turnReview.states.${review.state}`) : "";
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
			className="relative flex h-full min-h-0 flex-col bg-background"
			aria-label={t("turnReview.title")}
		>
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
					<div className="shrink-0 space-y-1.5 border-b border-border/50 px-3 py-2 text-[10.5px]">
						<div className="flex items-center gap-2">
							<span className="font-medium text-foreground">{stateLabel}</span>
							{outcome ? (
								<span
									className={cn(
										"text-muted-foreground",
										outcome.outcome === "aborted" && "text-amber-600",
									)}
								>
									· {t(`turnReview.outcomes.${outcome.outcome}`)}
								</span>
							) : null}
							<span className="ml-auto tabular-nums text-muted-foreground">
								{t("turnReview.fileCount", { count: review.files.length })} ·{" "}
								<span className="text-emerald-600">+{review.insertions}</span>{" "}
								<span className="text-destructive">−{review.deletions}</span>
							</span>
							<Tooltip>
								<TooltipTrigger asChild>
									<span
										className={cn(
											"inline-flex size-5 items-center justify-center rounded-full",
											guardedUndo.state === "eligible"
												? "bg-emerald-500/10 text-emerald-600"
												: "bg-muted text-muted-foreground",
										)}
										tabIndex={0}
									>
										{guardedUndo.state === "eligible" ? (
											<ShieldCheck className="size-3.5" />
										) : (
											<ShieldAlert className="size-3.5" />
										)}
									</span>
								</TooltipTrigger>
								<TooltipContent side="bottom" className="max-w-72">
									<p className="font-medium">
										{t(`turnReview.guardedUndo.states.${guardedUndo.state}`)}
									</p>
									<p className="mt-1 text-muted-foreground">{guardedUndoReason}</p>
								</TooltipContent>
							</Tooltip>
						</div>
						<p className="text-muted-foreground">{t("turnReview.attributionNotice")}</p>
						{review.compatibility === "diverged" ? (
							<p className="flex gap-1.5 text-amber-600">
								<AlertTriangle className="mt-0.5 size-3.5 shrink-0" />
								{t("turnReview.divergedNotice")}
							</p>
						) : null}
						{activeUndo ? (
							<div
								className={cn(
									"flex items-center gap-2 rounded-md border px-2 py-1.5",
									activeUndo.status === "recovery_required"
										? "border-destructive/40 bg-destructive/5 text-destructive"
										: "border-amber-500/40 bg-amber-500/5 text-amber-700 dark:text-amber-400",
								)}
							>
								{activeUndo.status === "recovery_required" ? (
									<ShieldAlert className="size-3.5 shrink-0" />
								) : (
									<Loader2 className="size-3.5 shrink-0 animate-spin" />
								)}
								<span className="min-w-0 flex-1 truncate font-medium">
									{t(`turnReview.guardedUndo.operationStates.${activeUndo.status}`)}
								</span>
								<Button
									type="button"
									variant="ghost"
									size="xs"
									className="h-6 px-2"
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
							<p className="text-amber-600">
								{t(`turnReview.guardedUndo.failureReasons.${prepareProblemReason}`)}
							</p>
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
					</div>
					<div className="min-h-0 flex-1 overflow-y-auto bg-muted/[0.08] px-2 pb-20 pt-2">
						{review.files.length === 0 ? (
							<div className="flex min-h-36 items-center justify-center px-4 text-center text-xs text-muted-foreground">
								{review.state === "collecting"
									? t("turnReview.collectingHint")
									: t("turnReview.noChanges")}
							</div>
						) : (
							<div className="space-y-2">
								{review.files.map((file, index) => (
									<TurnReviewCard
										key={`${review.snapshotId}:${file.path}`}
										snapshotId={review.snapshotId}
										file={file}
										index={index}
									/>
								))}
							</div>
						)}
					</div>
					{undoAvailable ? (
						<div className="pointer-events-none absolute inset-x-0 bottom-3 z-20 flex justify-center px-3">
							<div className="pointer-events-auto flex items-center rounded-full border border-border/70 bg-background/95 p-1 shadow-lg backdrop-blur">
								<Button
									type="button"
									variant="ghost"
									size="sm"
									className="h-8 rounded-full gap-1.5 px-3"
									disabled={prepareUndoMutation.isPending}
									onClick={() => prepareUndoMutation.mutate(review.snapshotId)}
								>
									{prepareUndoMutation.isPending ? (
										<Loader2 className="size-3.5 animate-spin" />
									) : (
										<RotateCcw className="size-3.5" />
									)}
									{t("turnReview.guardedUndo.action")}
								</Button>
							</div>
						</div>
					) : null}
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
