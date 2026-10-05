import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Check, Circle, FolderOpen, Loader2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
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
import { dccQueryKeys } from "@/lib/query-client";
import { discardIdea, type IdeaVisibility, publishIdea } from "@/lib/research-api";
import { cn } from "@/lib/utils";
import { listRepositories, readWorkspaceFile } from "@/lib/workspace-api";
import { dispatchIdeaFinished } from "./idea-events";
import {
	IDEA_FILE,
	IDEA_MILESTONES,
	type IdeaStatus,
	isValidRepositoryName,
	parseIdeaStatus,
	repositoryNameFromCandidate,
} from "./idea-status";
import { isSessionRunning } from "./review-offer";
import { useIdeas } from "./use-ideas";

function parentFolder(path: string): string {
	const trimmed = path.replace(/\/+$/, "");
	return trimmed.slice(0, Math.max(trimmed.lastIndexOf("/"), 1));
}

/**
 * Above the composer of an idea's task: how far the research got, read by
 * DCC from IDEIA.md after every turn, and the card that publishes or
 * discards the idea. Renders nothing in any other task.
 */
export function IdeaPanel({
	workspaceId,
	workspacePath,
	sessions,
}: {
	workspaceId: string;
	workspacePath: string | null;
	sessions: WorkspaceSessionSummary[];
}) {
	const { ideaRootPaths } = useIdeas();
	if (!workspacePath || !ideaRootPaths.has(workspacePath)) {
		return null;
	}
	return <IdeaProgress workspaceId={workspaceId} rootPath={workspacePath} sessions={sessions} />;
}

function IdeaProgress({
	workspaceId,
	rootPath,
	sessions,
}: {
	workspaceId: string;
	rootPath: string;
	sessions: WorkspaceSessionSummary[];
}) {
	const { t } = useTranslation("common");
	const busy = sessions.some(isSessionRunning);
	// Every finished turn may have changed the file.
	const lastTurn = sessions.reduce<string>(
		(latest, summary) =>
			summary.lastTurnCompletedAt && summary.lastTurnCompletedAt > latest
				? summary.lastTurnCompletedAt
				: latest,
		"",
	);
	const statusQuery = useQuery({
		queryKey: ["ideaStatus", rootPath, lastTurn],
		queryFn: async () =>
			parseIdeaStatus(
				(await readWorkspaceFile({ workspaceRoot: rootPath, relativePath: IDEA_FILE })).content,
			),
		staleTime: Number.POSITIVE_INFINITY,
		retry: false,
	});
	const status = statusQuery.data ?? null;
	const ready = status !== null && status.filledCount === IDEA_MILESTONES.length && status.verdict === "seguir";
	const [cardOpen, setCardOpen] = useState(false);
	const [autoOpened, setAutoOpened] = useState(false);
	// The card shows itself once, when the idea first becomes ready.
	useEffect(() => {
		if (ready && !autoOpened) {
			setAutoOpened(true);
			setCardOpen(true);
		}
	}, [autoOpened, ready]);

	return (
		<div className="mb-2 flex flex-col gap-2">
			<div className="flex items-center gap-3 rounded-[14px] border border-border/70 bg-card px-3 py-2">
				<div className="min-w-0 flex-1">
					<div className="flex items-center justify-between gap-2 text-[11px] text-muted-foreground">
						<span className="truncate font-medium text-foreground">{t("agents.research.progress")}</span>
						<span className="shrink-0 tabular-nums">
							{t("agents.research.progressCount", {
								count: status?.filledCount ?? 0,
								total: IDEA_MILESTONES.length,
							})}
						</span>
					</div>
					<ul className="mt-1 flex flex-wrap gap-x-3 gap-y-0.5">
						{IDEA_MILESTONES.map((milestone) => {
							const filled = status?.milestones[milestone] ?? false;
							const Icon = filled ? Check : Circle;
							return (
								<li
									key={milestone}
									className={cn(
										"flex items-center gap-1 text-[11px]",
										filled ? "text-emerald-700 dark:text-emerald-300" : "text-muted-foreground",
									)}
								>
									<Icon className={filled ? "size-3" : "size-2.5"} />
									{t(`agents.research.milestones.${milestone}`)}
								</li>
							);
						})}
					</ul>
				</div>
				<Button
					type="button"
					size="sm"
					variant={ready ? "default" : "outline"}
					onClick={() => setCardOpen((open) => !open)}
				>
					{t("agents.research.card")}
				</Button>
			</div>
			{cardOpen && (
				<IdeaCard
					workspaceId={workspaceId}
					rootPath={rootPath}
					status={status}
					busy={busy}
					onClose={() => setCardOpen(false)}
				/>
			)}
		</div>
	);
}

function CardRow({ label, value, missing }: { label: string; value: string; missing?: boolean }) {
	return (
		<div className="grid grid-cols-[7rem_1fr] gap-3 text-[12px]">
			<span className="text-muted-foreground">{label}</span>
			<span className={cn("line-clamp-2", missing ? "text-amber-700 dark:text-amber-300" : "text-foreground")}>
				{value}
			</span>
		</div>
	);
}

