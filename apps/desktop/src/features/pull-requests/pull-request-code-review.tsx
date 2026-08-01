import type {
	ProviderCatalog,
	ProviderRuntimeConfig,
	PullRequestHubDetailOutput,
	PullRequestHubDraftComment,
	PullRequestHubInlineComment,
	PullRequestHubItem,
	PullRequestHubReviewEvent,
} from "@dcc/contracts";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
	Check,
	ChevronRight,
	CircleAlert,
	FileCode2,
	Loader2,
	MessageSquarePlus,
	Plus,
	Reply,
	Send,
	Sparkles,
	Trash2,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { ComposerProviderModelMenu } from "@/features/composer/ComposerProviderModelMenu";
import { runPullRequestReviewAgent } from "@/lib/session-api";
import {
	pullRequestHubReplyThread,
	pullRequestHubResolveThread,
	pullRequestHubSubmitReview,
} from "@/lib/workspace-api";
import { cn } from "@/lib/utils";
import {
	buildInlineAgentPrompt,
	buildReviewAgentPrompt,
	buildThreadReplyAgentPrompt,
	parseAgentReply,
	parseAgentReview,
} from "./pull-request-agent";
import { parseUnifiedDiff, type UnifiedDiffLine } from "./unified-diff";

type PullRequestCodeReviewProps = {
	item: PullRequestHubItem;
	detail: PullRequestHubDetailOutput | undefined;
	isLoading: boolean;
	error: Error | null;
	providers: ProviderCatalog["providers"];
	selectedProviderId: string | null;
	selectedModelId: string | null;
	selectedProviderRuntime: ProviderRuntimeConfig | null;
	onSelectProvider: (providerId: string) => void;
	onSelectModel: (modelId: string) => void;
};

function draftKey(path: string, line: number, side: string) {
	return `${path}:${side}:${line}`;
}

