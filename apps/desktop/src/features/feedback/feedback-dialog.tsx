import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
	Bug,
	Check,
	ExternalLink,
	Lightbulb,
	Loader2,
	MessageSquareMore,
	RefreshCw,
	Send,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { openExternal } from "@/lib/shell-api";
import { cn } from "@/lib/utils";
import {
	emptyFeedbackDraft,
	feedbackApi,
	feedbackError,
	feedbackStatus,
	FEEDBACK_REPOSITORY,
	readFeedbackDraft,
	readFeedbackHistory,
	saveFeedbackDraft,
	saveFeedbackHistory,
	type FeedbackDraft,
	type FeedbackIssue,
	type FeedbackPage,
} from "./feedback-api";
import "./feedback.css";

export function FeedbackDialog({
	open,
	onOpenChange,
	onOpenSettings,
}: {
	open: boolean;
	onOpenChange: (value: boolean) => void;
	onOpenSettings: () => void;
}) {
	const { t, i18n } = useTranslation("common");
	const queryClient = useQueryClient();
	const [tab, setTab] = useState("new");
	const [draft, setDraft] = useState(readFeedbackDraft);
	const [reviewing, setReviewing] = useState(false);
	const [sending, setSending] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [saved, setSaved] = useState(true);
	const [submitted, setSubmitted] = useState<FeedbackIssue | null>(null);
	const [page, setPage] = useState(1);
	const sendingRef = useRef(false);
	const account = useQuery({
		queryKey: ["dccFeedbackContext"],
		queryFn: feedbackApi.context,
		enabled: open,
		retry: false,
		staleTime: 0,
	});
	const login = account.data?.login;
	const historyKey = ["dccFeedbackHistory", login, page];
	const history = useQuery({
		queryKey: historyKey,
		queryFn: async () => {
			const result = await feedbackApi.list(login!, page);
			saveFeedbackHistory(login!, page, result);
			return result;
		},
		enabled: open && tab === "history" && Boolean(login),
		retry: false,
		staleTime: 0,
		initialData: () => (login ? readFeedbackHistory(login, page) : undefined),
		initialDataUpdatedAt: 0,
	});
	useEffect(() => {
		setPage(1);
		setReviewing(false);
		setSubmitted(null);
	}, [login]);

	function edit(patch: Partial<FeedbackDraft>) {
		const next = { ...draft, ...patch };
		setDraft(next);
		setSaved(saveFeedbackDraft(next));
		setError(null);
		setReviewing(false);
	}
	function startNew() {
		const next = emptyFeedbackDraft();
		setDraft(next);
		setSaved(saveFeedbackDraft(next));
		setReviewing(false);
		setError(null);
		setTab("new");
	}
	async function openIssue(number?: number) {
		try {
			await openExternal(
				number
					? `https://github.com/${FEEDBACK_REPOSITORY}/issues/${number}`
					: `https://github.com/${FEEDBACK_REPOSITORY}/issues?q=${encodeURIComponent(`is:issue author:${login ?? "@me"}`)}`,
			);
		} catch {
			setError("openFailed");
		}
	}
	async function submit() {
		if (!account.data || sendingRef.current) return;
		sendingRef.current = true;
		setSending(true);
		setError(null);
		const { pending: _pending, ...fields } = draft;
		const input = draft.pending ?? { ...fields, login: account.data.login };
		const pending = { ...draft, pending: input };
		setDraft(pending);
		setSaved(saveFeedbackDraft(pending));
		try {
			const result = await feedbackApi.create(input);
			const key = ["dccFeedbackHistory", input.login, 1];
			const previous =
				queryClient.getQueryData<FeedbackPage>(key) ??
				readFeedbackHistory(input.login, 1);
			const updated = {
				issues: [
					result,
					...(previous?.issues ?? []).filter(
						(item) => item.number !== result.number,
					),
				].slice(0, 30),
				hasNext: previous?.hasNext ?? false,
			};
			saveFeedbackHistory(input.login, 1, updated);
			queryClient.setQueryData(key, updated);
			startNew();
			setSubmitted(result);
			setPage(1);
			setTab("history");
			void queryClient.invalidateQueries({
				queryKey: ["dccFeedbackHistory", input.login],
			});
		} catch (failure) {
			const mapped = feedbackError(failure);
			// An IPC failure may hide a successful publication; preserve the original request.
			const reason = mapped === "failed" ? "uncertain" : mapped;
			setError(reason);
			if (reason !== "uncertain" && reason !== "accountChanged") {
				const next = { ...draft, pending: null };
				setDraft(next);
				setSaved(saveFeedbackDraft(next));
			}
			if (reason === "accountChanged") {
				setReviewing(false);
				void account.refetch();
			}
		} finally {
			sendingRef.current = false;
			setSending(false);
		}
	}
	const canReview =
		Boolean(login) &&
		!account.isError &&
		draft.title.trim().length > 0 &&
		draft.description.trim().length > 0;
	const locked = sending || Boolean(draft.pending);
	const showReview = reviewing || Boolean(draft.pending);
	const reportTitle = `[${draft.category === "bug" ? "Bug" : draft.category === "improvement" ? "Feature" : "Feedback"}]: ${draft.title.trim().replace(/\s+/g, " ")}`;
	const environment = account.data;

	return (
		<Dialog
			open={open}
			onOpenChange={(value) => {
				if (!sendingRef.current) onOpenChange(value);
			}}
		>
			<DialogContent
				className="dcc-feedback-dialog sm:max-w-[680px]"
				showCloseButton={!sending}
			>
				<DialogHeader className="pr-8">
					<div className="flex items-center gap-3">
						<span className="dcc-feedback-mark">
							<MessageSquareMore size={21} strokeWidth={1.7} aria-hidden />
						</span>
						<div>
							<DialogTitle>{t("feedback.title")}</DialogTitle>
							<DialogDescription className="mt-1.5 text-xs">
								{t("feedback.subtitle")}
							</DialogDescription>
						</div>
					</div>
				</DialogHeader>
				<Tabs value={tab} onValueChange={setTab}>
					<TabsList className="w-full">
						<TabsTrigger className="py-2" value="new" disabled={sending}>
							{t("feedback.new")}
						</TabsTrigger>
						<TabsTrigger className="py-2" value="history" disabled={sending}>
							{t("feedback.mine")}
						</TabsTrigger>
					</TabsList>
					<div className="flex flex-wrap items-center justify-between gap-2 px-1 pt-2 text-xs text-muted-foreground">
						<span className="break-all">{FEEDBACK_REPOSITORY}</span>
						{login && <span>{t("feedback.account", { login })}</span>}
					</div>
					{account.isLoading && (
						<p className="dcc-feedback-notice" role="status">
							<Loader2 size={14} className="animate-spin" />
							{t("feedback.connecting")}
						</p>
					)}
					{account.isError && (
						<div className="dcc-feedback-notice" role="alert">
							<p>{t("feedback.connectHint")}</p>
							<Button
								size="sm"
								variant="outline"
								onClick={() => {
									onOpenChange(false);
									onOpenSettings();
								}}
							>
								{t("feedback.settings")}
							</Button>
							<Button
								size="sm"
								variant="ghost"
								disabled={account.isFetching}
								onClick={() => void account.refetch()}
							>
								{t("feedback.retry")}
							</Button>
						</div>
					)}
					{error && (
						<div className="dcc-feedback-notice text-destructive" role="alert">
							{t(`feedback.errors.${error}`)}
						</div>
					)}
					{!saved && (
						<p role="status" className="text-xs text-amber-600">
							{t("feedback.storageFailed")}
						</p>
					)}
					<TabsContent value="new" className="mt-3">
						<form
							onSubmit={(event) => {
								event.preventDefault();
								if (showReview) void submit();
								else if (canReview) setReviewing(true);
							}}
						>
							{showReview ? (
								<section
									className="dcc-feedback-review"
									aria-label={t("feedback.preview")}
								>
									<p className="dcc-feedback-eyebrow">
										{t("feedback.preview")}
									</p>
									<h2 className="break-words text-base font-semibold">
										{reportTitle}
									</h2>
									<p className="whitespace-pre-wrap break-words leading-relaxed">
										{draft.description.trim()}
									</p>
									{draft.category === "bug" &&
										(["steps", "expected"] as const).map(
											(field) =>
												draft[field].trim() && (
													<div key={field}>
														<h3 className="mb-1 font-medium">
															{t(`feedback.${field}`)}
														</h3>
														<p className="whitespace-pre-wrap break-words text-muted-foreground">
															{draft[field].trim()}
														</p>
													</div>
												),
										)}
								</section>
							) : (
								<fieldset
									disabled={locked}
									className="flex min-w-0 flex-col gap-4"
								>
									<div
										className="grid grid-cols-3 gap-2"
										role="group"
										aria-label={t("feedback.category")}
									>
										{(
											[
												{ category: "bug", Icon: Bug },
												{ category: "improvement", Icon: Lightbulb },
												{ category: "other", Icon: MessageSquareMore },
											] as const
										).map(({ category, Icon }) => (
											<button
												key={category}
												type="button"
												aria-pressed={draft.category === category}
												className="dcc-feedback-category"
												onClick={() => edit({ category })}
											>
												<Icon size={17} strokeWidth={1.7} aria-hidden />
												{t(`feedback.categories.${category}`)}
											</button>
										))}
									</div>
									<label className="dcc-feedback-field">
										{t("feedback.subject")}
										<Input
											autoFocus
											required
											maxLength={200}
											value={draft.title}
											placeholder={t("feedback.subjectPlaceholder")}
											onChange={(event) => edit({ title: event.target.value })}
										/>
									</label>
									<label className="dcc-feedback-field">
										{t("feedback.description")}
										<textarea
											className="dcc-feedback-textarea"
											required
											maxLength={5000}
											value={draft.description}
											placeholder={t(
												`feedback.descriptionPlaceholder.${draft.category}`,
											)}
											onChange={(event) =>
												edit({ description: event.target.value })
											}
										/>
									</label>
									{draft.category === "bug" && (
										<details className="text-xs text-muted-foreground">
											<summary className="cursor-pointer">
												{t("feedback.reproduction")}
											</summary>
											<div className="mt-3 grid gap-3">
												{(["steps", "expected"] as const).map((field) => (
													<label key={field} className="dcc-feedback-field">
														{t(`feedback.${field}`)}
														<textarea
															className="dcc-feedback-textarea min-h-20"
															maxLength={2000}
															value={draft[field]}
															onChange={(event) =>
																edit({ [field]: event.target.value })
															}
														/>
													</label>
												))}
											</div>
										</details>
									)}
								</fieldset>
							)}
							<div className="my-4 rounded-lg border border-border/70 p-3 text-xs">
								<label className="flex items-center gap-2 font-medium">
									<input
										type="checkbox"
										checked={draft.includeDiagnostics}
										disabled={locked}
										onChange={(event) =>
											edit({ includeDiagnostics: event.target.checked })
										}
									/>
									{t("feedback.diagnostics")}
								</label>
								{draft.includeDiagnostics && environment && (
									<p className="mt-2 text-muted-foreground">
										DCC {environment.version} · {environment.platform} ·{" "}
										{environment.architecture}
									</p>
								)}
							</div>
							<p className="text-xs leading-relaxed text-muted-foreground">
								{t("feedback.publicNotice")}
							</p>
							<div className="mt-5 flex flex-wrap items-center justify-between gap-2 border-t border-border pt-4">
								<span className="text-xs text-muted-foreground">
									{t(
										draft.pending
											? "feedback.pending"
											: saved
												? "feedback.draftSaved"
												: "feedback.draftUnsaved",
									)}
								</span>
								<div className="flex gap-2">
									{showReview && !draft.pending && (
										<Button
											type="button"
											variant="ghost"
											disabled={sending}
											onClick={() => setReviewing(false)}
										>
											{t("feedback.edit")}
										</Button>
									)}
									{draft.pending && (
										<Button
											type="button"
											variant="outline"
											disabled={sending}
											onClick={() => void openIssue()}
										>
											{t("feedback.github")}
										</Button>
									)}
									<Button
										type="submit"
										disabled={
											sending ||
											!canReview ||
											Boolean(draft.pending && draft.pending.login !== login)
										}
									>
										{sending ? (
											<Loader2 size={14} className="animate-spin" />
										) : (
											<Send size={14} />
										)}
										{t(
											sending
												? "feedback.sending"
												: draft.pending
													? "feedback.checkDelivery"
													: showReview
														? "feedback.publish"
														: "feedback.review",
										)}
									</Button>
								</div>
							</div>
							{draft.pending && !sending && (
								<div className="mt-3 flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
									<span>
										{t("feedback.pendingAccount", {
											login: draft.pending.login,
										})}
									</span>
									<Button
										type="button"
										variant="ghost"
										size="sm"
										onClick={startNew}
									>
										{t("feedback.another")}
									</Button>
								</div>
							)}
						</form>
					</TabsContent>
					<TabsContent value="history" className="mt-3">
						{submitted && (
							<div
								className="dcc-feedback-notice text-emerald-600"
								role="status"
							>
								<Check size={16} />
								{t("feedback.sent", { number: submitted.number })}
								<Button
									size="sm"
									variant="ghost"
									onClick={() => void openIssue(submitted.number)}
								>
									{t("feedback.github")}
									<ExternalLink size={13} />
								</Button>
							</div>
						)}
						<div className="mb-3 flex items-center justify-between gap-2">
							<p className="text-xs text-muted-foreground">
								{t("feedback.historyHint")}
							</p>
							<Button
								size="icon-sm"
								variant="ghost"
								aria-label={t("feedback.refresh")}
								disabled={!login || history.isFetching}
								onClick={() => void history.refetch()}
							>
								<RefreshCw
									size={14}
									className={cn(history.isFetching && "animate-spin")}
								/>
							</Button>
						</div>
						{history.isError && (
							<p className="dcc-feedback-notice" role="alert">
								{t(history.data ? "feedback.cached" : "feedback.errors.failed")}
							</p>
						)}
						{history.isLoading && (
							<p className="dcc-feedback-notice" role="status">
								<Loader2 size={14} className="animate-spin" />
								{t("feedback.loading")}
							</p>
						)}
						{history.data?.issues.length === 0 && (
							<div className="dcc-feedback-empty">
								<MessageSquareMore size={30} strokeWidth={1.4} />
								<h2>{t("feedback.empty")}</h2>
								<p>{t("feedback.emptyHint")}</p>
								<Button variant="outline" onClick={() => setTab("new")}>
									{t("feedback.new")}
								</Button>
							</div>
						)}
						<ul className="grid gap-2">
							{history.data?.issues.map((item) => (
								<li key={item.number}>
									<button
										className="dcc-feedback-item"
										onClick={() => void openIssue(item.number)}
									>
										<div className="flex items-start justify-between gap-3">
											<span className="min-w-0 break-words font-medium">
												{item.title}
											</span>
											<ExternalLink
												size={14}
												className="mt-0.5 shrink-0 text-muted-foreground"
												aria-hidden
											/>
										</div>
										<div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
											<span>#{item.number}</span>
											<span>·</span>
											<span>{t(`feedback.categories.${item.category}`)}</span>
											<time dateTime={item.createdAt}>
												{new Date(item.createdAt).toLocaleDateString(
													i18n.language,
												)}
											</time>
											<span
												className="dcc-feedback-status"
												data-state={feedbackStatus(item)}
											>
												{t(`feedback.status.${feedbackStatus(item)}`)}
											</span>
										</div>
									</button>
								</li>
							))}
						</ul>
						{login && (
							<div className="mt-4 flex items-center justify-between gap-2">
								<Button
									variant="ghost"
									size="sm"
									onClick={() => void openIssue()}
								>
									{t("feedback.github")}
									<ExternalLink size={13} />
								</Button>
								<div className="flex items-center gap-2 text-xs">
									<Button
										variant="outline"
										size="sm"
										disabled={page === 1 || history.isFetching}
										onClick={() => setPage(page - 1)}
									>
										{t("feedback.previous")}
									</Button>
									<span>{page}</span>
									<Button
										variant="outline"
										size="sm"
										disabled={!history.data?.hasNext || history.isFetching}
										onClick={() => setPage(page + 1)}
									>
										{t("feedback.next")}
									</Button>
								</div>
							</div>
						)}
					</TabsContent>
				</Tabs>
			</DialogContent>
		</Dialog>
	);
}
