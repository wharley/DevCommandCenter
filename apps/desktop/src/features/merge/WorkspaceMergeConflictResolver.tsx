import { useQuery } from "@tanstack/react-query";
import type {
	WorkspaceGitConflictEntry,
	WorkspaceGitConflictSide,
} from "@dcc/contracts";
import {
	AlertTriangle,
	Check,
	ChevronLeft,
	ChevronRight,
	FileWarning,
	GitMerge,
	Loader2,
	RotateCcw,
	Trash2,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import {
	workspaceGitAbortMerge,
	workspaceGitAcceptConflict,
	workspaceGitCompleteMerge,
	workspaceGitConflictState,
	workspaceGitMarkConflictResolved,
	workspaceGitSyncBase,
	writeWorkspaceFile,
} from "@/lib/workspace-api";
import { cn } from "@/lib/utils";
import {
	type FileEditorHandle,
	WorkspaceFileEditor,
} from "@/features/editor/WorkspaceFileSurface";
import {
	applyMergeConflictResolution,
	hasMergeConflictMarkerFragments,
	parseMergeConflictHunks,
	type MergeConflictResolution,
} from "./merge-conflict-hunks";

const CONFLICT_STATE_QUERY_KEY = "workspaceGitConflictState";

type Props = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	workspaceRoot: string;
	baseBranch: string | null;
	forgeLogin: string | null;
	onStateChanged: () => Promise<void> | void;
};

function errorMessage(error: unknown) {
	return error instanceof Error ? error.message : String(error);
}

