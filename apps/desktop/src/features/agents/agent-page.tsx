import type { ProviderCatalog } from "@dcc/contracts";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ChevronDown, ChevronRight, Loader2, Pencil } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import {
	AGENT_AVATAR_ARMS,
	AGENT_AVATAR_COLORS,
	AGENT_AVATAR_EYES,
	type ResidentAgentDraft,
	saveAgent,
} from "@/lib/agents-api";
import { cn } from "@/lib/utils";
import { AgentAvatar } from "./agent-avatar";
import { requestOpenPullRequest } from "./pr-review-jobs";
import { AGENTS_QUERY_KEY, type AgentSessionView, type AgentView } from "./use-agents";

type Providers = ProviderCatalog["providers"];

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
	return (
		<div className="space-y-1.5">
			<div className="text-[12px] font-medium text-foreground">{label}</div>
			{children}
			{hint && <p className="text-[11px] leading-relaxed text-muted-foreground">{hint}</p>}
		</div>
	);
}

function Choice({
	selected,
	label,
	onSelect,
	children,
}: {
	selected: boolean;
	label: string;
	onSelect: () => void;
	children: React.ReactNode;
}) {
	return (
		<button
			type="button"
			aria-label={label}
			aria-pressed={selected}
			onClick={onSelect}
			className={cn(
				"grid size-11 cursor-pointer place-items-center rounded-lg border transition-colors",
				selected ? "border-ring bg-accent" : "border-border/70 hover:bg-accent/50",
			)}
		>
			{children}
		</button>
	);
}

/**
 * What the person can change on a DCC agent: its name, mascot, provider and
 * model, extra instructions, and whether it offers itself. The role is DCC's.
 */
