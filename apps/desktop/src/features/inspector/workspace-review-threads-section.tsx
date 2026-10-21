import type { WorkspacePrReviewComment } from "@dcc/contracts";
import { Bot, ChevronRight, MessageSquare } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { ReviewThreadStateChip } from "@/features/editor/review-threads-bar";
import {
	buildReviewThreads,
	buildReviewThreadsAgentPrompt,
	reviewCommentLine,
	reviewThreadRoot,
	reviewThreadState,
	sortReviewThreadsForList,
	type ReviewThread,
} from "@/features/editor/review-threads.logic";
import { cn } from "@/lib/utils";

function firstLine(body: string): string {
	return body.split("\n").map((line) => line.trim()).find(Boolean) ?? "";
}

function splitPath(path: string): { name: string; folder: string } {
	const slash = path.lastIndexOf("/");
	return slash === -1
		? { name: path, folder: "" }
		: { name: path.slice(slash + 1), folder: path.slice(0, slash) };
}

/**
 * The pull request's conversations across every file, under the review state.
 * Pick the ones to hand over and the agent gets one editable request in the
 * composer; replying and resolving stay with the person.
 */
export function WorkspaceReviewThreadsSection({
	comments,
	pullRequestUrl,
	onOpenThread,
	onAskAgent,
}: {
	comments: WorkspacePrReviewComment[];
	pullRequestUrl: string | null;
	onOpenThread: (thread: ReviewThread) => void;
	/** Fills the composer; false when no composer could take it. */
	onAskAgent?: (prompt: string) => boolean;
}) {
	const { t } = useTranslation("common");
	const [open, setOpen] = useState(false);

	const groups = useMemo(() => {
		const byPath = new Map<string, WorkspacePrReviewComment[]>();
		for (const comment of comments) {
			const list = byPath.get(comment.path);
			if (list) list.push(comment);
			else byPath.set(comment.path, [comment]);
		}
		return [...byPath.entries()]
			.sort(([left], [right]) => left.localeCompare(right))
			.map(([path, fileComments]) => ({
				path,
				threads: sortReviewThreadsForList(buildReviewThreads(fileComments)),
			}));
	}, [comments]);
	const threads = useMemo(() => groups.flatMap((group) => group.threads), [groups]);

	// Every conversation still asking for something starts picked; ones that
	// arrive later join the pick, and the person's unpicks are kept.
	const [picked, setPicked] = useState<Set<string>>(() => new Set());
	const seenRef = useRef<Set<string>>(new Set());
	useEffect(() => {
		const fresh = threads.filter(
			(thread) => !seenRef.current.has(thread.id) && reviewThreadState(thread) !== "resolved",
		);
		for (const thread of threads) seenRef.current.add(thread.id);
		if (fresh.length === 0) return;
		setPicked((previous) => {
			const next = new Set(previous);
			for (const thread of fresh) next.add(thread.id);
			return next;
		});
	}, [threads]);

	const pickedThreads = threads.filter((thread) => picked.has(thread.id));
	const counts = { open: 0, outdated: 0, resolved: 0 };
	for (const thread of threads) counts[reviewThreadState(thread)] += 1;
	const summary = [
		counts.open > 0 ? t("reviewThread.bar.open", { count: counts.open }) : null,
		counts.outdated > 0 ? t("reviewThread.bar.outdated", { count: counts.outdated }) : null,
		counts.resolved > 0 ? t("reviewThread.bar.resolved", { count: counts.resolved }) : null,
	]
		.filter(Boolean)
		.join(" · ");

	if (threads.length === 0) return null;

	const toggle = (id: string) =>
		setPicked((previous) => {
			const next = new Set(previous);
			if (next.has(id)) next.delete(id);
			else next.add(id);
			return next;
		});

	const askAgent = () => {
		if (!onAskAgent || pickedThreads.length === 0) return;
		const delivered = onAskAgent(
			buildReviewThreadsAgentPrompt({ threads: pickedThreads, pullRequestUrl }),
		);
		if (delivered) {
			toast.success(t("reviewThread.section.inComposer", { count: pickedThreads.length }));
		}
	};

	return (
		<div
			data-workspace-review-threads
			className={cn(
				"shrink-0 overflow-hidden rounded-md border bg-muted/15",
				counts.open > 0 ? "border-amber-500/25" : "border-border/60",
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
						className={cn("size-3.5 shrink-0 transition-transform", open && "rotate-90")}
					/>
					<MessageSquare className="size-3.5 shrink-0 text-primary" />
					<span className="shrink-0 text-[11px] font-medium">
						{t("reviewThread.section.title")}
					</span>
					<span className="truncate text-[9.5px] font-normal text-muted-foreground">
						{summary}
					</span>
				</Button>
				{onAskAgent ? (
					<Button
						type="button"
						variant="outline"
						size="xs"
						className="h-6 shrink-0 gap-1 px-1.5 text-[10px]"
						disabled={pickedThreads.length === 0}
						onClick={askAgent}
						title={t("reviewThread.section.askAgentHint")}
					>
						<Bot className="size-3" />
						{t("reviewThread.section.askAgent", { count: pickedThreads.length })}
					</Button>
				) : null}
			</div>

			{open ? (
				<div className="max-h-72 overflow-y-auto border-t border-border/40 px-1.5 py-1.5">
					{groups.map((group) => {
						const { name, folder } = splitPath(group.path);
						return (
							<div key={group.path} className="mb-1 last:mb-0">
								<p className="flex min-w-0 items-baseline gap-1.5 px-1 py-0.5" title={group.path}>
									<span className="truncate text-[10.5px] font-medium text-foreground">{name}</span>
									{folder ? (
										<span className="truncate text-[9.5px] text-muted-foreground">{folder}</span>
									) : null}
								</p>
								<ul>
									{group.threads.map((thread) => {
										const root = reviewThreadRoot(thread);
										const state = reviewThreadState(thread);
										const line = reviewCommentLine(root);
										const label = firstLine(root.body);
										return (
											<li
												key={thread.id}
												className={cn(
													"flex items-center gap-1.5 rounded-md px-1 hover:bg-accent/50",
													state === "resolved" && "opacity-60",
												)}
											>
												<input
													type="checkbox"
													className="size-3 shrink-0 accent-primary"
													checked={picked.has(thread.id)}
													onChange={() => toggle(thread.id)}
													aria-label={t("reviewThread.section.pick", { label })}
												/>
												<button
													type="button"
													onClick={() => onOpenThread(thread)}
													className="flex min-w-0 flex-1 items-center gap-1.5 py-1 text-left text-[10.5px]"
												>
													<span className="w-8 shrink-0 font-mono text-[9.5px] text-muted-foreground tabular-nums">
														{line != null && state !== "outdated" ? `L${line}` : "—"}
													</span>
													<span className="shrink-0 font-medium">
														{root.author?.login ? `@${root.author.login}` : t("reviewThread.reviewer")}
													</span>
													<span className="min-w-0 flex-1 truncate text-muted-foreground">{label}</span>
													{state !== "open" ? <ReviewThreadStateChip state={state} /> : null}
												</button>
											</li>
										);
									})}
								</ul>
							</div>
						);
					})}
				</div>
			) : null}
		</div>
	);
}
