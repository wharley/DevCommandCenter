import type { PullRequestHubItem } from "@dcc/contracts";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
	ArrowUpRight,
	Check,
	CircleAlert,
	CircleDot,
	Clock3,
	Code2,
	GitBranch,
	GitPullRequest,
	Loader2,
	MessageSquare,
	RefreshCw,
	Search,
	Send,
	UserRoundCheck,
} from "lucide-react";
import { Suspense, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { LazyStreamdown } from "@/components/streamdown-loader";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import {
	pullRequestHubComment,
	pullRequestHubDetail,
	pullRequestHubList,
} from "@/lib/workspace-api";
import { cn } from "@/lib/utils";
import { PullRequestCodeReview } from "./pull-request-code-review";

type PullRequestFilter = "all" | "reviewing" | "mine";
type PullRequestTab = "summary" | "code";

type PullRequestsHubProps = {
	onOpenWorkspace: (workspaceId: string) => void;
	onWorkOnPullRequest: (item: PullRequestHubItem) => Promise<void>;
};

const LIST_QUERY_KEY = ["pullRequestHub", "list"] as const;

function relativeDate(value: string | null, locale: string) {
	if (!value) return "";
	const date = new Date(value);
	if (Number.isNaN(date.getTime())) return "";
	const elapsed = Date.now() - date.getTime();
	const minutes = Math.max(1, Math.floor(elapsed / 60_000));
	const formatter = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
	if (minutes < 60) return formatter.format(-minutes, "minute");
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return formatter.format(-hours, "hour");
	return formatter.format(-Math.floor(hours / 24), "day");
}

function CheckState({ state }: { state: string }) {
	if (state === "success") {
		return <Check className="size-3.5 text-emerald-500" strokeWidth={2.2} />;
	}
	if (state === "failure") {
		return <CircleAlert className="size-3.5 text-red-500" strokeWidth={2} />;
	}
	if (state === "pending") {
		return <Clock3 className="size-3.5 text-amber-500" strokeWidth={2} />;
	}
	return <CircleDot className="size-3.5 text-muted-foreground" strokeWidth={1.8} />;
}

function ActorAvatar({ item }: { item: PullRequestHubItem }) {
	const label = item.author?.name || item.author?.login || "?";
	if (item.author?.avatarUrl) {
		return (
			<img
				src={item.author.avatarUrl}
				alt=""
				className="size-5 rounded-full object-cover ring-1 ring-border"
			/>
		);
	}
	return (
		<span className="grid size-5 place-items-center rounded-full bg-muted text-[9px] font-semibold uppercase text-muted-foreground ring-1 ring-border">
			{label.slice(0, 2)}
		</span>
	);
}

export function PullRequestsHub({
	onOpenWorkspace,
	onWorkOnPullRequest,
}: PullRequestsHubProps) {
	const { t, i18n } = useTranslation("common");
	const queryClient = useQueryClient();
	const [filter, setFilter] = useState<PullRequestFilter>("all");
	const [search, setSearch] = useState("");
	const [selectedId, setSelectedId] = useState<string | null>(null);
	const [activeTab, setActiveTab] = useState<PullRequestTab>("summary");
	const [comment, setComment] = useState("");
	const [workingOnId, setWorkingOnId] = useState<string | null>(null);

	const listQuery = useQuery({
		queryKey: LIST_QUERY_KEY,
		queryFn: () => pullRequestHubList(),
		staleTime: 30_000,
		refetchOnWindowFocus: true,
	});

	const filteredItems = useMemo(() => {
		const normalized = search.trim().toLocaleLowerCase();
		return (listQuery.data?.items ?? []).filter((item) => {
			if (filter === "reviewing" && !item.reviewRequestedForViewer) return false;
			if (filter === "mine" && !item.createdByViewer) return false;
			if (!normalized) return true;
			return `${item.title} ${item.repositoryName} ${item.author?.login ?? ""} ${item.headBranch}`
				.toLocaleLowerCase()
				.includes(normalized);
		});
	}, [filter, listQuery.data?.items, search]);

	useEffect(() => {
		if (filteredItems.length === 0) {
			setSelectedId(null);
			return;
		}
		if (!selectedId || !filteredItems.some((item) => item.id === selectedId)) {
			setSelectedId(filteredItems[0].id);
		}
	}, [filteredItems, selectedId]);

	const selected = filteredItems.find((item) => item.id === selectedId) ?? null;
	const detailQuery = useQuery({
		queryKey: ["pullRequestHub", "detail", selected?.id],
		queryFn: () =>
			pullRequestHubDetail({
				repositoryRoot: selected!.repositoryRoot,
				number: selected!.number,
				forgeLogin: selected!.forgeLogin,
				includeCode: false,
			}),
		enabled: Boolean(selected),
		staleTime: 20_000,
	});
	const codeDetailQuery = useQuery({
		queryKey: ["pullRequestHub", "detailCode", selected?.id],
		queryFn: () =>
			pullRequestHubDetail({
				repositoryRoot: selected!.repositoryRoot,
				number: selected!.number,
				forgeLogin: selected!.forgeLogin,
				includeCode: true,
			}),
		enabled: Boolean(selected) && activeTab === "code",
		staleTime: 20_000,
	});
	const commentMutation = useMutation({
		mutationFn: async () => {
			if (!selected) throw new Error(t("pullRequests.noSelection"));
			return pullRequestHubComment({
				repositoryRoot: selected.repositoryRoot,
				number: selected.number,
				body: comment,
				forgeLogin: selected.forgeLogin,
			});
		},
		onSuccess: async () => {
			setComment("");
			await Promise.all([
				queryClient.invalidateQueries({
					queryKey: ["pullRequestHub", "detail", selected?.id],
				}),
				queryClient.invalidateQueries({ queryKey: LIST_QUERY_KEY }),
			]);
		},
	});

	const handleWorkOnPullRequest = async () => {
		if (!selected || workingOnId) return;
		setWorkingOnId(selected.id);
		try {
			await onWorkOnPullRequest(selected);
		} finally {
			setWorkingOnId(null);
		}
	};

	return (
		<div className="flex h-full min-h-0 bg-background pt-9">
			<section className="flex w-[350px] min-w-[310px] shrink-0 flex-col border-r border-border/70 bg-sidebar/35">
				<header className="shrink-0 border-b border-border/70 px-4 pb-3 pt-2">
					<div className="mb-3 flex items-center justify-between">
						<div>
							<h1 className="text-[15px] font-semibold tracking-[-0.01em]">
								{t("pullRequests.title")}
							</h1>
							<p className="mt-0.5 text-[11px] text-muted-foreground">
								{t("pullRequests.subtitle")}
							</p>
						</div>
						<Button
							variant="ghost"
							size="icon-sm"
							aria-label={t("pullRequests.refresh")}
							onClick={() => void listQuery.refetch()}
							disabled={listQuery.isFetching}
						>
							<RefreshCw className={cn("size-3.5", listQuery.isFetching && "animate-spin")} />
						</Button>
					</div>
					<div className="mb-3 grid grid-cols-[0.8fr_1fr_1.45fr] gap-1 rounded-lg bg-muted/55 p-1">
						{(["all", "reviewing", "mine"] as const).map((value) => (
							<button
								key={value}
								type="button"
								onClick={() => setFilter(value)}
								className={cn(
									"min-w-0 whitespace-nowrap rounded-md px-1.5 py-1.5 text-[10.5px] font-medium transition-colors",
									filter === value
										? "bg-background text-foreground shadow-sm ring-1 ring-border/70"
										: "text-muted-foreground hover:text-foreground",
								)}
							>
								{t(`pullRequests.filters.${value}`)}
							</button>
						))}
					</div>
					<label className="flex h-8 items-center gap-2 rounded-lg border border-border/80 bg-background/80 px-2.5 focus-within:border-ring">
						<Search className="size-3.5 text-muted-foreground" />
						<input
							value={search}
							onChange={(event) => setSearch(event.target.value)}
							placeholder={t("pullRequests.search")}
							className="min-w-0 flex-1 bg-transparent text-[12px] outline-none placeholder:text-muted-foreground"
						/>
					</label>
				</header>

				<div className="min-h-0 flex-1 overflow-y-auto p-2 [scrollbar-width:thin]">
					{listQuery.isPending ? (
						<div className="grid h-48 place-items-center text-muted-foreground">
							<Loader2 className="size-5 animate-spin" />
						</div>
					) : listQuery.isError ? (
						<div className="m-2 rounded-lg border border-red-500/20 bg-red-500/5 p-3 text-[12px] text-red-600 dark:text-red-400">
							{listQuery.error instanceof Error ? listQuery.error.message : t("pullRequests.loadError")}
						</div>
					) : filteredItems.length === 0 ? (
						<div className="flex h-48 flex-col items-center justify-center px-6 text-center">
							<GitPullRequest className="mb-3 size-7 text-muted-foreground/60" />
							<p className="text-[13px] font-medium">{t("pullRequests.empty")}</p>
							<p className="mt-1 text-[11px] leading-4 text-muted-foreground">
								{t("pullRequests.emptyHint")}
							</p>
						</div>
					) : (
						<div className="space-y-1">
							{filteredItems.map((item) => (
								<button
									key={item.id}
									type="button"
									onClick={() => {
										setSelectedId(item.id);
										setActiveTab("summary");
									}}
									className={cn(
										"w-full rounded-xl border px-3 py-2.5 text-left transition-colors",
										selectedId === item.id
											? "border-border bg-accent/70 shadow-sm"
											: "border-transparent hover:bg-accent/40",
									)}
								>
									<div className="flex items-start gap-2.5">
										<div className="mt-0.5 rounded-md bg-emerald-500/10 p-1.5 text-emerald-500">
											<GitPullRequest className="size-3.5" strokeWidth={1.9} />
										</div>
										<div className="min-w-0 flex-1">
											<div className="flex items-center gap-2 text-[10px] text-muted-foreground">
												<span className="truncate font-medium">{item.repositoryName}</span>
												<span>#{item.number}</span>
												<span className="ml-auto shrink-0">{relativeDate(item.updatedAt, i18n.language)}</span>
											</div>
											<p className="mt-1 line-clamp-2 text-[12px] font-medium leading-[17px] text-foreground">
												{item.title}
											</p>
											<div className="mt-2 flex items-center gap-1.5 text-[10px] text-muted-foreground">
												<ActorAvatar item={item} />
												<span className="max-w-24 truncate">{item.author?.login ?? t("pullRequests.unknownAuthor")}</span>
												<span className="truncate">{item.headBranch}</span>
												<span className="ml-auto flex shrink-0 items-center gap-1">
													<CheckState state={item.checksState} />
													{item.additions != null ? <b className="font-medium text-emerald-500">+{item.additions}</b> : null}
													{item.deletions != null ? <b className="font-medium text-red-500">−{item.deletions}</b> : null}
												</span>
											</div>
										</div>
									</div>
								</button>
							))}
						</div>
					)}
				</div>
				{(listQuery.data?.warnings.length ?? 0) > 0 ? (
					<div className="shrink-0 border-t border-border/70 px-3 py-2 text-[10px] text-amber-600 dark:text-amber-400">
						{t("pullRequests.partialWarning", { count: listQuery.data?.warnings.length })}
					</div>
				) : null}
			</section>

			<section className="flex min-w-0 flex-1 flex-col">
				{!selected ? (
					<div className="grid h-full place-items-center text-[13px] text-muted-foreground">
						{t("pullRequests.selectPrompt")}
					</div>
				) : (
					<>
						<header className="shrink-0 border-b border-border/70 px-7 pb-4 pt-3">
							<div className="flex items-center justify-between gap-4">
								<div className="flex items-center gap-1 rounded-lg bg-muted/50 p-1">
									<button
										type="button"
										onClick={() => setActiveTab("summary")}
										className={cn("rounded-md px-3 py-1.5 text-[11px] font-medium", activeTab === "summary" ? "bg-background shadow-sm ring-1 ring-border/70" : "text-muted-foreground")}
									>
										{t("pullRequests.tabs.summary")}
									</button>
									<button
										type="button"
										onClick={() => setActiveTab("code")}
										className={cn("rounded-md px-3 py-1.5 text-[11px] font-medium", activeTab === "code" ? "bg-background shadow-sm ring-1 ring-border/70" : "text-muted-foreground")}
									>
										{t("pullRequests.tabs.code")}
									</button>
								</div>
								<div className="flex items-center gap-2">
									{selected.linkedWorkspaceId ? (
										<Button size="sm" variant="outline" onClick={() => onOpenWorkspace(selected.linkedWorkspaceId!)}>
											{t("pullRequests.openTask")}
										</Button>
									) : (
										<Button size="sm" onClick={() => void handleWorkOnPullRequest()} disabled={Boolean(workingOnId)}>
											{workingOnId === selected.id ? <Loader2 className="mr-1.5 size-3.5 animate-spin" /> : <Code2 className="mr-1.5 size-3.5" />}
											{t("pullRequests.workOn")}
										</Button>
									)}
									<Button
										size="icon-sm"
										variant="ghost"
										aria-label={t("pullRequests.openOnForge")}
										onClick={() => window.open(selected.url, "_blank", "noopener,noreferrer")}
									>
										<ArrowUpRight className="size-3.5" />
									</Button>
								</div>
							</div>
							<h2 className="mt-5 max-w-4xl text-[20px] font-semibold leading-7 tracking-[-0.02em]">
								{selected.title}
							</h2>
							<div className="mt-2 flex items-center gap-2 text-[12px] text-muted-foreground">
								<ActorAvatar item={selected} />
								<span>{selected.author?.name || selected.author?.login}</span>
								<span>·</span>
								<span>{selected.repositoryName} #{selected.number}</span>
								<span>·</span>
								<span>{relativeDate(selected.updatedAt, i18n.language)}</span>
							</div>
						</header>

						<div
							className={cn(
								"min-h-0 flex-1",
								activeTab === "summary" &&
									"overflow-y-auto px-7 py-5 [scrollbar-width:thin]",
							)}
						>
							{activeTab === "code" ? (
								<PullRequestCodeReview
									key={selected.id}
									item={selected}
									detail={codeDetailQuery.data}
									isLoading={codeDetailQuery.isPending}
									error={
										codeDetailQuery.error instanceof Error
											? codeDetailQuery.error
											: null
									}
								/>
							) : (
								<div className="mx-auto max-w-4xl space-y-6">
									<div className="grid grid-cols-[140px_minmax(0,1fr)] gap-x-4 gap-y-3 text-[12px]">
										<span className="flex items-center gap-2 text-muted-foreground"><GitBranch className="size-3.5" />{t("pullRequests.branch")}</span>
										<span className="truncate">{selected.headBranch} <span className="text-muted-foreground">→ {selected.baseBranch}</span></span>
										<span className="flex items-center gap-2 text-muted-foreground"><UserRoundCheck className="size-3.5" />{t("pullRequests.reviewers")}</span>
										<span>{selected.reviewers.length > 0 ? selected.reviewers.map((reviewer) => `@${reviewer.login}`).join(", ") : t("pullRequests.noReviewers")}</span>
										<span className="flex items-center gap-2 text-muted-foreground"><MessageSquare className="size-3.5" />{t("pullRequests.comments")}</span>
										<span>{detailQuery.data?.comments.length ?? selected.commentCount}</span>
										<span className="flex items-center gap-2 text-muted-foreground"><CheckState state={selected.checksState} />{t("pullRequests.checks")}</span>
										<span>{t(`pullRequests.checkStates.${selected.checksState}`, { defaultValue: selected.checksState })}</span>
									</div>

									<section className="border-t border-border/70 pt-5">
										<h3 className="text-[14px] font-medium">{t("pullRequests.description")}</h3>
										<p className="mt-3 whitespace-pre-wrap text-[12px] leading-5 text-foreground/85">
											{detailQuery.data?.body || selected.body || t("pullRequests.noDescription")}
										</p>
									</section>

									<section className="border-t border-border/70 pt-5">
										<h3 className="mb-3 text-[14px] font-medium">{t("pullRequests.checks")}</h3>
										{detailQuery.isPending ? <Loader2 className="size-4 animate-spin text-muted-foreground" /> : (detailQuery.data?.checks.length ?? 0) > 0 ? (
											<div className="space-y-2">{detailQuery.data?.checks.map((check, index) => (
												<div key={`${check.name}-${index}`} className="flex items-center gap-2 rounded-lg border border-border/60 px-3 py-2 text-[12px]"><CheckState state={check.state} /><span className="min-w-0 flex-1 truncate">{check.name}</span><span className="text-[10px] text-muted-foreground">{t(`pullRequests.checkStates.${check.state}`, { defaultValue: check.state })}</span></div>
											))}</div>
										) : <p className="text-[11px] text-muted-foreground">{t("pullRequests.noChecks")}</p>}
									</section>

									<section className="border-t border-border/70 pt-5">
										<h3 className="mb-3 text-[14px] font-medium">{t("pullRequests.activity")}</h3>
						{detailQuery.isError ? <p className="text-[11px] text-red-500">{detailQuery.error instanceof Error ? detailQuery.error.message : t("pullRequests.detailError")}</p> : (detailQuery.data?.comments.length ?? 0) > 0 ? (
							<div className="space-y-3">{detailQuery.data?.comments.map((entry) => (
								<article key={entry.id} className="rounded-xl border border-border/70 bg-card/35 p-3">
									<div className="flex items-center gap-2 text-[10px] text-muted-foreground"><strong className="font-medium text-foreground">@{entry.author?.login ?? t("pullRequests.unknownAuthor")}</strong><span>·</span><span>{relativeDate(entry.createdAt, i18n.language)}</span></div>
									<Suspense fallback={<div className="mt-3 h-12 animate-pulse rounded-lg bg-muted/50" />}>
										<LazyStreamdown
											mode="static"
											controls={false}
											className="mt-2 min-w-0 overflow-hidden text-[12px] leading-5 [&_a]:break-words [&_img]:max-w-full [&_p]:my-2 [&_table]:my-3 [&_table]:text-[11px]"
										>
											{entry.body}
										</LazyStreamdown>
									</Suspense>
								</article>
							))}</div>
										) : <p className="text-[11px] text-muted-foreground">{t("pullRequests.noActivity")}</p>}
									</section>
								</div>
							)}
						</div>

						{activeTab === "summary" ? (
							<footer className="shrink-0 border-t border-border/70 bg-background/95 px-7 py-3">
								<div className="mx-auto flex max-w-4xl items-end gap-2 rounded-xl border border-border bg-muted/25 p-2 focus-within:border-ring">
									<Textarea
										value={comment}
										onChange={(event) => setComment(event.target.value)}
										placeholder={t("pullRequests.commentPlaceholder")}
										className="min-h-12 resize-none border-0 bg-transparent text-[12px] shadow-none focus-visible:ring-0 dark:bg-transparent"
									/>
									<Button
										size="icon-sm"
										aria-label={t("pullRequests.sendComment")}
										disabled={!comment.trim() || commentMutation.isPending}
										onClick={() => commentMutation.mutate()}
									>
										{commentMutation.isPending ? <Loader2 className="size-3.5 animate-spin" /> : <Send className="size-3.5" />}
									</Button>
								</div>
								{commentMutation.isError ? <p className="mx-auto mt-1.5 max-w-4xl text-[10px] text-red-500">{commentMutation.error instanceof Error ? commentMutation.error.message : t("pullRequests.commentError")}</p> : null}
							</footer>
						) : null}
					</>
				)}
			</section>
		</div>
	);
}
