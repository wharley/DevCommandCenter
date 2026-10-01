import type { ProviderCatalog } from "@dcc/contracts";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Copy, Loader2, Pencil, Play, Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
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
	deleteAgent,
	saveAgent,
} from "@/lib/agents-api";
import { cn } from "@/lib/utils";
import { AgentAvatar } from "./agent-avatar";
import { AGENTS_QUERY_KEY, type AgentSessionView, type AgentView } from "./use-agents";

type Providers = ProviderCatalog["providers"];

const NEW_AGENT_DRAFT: ResidentAgentDraft = {
	name: "",
	role: "",
	kickoffPrompt: "",
	offerPrompt: "",
	extraInstructions: "",
	providerId: null,
	model: null,
	avatar: { color: "violet", arms: 4, eyes: "round" },
};

function errorMessage(error: unknown) {
	return error instanceof Error ? error.message : String(error);
}

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

function AgentEditorDialog({
	agent,
	initialDraft,
	open,
	providers,
	onOpenChange,
	onSaved,
	onDeleted,
}: {
	agent: AgentView | null;
	/** Starting point for a new agent, used when duplicating. */
	initialDraft?: ResidentAgentDraft;
	open: boolean;
	providers: Providers;
	onOpenChange: (open: boolean) => void;
	onSaved: (agentId: string) => void;
	onDeleted: () => void;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [draft, setDraft] = useState<ResidentAgentDraft>(NEW_AGENT_DRAFT);
	const [confirmingDelete, setConfirmingDelete] = useState(false);
	useEffect(() => {
		if (!open) {
			return;
		}
		setConfirmingDelete(false);
		setDraft(
			agent
				? {
						name: agent.name,
						role: agent.role,
						kickoffPrompt: agent.kickoffPrompt,
						offerPrompt: agent.offerPrompt,
						extraInstructions: agent.extraInstructions,
						providerId: agent.providerId,
						model: agent.model,
						avatar: agent.avatar,
					}
				: (initialDraft ?? NEW_AGENT_DRAFT),
		);
	}, [agent, initialDraft, open]);

	const save = useMutation({
		mutationFn: () => saveAgent(agent?.id ?? null, draft),
		onSuccess: async (saved) => {
			await queryClient.invalidateQueries({ queryKey: AGENTS_QUERY_KEY });
			onSaved(saved.id);
			onOpenChange(false);
		},
		onError: (error) => toast.error(errorMessage(error)),
	});
	const remove = useMutation({
		mutationFn: () => deleteAgent(agent?.id ?? ""),
		onSuccess: async () => {
			await queryClient.invalidateQueries({ queryKey: AGENTS_QUERY_KEY });
			onDeleted();
			onOpenChange(false);
		},
		onError: (error) => toast.error(errorMessage(error)),
	});

	const provider = providers.find((candidate) => candidate.id === draft.providerId) ?? null;
	const canSave = draft.name.trim().length > 0 && draft.role.trim().length > 0;
	// A built-in agent: its role and first message are DCC's, not editable.
	const builtIn = Boolean(agent?.preset);
	const selectClass =
		"h-8 w-full rounded-lg border border-border/80 bg-background px-2 text-[13px] text-foreground";

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-xl">
				<DialogHeader>
					<DialogTitle>{agent ? t("agents.editor.editTitle") : t("agents.editor.newTitle")}</DialogTitle>
					<DialogDescription>
						{builtIn ? t("agents.editor.builtInDescription") : t("agents.editor.description")}
					</DialogDescription>
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
					{builtIn ? (
						<>
							<Field label={t("agents.editor.extra")} hint={t("agents.editor.extraHint")}>
								<Textarea
									value={draft.extraInstructions}
									rows={4}
									maxLength={2000}
									onChange={(event) =>
										setDraft({ ...draft, extraInstructions: event.target.value })
									}
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
										setDraft({
											...draft,
											offerPrompt: checked ? t("agents.presets.reviewerOffer") : "",
										})
									}
								/>
							</div>
						</>
					) : (
						<>
							<Field label={t("agents.editor.role")} hint={t("agents.editor.roleHint")}>
								<Textarea
									value={draft.role}
									rows={8}
									maxLength={12000}
									onChange={(event) => setDraft({ ...draft, role: event.target.value })}
								/>
							</Field>
							<Field label={t("agents.editor.kickoff")} hint={t("agents.editor.kickoffHint")}>
								<Textarea
									value={draft.kickoffPrompt}
									rows={2}
									maxLength={2000}
									onChange={(event) => setDraft({ ...draft, kickoffPrompt: event.target.value })}
								/>
							</Field>
							<Field label={t("agents.editor.offer")} hint={t("agents.editor.offerHint")}>
								<Input
									value={draft.offerPrompt}
									maxLength={200}
									onChange={(event) => setDraft({ ...draft, offerPrompt: event.target.value })}
								/>
							</Field>
						</>
					)}
				</div>
				<DialogFooter className="sm:justify-between">
					{agent && !builtIn ? (
						<Button
							type="button"
							variant={confirmingDelete ? "destructive" : "ghost"}
							disabled={remove.isPending}
							onClick={() => (confirmingDelete ? remove.mutate() : setConfirmingDelete(true))}
						>
							<Trash2 className="size-4" />
							<span>
								{confirmingDelete ? t("agents.editor.confirmDelete") : t("agents.editor.delete")}
							</span>
						</Button>
					) : (
						<span />
					)}
					<div className="flex gap-2">
						<Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
							<span>{t("agents.editor.cancel")}</span>
						</Button>
						<Button type="button" disabled={!canSave || save.isPending} onClick={() => save.mutate()}>
							{save.isPending && <Loader2 className="size-4 animate-spin" />}
							<span>{t("agents.editor.save")}</span>
						</Button>
					</div>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

/** The home of one resident agent: who it is, and its sessions in every project. */
export function AgentPage({
	agent,
	providers,
	currentWorkspaceName,
	workspaceNames,
	projectLabels,
	isCalling,
	onCall,
	onOpenSession,
	onSelectAgent,
	onClose,
}: {
	agent: AgentView | null;
	providers: Providers;
	currentWorkspaceName: string | null;
	workspaceNames: Record<string, string>;
	projectLabels: Record<string, string>;
	isCalling: boolean;
	onCall: (agent: AgentView) => void;
	onOpenSession: (session: AgentSessionView) => void;
	onSelectAgent: (agentId: string) => void;
	onClose: () => void;
}) {
	const { t, i18n } = useTranslation("common");
	const [editing, setEditing] = useState<"edit" | "new" | "duplicate" | null>(null);

	const editor = (
		<AgentEditorDialog
			agent={editing === "edit" ? agent : null}
			initialDraft={
				editing === "duplicate" && agent
					? {
							// A copy is the person's own agent, so it starts from the
							// role in their language and is theirs to rewrite.
							name: t("agents.page.copyName", { agent: agent.name }),
							role: agent.preset ? t("agents.presets.reviewerRole") : agent.role,
							kickoffPrompt: agent.kickoffPrompt,
							offerPrompt: agent.offerPrompt,
							extraInstructions: agent.extraInstructions,
							providerId: agent.providerId,
							model: agent.model,
							avatar: agent.avatar,
						}
					: undefined
			}
			open={editing !== null}
			providers={providers}
			onOpenChange={(open) => !open && setEditing(null)}
			onSaved={onSelectAgent}
			onDeleted={onClose}
		/>
	);

	if (!agent) {
		return (
			<div className="grid h-full place-items-center bg-background pt-9">
				<div className="space-y-3 text-center">
					<p className="text-[13px] text-muted-foreground">{t("agents.page.missing")}</p>
					<Button type="button" onClick={() => setEditing("new")}>
						<Plus className="size-4" />
						<span>{t("agents.page.newAgent")}</span>
					</Button>
				</div>
				{editor}
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
					<div className="flex shrink-0 gap-2">
						<Button type="button" variant="ghost" onClick={() => setEditing("new")}>
							<Plus className="size-4" />
							<span>{t("agents.page.newAgent")}</span>
						</Button>
						<Button type="button" variant="ghost" onClick={() => setEditing("duplicate")}>
							<Copy className="size-4" />
							<span>{t("agents.page.duplicate")}</span>
						</Button>
						<Button type="button" variant="outline" onClick={() => setEditing("edit")}>
							<Pencil className="size-4" />
							<span>{t("agents.page.edit")}</span>
						</Button>
					</div>
				</header>

				<section className="rounded-[18px] border border-border/70 bg-card p-5">
					<div className="flex items-center gap-4">
						<div className="min-w-0 flex-1">
							<h2 className="text-[14px] font-semibold">{t("agents.page.callTitle")}</h2>
							<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
								{currentWorkspaceName
									? t("agents.page.callHint", { task: currentWorkspaceName })
									: t("agents.page.callNoTask")}
							</p>
						</div>
						<Button
							type="button"
							disabled={!currentWorkspaceName || isCalling}
							onClick={() => onCall(agent)}
						>
							{isCalling ? <Loader2 className="size-4 animate-spin" /> : <Play className="size-4" />}
							<span>{t("agents.page.call")}</span>
						</Button>
					</div>
				</section>

				<section>
					<h2 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
						{t("agents.page.sessions")}
					</h2>
					{agent.sessions.length === 0 ? (
						<p className="rounded-[18px] border border-dashed border-border/70 px-5 py-6 text-[13px] text-muted-foreground">
							{t("agents.page.noSessions")}
						</p>
					) : (
						<ul className="divide-y divide-border/60 overflow-hidden rounded-[18px] border border-border/70 bg-card">
							{agent.sessions.map((session) => (
								<li key={session.sessionId}>
									<button
										type="button"
										onClick={() => onOpenSession(session)}
										className="flex w-full cursor-pointer items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-accent/50"
									>
										<AgentAvatar avatar={agent.avatar} state={session.state} size={28} />
										<span className="min-w-0 flex-1">
											<span className="block truncate text-[13px] font-medium text-foreground">
												{workspaceNames[session.workspaceId] ?? session.title ?? session.sessionId}
											</span>
											<span className="block truncate text-[11px] text-muted-foreground">
												{[projectLabels[session.projectId], dateFormat.format(new Date(session.updatedAt))]
													.filter(Boolean)
													.join(" · ")}
											</span>
										</span>
										<span
											className={cn(
												"shrink-0 text-[11px]",
												session.state === "needsYou"
													? "text-amber-700 dark:text-amber-300"
													: "text-muted-foreground",
											)}
										>
											{t(`agents.state.${session.state}`)}
										</span>
									</button>
								</li>
							))}
						</ul>
					)}
				</section>

				<section>
					<h2 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
						{agent.preset ? t("agents.page.roleBuiltIn") : t("agents.page.role")}
					</h2>
					<pre className="whitespace-pre-wrap rounded-[18px] border border-border/70 bg-card px-5 py-4 font-sans text-[13px] leading-relaxed text-foreground">
						{agent.role}
					</pre>
					{agent.preset && (
						<p className="mt-2 text-[11px] leading-relaxed text-muted-foreground">
							{t("agents.page.roleBuiltInHint")}
						</p>
					)}
				</section>
				{agent.extraInstructions && (
					<section>
						<h2 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
							{t("agents.editor.extra")}
						</h2>
						<pre className="whitespace-pre-wrap rounded-[18px] border border-border/70 bg-card px-5 py-4 font-sans text-[13px] leading-relaxed text-foreground">
							{agent.extraInstructions}
						</pre>
					</section>
				)}
			</div>
			{editor}
		</div>
	);
}