function IdeaCard({
	workspaceId,
	rootPath,
	status,
	busy,
	onClose,
}: {
	workspaceId: string;
	rootPath: string;
	status: IdeaStatus | null;
	busy: boolean;
	onClose: () => void;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const repositories = useQuery({
		queryKey: ["ideaDestination"],
		queryFn: listRepositories,
		staleTime: 60_000,
	});
	const { ideaRootPaths } = useIdeas();
	// Next to the most recent project; the home folder otherwise.
	const suggestedDestination = useMemo(() => {
		const latest = (repositories.data?.repositories ?? []).find(
			(repository) => !ideaRootPaths.has(repository.rootPath),
		);
		return latest ? parentFolder(latest.rootPath) : parentFolder(parentFolder(rootPath));
	}, [ideaRootPaths, repositories.data, rootPath]);

	const candidates = status?.names ?? [];
	const [name, setName] = useState(() => repositoryNameFromCandidate(status?.favoriteName ?? ""));
	const [nameTouched, setNameTouched] = useState(false);
	useEffect(() => {
		if (!nameTouched && status?.favoriteName) {
			setName(repositoryNameFromCandidate(status.favoriteName));
		}
	}, [nameTouched, status?.favoriteName]);
	const [destination, setDestination] = useState<string | null>(null);
	const folder = destination ?? suggestedDestination;
	const [visibility, setVisibility] = useState<IdeaVisibility>("private");
	const [keepIdeaLocal, setKeepIdeaLocal] = useState(false);
	const [pending, setPending] = useState<"publish" | "discard" | null>(null);
	const [confirmDiscard, setConfirmDiscard] = useState(false);

	const missing = (milestone: (typeof IDEA_MILESTONES)[number]) => !(status?.milestones[milestone] ?? false);
	const nameValid = isValidRepositoryName(name);
	const warnings = [
		status && status.verdict !== "seguir" ? t("agents.research.warnVerdict") : null,
		status && !status.hasArchitecture ? t("agents.research.warnArchitecture") : null,
		status && !status.hasRoadmap ? t("agents.research.warnRoadmap") : null,
	].filter((warning): warning is string => Boolean(warning));

	const pickFolder = async () => {
		const picked = await openDialog({
			directory: true,
			multiple: false,
			defaultPath: folder,
			title: t("agents.research.folder"),
		});
		if (typeof picked === "string") setDestination(picked);
	};

	const refresh = async () => {
		await Promise.all([
			queryClient.invalidateQueries({ queryKey: dccQueryKeys.ideas }),
			queryClient.invalidateQueries({ queryKey: dccQueryKeys.repositories }),
			queryClient.invalidateQueries({ queryKey: dccQueryKeys.workspaces }),
			queryClient.invalidateQueries({ queryKey: dccQueryKeys.agents }),
		]);
	};

	const publish = async () => {
		setPending("publish");
		try {
			const published = await publishIdea({
				rootPath,
				name,
				destination: folder,
				visibility,
				keepIdeaLocal: visibility === "public" && keepIdeaLocal,
			});
			await refresh();
			toast.success(
				t("agents.research.published", { name, url: published.repositoryUrl ?? published.rootPath }),
			);
			dispatchIdeaFinished({ kind: "published", ideaWorkspaceId: workspaceId, rootPath: published.rootPath });
		} catch (error) {
			toast.error(error instanceof Error ? error.message : String(error));
		} finally {
			setPending(null);
		}
	};

	const discard = async () => {
		setPending("discard");
		try {
			await discardIdea(rootPath);
			setConfirmDiscard(false);
			dispatchIdeaFinished({ kind: "discarded", ideaWorkspaceId: workspaceId });
			await refresh();
		} catch (error) {
			toast.error(error instanceof Error ? error.message : String(error));
		} finally {
			setPending(null);
		}
	};

	const empty = t("agents.research.empty");
	return (
		<section className="rounded-[18px] border border-border/70 bg-card p-4">
			<header className="mb-3 flex items-center justify-between gap-2">
				<h3 className="truncate text-[13px] font-semibold">
					{t("agents.research.cardTitle", { idea: status?.title ?? t("agents.research.untitled") })}
				</h3>
				<span className="shrink-0 rounded-full border border-border/70 px-2 py-0.5 text-[10px] uppercase tracking-[0.08em] text-muted-foreground">
					{t("agents.research.draft")}
				</span>
			</header>
			<div className="space-y-1.5">
				<CardRow label={t("agents.research.milestones.problem")} value={status?.problem || empty} missing={missing("problem")} />
				<CardRow label={t("agents.research.milestones.audience")} value={status?.audience || empty} missing={missing("audience")} />
				<CardRow
					label={t("agents.research.verdict")}
					value={status?.verdict ? t(`agents.research.verdicts.${status.verdict}`) : empty}
					missing={status?.verdict !== "seguir"}
				/>
				<CardRow label="MVP" value={status?.mvp || empty} missing={!status?.mvp} />
			</div>

			<div className="mt-4 space-y-3">
				<label className="block space-y-1">
					<span className="text-[11px] font-medium text-muted-foreground">{t("agents.research.name")}</span>
					<Input
						value={name}
						maxLength={100}
						aria-invalid={!nameValid}
						onChange={(event) => {
							setNameTouched(true);
							setName(event.target.value);
						}}
					/>
					{candidates.length > 0 && (
						<div className="flex flex-wrap gap-1 pt-1">
							{candidates.map((candidate) => (
								<button
									key={candidate}
									type="button"
									onClick={() => {
										setNameTouched(true);
										setName(repositoryNameFromCandidate(candidate));
									}}
									className="cursor-pointer rounded-full border border-border/70 px-2 py-0.5 text-[11px] text-muted-foreground hover:bg-accent/50 hover:text-foreground"
								>
									{candidate === status?.favoriteName ? `★ ${candidate}` : candidate}
								</button>
							))}
						</div>
					)}
					{!nameValid && name.length > 0 && (
						<span className="text-[11px] text-destructive">{t("agents.research.nameInvalid")}</span>
					)}
				</label>

				<div className="space-y-1">
					<span className="text-[11px] font-medium text-muted-foreground">{t("agents.research.repository")}</span>
					<div className="flex gap-1.5">
						{(["private", "public"] as const).map((option) => (
							<Button
								key={option}
								type="button"
								size="sm"
								variant={visibility === option ? "default" : "outline"}
								onClick={() => setVisibility(option)}
							>
								{t(`agents.research.visibility.${option}`)}
							</Button>
						))}
					</div>
					{visibility === "public" && (
						<div className="space-y-1.5 rounded-[12px] border border-amber-500/40 bg-amber-500/5 p-2.5 text-[11px] leading-relaxed text-amber-800 dark:text-amber-200">
							<p>{t("agents.research.publicWarning")}</p>
							<label className="flex cursor-pointer items-center gap-2 text-foreground">
								<input
									type="checkbox"
									checked={keepIdeaLocal}
									onChange={(event) => setKeepIdeaLocal(event.target.checked)}
								/>
								{t("agents.research.keepLocal")}
							</label>
						</div>
					)}
				</div>

				<div className="space-y-1">
					<span className="text-[11px] font-medium text-muted-foreground">{t("agents.research.folder")}</span>
					<div className="flex items-center gap-2">
						<span className="min-w-0 flex-1 truncate rounded-md border border-border/70 px-2.5 py-1.5 font-mono text-[11px]">
							{folder}/{name || "…"}
						</span>
						<Button type="button" size="sm" variant="outline" onClick={() => void pickFolder()}>
							<FolderOpen className="size-3.5" />
							<span>{t("agents.research.chooseFolder")}</span>
						</Button>
					</div>
				</div>
			</div>

			{warnings.length > 0 && (
				<ul className="mt-3 space-y-0.5 text-[11px] text-amber-700 dark:text-amber-300">
					{warnings.map((warning) => (
						<li key={warning}>• {warning}</li>
					))}
				</ul>
			)}

			<footer className="mt-4 flex items-center justify-between gap-2 border-t border-border/60 pt-3">
				<span className="text-[11px] text-muted-foreground">
					{busy ? t("agents.research.waitTurn") : t("agents.research.changeLater")}
				</span>
				<div className="flex gap-1.5">
					<Button
						type="button"
						size="sm"
						variant="ghost"
						disabled={busy || pending !== null}
						onClick={() => setConfirmDiscard(true)}
					>
						{t("agents.research.discard")}
					</Button>
					<Button type="button" size="sm" variant="outline" onClick={onClose}>
						<X className="size-3.5" />
						<span>{t("agents.research.adjust")}</span>
					</Button>
					<Button
						type="button"
						size="sm"
						disabled={busy || pending !== null || !nameValid || !status}
						onClick={() => void publish()}
					>
						{pending === "publish" && <Loader2 className="size-3.5 animate-spin" />}
						<span>{t("agents.research.publish")}</span>
					</Button>
				</div>
			</footer>

			<Dialog open={confirmDiscard} onOpenChange={(open) => pending === null && setConfirmDiscard(open)}>
				<DialogContent className="sm:max-w-md">
					<DialogHeader>
						<DialogTitle>{t("agents.research.discardTitle")}</DialogTitle>
						<DialogDescription>{t("agents.research.discardDescription", { folder: rootPath })}</DialogDescription>
					</DialogHeader>
					<DialogFooter>
						<Button type="button" variant="ghost" disabled={pending !== null} onClick={() => setConfirmDiscard(false)}>
							{t("agents.editor.cancel")}
						</Button>
						<Button type="button" variant="destructive" disabled={pending !== null} onClick={() => void discard()}>
							{pending === "discard" && <Loader2 className="size-4 animate-spin" />}
							<span>{t("agents.research.discardConfirm")}</span>
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</section>
	);
}
