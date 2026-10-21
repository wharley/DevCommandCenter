import { ChevronRight, MessageSquare } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import {
	reviewCommentLine,
	reviewThreadRoot,
	reviewThreadState,
	type ReviewThread,
	type ReviewThreadState,
} from "./review-threads.logic";

function firstLine(body: string): string {
	return body.split("\n").map((line) => line.trim()).find(Boolean) ?? "";
}

export function ReviewThreadStateChip({ state }: { state: ReviewThreadState }) {
	const { t } = useTranslation("common");
	return (
		<span
			className={cn(
				"shrink-0 rounded-full px-1.5 text-[9.5px] font-medium leading-[15px]",
				state === "open" &&
					"bg-amber-500/10 text-amber-700 dark:text-amber-400",
				state === "resolved" &&
					"bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
				state === "outdated" && "bg-muted text-muted-foreground",
			)}
		>
			{t(
				state === "open"
					? "reviewThread.unresolved"
					: state === "resolved"
					? "reviewThread.resolved"
					: "reviewThread.outdated",
			)}
		</span>
	);
}

/**
 * Every pull request conversation on the open file, including the ones the
 * diff cannot draw (outdated code, or a local-changes diff). Collapsed to one
 * calm line until asked.
 */
export function ReviewThreadsBar({
	threads,
	activeThreadId,
	defaultOpen,
	showingPullRequestDiff,
	onShowPullRequestDiff,
	onShowLocalDiff,
	onSelectThread,
}: {
	threads: ReviewThread[];
	activeThreadId: string | null;
	defaultOpen: boolean;
	showingPullRequestDiff: boolean;
	onShowPullRequestDiff?: () => void;
	onShowLocalDiff?: () => void;
	onSelectThread: (thread: ReviewThread) => void;
}) {
	const { t } = useTranslation("common");
	const [open, setOpen] = useState(defaultOpen);
	const counts = { open: 0, outdated: 0, resolved: 0 };
	for (const thread of threads) counts[reviewThreadState(thread)] += 1;
	const summary = [
		counts.open > 0 ? t("reviewThread.bar.open", { count: counts.open }) : null,
		counts.outdated > 0 ? t("reviewThread.bar.outdated", { count: counts.outdated }) : null,
		counts.resolved > 0 ? t("reviewThread.bar.resolved", { count: counts.resolved }) : null,
	]
		.filter(Boolean)
		.join(" · ");

	return (
		<div className="shrink-0 border-b border-border bg-muted/20">
			<div className="flex h-8 items-center gap-2 px-3">
				<button
					type="button"
					onClick={() => setOpen((value) => !value)}
					aria-expanded={open}
					className="flex min-w-0 flex-1 items-center gap-1.5 text-left text-[11px] text-muted-foreground hover:text-foreground"
				>
					<ChevronRight
						className={cn("size-3 shrink-0 transition-transform", open && "rotate-90")}
						strokeWidth={2}
						aria-hidden
					/>
					<MessageSquare className="size-3.5 shrink-0 text-primary" strokeWidth={2} aria-hidden />
					<span className="shrink-0 font-medium text-foreground">
						{t("reviewThread.bar.title", { count: threads.length })}
					</span>
					<span className="truncate">{summary}</span>
				</button>
				{onShowPullRequestDiff ? (
					<button
						type="button"
						onClick={onShowPullRequestDiff}
						className="shrink-0 rounded px-1.5 py-0.5 text-[10.5px] font-medium text-primary hover:bg-primary/10"
					>
						{t("reviewThread.bar.showPullRequestDiff")}
					</button>
				) : null}
				{onShowLocalDiff ? (
					<button
						type="button"
						onClick={onShowLocalDiff}
						className="shrink-0 rounded px-1.5 py-0.5 text-[10.5px] text-muted-foreground hover:bg-muted hover:text-foreground"
					>
						{t("reviewThread.bar.showLocalDiff")}
					</button>
				) : null}
			</div>
			{open ? (
				<ul className="max-h-44 overflow-y-auto px-1.5 pb-1.5">
					{!showingPullRequestDiff ? (
						<li className="px-1.5 pb-1 text-[10.5px] leading-snug text-muted-foreground">
							{t("reviewThread.bar.localDiffHint")}
						</li>
					) : null}
					{threads.map((thread) => {
						const root = reviewThreadRoot(thread);
						const state = reviewThreadState(thread);
						const line = reviewCommentLine(root);
						const replies = thread.comments.length - 1;
						return (
							<li key={thread.id}>
								<button
									type="button"
									onClick={() => onSelectThread(thread)}
									className={cn(
										"flex w-full items-center gap-2 rounded-md px-1.5 py-1 text-left text-[11px] hover:bg-accent/60",
										thread.id === activeThreadId && "bg-primary/[0.08]",
										state === "resolved" && "opacity-60",
									)}
								>
									<span className="w-9 shrink-0 font-mono text-[10px] text-muted-foreground tabular-nums">
										{line != null && state !== "outdated" ? `L${line}` : "—"}
									</span>
									<span className="shrink-0 font-medium text-foreground">
										{root.author?.login ? `@${root.author.login}` : t("reviewThread.reviewer")}
									</span>
									<span className="min-w-0 flex-1 truncate text-muted-foreground">
										{firstLine(root.body)}
									</span>
									{replies > 0 ? (
										<span className="shrink-0 text-[10px] text-muted-foreground tabular-nums">
											{t("reviewThread.bar.replies", { count: replies })}
										</span>
									) : null}
									{state !== "open" ? <ReviewThreadStateChip state={state} /> : null}
								</button>
							</li>
						);
					})}
				</ul>
			) : null}
		</div>
	);
}
