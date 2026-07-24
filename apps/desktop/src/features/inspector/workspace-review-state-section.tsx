import type { WorkspaceReviewer } from "@dcc/contracts";
import {
	CheckCircle2,
	ChevronRight,
	CircleAlert,
	CircleDot,
	ExternalLink,
	GitMerge,
	Loader2,
	MessageSquareWarning,
	ShieldCheck,
	UserRoundCheck,
	Users,
	XCircle,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { openExternal } from "@/lib/shell-api";
import { cn } from "@/lib/utils";
import { useWorkspaceReviewState } from "./use-workspace-review-state";

function reviewStateClass(state: string | null) {
	switch (state) {
		case "approved":
		case "not_required":
			return "border-emerald-500/20 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400";
		case "changes_requested":
			return "border-destructive/20 bg-destructive/10 text-destructive";
		case "pending":
			return "border-amber-500/20 bg-amber-500/10 text-amber-700 dark:text-amber-400";
		default:
			return "border-border/60 bg-muted/50 text-muted-foreground";
	}
}

function ReviewerAvatar({ reviewer }: { reviewer: WorkspaceReviewer }) {
	const label = reviewer.login ?? reviewer.name ?? "?";
	return (
		<span
			className="flex size-6 shrink-0 items-center justify-center overflow-hidden rounded-full bg-muted text-[8.5px] font-semibold text-muted-foreground ring-1 ring-border/70"
			title={label}
		>
			{reviewer.avatarUrl ? (
				<img src={reviewer.avatarUrl} alt="" className="size-full object-cover" />
			) : (
				label.slice(0, 2).toUpperCase()
			)}
		</span>
	);
}

export function WorkspaceReviewStateSection({
	workspaceRoot,
	branch,
	forgeLogin,
	enabled,
}: {
	workspaceRoot: string | null;
	branch: string | null;
	forgeLogin: string | null;
	enabled: boolean;
}) {
	const { t } = useTranslation("common");
	const [open, setOpen] = useState(false);
	const query = useWorkspaceReviewState(
		workspaceRoot,
		branch,
		forgeLogin,
		enabled,
	);
	const review = query.data ?? null;

	if (!enabled) return null;

	if (query.isPending) {
		return (
			<div
				data-workspace-review-state
				className="flex shrink-0 items-center gap-2 rounded-md border border-border/50 bg-muted/20 px-2.5 py-2 text-[11px] text-muted-foreground"
			>
				<Loader2 className="size-3.5 animate-spin" />
				{t("inspector.reviewState.loading")}
			</div>
		);
	}

	if (query.isError) {
		return (
			<div
				data-workspace-review-state
				className="flex shrink-0 items-center gap-2 rounded-md border border-amber-500/20 bg-amber-500/5 px-2.5 py-2 text-[11px] text-amber-700 dark:text-amber-300"
			>
				<CircleAlert className="size-3.5 shrink-0" />
				<span className="min-w-0 flex-1 truncate">
					{t("inspector.reviewState.loadFailed")}
				</span>
				<Button
					type="button"
					variant="ghost"
					size="xs"
					className="h-6 px-1.5 text-[10px]"
					onClick={() => void query.refetch()}
				>
					{t("inspector.reviewState.tryAgain")}
				</Button>
			</div>
		);
	}

	if (!review?.number) return null;

	const state = review.reviewState ?? "unknown";
	const pendingReviewers = review.reviewers.filter(
		(reviewer) => reviewer.state === "pending",
	).length;
	const hasBlockingSignal =
		review.hasConflicts === true ||
		review.reviewState === "changes_requested" ||
		review.discussionsResolved === false;
	const approvalsLabel = !review.approvalsAvailable
		? t("inspector.reviewState.approvalsUnavailable")
		: review.approvalsRequired != null || review.approvalsLeft != null
			? t("inspector.reviewState.approvals", {
					received: review.approvalsReceived,
					required:
						review.approvalsRequired ??
						review.approvalsReceived + (review.approvalsLeft ?? 0),
				})
			: state === "not_required"
				? t("inspector.reviewState.noApprovalRequired")
				: review.approvalsReceived > 0
					? t("inspector.reviewState.approvalsRecorded", {
							count: review.approvalsReceived,
						})
					: t("inspector.reviewState.approvalRequirementUnknown");

	return (
		<div
			data-workspace-review-state
			className={cn(
				"shrink-0 overflow-hidden rounded-md border bg-muted/15",
				hasBlockingSignal ? "border-amber-500/25" : "border-border/60",
			)}
		>
			<div className="flex min-w-0 items-center gap-2 px-2.5 py-2">
				<Button
					type="button"
					variant="ghost"
					size="xs"
					className="-ml-1 h-6 min-w-0 flex-1 justify-start gap-2 px-1 text-left hover:bg-transparent"
					onClick={() => setOpen((value) => !value)}
					aria-expanded={open}
				>
					<ChevronRight
						className={cn(
							"size-3.5 shrink-0 transition-transform",
							open && "rotate-90",
						)}
					/>
					{state === "approved" || state === "not_required" ? (
						<ShieldCheck className="size-3.5 shrink-0 text-emerald-500" />
					) : state === "changes_requested" ? (
						<MessageSquareWarning className="size-3.5 shrink-0 text-destructive" />
					) : (
						<Users className="size-3.5 shrink-0 text-amber-500" />
					)}
					<span className="truncate text-[11px] font-medium">
						{t("inspector.reviewState.title")}
					</span>
					{pendingReviewers > 0 ? (
						<span className="truncate text-[9.5px] font-normal text-muted-foreground">
							{t("inspector.reviewState.pendingReviewers", {
								count: pendingReviewers,
							})}
						</span>
					) : null}
				</Button>
				<Badge
					variant="outline"
					className={cn(
						"h-5 rounded-full px-1.5 text-[9px] font-medium",
						reviewStateClass(state),
					)}
				>
					{t(`inspector.reviewState.state.${state}`, {
						defaultValue: state,
					})}
				</Badge>
				{review.url ? (
					<Button
						type="button"
						variant="ghost"
						size="icon-xs"
						className="size-6 text-muted-foreground"
						onClick={() => void openExternal(review.url!)}
						aria-label={t("inspector.reviewState.openReview")}
					>
						<ExternalLink className="size-3.5" />
					</Button>
				) : null}
			</div>

			{open ? (
				<div className="border-t border-border/40 px-2.5 py-2">
					<div className="grid grid-cols-2 gap-1.5">
						<div className="flex items-center gap-1.5 rounded-md bg-background/60 px-2 py-1.5 text-[9.5px] text-muted-foreground">
							<UserRoundCheck className="size-3.5 shrink-0" />
							<span className="truncate">{approvalsLabel}</span>
						</div>
						<div className="flex items-center gap-1.5 rounded-md bg-background/60 px-2 py-1.5 text-[9.5px] text-muted-foreground">
							{review.hasConflicts === true ? (
								<XCircle className="size-3.5 shrink-0 text-destructive" />
							) : review.mergeable === "mergeable" ? (
								<CheckCircle2 className="size-3.5 shrink-0 text-emerald-500" />
							) : (
								<GitMerge className="size-3.5 shrink-0" />
							)}
							<span className="truncate">
								{review.hasConflicts === true
									? t("inspector.reviewState.conflicts")
									: review.mergeable === "mergeable"
										? t("inspector.reviewState.mergeable")
										: t("inspector.reviewState.mergeabilityUnknown")}
							</span>
						</div>
						<div className="flex items-center gap-1.5 rounded-md bg-background/60 px-2 py-1.5 text-[9.5px] text-muted-foreground">
							<GitMerge className="size-3.5 shrink-0" />
							<span className="truncate">
								{review.behindBy == null
									? t("inspector.reviewState.baseUnknown")
									: review.behindBy > 0
										? t("inspector.reviewState.behindBase", {
												count: review.behindBy,
											})
										: t("inspector.reviewState.baseCurrent")}
							</span>
						</div>
						<div className="flex items-center gap-1.5 rounded-md bg-background/60 px-2 py-1.5 text-[9.5px] text-muted-foreground">
							<CircleDot className="size-3.5 shrink-0" />
							<span className="truncate">
								{review.discussionsResolved == null
									? t("inspector.reviewState.discussionsUnknown")
									: review.discussionsResolved
										? t("inspector.reviewState.discussionsResolved")
										: t("inspector.reviewState.discussionsOpen")}
							</span>
						</div>
					</div>

					{review.reviewersAvailable ? (
						<div className="mt-2">
							<p className="mb-1 text-[9px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
								{t("inspector.reviewState.reviewers")}
							</p>
							{review.reviewers.length > 0 ? (
								<div className="space-y-1">
									{review.reviewers.map((reviewer, index) => (
										<div
											key={`${reviewer.login ?? reviewer.name ?? "reviewer"}-${index}`}
											className="flex items-center gap-2 rounded-md px-1 py-1"
										>
											<ReviewerAvatar reviewer={reviewer} />
											<span className="min-w-0 flex-1 truncate text-[10px]">
												{reviewer.login
													? `@${reviewer.login}`
													: reviewer.name ??
														t("inspector.reviewState.unknownReviewer")}
											</span>
											<span className="text-[9.5px] text-muted-foreground">
												{t(
													`inspector.reviewState.reviewerState.${reviewer.state}`,
													{ defaultValue: reviewer.state },
												)}
											</span>
										</div>
									))}
								</div>
							) : (
								<p className="text-[10px] text-muted-foreground">
									{t("inspector.reviewState.noReviewers")}
								</p>
							)}
						</div>
					) : null}
				</div>
			) : null}
		</div>
	);
}
