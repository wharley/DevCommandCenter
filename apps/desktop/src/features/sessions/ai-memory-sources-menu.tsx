import { BrainCircuit, EyeOff, LoaderCircle, PencilLine, Pin, PinOff, RotateCcw, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { AiMemoryQueryHit, AiMemorySourceActionOutput } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";
import { loadAiMemoryRecoveredSources, loadAiMemorySidecarStatus, loadAiMemorySourceActions, saveAiMemorySourceAction } from "@/lib/session-api";

type SourceAction = "corrected" | "ignored" | "pinned" | "cleared";
type SourceKind = "prompt" | "answer" | "page" | "observation";

/** "8 de out., 15:22"; the year only appears when it is not the current one. */
function formatAiMemorySourceTimestamp(value: string, language: string) {
	const parsed = new Date(value);
	if (Number.isNaN(parsed.getTime())) return value;
	return new Intl.DateTimeFormat(language === "en" || language.startsWith("en-") ? "en-US" : "pt-BR", {
		day: "numeric",
		month: "short",
		year: parsed.getFullYear() === new Date().getFullYear() ? undefined : "numeric",
		hour: "2-digit",
		minute: "2-digit",
	}).format(parsed);
}

/** ai-memory highlights matches with markup; show recovered text as readable plain text. */
export function cleanAiMemoryText(value: string) {
	const withoutMarkup = value
		.replace(/<\/?mark\b[^>]*>/gi, "")
		.replace(/<[^>]*>/g, " ")
		.replace(/\s+/g, " ")
		.trim();
	if (typeof document === "undefined") return withoutMarkup;
	const decoder = document.createElement("textarea");
	decoder.innerHTML = withoutMarkup;
	return decoder.value;
}

/** Same identity the backend uses for curation (`AiMemoryHit::source_key`). */
export function aiMemorySourceKey(source: { path?: string | null; title?: string | null; snippet?: string | null }) {
	return [source.path ?? "", source.title ?? "", (source.snippet ?? "").slice(0, 160)].join("|");
}

function sourceKind(source: AiMemoryQueryHit): SourceKind {
	if (source.path) return "page";
	if (source.kind === "user-prompt") return "prompt";
	if (source.kind === "notification") return "answer";
	return "observation";
}

const KIND_TONE: Record<SourceKind, string> = {
	prompt: "bg-sky-500/10 text-sky-600 dark:text-sky-400",
	answer: "bg-violet-500/10 text-violet-600 dark:text-violet-400",
	page: "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
	observation: "bg-muted text-muted-foreground",
};

type AiMemorySourcesMenuProps = {
	sessionId: string;
	projectLabel: string | null;
	workspacePath: string | null;
};

/** What the agent received from project memory on its latest turn, with the user's curation. */
export function AiMemorySourcesMenu({ sessionId, projectLabel, workspacePath }: AiMemorySourcesMenuProps) {
	const { t, i18n } = useTranslation("common");
	const language = i18n.resolvedLanguage ?? i18n.language;
	const sidecarQuery = useQuery({
		queryKey: ["ai-memory-sidecar-status"],
		queryFn: loadAiMemorySidecarStatus,
		staleTime: 30_000,
		refetchInterval: 30_000,
	});
	const sourcesQuery = useQuery({
		queryKey: ["ai-memory-recovered-sources", sessionId],
		queryFn: () => loadAiMemoryRecoveredSources(sessionId),
		staleTime: 30_000,
	});
	const actionsQuery = useQuery({
		queryKey: ["ai-memory-source-actions"],
		queryFn: () => loadAiMemorySourceActions(),
		staleTime: 30_000,
	});
	const [search, setSearch] = useState("");
	const [editingKey, setEditingKey] = useState<string | null>(null);
	const [draft, setDraft] = useState("");
	const [savingKey, setSavingKey] = useState<string | null>(null);
	const [showIgnored, setShowIgnored] = useState(false);
	useEffect(() => {
		setSearch("");
		setEditingKey(null);
		setShowIgnored(false);
	}, [sessionId]);

	const sidecarMode = sidecarQuery.data?.mode;
	const sidecarActive = sidecarMode === "managed" || sidecarMode === "remote" || sidecarMode === "shared";
	const sidecarUnavailable = sidecarMode === "unavailable";
	const actions = useMemo(
		() => new Map<string, AiMemorySourceActionOutput>((actionsQuery.data ?? []).map((action) => [action.sourceKey, action])),
		[actionsQuery.data],
	);
	const { visible, ignored } = useMemo(() => {
		const sources = sourcesQuery.data ?? [];
		const isIgnored = (source: AiMemoryQueryHit) => actions.get(aiMemorySourceKey(source))?.action === "ignored";
		const isPinned = (source: AiMemoryQueryHit) => actions.get(aiMemorySourceKey(source))?.action === "pinned";
		return {
			// Pinned sources lead, as they do in the context the agent receives.
			visible: sources.filter((source) => !isIgnored(source)).sort((left, right) => Number(isPinned(right)) - Number(isPinned(left))),
			ignored: sources.filter(isIgnored),
		};
	}, [actions, sourcesQuery.data]);
	const listed = useMemo(() => {
		const query = search.trim().toLocaleLowerCase();
		const base = showIgnored ? [...visible, ...ignored] : visible;
		if (!query) return base;
		return base.filter((source) =>
			[source.title, source.path, source.snippet]
				.filter(Boolean)
				.some((value) => cleanAiMemoryText(value ?? "").toLocaleLowerCase().includes(query)),
		);
	}, [ignored, search, showIgnored, visible]);

	if (!sourcesQuery.isLoading && visible.length === 0 && ignored.length === 0 && !sidecarActive && !sidecarUnavailable) return null;

	const triggerLabel = sourcesQuery.isLoading
		? t("workbench.aiMemory.sourcesLoading")
		: sidecarActive
			? t("workbench.aiMemory.statusActive")
			: sidecarUnavailable
				? t("workbench.aiMemory.statusUnavailable")
				: t("workbench.aiMemory.sourcesAria");

	const save = async (sourceKey: string, action: SourceAction, correction?: string) => {
		setSavingKey(sourceKey);
		try {
			await saveAiMemorySourceAction({ sourceKey, action, correction: correction ?? null });
			await actionsQuery.refetch();
			if (action === "corrected") setEditingKey(null);
		} catch (error) {
			toast.error(error instanceof Error ? error.message : t("workbench.aiMemory.actionError"));
		} finally {
			setSavingKey(null);
		}
	};
	const startEditing = (sourceKey: string, current: string | null | undefined) => {
		setEditingKey(sourceKey);
		setDraft(current ?? "");
	};

	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button type="button" variant="ghost" size="icon-sm" title={triggerLabel} aria-label={triggerLabel} className={cn("relative", sourcesQuery.isLoading ? "text-muted-foreground" : sidecarUnavailable ? "text-amber-500 hover:text-amber-400" : sidecarActive || visible.length > 0 ? "text-emerald-500 hover:text-emerald-400" : "text-muted-foreground hover:text-foreground")}>
					<BrainCircuit className={cn("size-3.5", sourcesQuery.isLoading && "animate-pulse")} />
					{visible.length > 0 ? <span className="absolute -right-1 -top-1 flex h-4 min-w-4 items-center justify-center rounded-full border border-background bg-emerald-500 px-1 text-[9px] font-medium leading-none text-white">{visible.length}</span> : null}
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end" className="flex max-h-[min(70vh,36rem)] w-96 max-w-[min(92vw,24rem)] flex-col overflow-hidden p-0">
				<div className="border-b border-border/50 px-3 pb-2.5 pt-3">
					<div className="flex items-center justify-between gap-2">
						<p className="flex items-center gap-1.5 text-[12px] font-medium text-foreground">
							<BrainCircuit className="size-3.5 text-emerald-500" />
							{t("workbench.aiMemory.sourcesTitle")}
						</p>
						{visible.length > 0 ? <span className="text-[10px] tabular-nums text-muted-foreground">{t("workbench.aiMemory.sourcesCount", { count: visible.length })}</span> : null}
					</div>
					<p className="mt-1 text-[11px] leading-snug text-muted-foreground" title={workspacePath ?? undefined}>
						{t("workbench.aiMemory.sourcesHint", { project: projectLabel ?? "—" })}
					</p>
					{visible.length + ignored.length >= 4 ? (
						<div className="relative mt-2.5 text-[11px]" onKeyDown={(event) => event.stopPropagation()}>
							<Search className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
							<Input
								value={search}
								onChange={(event) => setSearch(event.target.value)}
								placeholder={t("workbench.aiMemory.sourcesSearchPlaceholder")}
								className="h-7 pl-7"
								aria-label={t("workbench.aiMemory.sourcesSearchPlaceholder")}
							/>
						</div>
					) : null}
				</div>

				<div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
					{sourcesQuery.isLoading ? <div className="flex items-center gap-2 px-3 py-6 text-[11px] text-muted-foreground"><LoaderCircle className="size-3.5 animate-spin" />{t("workbench.aiMemory.sourcesLoading")}</div> : null}
					{!sourcesQuery.isLoading && visible.length === 0 && !showIgnored ? <p className="px-3 py-6 text-center text-[11px] text-muted-foreground">{t("workbench.aiMemory.sourcesEmpty")}</p> : null}
					{!sourcesQuery.isLoading && listed.length === 0 && search.trim() ? <p className="px-3 py-6 text-center text-[11px] text-muted-foreground">{t("workbench.aiMemory.sourcesNoMatches")}</p> : null}
					{!sourcesQuery.isLoading ? listed.map((source) => {
						const sourceKey = aiMemorySourceKey(source);
						const action = actions.get(sourceKey);
						const pinned = action?.action === "pinned";
						const isIgnored = action?.action === "ignored";
						const correction = action?.action === "corrected" ? action.correction : null;
						const editing = editingKey === sourceKey;
						const saving = savingKey === sourceKey;
						const kind = sourceKind(source);
						const title = (source.title ? cleanAiMemoryText(source.title) : "") || source.path || t("workbench.aiMemory.untitled");
						const snippet = source.snippet ? cleanAiMemoryText(source.snippet) : null;
						return (
							<div key={sourceKey} className={cn("group/source border-b border-border/40 px-3 py-2.5 last:border-b-0", pinned && "bg-amber-500/[0.04]", isIgnored && "opacity-60")}>
								<div className="flex items-center gap-1.5">
									<span className={cn("rounded px-1.5 py-px text-[10px] font-medium", KIND_TONE[kind])}>{t(`workbench.aiMemory.kind.${kind}`)}</span>
									{pinned ? <span className="inline-flex items-center gap-0.5 text-[10px] font-medium text-amber-600 dark:text-amber-400"><Pin className="size-3" />{t("workbench.aiMemory.pinned")}</span> : null}
									{isIgnored ? <span className="text-[10px] text-muted-foreground">{t("workbench.aiMemory.ignoredLabel")}</span> : null}
									{source.createdAt ? <span className="ml-auto shrink-0 text-[10px] tabular-nums text-muted-foreground" title={source.createdAt}>{formatAiMemorySourceTimestamp(source.createdAt, language)}</span> : null}
								</div>
								<p className="mt-1.5 line-clamp-2 text-[12px] font-medium leading-snug text-foreground" title={source.path ?? undefined}>{title}</p>
								{snippet ? <p className={cn("mt-1 line-clamp-3 text-[11px] leading-relaxed text-muted-foreground", correction && "line-through decoration-muted-foreground/40")}>{snippet}</p> : null}
								{correction && !editing ? (
									<div className="mt-1.5 rounded-md border border-emerald-500/20 bg-emerald-500/[0.06] px-2 py-1.5">
										<p className="text-[10px] font-medium text-emerald-600 dark:text-emerald-400">{t("workbench.aiMemory.correctionLabel")}</p>
										<p className="mt-0.5 text-[11px] leading-relaxed text-foreground">{correction}</p>
									</div>
								) : null}
								{editing ? (
									<div className="mt-2 text-[11px]" onKeyDown={(event) => {
										event.stopPropagation();
										if (event.key === "Escape") setEditingKey(null);
										if (event.key === "Enter" && (event.metaKey || event.ctrlKey) && draft.trim()) void save(sourceKey, "corrected", draft.trim());
									}}>
										<Textarea
											autoFocus
											value={draft}
											onChange={(event) => setDraft(event.target.value)}
											placeholder={t("workbench.aiMemory.correctPlaceholder")}
											className="min-h-16 resize-none px-2 py-1.5"
											aria-label={t("workbench.aiMemory.correct")}
										/>
										<div className="mt-1.5 flex items-center justify-end gap-1">
											<Button type="button" variant="ghost" size="sm" className="h-6 px-2" onClick={() => setEditingKey(null)}><span className="text-[11px]">{t("workbench.aiMemory.cancel")}</span></Button>
											<Button type="button" size="sm" className="h-6 px-2" disabled={saving || !draft.trim()} onClick={() => void save(sourceKey, "corrected", draft.trim())}><span className="text-[11px]">{t("workbench.aiMemory.correct")}</span></Button>
										</div>
									</div>
								) : (
									<div className="-ml-1.5 mt-1 flex items-center gap-0.5">
										{isIgnored ? (
											<SourceActionButton icon={RotateCcw} label={t("workbench.aiMemory.restore")} disabled={saving} onClick={() => void save(sourceKey, "cleared")} />
										) : (
											<>
												<SourceActionButton icon={pinned ? PinOff : Pin} label={pinned ? t("workbench.aiMemory.unpin") : t("workbench.aiMemory.pin")} disabled={saving} onClick={() => void save(sourceKey, pinned ? "cleared" : "pinned")} />
												<SourceActionButton icon={PencilLine} label={correction ? t("workbench.aiMemory.editCorrection") : t("workbench.aiMemory.correctAction")} disabled={saving} onClick={() => startEditing(sourceKey, correction)} />
												{correction ? <SourceActionButton icon={RotateCcw} label={t("workbench.aiMemory.undoCorrection")} disabled={saving} onClick={() => void save(sourceKey, "cleared")} /> : null}
												<SourceActionButton icon={EyeOff} label={t("workbench.aiMemory.ignore")} disabled={saving} onClick={() => void save(sourceKey, "ignored")} />
											</>
										)}
										{saving ? <LoaderCircle className="ml-1 size-3 animate-spin text-muted-foreground" /> : null}
									</div>
								)}
							</div>
						);
					}) : null}
				</div>

				{ignored.length > 0 ? (
					<div className="border-t border-border/50 px-3 py-1.5">
						<Button type="button" variant="ghost" size="sm" className="-ml-1.5 h-6 gap-1 px-1.5 text-muted-foreground/80 hover:text-foreground" onClick={() => setShowIgnored((current) => !current)}>
							<EyeOff className="size-3" />
							<span className="text-[11px]">{showIgnored ? t("workbench.aiMemory.hideIgnored") : t("workbench.aiMemory.showIgnored", { count: ignored.length })}</span>
						</Button>
					</div>
				) : null}
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

function SourceActionButton({ icon: Icon, label, disabled, onClick }: { icon: typeof Pin; label: string; disabled?: boolean; onClick: () => void }) {
	return (
		<Button
			type="button"
			variant="ghost"
			size="sm"
			className="h-6 gap-1 px-1.5 text-muted-foreground/80 hover:text-foreground"
			disabled={disabled}
			onClick={(event) => { event.preventDefault(); event.stopPropagation(); onClick(); }}
		>
			<Icon className="size-3" />
			<span className="text-[11px]">{label}</span>
		</Button>
	);
}