function AgentEditorDialog({
	agent,
	open,
	providers,
	onOpenChange,
}: {
	agent: AgentView;
	open: boolean;
	providers: Providers;
	onOpenChange: (open: boolean) => void;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [draft, setDraft] = useState<ResidentAgentDraft>(agent);
	// Load the stored values when the dialog opens, and only then: the agent
	// object is rebuilt whenever its sessions change, which must not wipe what
	// the person is typing.
	const latestAgent = useRef(agent);
	latestAgent.current = agent;
	useEffect(() => {
		if (!open) return;
		const stored = latestAgent.current;
		setDraft({
			name: stored.name,
			role: stored.role,
			kickoffPrompt: stored.kickoffPrompt,
			offerPrompt: stored.offerPrompt,
			extraInstructions: stored.extraInstructions,
			providerId: stored.providerId,
			model: stored.model,
			avatar: stored.avatar,
		});
	}, [open]);

	const save = useMutation({
		mutationFn: () => saveAgent(agent.id, draft),
		onSuccess: async () => {
			await queryClient.invalidateQueries({ queryKey: AGENTS_QUERY_KEY });
			onOpenChange(false);
		},
		onError: (error) => toast.error(error instanceof Error ? error.message : String(error)),
	});

	const provider = providers.find((candidate) => candidate.id === draft.providerId) ?? null;
	const selectClass =
		"h-8 w-full rounded-lg border border-border/80 bg-background px-2 text-[13px] text-foreground";

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-xl">
				<DialogHeader>
					<DialogTitle>{t("agents.editor.title", { agent: agent.name })}</DialogTitle>
					<DialogDescription>{t("agents.editor.description")}</DialogDescription>
				</DialogHeader>
				<div className="space-y-4">
					<div className="flex items-center gap-4">
						<AgentAvatar avatar={draft.avatar} size={72} />
						<div className="min-w-0 flex-1">
							<Field label={t("agents.editor.name")}>
								<Input
									value={draft.name}
									maxLength={60}
									onChange={(event) => setDraft({ ...draft, name: event.target.value })}
								/>
							</Field>
						</div>
					</div>
					<Field label={t("agents.editor.avatar")}>
						<div className="flex flex-wrap gap-1.5">
							{AGENT_AVATAR_COLORS.map((color) => (
								<Choice
									key={color}
									selected={draft.avatar.color === color}
									label={t(`agents.editor.colors.${color}`)}
									onSelect={() => setDraft({ ...draft, avatar: { ...draft.avatar, color } })}
								>
									<AgentAvatar avatar={{ ...draft.avatar, color }} size={28} />
								</Choice>
							))}
						</div>
						<div className="flex flex-wrap gap-1.5">
							{AGENT_AVATAR_ARMS.map((arms) => (
								<Choice
									key={arms}
									selected={draft.avatar.arms === arms}
									label={t("agents.editor.arms", { count: arms })}
									onSelect={() => setDraft({ ...draft, avatar: { ...draft.avatar, arms } })}
								>
									<AgentAvatar avatar={{ ...draft.avatar, arms }} size={28} />
								</Choice>
							))}
							{AGENT_AVATAR_EYES.map((eyes) => (
								<Choice
									key={eyes}
									selected={draft.avatar.eyes === eyes}
									label={t(`agents.editor.eyes.${eyes}`)}
									onSelect={() => setDraft({ ...draft, avatar: { ...draft.avatar, eyes } })}
								>
									<AgentAvatar avatar={{ ...draft.avatar, eyes }} size={28} />
								</Choice>
							))}
						</div>
					</Field>
					<div className="grid grid-cols-2 gap-3">
						<Field label={t("agents.editor.provider")}>
							<select
								className={selectClass}
								aria-label={t("agents.editor.provider")}
								value={draft.providerId ?? ""}
								onChange={(event) =>
									setDraft({ ...draft, providerId: event.target.value || null, model: null })
								}
							>
								<option value="">{t("agents.editor.providerCurrent")}</option>
								{providers.map((candidate) => (
									<option key={candidate.id} value={candidate.id}>
										{candidate.label}
									</option>
								))}
							</select>
						</Field>
						<Field label={t("agents.editor.model")}>
							<select
								className={selectClass}
								aria-label={t("agents.editor.model")}
								value={draft.model ?? ""}
								disabled={!provider}
								onChange={(event) => setDraft({ ...draft, model: event.target.value || null })}
							>
								<option value="">{t("agents.editor.modelDefault")}</option>
								{(provider?.models ?? []).map((model) => (
									<option key={model.id} value={model.id}>
										{model.label}
									</option>
								))}
							</select>
						</Field>
					</div>
					<Field label={t("agents.editor.extra")} hint={t("agents.editor.extraHint")}>
						<Textarea
							value={draft.extraInstructions}
							rows={4}
							maxLength={2000}
							onChange={(event) => setDraft({ ...draft, extraInstructions: event.target.value })}
						/>
					</Field>
					<div className="flex items-center justify-between gap-4">
						<div className="space-y-1">
							<div className="text-[12px] font-medium text-foreground">
								{t("agents.editor.offerSwitch")}
							</div>
							<p className="text-[11px] leading-relaxed text-muted-foreground">
								{t("agents.editor.offerSwitchHint")}
							</p>
						</div>
						<Switch
							aria-label={t("agents.editor.offerSwitch")}
							checked={draft.offerPrompt.trim().length > 0}
							onCheckedChange={(checked) =>
								setDraft({ ...draft, offerPrompt: checked ? t("agents.presets.reviewerOffer") : "" })
							}
						/>
					</div>
				</div>
				<DialogFooter>
					<Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
						<span>{t("agents.editor.cancel")}</span>
					</Button>
					<Button
						type="button"
						disabled={draft.name.trim().length === 0 || save.isPending}
						onClick={() => save.mutate()}
					>
						{save.isPending && <Loader2 className="size-4 animate-spin" />}
						<span>{t("agents.editor.save")}</span>
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

const SECTION_TITLE =
	"mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground";

/** The home of one DCC agent: how it is called, and its work in every project. */
export function AgentPage({
	agent,
	providers,
	workspaceNames,
	projectLabels,
	onOpenSession,
}: {
	agent: AgentView | null;
	providers: Providers;
	workspaceNames: Record<string, string>;
	projectLabels: Record<string, string>;
	onOpenSession: (session: AgentSessionView) => void;
}) {
	const { t, i18n } = useTranslation("common");
	const [editing, setEditing] = useState(false);
	const [roleOpen, setRoleOpen] = useState(false);

	if (!agent) {
		return (
			<div className="grid h-full place-items-center bg-background pt-9">
				<p className="text-[13px] text-muted-foreground">{t("agents.page.missing")}</p>
			</div>
		);
	}

	const provider = providers.find((candidate) => candidate.id === agent.providerId);
	const model = provider?.models.find((candidate) => candidate.id === agent.model);
	const runsOn = provider
		? [provider.label, model?.label].filter(Boolean).join(" · ")
		: t("agents.page.runsOnCurrent");
	const dateFormat = new Intl.DateTimeFormat(i18n.language, {
		dateStyle: "medium",
		timeStyle: "short",
	});
	const RoleChevron = roleOpen ? ChevronDown : ChevronRight;

	return (
		<div className="h-full min-h-0 overflow-y-auto bg-background pt-9">
			<div className="mx-auto flex max-w-3xl flex-col gap-8 px-8 py-8">
				<header className="flex items-center gap-5">
					<AgentAvatar avatar={agent.avatar} state={agent.state} size={88} />
					<div className="min-w-0 flex-1">
						<h1 className="truncate text-[22px] font-semibold tracking-[-0.01em]">{agent.name}</h1>
						<p className="mt-1 text-[12px] text-muted-foreground">
							{t(`agents.state.${agent.state}`)} · {runsOn}
						</p>
					</div>
					<Button type="button" variant="outline" onClick={() => setEditing(true)}>
						<Pencil className="size-4" />
						<span>{t("agents.page.edit")}</span>
					</Button>
				</header>

				{/* The reviewer needs a diff, which only a task in progress has: it
				    is called from the task or the pull request, never from here. */}
				<section className="rounded-[18px] border border-border/70 bg-card p-5">
					<h2 className="text-[14px] font-semibold">{t("agents.page.howToCall")}</h2>
					<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
						{t("agents.page.howToCallReviewer")}
					</p>
				</section>

				<section>
					<h2 className={SECTION_TITLE}>{t("agents.page.sessions")}</h2>
					{agent.sessions.length === 0 ? (
						<p className="rounded-[18px] border border-dashed border-border/70 px-5 py-6 text-[13px] text-muted-foreground">
							{t("agents.page.noSessions")}
						</p>
					) : (
						<ul className="divide-y divide-border/60 overflow-hidden rounded-[18px] border border-border/70 bg-card">
							{agent.sessions.map((session) => {
								const read = session.state === "done" && !session.unread;
								return (
									<li key={session.sessionId}>
										<button
											type="button"
											onClick={() => onOpenSession(session)}
											className="flex w-full cursor-pointer items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-accent/50"
										>
											{/* A result already read shows the plain mascot. */}
											<AgentAvatar
												avatar={agent.avatar}
												state={read ? undefined : session.state}
												size={28}
											/>
											<span className="min-w-0 flex-1">
												<span className="block truncate text-[13px] font-medium text-foreground">
													{workspaceNames[session.workspaceId] ?? session.title ?? session.sessionId}
												</span>
												<span className="block truncate text-[11px] text-muted-foreground">
													{[
														projectLabels[session.projectId],
														dateFormat.format(new Date(session.updatedAt)),
													]
														.filter(Boolean)
														.join(" · ")}
												</span>
											</span>
											<span
												className={cn(
													"shrink-0 text-[11px]",
													session.state === "needsYou"
														? "text-amber-700 dark:text-amber-300"
														: session.unread
															? "font-medium text-emerald-700 dark:text-emerald-300"
															: "text-muted-foreground",
												)}
											>
												{read ? t("agents.state.finished") : t(`agents.state.${session.state}`)}
											</span>
										</button>
									</li>
								);
							})}
						</ul>
					)}
				</section>

				{agent.prReviews.length > 0 && (
					<section>
						<h2 className={SECTION_TITLE}>{t("agents.page.pullRequests")}</h2>
						<ul className="divide-y divide-border/60 overflow-hidden rounded-[18px] border border-border/70 bg-card">
							{agent.prReviews.map((job) => {
								const unread = job.status === "done" && !job.seen;
								return (
									<li key={job.prId}>
										<button
											type="button"
											onClick={() => requestOpenPullRequest(job.prId)}
											className="flex w-full cursor-pointer items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-accent/50"
										>
											<AgentAvatar
												avatar={agent.avatar}
												state={job.status === "running" ? "working" : unread ? "done" : undefined}
												size={28}
											/>
											<span className="min-w-0 flex-1">
												<span className="block truncate text-[13px] font-medium text-foreground">
													{job.title}
												</span>
												<span className="block truncate font-mono text-[11px] text-muted-foreground">
													{job.label}
												</span>
											</span>
											<span
												className={cn(
													"shrink-0 text-[11px]",
													unread
														? "font-medium text-emerald-700 dark:text-emerald-300"
														: job.status === "failed"
															? "text-destructive"
															: "text-muted-foreground",
												)}
											>
												{job.status === "running"
													? t("agents.state.working")
													: job.status === "failed"
														? t("agents.state.failed")
														: unread
															? t("agents.state.done")
															: t("agents.state.finished")}
											</span>
										</button>
									</li>
								);
							})}
						</ul>
					</section>
				)}

				{agent.extraInstructions && (
					<section>
						<h2 className={SECTION_TITLE}>{t("agents.editor.extra")}</h2>
						<pre className="whitespace-pre-wrap rounded-[18px] border border-border/70 bg-card px-5 py-4 font-sans text-[13px] leading-relaxed text-foreground">
							{agent.extraInstructions}
						</pre>
					</section>
				)}

				{/* The role is DCC's and public; it is shown on request, not as the
				    page's main content. */}
				<section>
					<button
						type="button"
						aria-expanded={roleOpen}
						onClick={() => setRoleOpen((current) => !current)}
						className="flex cursor-pointer items-center gap-1.5 rounded-md text-muted-foreground transition-colors hover:text-foreground"
					>
						<RoleChevron className="size-3.5" />
						<span className="text-[12px] font-medium">{t("agents.page.showRole")}</span>
					</button>
					{roleOpen && (
						<>
							<p className="mt-2 text-[11px] leading-relaxed text-muted-foreground">
								{t("agents.page.roleHint")}
							</p>
							<pre className="mt-2 whitespace-pre-wrap rounded-[18px] border border-border/70 bg-card px-5 py-4 font-sans text-[12px] leading-relaxed text-foreground">
								{agent.role}
							</pre>
						</>
					)}
				</section>
			</div>
			<AgentEditorDialog agent={agent} open={editing} providers={providers} onOpenChange={setEditing} />
		</div>
	);
}