function shortRef(value: string | null | undefined, fallback: string) {
	if (!value) return fallback;
	return value.replace(/^refs\/(heads|remotes)\//, "").replace(/^remotes\//, "");
}

function conflictKindLabel(entry: WorkspaceGitConflictEntry) {
	switch (entry.kind) {
		case "both-modified":
			return "Alterado nos dois lados";
		case "both-added":
			return "Adicionado nos dois lados";
		case "deleted-by-current":
			return "Excluído na branch atual";
		case "deleted-by-incoming":
			return "Excluído na branch base";
		case "both-deleted":
			return "Excluído nos dois lados";
		default:
			return "Conflito Git";
	}
}

function editableGitMode(mode: string | null) {
	return mode == null || mode.startsWith("100");
}

function SidePreview({ label, text }: { label: string; text: string }) {
	return (
		<div className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-lg border border-border/60 bg-muted/15">
			<div className="shrink-0 border-b border-border/50 px-2.5 py-1.5 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
				{label}
			</div>
			<pre className="max-h-32 min-h-16 overflow-auto whitespace-pre-wrap break-words px-2.5 py-2 font-mono text-[11px] leading-5">
				{text || "∅"}
			</pre>
		</div>
	);
}

export function WorkspaceMergeConflictResolver({
	open,
	onOpenChange,
	workspaceRoot,
	baseBranch,
	forgeLogin,
	onStateChanged,
}: Props) {
	const editorRef = useRef<FileEditorHandle | null>(null);
	const initialResultRef = useRef<string | null>(null);
	const totalConflictsRef = useRef(0);
	const [selectedPath, setSelectedPath] = useState<string | null>(null);
	const [buffer, setBuffer] = useState("");
	const [activeHunk, setActiveHunk] = useState(0);
	const [busy, setBusy] = useState<string | null>(null);

	const query = useQuery({
		queryKey: [CONFLICT_STATE_QUERY_KEY, workspaceRoot],
		queryFn: () => workspaceGitConflictState({ workspaceRoot }),
		enabled: open && Boolean(workspaceRoot),
		staleTime: 0,
	});

	const state = query.data;
	const conflicts = state?.conflicts ?? [];
	if (open && conflicts.length > totalConflictsRef.current) {
		totalConflictsRef.current = conflicts.length;
	}

	const selected = useMemo(
		() => conflicts.find((entry) => entry.path === selectedPath) ?? null,
		[conflicts, selectedPath],
	);
	const hunks = useMemo(() => parseMergeConflictHunks(buffer), [buffer]);
	const hunk = hunks[Math.min(activeHunk, Math.max(hunks.length - 1, 0))] ?? null;
	const dirty = selected != null && buffer !== (initialResultRef.current ?? "");
	const currentLabel = shortRef(state?.currentBranch, "branch atual");
	const incomingLabel = shortRef(state?.incomingRef ?? baseBranch, "branch base");

	const openEntry = useCallback((entry: WorkspaceGitConflictEntry) => {
		const result =
			entry.result.text ?? entry.current.text ?? entry.incoming.text ?? "";
		setSelectedPath(entry.path);
		setBuffer(result);
		initialResultRef.current = entry.result.text;
		setActiveHunk(0);
	}, []);

	useEffect(() => {
		if (!open) {
			setSelectedPath(null);
			setBuffer("");
			initialResultRef.current = null;
			totalConflictsRef.current = 0;
			return;
		}
		if (conflicts.length === 0) {
			setSelectedPath(null);
			return;
		}
		if (!selectedPath || !conflicts.some((entry) => entry.path === selectedPath)) {
			openEntry(conflicts[0]!);
		}
	}, [conflicts, open, openEntry, selectedPath]);

	useEffect(() => {
		if (activeHunk >= hunks.length) {
			setActiveHunk(Math.max(hunks.length - 1, 0));
		}
	}, [activeHunk, hunks.length]);

	const refresh = useCallback(async () => {
		await query.refetch();
		await onStateChanged();
	}, [onStateChanged, query]);

	const requestOpenChange = useCallback(
		(next: boolean) => {
			if (
				!next &&
				dirty &&
				!window.confirm("Descartar a edição ainda não salva deste conflito?")
			) {
				return;
			}
			onOpenChange(next);
		},
		[dirty, onOpenChange],
	);

	const startMerge = useCallback(async () => {
		setBusy("start");
		let failure: unknown = null;
		try {
			await workspaceGitSyncBase({
				workspaceRoot,
				baseBranch,
				forgeLogin,
			});
		} catch (error) {
			failure = error;
		}
		const next = await query.refetch();
		await onStateChanged();
		setBusy(null);
		if ((next.data?.conflicts.length ?? 0) > 0) {
			toast.info("Merge iniciado. Resolva os arquivos indicados.");
			return;
		}
		if (failure) {
			toast.error(errorMessage(failure));
			return;
		}
		toast.success("Branch atualizada sem conflitos. Agora ela pode ser enviada.");
		onOpenChange(false);
	}, [
		baseBranch,
		forgeLogin,
		onOpenChange,
		onStateChanged,
		query,
		workspaceRoot,
	]);

	const applyHunk = useCallback(
		(resolution: MergeConflictResolution) => {
			if (!hunk) return;
			const source = editorRef.current?.getValue() ?? buffer;
			const nextHunk = parseMergeConflictHunks(source).find(
				(candidate) => candidate.startOffset === hunk.startOffset,
			);
			if (!nextHunk) return;
			const next = applyMergeConflictResolution(source, nextHunk, resolution);
			editorRef.current?.setValue(next);
			setBuffer(next);
		},
		[buffer, hunk],
	);

	const saveAndResolve = useCallback(async () => {
		if (!selected) return;
		const content = editorRef.current?.getValue() ?? buffer;
		if (parseMergeConflictHunks(content).length > 0) {
			toast.warning(
				"Resolva todos os blocos deste arquivo antes de marcá-lo como resolvido.",
			);
			return;
		}
		if (
			hasMergeConflictMarkerFragments(content) &&
			!window.confirm(
				"Ainda existem linhas parecidas com marcadores de conflito. Marcar como resolvido mesmo assim?",
			)
		) {
			return;
		}
		setBusy("save");
		try {
			if (content !== (initialResultRef.current ?? "") || !selected.result.exists) {
				const result = await writeWorkspaceFile({
					workspaceRoot,
					relativePath: selected.path,
					content,
					expectedPrevious: selected.result.exists ? initialResultRef.current : null,
				});
				if (result.conflicted) {
					throw new Error(
						"O arquivo mudou no disco. Reabra o conflito antes de sobrescrever.",
					);
				}
			}
			await workspaceGitMarkConflictResolved({
				workspaceRoot,
				relativePath: selected.path,
				delete: false,
			});
			toast.success(`${selected.path} marcado como resolvido`);
			await refresh();
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			setBusy(null);
		}
	}, [buffer, refresh, selected, workspaceRoot]);

	const acceptWholeSide = useCallback(
		async (side: WorkspaceGitConflictSide) => {
			if (!selected) return;
			setBusy(side);
			try {
				await workspaceGitAcceptConflict({
					workspaceRoot,
					relativePath: selected.path,
					side,
				});
				toast.success(
					`${selected.path} resolvido com ${side === "current" ? currentLabel : incomingLabel}`,
				);
				await refresh();
			} catch (error) {
				toast.error(errorMessage(error));
			} finally {
				setBusy(null);
			}
		},
		[currentLabel, incomingLabel, refresh, selected, workspaceRoot],
	);

	const deleteResult = useCallback(async () => {
		if (!selected) return;
		if (!window.confirm(`Excluir ${selected.path} como resultado deste merge?`)) {
			return;
		}
		setBusy("delete");
		try {
			await workspaceGitMarkConflictResolved({
				workspaceRoot,
				relativePath: selected.path,
				delete: true,
			});
			toast.success(`${selected.path} removido e marcado como resolvido`);
			await refresh();
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			setBusy(null);
		}
	}, [refresh, selected, workspaceRoot]);

	const abortMerge = useCallback(async () => {
		if (
			!window.confirm(
				"Abortar o merge e descartar todas as resoluções realizadas?",
			)
		) {
			return;
		}
		setBusy("abort");
		try {
			await workspaceGitAbortMerge({ workspaceRoot });
			await onStateChanged();
			toast.success("Merge abortado");
			onOpenChange(false);
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			setBusy(null);
		}
	}, [onOpenChange, onStateChanged, workspaceRoot]);

	const completeMerge = useCallback(async () => {
		setBusy("complete");
		try {
			await workspaceGitCompleteMerge({ workspaceRoot, forgeLogin });
			await onStateChanged();
			toast.success("Merge concluído e enviado");
			onOpenChange(false);
		} catch (error) {
			toast.error(errorMessage(error));
		} finally {
			setBusy(null);
		}
	}, [forgeLogin, onOpenChange, onStateChanged, workspaceRoot]);

	const total = Math.max(totalConflictsRef.current, conflicts.length);
	const resolved = Math.max(total - conflicts.length, 0);
	const unsupported = state && !["none", "merge"].includes(state.operation);
	const textUnavailable = selected
		? selected.result.binary || selected.result.truncated ||
			!editableGitMode(selected.current.mode) ||
			!editableGitMode(selected.incoming.mode) ||
			(selected.current.text == null && selected.incoming.text == null)
		: false;

	return (
		<Dialog open={open} onOpenChange={requestOpenChange}>
			<DialogContent
				showCloseButton={false}
				className="flex h-[min(92vh,920px)] w-[min(96vw,1440px)] max-w-none flex-col gap-0 overflow-hidden p-0"
			>
				<DialogHeader className="shrink-0 border-b border-border/60 px-4 py-3">
					<div className="flex items-center gap-3">
						<div className="flex size-9 items-center justify-center rounded-lg bg-amber-500/15 text-amber-700 dark:text-amber-300">
							<GitMerge className="size-5" />
						</div>
						<div className="min-w-0 flex-1">
							<DialogTitle>Resolver conflitos</DialogTitle>
							<DialogDescription className="mt-1">
								{currentLabel} ← {incomingLabel}
							</DialogDescription>
						</div>
						{state?.operation === "merge" ? (
							<Button
								variant="outline"
								size="sm"
								disabled={Boolean(busy)}
								onClick={() => void abortMerge()}
							>
								<RotateCcw className="size-3.5" />
								Abortar merge
							</Button>
						) : null}
						<Button
							variant="ghost"
							size="sm"
							onClick={() => requestOpenChange(false)}
						>
							Fechar
						</Button>
					</div>
				</DialogHeader>

				{query.isPending ? (
					<div className="flex flex-1 items-center justify-center gap-2 text-muted-foreground">
						<Loader2 className="size-4 animate-spin" /> Lendo estado do Git…
					</div>
				) : query.isError ? (
					<div className="flex flex-1 items-center justify-center p-8 text-destructive">
						{errorMessage(query.error)}
					</div>
				) : unsupported ? (
					<div className="flex flex-1 items-center justify-center p-8 text-center">
						<div className="max-w-md">
							<AlertTriangle className="mx-auto size-8 text-amber-500" />
							<h3 className="mt-3 font-semibold">
								Operação {state.operation} detectada
							</h3>
							<p className="mt-2 text-sm text-muted-foreground">
								Esta entrega resolve merges. Continue ou aborte esta operação pelo
								terminal.
							</p>
						</div>
					</div>
				) : state?.operation === "none" && conflicts.length === 0 ? (
					<div className="flex flex-1 items-center justify-center p-8 text-center">
						<div className="max-w-lg">
							<GitMerge className="mx-auto size-10 text-amber-500" />
							<h3 className="mt-4 text-base font-semibold">
								Trazer {incomingLabel} para {currentLabel}
							</h3>
							<p className="mt-2 text-sm leading-6 text-muted-foreground">
								O DCC vai buscar a base mais recente e iniciar o merge local. Se
								houver conflitos, eles aparecerão aqui.
							</p>
							<Button
								className="mt-5"
								disabled={Boolean(busy)}
								onClick={() => void startMerge()}
							>
								{busy === "start" ? (
									<Loader2 className="size-4 animate-spin" />
								) : (
									<GitMerge className="size-4" />
								)}
								Iniciar resolução
							</Button>
						</div>
					</div>
				) : state?.operation === "merge" && conflicts.length === 0 ? (
					<div className="flex flex-1 items-center justify-center p-8 text-center">
						<div className="max-w-md">
							<div className="mx-auto flex size-12 items-center justify-center rounded-full bg-emerald-500/15 text-emerald-600">
								<Check className="size-6" />
							</div>
							<h3 className="mt-4 text-base font-semibold">
								Todos os conflitos foram resolvidos
							</h3>
							<p className="mt-2 text-sm text-muted-foreground">
								Conclua o commit de merge e envie a branch para atualizar o PR/MR.
							</p>
							<Button
								className="mt-5"
								disabled={Boolean(busy)}
								onClick={() => void completeMerge()}
							>
								{busy === "complete" ? (
									<Loader2 className="size-4 animate-spin" />
								) : (
									<GitMerge className="size-4" />
								)}
								Concluir merge e enviar
							</Button>
						</div>
					</div>
				) : (
					<div className="flex min-h-0 flex-1">
						<aside className="flex w-72 shrink-0 flex-col border-r border-border/60 bg-muted/10">
							<div className="border-b border-border/50 px-3 py-2.5">
								<div className="flex items-center justify-between text-xs font-medium">
									<span>Arquivos</span>
									<Badge variant="secondary">
										{resolved}/{total}
									</Badge>
								</div>
								<div className="mt-2 h-1.5 overflow-hidden rounded-full bg-muted">
									<div
										className="h-full bg-emerald-500 transition-all"
										style={{
											width: `${total ? (resolved / total) * 100 : 0}%`,
										}}
									/>
								</div>
							</div>
							<div className="min-h-0 flex-1 overflow-y-auto p-1.5">
								{conflicts.map((entry) => (
									<button
										key={entry.path}
										type="button"
										onClick={() => openEntry(entry)}
										className={cn(
											"mb-1 flex w-full items-start gap-2 rounded-lg px-2.5 py-2 text-left transition-colors",
											selected?.path === entry.path
												? "bg-accent text-accent-foreground"
												: "hover:bg-accent/60",
										)}
									>
										<FileWarning className="mt-0.5 size-3.5 shrink-0 text-amber-500" />
										<span className="min-w-0">
											<span className="block truncate font-mono text-[11px] font-medium">
												{entry.path}
											</span>
											<span className="mt-0.5 block text-[10px] text-muted-foreground">
												{conflictKindLabel(entry)}
											</span>
										</span>
									</button>
								))}
							</div>
						</aside>

						{selected ? (
							<section className="flex min-w-0 flex-1 flex-col">
								<div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border/60 px-3 py-2">
									<div className="mr-auto min-w-0"><p className="truncate font-mono text-xs font-semibold">{selected.path}</p><p className="text-[10px] text-muted-foreground">{conflictKindLabel(selected)}</p></div>
									<Button variant="outline" size="xs" disabled={Boolean(busy)} onClick={() => void acceptWholeSide("current")} title={`Aceita e marca o arquivo inteiro usando ${currentLabel}`}>Usar arquivo {currentLabel}</Button>
									<Button variant="outline" size="xs" disabled={Boolean(busy)} onClick={() => void acceptWholeSide("incoming")} title={`Aceita e marca o arquivo inteiro usando ${incomingLabel}`}>Usar arquivo {incomingLabel}</Button>
									<Button variant="destructive" size="xs" disabled={Boolean(busy)} onClick={() => void deleteResult()}><Trash2 className="size-3.5" />Excluir resultado</Button>
								</div>

								{textUnavailable ? (
									<div className="flex flex-1 items-center justify-center p-8 text-center"><div className="max-w-md"><FileWarning className="mx-auto size-8 text-amber-500" /><h3 className="mt-3 font-semibold">Conteúdo indisponível no editor</h3><p className="mt-2 text-sm text-muted-foreground">{selected.result.truncated ? "O arquivo excede o limite seguro do editor." : "O arquivo é binário, não UTF-8 ou existe somente como objeto especial do Git."} Escolha uma das versões completas ou a exclusão.</p></div></div>
								) : (
									<>
										<div className="shrink-0 border-b border-border/60 bg-muted/5 p-3">
											{hunk ? (
												<>
													<div className="mb-2 flex items-center gap-2"><Badge variant="outline">Bloco {activeHunk + 1} de {hunks.length}</Badge><div className="ml-auto flex gap-1"><Button variant="ghost" size="icon-xs" disabled={activeHunk === 0} onClick={() => setActiveHunk((value) => Math.max(0, value - 1))}><ChevronLeft /></Button><Button variant="ghost" size="icon-xs" disabled={activeHunk >= hunks.length - 1} onClick={() => setActiveHunk((value) => Math.min(hunks.length - 1, value + 1))}><ChevronRight /></Button></div></div>
													<div className="flex gap-2"><SidePreview label={currentLabel} text={hunk.currentText} /><SidePreview label={incomingLabel} text={hunk.incomingText} /></div>
													<div className="mt-2 flex flex-wrap gap-1.5"><Button size="xs" variant="outline" onClick={() => applyHunk("current")}>Aceitar {currentLabel}</Button><Button size="xs" variant="outline" onClick={() => applyHunk("incoming")}>Aceitar {incomingLabel}</Button><Button size="xs" onClick={() => applyHunk("both")}>Aceitar ambos</Button></div>
												</>
											) : (
												<div className="flex items-center gap-2 text-xs text-emerald-600"><Check className="size-4" />Sem blocos pendentes. Revise o resultado e marque o arquivo como resolvido.</div>
											)}
										</div>
										<div className="min-h-0 flex-1"><WorkspaceFileEditor key={selected.path} ref={editorRef} path={selected.path} content={buffer} readOnly={false} annotateLabel="" onChange={() => setBuffer(editorRef.current?.getValue() ?? "")} /></div>
										<div className="flex shrink-0 items-center justify-between border-t border-border/60 px-3 py-2"><span className="text-[11px] text-muted-foreground">{hunks.length > 0 ? `${hunks.length} bloco(s) ainda pendente(s)` : dirty ? "Resultado alterado" : "Pronto para marcar como resolvido"}</span><Button size="sm" disabled={Boolean(busy) || hunks.length > 0} onClick={() => void saveAndResolve()}>{busy === "save" ? <Loader2 className="size-4 animate-spin" /> : <Check className="size-4" />}Salvar e marcar resolvido</Button></div>
									</>
								)}
							</section>
						) : null}
					</div>
				)}
			</DialogContent>
		</Dialog>
	);
}
