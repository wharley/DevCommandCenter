import { useState } from "react";
import { useMutation, useQueries, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, CheckCircle2, Loader2, RotateCcw } from "lucide-react";
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
import { executeGuardedUndo, prepareGuardedUndo } from "@/lib/session-api";
import { cn } from "@/lib/utils";
import { resolveGuardedUndoFailureReason } from "./turn-review.logic";
import { turnReviewQueryOptions } from "./turn-review-query";
import {
	executeTurnUndo,
	planTurnUndo,
	prepareTurnUndo,
	type PreparedTurnUndo,
	type TurnUndoPreparation,
	type TurnUndoStepResult,
} from "./turn-undo-across-projects.logic";

export type TurnUndoWorkspace = { id: string; name: string };

/**
 * Undo for a multi-project turn: every project the turn changed is prepared
 * first and restored only when all of them are free, one after the other.
 */
export function TurnUndoAcrossProjects({
	sessionId,
	turnId,
	workspaces,
}: {
	sessionId: string;
	turnId: string;
	workspaces: TurnUndoWorkspace[];
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const reviewQueries = useQueries({
		queries: workspaces.map((workspace) =>
			turnReviewQueryOptions({ sessionId, workspaceId: workspace.id, turnId }),
		),
	});
	const plan = planTurnUndo(
		workspaces.map((workspace, index) => ({
			workspaceId: workspace.id,
			name: workspace.name,
			review: reviewQueries[index]?.data ?? null,
		})),
	);
	const [dialogOpen, setDialogOpen] = useState(false);
	const [prepared, setPrepared] = useState<PreparedTurnUndo[] | null>(null);
	const [blocked, setBlocked] = useState<
		Extract<TurnUndoPreparation, { status: "blocked" }> | null
	>(null);
	const [results, setResults] = useState<TurnUndoStepResult[] | null>(null);

	const prepareMutation = useMutation({
		mutationFn: () => prepareTurnUndo(plan.targets, prepareGuardedUndo),
		onSuccess: (preparation) => {
			setResults(null);
			if (preparation.status === "ready") {
				setBlocked(null);
				setPrepared(preparation.prepared);
				setDialogOpen(true);
				return;
			}
			setPrepared(null);
			setBlocked(preparation);
		},
	});
	const executeMutation = useMutation({
		mutationFn: (projects: PreparedTurnUndo[]) =>
			executeTurnUndo(projects, executeGuardedUndo),
		onSuccess: setResults,
		onSettled: () =>
			queryClient.invalidateQueries({
				predicate: (query) =>
					[
						"turnReview",
						"lastTurnReview",
						"workspaceGitStatus",
						"workspaceGitBranchDiff",
						"workspaceGitFilePreviewContent",
					].includes(String(query.queryKey[0] ?? "")),
			}),
	});

	if (plan.targets.length === 0) return null;

	const fileCount = prepared?.reduce((sum, project) => sum + project.preview.fileCount, 0) ?? 0;
	const expired =
		prepared?.some((project) => Date.parse(project.preview.expiresAt) <= Date.now()) ?? false;
	const closeDialog = (open: boolean) => {
		if (executeMutation.isPending) return;
		setDialogOpen(open);
		if (!open) {
			setPrepared(null);
			setResults(null);
		}
	};
	const blockedReason = blocked
		? resolveGuardedUndoFailureReason(blocked.reasonCode)
		: null;

	return (
		<>
			{blocked && blockedReason ? (
				<p className="pointer-events-auto absolute inset-x-3 bottom-14 z-20 rounded-md border border-amber-500/40 bg-background/95 px-2.5 py-1.5 text-[10.5px] text-amber-700 shadow-sm dark:text-amber-400">
					{t("turnReview.turnUndo.blocked", { project: blocked.target.name })}{" "}
					{t(`turnReview.guardedUndo.failureReasons.${blockedReason}`)}
				</p>
			) : null}
			<div className="pointer-events-none absolute inset-x-0 bottom-3 z-20 flex justify-center px-3">
				<div className="pointer-events-auto flex items-center rounded-full border border-border/70 bg-background/95 p-1 shadow-lg backdrop-blur">
					<Button
						type="button"
						variant="ghost"
						size="sm"
						className="h-8 gap-1.5 rounded-full px-3"
						disabled={prepareMutation.isPending}
						onClick={() => prepareMutation.mutate()}
					>
						{prepareMutation.isPending ? (
							<Loader2 className="size-3.5 animate-spin" />
						) : (
							<RotateCcw className="size-3.5" />
						)}
						{t("turnReview.turnUndo.action", { count: plan.targets.length })}
					</Button>
				</div>
			</div>
			<Dialog open={dialogOpen} onOpenChange={closeDialog}>
				<DialogContent
					className="flex max-h-[86vh] max-w-2xl flex-col overflow-hidden"
					showCloseButton={!executeMutation.isPending}
				>
					<DialogHeader>
						<DialogTitle>{t("turnReview.turnUndo.confirmTitle")}</DialogTitle>
						<DialogDescription>
							{t("turnReview.turnUndo.confirmSummary", {
								count: fileCount,
								projects: prepared?.length ?? 0,
							})}
						</DialogDescription>
					</DialogHeader>
					<div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1 text-xs">
						<div className="rounded-md border border-amber-500/40 bg-amber-500/5 p-3 text-amber-800 dark:text-amber-300">
							<p className="flex gap-2">
								<AlertTriangle className="mt-0.5 size-4 shrink-0" />
								{t("turnReview.turnUndo.orderWarning")}
							</p>
						</div>
						{plan.uncovered.length > 0 ? (
							<p className="text-muted-foreground">
								{t("turnReview.turnUndo.uncovered", {
									projects: plan.uncovered.map((project) => project.name).join(", "),
								})}
							</p>
						) : null}
						{prepared?.map((project) => {
							const result = results?.find(
								(candidate) => candidate.workspaceId === project.workspaceId,
							);
							const reason = result
								? resolveGuardedUndoFailureReason(result.reasonCode)
								: null;
							return (
								<section key={project.workspaceId} className="space-y-1.5">
									<header className="flex items-center gap-2">
										<span className="font-medium text-foreground">{project.name}</span>
										<span className="text-muted-foreground">
											{t("turnReview.fileCount", { count: project.preview.fileCount })}
										</span>
										{result ? (
											<span
												className={cn(
													"ml-auto inline-flex items-center gap-1 font-medium",
													result.status === "completed"
														? "text-emerald-600"
														: result.status === "not_attempted"
															? "text-muted-foreground"
															: result.status === "recovery_required"
																? "text-destructive"
																: "text-amber-600",
												)}
											>
												{result.status === "completed" ? (
													<CheckCircle2 className="size-3.5" />
												) : null}
												{t(`turnReview.turnUndo.results.${result.status}`)}
											</span>
										) : null}
									</header>
									{result && result.status !== "completed" && result.status !== "not_attempted" && reason ? (
										<p className="text-amber-700 dark:text-amber-400">
											{t(`turnReview.guardedUndo.failureReasons.${reason}`)}
										</p>
									) : null}
									{project.preview.files.map((file, index) => (
										<div
											key={`${index}:${file.displayPath}`}
											className="overflow-hidden rounded-md border"
										>
											<div className="flex items-center gap-2 bg-muted/40 px-2.5 py-2">
												<span className="min-w-0 flex-1 truncate font-mono">
													{file.displayPath}
												</span>
												<span className="text-muted-foreground">
													{file.binary
														? t("turnReview.guardedUndo.binary")
														: t("turnReview.guardedUndo.text")}
												</span>
												<span className="tabular-nums text-muted-foreground">
													{file.size} B
												</span>
											</div>
											{!file.binary && file.preview ? (
												<pre className="max-h-52 overflow-auto p-2.5 font-mono text-[10.5px] leading-5">
													<code>{file.preview}</code>
												</pre>
											) : null}
										</div>
									))}
								</section>
							);
						})}
						{executeMutation.isPending ? (
							<div className="flex items-start gap-2 rounded-md border border-primary/30 bg-primary/5 p-3">
								<Loader2 className="mt-0.5 size-4 shrink-0 animate-spin text-primary" />
								<p className="font-medium">{t("turnReview.guardedUndo.executing")}</p>
							</div>
						) : null}
						{expired && !results ? (
							<p className="text-destructive">{t("turnReview.guardedUndo.previewExpired")}</p>
						) : null}
						{executeMutation.isError ? (
							<p className="text-destructive">{t("turnReview.guardedUndo.executeFailed")}</p>
						) : null}
					</div>
					<DialogFooter>
						<Button
							type="button"
							variant="outline"
							disabled={executeMutation.isPending}
							onClick={() => closeDialog(false)}
						>
							{results
								? t("turnReview.guardedUndo.closeResult")
								: t("turnReview.guardedUndo.cancel")}
						</Button>
						{prepared && !results && !executeMutation.isError ? (
							<Button
								type="button"
								variant="destructive"
								disabled={executeMutation.isPending || expired}
								onClick={() => executeMutation.mutate(prepared)}
							>
								{executeMutation.isPending ? (
									<Loader2 className="size-4 animate-spin" />
								) : (
									<RotateCcw className="size-4" />
								)}
								{t("turnReview.turnUndo.confirmAction", { count: prepared.length })}
							</Button>
						) : null}
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</>
	);
}
