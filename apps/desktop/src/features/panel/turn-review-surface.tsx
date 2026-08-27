import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, CheckCircle2, FileDiff, Loader2, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { loadLastTurnReview, loadTurnReviewFileDiff } from "@/lib/session-api";
import { cn } from "@/lib/utils";
import {
	reconcileTurnReviewSelection,
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
	const fingerprints = useMemo(() => {
		if (!review) return null;
		return {
			base: review.baseFingerprint?.slice(0, 12) ?? "—",
			result: review.resultFingerprint?.slice(0, 12) ?? "—",
		};
	}, [review]);

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
		</section>
	);
}