function DiffRow({
	path,
	row,
	existingComments,
	draft,
	isEditing,
	draftBody,
	onDraftBodyChange,
	onStartComment,
	onCancelComment,
	onSaveComment,
	onRemoveDraft,
	onPrepareInlineWithAgent,
	agentPendingKey,
	replyingToId,
	replyBody,
	onReplyBodyChange,
	onStartReply,
	onCancelReply,
	onSubmitReply,
	onPrepareReplyWithAgent,
	onToggleResolved,
}: {
	path: string;
	row: UnifiedDiffLine;
	existingComments: PullRequestHubDetailOutput["inlineComments"];
	draft: PullRequestHubDraftComment | null;
	isEditing: boolean;
	draftBody: string;
	onDraftBodyChange: (value: string) => void;
	onStartComment: () => void;
	onCancelComment: () => void;
	onSaveComment: () => void;
	onRemoveDraft: () => void;
	onPrepareInlineWithAgent: () => void;
	agentPendingKey: string | null;
	replyingToId: string | null;
	replyBody: string;
	onReplyBodyChange: (value: string) => void;
	onStartReply: (comment: PullRequestHubInlineComment) => void;
	onCancelReply: () => void;
	onSubmitReply: (comment: PullRequestHubInlineComment) => void;
	onPrepareReplyWithAgent: (comment: PullRequestHubInlineComment) => void;
	onToggleResolved: (comment: PullRequestHubInlineComment) => void;
}) {
	const { t } = useTranslation("common");
	const canComment = row.reviewLine != null && row.reviewSide != null;
	if (row.kind === "hunk") {
		return (
			<div className="border-y border-sky-500/15 bg-sky-500/7 px-3 py-1 font-mono text-[10px] text-sky-600 dark:text-sky-400">
				{row.content}
			</div>
		);
	}
	if (row.kind === "meta") {
		return (
			<div className="px-3 py-2 font-mono text-[10px] italic text-muted-foreground">
				{row.content || " "}
			</div>
		);
	}
	return (
		<div>
			<div
				className={cn(
					"group/line grid min-h-6 grid-cols-[28px_42px_42px_minmax(max-content,1fr)] border-b border-border/25 font-mono text-[11px] leading-6",
					row.kind === "addition" && "bg-emerald-500/8",
					row.kind === "deletion" && "bg-red-500/8",
				)}
			>
				<div className="grid place-items-center border-r border-border/30">
					{canComment ? (
						<button
							type="button"
							aria-label={t("pullRequests.code.addInlineComment", {
								line: row.reviewLine,
							})}
							onClick={onStartComment}
							className="grid size-5 place-items-center rounded text-sky-500 opacity-0 transition-opacity hover:bg-sky-500/10 group-hover/line:opacity-100 focus:opacity-100"
						>
							<Plus className="size-3.5" strokeWidth={2.2} />
						</button>
					) : null}
				</div>
				<div className="select-none border-r border-border/30 px-2 text-right text-muted-foreground/60">
					{row.oldLine ?? ""}
				</div>
				<div className="select-none border-r border-border/30 px-2 text-right text-muted-foreground/60">
					{row.newLine ?? ""}
				</div>
				<pre className="min-w-0 whitespace-pre px-2 text-foreground/90">
					<span
						className={cn(
							"mr-2 select-none",
							row.kind === "addition" && "text-emerald-500",
							row.kind === "deletion" && "text-red-500",
						)}
					>
						{row.kind === "addition" ? "+" : row.kind === "deletion" ? "−" : " "}
					</span>
					{row.content}
				</pre>
			</div>

			{existingComments.map((comment) => (
				<div
					key={comment.id}
					className="ml-7 border-b border-l-2 border-sky-500/40 bg-sky-500/5 px-4 py-3"
				>
					<div className="flex items-center gap-2 text-[10px] text-muted-foreground">
						<strong className="font-medium text-foreground">
							@{comment.author?.login ?? t("pullRequests.unknownAuthor")}
						</strong>
						{comment.resolved === true ? (
							<span className="flex items-center gap-1 text-emerald-500">
								<Check className="size-3" /> {t("pullRequests.code.resolved")}
							</span>
						) : null}
					</div>
					<p className="mt-1.5 whitespace-pre-wrap text-[11px] leading-4">{comment.body}</p>
					{comment.threadId ? (
						<div className="mt-2 flex items-center gap-1.5">
							<Button size="xs" variant="ghost" onClick={() => onStartReply(comment)}>
								<Reply className="mr-1 size-3" />{t("pullRequests.code.reply")}
							</Button>
							<Button size="xs" variant="ghost" onClick={() => onPrepareReplyWithAgent(comment)} disabled={agentPendingKey === `reply:${comment.id}`}>
								{agentPendingKey === `reply:${comment.id}` ? <Loader2 className="mr-1 size-3 animate-spin" /> : <Sparkles className="mr-1 size-3" />}
								{t("pullRequests.code.prepareWithAgent")}
							</Button>
							<Button size="xs" variant="ghost" onClick={() => onToggleResolved(comment)}>
								<Check className="mr-1 size-3" />{comment.resolved ? t("pullRequests.code.reopenThread") : t("pullRequests.code.resolveThread")}
							</Button>
						</div>
					) : null}
					{replyingToId === comment.id ? (
						<div className="mt-2 rounded-lg border border-border bg-background/70 p-2">
							<Textarea autoFocus value={replyBody} onChange={(event) => onReplyBodyChange(event.target.value)} placeholder={t("pullRequests.code.replyPlaceholder")} className="min-h-16 resize-y text-[11px]" />
							<div className="mt-2 flex justify-end gap-2">
								<Button size="xs" variant="ghost" onClick={onCancelReply}>{t("pullRequests.code.cancel")}</Button>
								<Button size="xs" disabled={!replyBody.trim()} onClick={() => onSubmitReply(comment)}><Send className="mr-1 size-3" />{t("pullRequests.code.publishReply")}</Button>
							</div>
						</div>
					) : null}
				</div>
			))}

			{draft ? (
				<div className="ml-7 flex items-start gap-3 border-b border-l-2 border-amber-500/50 bg-amber-500/5 px-4 py-3">
					<div className="min-w-0 flex-1">
						<span className="text-[9px] font-semibold uppercase tracking-[0.12em] text-amber-600 dark:text-amber-400">
							{t("pullRequests.code.pendingComment")}
						</span>
						<p className="mt-1 whitespace-pre-wrap text-[11px] leading-4">{draft.body}</p>
					</div>
					<Button
						variant="ghost"
						size="icon-xs"
						aria-label={t("pullRequests.code.removePendingComment")}
						onClick={onRemoveDraft}
					>
						<Trash2 className="size-3.5" />
					</Button>
				</div>
			) : null}

			{isEditing ? (
				<div className="ml-7 w-[348px] rounded-br-xl border-b border-l-2 border-sky-500 bg-card px-3 py-3 shadow-sm">
					<Textarea
						autoFocus
						value={draftBody}
						onChange={(event) => onDraftBodyChange(event.target.value)}
						placeholder={t("pullRequests.code.inlinePlaceholder", { path })}
						className="min-h-20 resize-y text-[12px]"
					/>
					<div className="mt-2 flex justify-end gap-2">
						<Tooltip>
							<TooltipTrigger asChild>
								<Button
									size="icon-xs"
									variant="ghost"
									aria-label={t("pullRequests.code.prepareWithAgent")}
									onClick={onPrepareInlineWithAgent}
									disabled={agentPendingKey?.startsWith("inline:")}
								>
									{agentPendingKey?.startsWith("inline:") ? <Loader2 className="size-3 animate-spin" /> : <Sparkles className="size-3" />}
								</Button>
							</TooltipTrigger>
							<TooltipContent side="top">{t("pullRequests.code.prepareWithAgent")}</TooltipContent>
						</Tooltip>
						<Button size="xs" variant="ghost" onClick={onCancelComment}>
							{t("pullRequests.code.cancel")}
						</Button>
						<Button size="xs" disabled={!draftBody.trim()} onClick={onSaveComment}>
							{t("pullRequests.code.addToReview")}
						</Button>
					</div>
				</div>
			) : null}
		</div>
	);
}

export function PullRequestCodeReview({
	item,
	detail,
	isLoading,
	error,
	providers,
	selectedProviderId,
	selectedModelId,
	selectedProviderRuntime,
	onSelectProvider,
	onSelectModel,
}: PullRequestCodeReviewProps) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [selectedPath, setSelectedPath] = useState<string | null>(null);
	const [drafts, setDrafts] = useState<PullRequestHubDraftComment[]>([]);
	const [editingKey, setEditingKey] = useState<string | null>(null);
	const [draftBody, setDraftBody] = useState("");
	const [reviewBody, setReviewBody] = useState("");
	const [reviewEvent, setReviewEvent] = useState<PullRequestHubReviewEvent>("comment");
	const [submitted, setSubmitted] = useState(false);
	const [submitWarning, setSubmitWarning] = useState<string | null>(null);
	const [agentPanelOpen, setAgentPanelOpen] = useState(false);
	const [agentInstruction, setAgentInstruction] = useState("");
	const [agentPendingKey, setAgentPendingKey] = useState<string | null>(null);
	const [agentError, setAgentError] = useState<string | null>(null);
	const [replyingToId, setReplyingToId] = useState<string | null>(null);
	const [replyBody, setReplyBody] = useState("");
	const files = detail?.files ?? [];
	const reviewProviders = providers.filter((provider) => provider.capabilities.supportsReadOnlyDelegation);
	const reviewProvider = reviewProviders.find((provider) => provider.id === selectedProviderId) ?? reviewProviders[0] ?? null;
	const reviewModel = reviewProvider?.models.find((model) => model.id === selectedModelId) ?? reviewProvider?.models[0] ?? null;

	useEffect(() => {
		if (files.length === 0) {
			setSelectedPath(null);
			return;
		}
		if (!selectedPath || !files.some((file) => file.path === selectedPath)) {
			setSelectedPath(files[0].path);
		}
	}, [files, selectedPath]);

	const selectedFile = files.find((file) => file.path === selectedPath) ?? null;
	const rows = useMemo(() => parseUnifiedDiff(selectedFile?.patch), [selectedFile?.patch]);
	const submitMutation = useMutation({
		mutationFn: () =>
			pullRequestHubSubmitReview({
				repositoryRoot: item.repositoryRoot,
				number: item.number,
				body: reviewBody.trim() || null,
				event: reviewEvent,
				comments: drafts,
				forgeLogin: item.forgeLogin,
			}),
		onSuccess: async (result) => {
			setDrafts((current) => current.slice(result.submittedCommentCount));
			if (result.bodySubmitted) setReviewBody("");
			if (result.decisionSubmitted) setReviewEvent("comment");
			setSubmitted(result.submitted);
			setSubmitWarning(result.warning);
			await Promise.all([
				queryClient.invalidateQueries({ queryKey: ["pullRequestHub", "detail", item.id] }),
				queryClient.invalidateQueries({ queryKey: ["pullRequestHub", "detailCode", item.id] }),
				queryClient.invalidateQueries({ queryKey: ["pullRequestHub", "list"] }),
			]);
		},
	});
	const replyMutation = useMutation({
		mutationFn: (comment: PullRequestHubInlineComment) => {
			const threadRoot = detail?.inlineComments.find((entry) => entry.threadId === comment.threadId) ?? comment;
			return pullRequestHubReplyThread({
			repositoryRoot: item.repositoryRoot,
			number: item.number,
			commentId: threadRoot.id,
			threadId: comment.threadId ?? "",
			body: replyBody,
			forgeLogin: item.forgeLogin,
			});
		},
		onSuccess: async () => {
			setReplyingToId(null);
			setReplyBody("");
			await queryClient.invalidateQueries({ queryKey: ["pullRequestHub", "detailCode", item.id] });
		},
	});
	const resolveMutation = useMutation({
		mutationFn: (comment: PullRequestHubInlineComment) => pullRequestHubResolveThread({
			repositoryRoot: item.repositoryRoot,
			number: item.number,
			threadId: comment.threadId ?? "",
			resolved: !comment.resolved,
			forgeLogin: item.forgeLogin,
		}),
		onSuccess: async () => {
			await queryClient.invalidateQueries({ queryKey: ["pullRequestHub", "detailCode", item.id] });
		},
	});

	const runAgent = async (prompt: string) => {
		if (!reviewProvider) throw new Error(t("pullRequests.code.noReviewProvider"));
		const result = await runPullRequestReviewAgent({
			workingDirectory: item.repositoryRoot,
			providerId: reviewProvider.id,
			model: reviewModel?.id ?? null,
			providerRuntime: reviewProvider.id === selectedProviderId ? selectedProviderRuntime : null,
			prompt,
		});
		return result.response;
	};
	const prepareFullReview = async () => {
		if (!detail || agentPendingKey) return;
		setAgentPendingKey("review");
		setAgentError(null);
		try {
			const result = parseAgentReview(await runAgent(buildReviewAgentPrompt(item, detail, agentInstruction)), detail);
			setReviewBody(result.summary);
			setDrafts((current) => {
				const generatedKeys = new Set(result.comments.map((draft) => draftKey(draft.path, draft.line, draft.side)));
				return [...current.filter((draft) => !generatedKeys.has(draftKey(draft.path, draft.line, draft.side))), ...result.comments];
			});
			setAgentPanelOpen(false);
			setSubmitted(false);
		} catch (error) {
			setAgentError(error instanceof Error ? error.message : t("pullRequests.code.agentError"));
		} finally {
			setAgentPendingKey(null);
		}
	};
	const prepareInline = async (row: UnifiedDiffLine) => {
		if (!selectedFile || row.reviewLine == null || row.reviewSide == null || agentPendingKey) return;
		const key = draftKey(selectedFile.path, row.reviewLine, row.reviewSide);
		setAgentPendingKey(`inline:${key}`);
		setAgentError(null);
		try {
			setDraftBody(parseAgentReply(await runAgent(buildInlineAgentPrompt(item, selectedFile.path, row, selectedFile.patch, agentInstruction))));
			setEditingKey(key);
		} catch (error) {
			setAgentError(error instanceof Error ? error.message : t("pullRequests.code.agentError"));
		} finally {
			setAgentPendingKey(null);
		}
	};
	const prepareReply = async (comment: PullRequestHubInlineComment) => {
		if (!selectedFile || agentPendingKey) return;
		setAgentPendingKey(`reply:${comment.id}`);
		setAgentError(null);
		try {
			setReplyBody(parseAgentReply(await runAgent(buildThreadReplyAgentPrompt(item, comment, selectedFile.patch, agentInstruction))));
			setReplyingToId(comment.id);
		} catch (error) {
			setAgentError(error instanceof Error ? error.message : t("pullRequests.code.agentError"));
		} finally {
			setAgentPendingKey(null);
		}
	};

	const startComment = (row: UnifiedDiffLine) => {
		if (row.reviewLine == null || row.reviewSide == null || !selectedFile) return;
		setEditingKey(draftKey(selectedFile.path, row.reviewLine, row.reviewSide));
		setDraftBody("");
	};
	const saveComment = (row: UnifiedDiffLine) => {
		if (row.reviewLine == null || row.reviewSide == null || !selectedFile || !draftBody.trim()) {
			return;
		}
		const next: PullRequestHubDraftComment = {
			path: selectedFile.path,
			body: draftBody.trim(),
			line: row.reviewLine,
			side: row.reviewSide,
		};
		const key = draftKey(next.path, next.line, next.side);
		setDrafts((current) => [...current.filter((draft) => draftKey(draft.path, draft.line, draft.side) !== key), next]);
		setEditingKey(null);
		setDraftBody("");
		setSubmitted(false);
	};

	const canSubmit =
		!submitMutation.isPending &&
		(reviewEvent === "approve" || drafts.length > 0 || Boolean(reviewBody.trim())) &&
		(reviewEvent !== "request_changes" || Boolean(reviewBody.trim()));

	if (isLoading) {
		return <div className="grid h-full place-items-center"><Loader2 className="size-5 animate-spin text-muted-foreground" /></div>;
	}
	if (error) {
		return (
			<div className="grid h-full place-items-center p-8">
				<div className="max-w-md rounded-xl border border-red-500/20 bg-red-500/5 p-4 text-[12px] text-red-500">
					{error.message}
				</div>
			</div>
		);
	}
	if (files.length === 0) {
		return (
			<div className="grid h-full place-items-center text-center">
				<div>
					<FileCode2 className="mx-auto size-7 text-muted-foreground/60" />
					<p className="mt-3 text-[13px] font-medium">{t("pullRequests.code.noFiles")}</p>
					<p className="mt-1 text-[11px] text-muted-foreground">{t("pullRequests.code.noFilesHint")}</p>
				</div>
			</div>
		);
	}

	return (
		<div className="flex h-full min-h-0 flex-col">
			{agentPanelOpen ? (
				<section className="shrink-0 border-b border-violet-500/20 bg-violet-500/5 px-4 py-3">
					<div className="flex items-center gap-2">
						<Sparkles className="size-4 text-violet-500" />
						<div className="min-w-0 flex-1">
							<strong className="text-[12px] font-medium">{t("pullRequests.code.agentReviewTitle")}</strong>
							<p className="text-[10px] text-muted-foreground">{t("pullRequests.code.agentReviewHint")}</p>
						</div>
						<ComposerProviderModelMenu providers={reviewProviders} selectedProviderId={reviewProvider?.id ?? null} selectedModelId={reviewModel?.id ?? null} onSelectProvider={onSelectProvider} onSelectModel={onSelectModel} disabled={agentPendingKey != null} />
						<Button size="xs" variant="ghost" onClick={() => setAgentPanelOpen(false)} disabled={agentPendingKey != null}>{t("pullRequests.code.cancel")}</Button>
						<Button size="xs" onClick={() => void prepareFullReview()} disabled={!reviewProvider || agentPendingKey != null}>
							{agentPendingKey === "review" ? <Loader2 className="mr-1 size-3 animate-spin" /> : <Sparkles className="mr-1 size-3" />}
							{t("pullRequests.code.generateDrafts")}
						</Button>
					</div>
					<Textarea value={agentInstruction} onChange={(event) => setAgentInstruction(event.target.value)} placeholder={t("pullRequests.code.agentInstructionPlaceholder")} className="mt-2 min-h-14 resize-y bg-background/70 text-[11px]" />
				</section>
			) : null}
			<div className="flex min-h-0 flex-1">
				<aside className="w-52 shrink-0 overflow-y-auto border-r border-border/70 bg-sidebar/25 p-2 [scrollbar-width:thin]">
					<div className="px-2 pb-2 pt-1 text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
						{t("pullRequests.code.changedFiles", { count: files.length })}
					</div>
					<div className="space-y-0.5">
						{files.map((file) => (
							<button
								key={file.path}
								type="button"
								onClick={() => setSelectedPath(file.path)}
								className={cn(
									"flex w-full items-center gap-2 rounded-lg px-2 py-2 text-left text-[11px] transition-colors",
									selectedPath === file.path ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
								)}
							>
								<FileCode2 className="size-3.5 shrink-0" />
								<span className="min-w-0 flex-1 truncate" title={file.path}>{file.path}</span>
								<span className="shrink-0 text-[9px]"><b className="font-medium text-emerald-500">+{file.additions}</b> <b className="font-medium text-red-500">−{file.deletions}</b></span>
								<ChevronRight className="size-3 shrink-0 opacity-50" />
							</button>
						))}
					</div>
				</aside>

				<section className="min-w-0 flex-1 overflow-auto bg-background [scrollbar-width:thin]">
					{selectedFile ? (
						<div className="min-w-max">
							<div className="sticky top-0 z-10 flex h-9 min-w-[620px] items-center border-b border-border bg-background/95 px-3 text-[11px] backdrop-blur">
								<FileCode2 className="mr-2 size-3.5 text-muted-foreground" />
								<strong className="font-medium">{selectedFile.path}</strong>
								<span className="ml-3 text-[10px] text-muted-foreground">{t(`pullRequests.code.fileStatuses.${selectedFile.status}`, { defaultValue: selectedFile.status })}</span>
							</div>
							{rows.length > 0 ? rows.map((row) => {
								const key = row.reviewLine != null && row.reviewSide ? draftKey(selectedFile.path, row.reviewLine, row.reviewSide) : null;
								const draft = key ? drafts.find((entry) => draftKey(entry.path, entry.line, entry.side) === key) ?? null : null;
								const existingComments = detail?.inlineComments.filter((comment) => comment.path === selectedFile.path && comment.line === row.reviewLine && comment.side === row.reviewSide) ?? [];
								return (
									<DiffRow
										key={row.id}
										path={selectedFile.path}
										row={row}
										existingComments={existingComments}
										draft={draft}
										isEditing={key != null && editingKey === key}
										draftBody={draftBody}
										onDraftBodyChange={setDraftBody}
										onStartComment={() => startComment(row)}
										onCancelComment={() => { setEditingKey(null); setDraftBody(""); }}
										onSaveComment={() => saveComment(row)}
										onRemoveDraft={() => key && setDrafts((current) => current.filter((entry) => draftKey(entry.path, entry.line, entry.side) !== key))}
										onPrepareInlineWithAgent={() => void prepareInline(row)}
										agentPendingKey={agentPendingKey}
										replyingToId={replyingToId}
										replyBody={replyBody}
										onReplyBodyChange={setReplyBody}
										onStartReply={(comment) => { setReplyingToId(comment.id); setReplyBody(""); }}
										onCancelReply={() => { setReplyingToId(null); setReplyBody(""); }}
										onSubmitReply={(comment) => replyMutation.mutate(comment)}
										onPrepareReplyWithAgent={(comment) => void prepareReply(comment)}
										onToggleResolved={(comment) => resolveMutation.mutate(comment)}
									/>
								);
							}) : (
								<div className="grid h-48 min-w-[620px] place-items-center text-[11px] text-muted-foreground">
									{t("pullRequests.code.patchUnavailable")}
								</div>
							)}
						</div>
					) : null}
				</section>
			</div>

			<footer className="shrink-0 border-t border-border bg-background px-4 py-3">
				<div className="flex items-center gap-2">
					<MessageSquarePlus className="size-4 text-muted-foreground" />
					<strong className="text-[12px] font-medium">{t("pullRequests.code.submitReview")}</strong>
					{drafts.length > 0 ? <span className="rounded-full bg-amber-500/10 px-2 py-0.5 text-[9px] font-medium text-amber-600 dark:text-amber-400">{t("pullRequests.code.pendingCount", { count: drafts.length })}</span> : null}
					{item.forgeLogin ? <span className="text-[9px] text-muted-foreground">{t("pullRequests.code.submittingAs", { login: item.forgeLogin })}</span> : null}
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								size="icon-xs"
								variant="outline"
								aria-label={t("pullRequests.code.reviewWithAgent")}
								aria-expanded={agentPanelOpen}
								onClick={() => { setAgentPanelOpen((current) => !current); setAgentError(null); }}
								disabled={reviewProviders.length === 0 || agentPendingKey != null}
							>
								<Sparkles className="size-3" />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="top">{t("pullRequests.code.reviewWithAgent")}</TooltipContent>
					</Tooltip>
					<div className="ml-auto flex items-center gap-1 rounded-lg bg-muted/50 p-1">
						{(["comment", "approve", "request_changes"] as const).map((event) => {
							const unsupported = event === "approve" ? !detail?.reviewCapabilities.approve : event === "request_changes" ? !detail?.reviewCapabilities.requestChanges : false;
							const ownPullRequest = item.createdByViewer && event !== "comment";
							const draftRestriction = item.isDraft && event !== "comment";
							const disabled = unsupported || ownPullRequest || draftRestriction;
							const disabledReason = unsupported
								? t("pullRequests.code.unsupportedReviewAction")
								: ownPullRequest
									? t("pullRequests.code.ownPullRequestRestriction")
									: draftRestriction
										? t("pullRequests.code.draftRestriction")
										: undefined;
							return (
								<button
									key={event}
									type="button"
									disabled={disabled}
									title={disabledReason}
									onClick={() => { setReviewEvent(event); setSubmitted(false); setSubmitWarning(null); }}
									className={cn("rounded-md px-2.5 py-1 text-[10px] font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-35", reviewEvent === event ? "bg-background text-foreground shadow-sm ring-1 ring-border/70" : "text-muted-foreground hover:text-foreground")}
								>
									{t(`pullRequests.code.reviewEvents.${event}`)}
								</button>
							);
						})}
					</div>
				</div>
				<div className="mt-2 flex items-end gap-2 rounded-xl border border-border bg-muted/25 p-2 focus-within:border-ring">
					<Textarea
						value={reviewBody}
						onChange={(event) => { setReviewBody(event.target.value); setSubmitted(false); setSubmitWarning(null); }}
						placeholder={t(`pullRequests.code.reviewPlaceholders.${reviewEvent}`)}
						className="min-h-14 flex-1 resize-y border-0 bg-transparent text-[11px] shadow-none focus-visible:ring-0 dark:bg-transparent"
					/>
					<Button
						size="icon-sm"
						aria-label={t("pullRequests.code.sendReview")}
						title={t("pullRequests.code.sendReview")}
						disabled={!canSubmit}
						onClick={() => submitMutation.mutate()}
					>
						{submitMutation.isPending ? <Loader2 className="size-3.5 animate-spin" /> : <Send className="size-3.5" />}
					</Button>
				</div>
				{submitMutation.isError ? <p className="mt-2 flex items-center gap-1.5 text-[10px] text-red-500"><CircleAlert className="size-3" />{submitMutation.error instanceof Error ? submitMutation.error.message : t("pullRequests.code.reviewError")}</p> : null}
				{replyMutation.isError || resolveMutation.isError || agentError ? <p className="mt-2 flex items-center gap-1.5 text-[10px] text-red-500"><CircleAlert className="size-3" />{agentError ?? (replyMutation.error instanceof Error ? replyMutation.error.message : resolveMutation.error instanceof Error ? resolveMutation.error.message : t("pullRequests.code.reviewError"))}</p> : null}
				{submitWarning ? <p className="mt-2 flex items-center gap-1.5 text-[10px] text-amber-500"><CircleAlert className="size-3" />{submitWarning}</p> : null}
				{submitted ? <p className="mt-2 flex items-center gap-1.5 text-[10px] text-emerald-500"><Check className="size-3" />{t("pullRequests.code.reviewSent")}</p> : null}
			</footer>
		</div>
	);
}
