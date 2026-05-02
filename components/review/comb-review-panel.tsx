"use client";

import React, { useCallback, useEffect, useMemo, useState } from "react";
import {
  ChevronsRight,
  ChevronDown,
  ExternalLink,
  Layers,
  Link2,
  PanelLeft,
  Plus,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ScrollArea } from "@/components/ui/scroll-area";
import { RepoReviewSection } from "@/components/review/repo-review-section";
import type { Comb, Project, ReviewTarget, UpdateCombDTO } from "@/lib/database/types";
import { recordProductSignal } from "@/lib/product/signals";
import { resolveReviewTargets } from "@/lib/review/resolve-review-targets";
import { openExternal } from "@/lib/shell-api";
import { toast } from "sonner";

function getCombStatusLabel(status: Comb["status"]): string {
  switch (status) {
    case "active":
      return "Ativo";
    case "ready_for_review":
      return "Revisão";
    case "applied":
      return "Aplicado";
    case "discarded":
      return "Descartado";
    case "archived":
      return "Arquivado";
    case "error":
      return "Erro";
    default:
      return status;
  }
}

type TargetStats = {
  ok: number;
  later: number;
  suspicious: number;
  fileCount: number;
};

/** token -> lista de ocorrências em outros targets */
type TokenHit = { targetId: string; label: string; filePath: string };

export function CombReviewPanel({
  comb,
  mainProjectPath: _mainProjectPath,
  projects,
  updateComb,
  onAction,
}: {
  comb: Comb;
  /** Mantido para compatibilidade com a página; paths vêm de `projects`. */
  mainProjectPath?: string;
  projects: Project[];
  updateComb: (id: string, data: UpdateCombDTO) => Promise<void>;
  onAction: () => void;
}) {
  const [statsByTarget, setStatsByTarget] = useState<
    Record<string, TargetStats>
  >({});
  const [tokenCatalog, setTokenCatalog] = useState<Record<string, TokenHit[]>>(
    {},
  );
  const [addTargetOpen, setAddTargetOpen] = useState(false);
  const [newTargetProjectId, setNewTargetProjectId] = useState("");
  const [newTargetLabel, setNewTargetLabel] = useState("");
  const [isContinuing, setIsContinuing] = useState(false);

  const resolved = useMemo(
    () => resolveReviewTargets(comb, projects),
    [comb, projects],
  );
  const forgeLink = comb.forgeLink;

  useEffect(() => {
    setStatsByTarget({});
    setTokenCatalog({});
  }, [comb.id]);

  const multi = resolved.length > 1;

  const handleStats = useCallback((targetId: string, s: TargetStats) => {
    setStatsByTarget((prev) => ({ ...prev, [targetId]: s }));
  }, []);

  const handleFileTokens = useCallback(
    (targetId: string, filePath: string, tokens: string[]) => {
      const label =
        resolved.find((r) => r.id === targetId)?.label ?? targetId;
      setTokenCatalog((prev) => {
        const next = { ...prev };
        for (const tok of tokens) {
          const list = [...(next[tok] ?? [])];
          const key = `${targetId}::${filePath}`;
          const filtered = list.filter(
            (h) => `${h.targetId}::${h.filePath}` !== key,
          );
          filtered.push({ targetId, label, filePath });
          next[tok] = filtered;
        }
        return next;
      });
    },
    [resolved],
  );

  const miniMap = useMemo(() => {
    let ok = 0;
    let later = 0;
    let suspicious = 0;
    let files = 0;
    for (const t of resolved) {
      const s = statsByTarget[t.id];
      if (!s) continue;
      ok += s.ok;
      later += s.later;
      suspicious += s.suspicious;
      files += s.fileCount;
    }
    return { ok, later, suspicious, files };
  }, [resolved, statsByTarget]);

  const addTargetProjectOptions = useMemo(() => {
    const inReview = new Set(resolved.map((r) => r.projectId));
    return projects.filter((p) => !inReview.has(p.id));
  }, [projects, resolved]);

  const handleAddReviewTarget = async () => {
    if (!newTargetProjectId.trim()) {
      toast.error("Selecione um projeto.");
      return;
    }
    const proj = projects.find((p) => p.id === newTargetProjectId);
    const label =
      newTargetLabel.trim() ||
      proj?.name ||
      newTargetProjectId.slice(0, 8);
    const extra: ReviewTarget = {
      id: `rt-${Date.now().toString(36)}`,
      label,
      projectId: newTargetProjectId,
      sourceCombId: null,
    };
    const current = comb.reviewTargets ?? [];
    await updateComb(comb.id, {
      reviewTargets: [...current, extra],
    });
    recordProductSignal("review_extra_repo_added");
    toast.success("Repositório adicionado à revisão");
    setAddTargetOpen(false);
    setNewTargetLabel("");
    setNewTargetProjectId("");
    onAction();
  };

  const handleContinue = async () => {
    const api = window.desktopAPI?.comb?.continueFromTargetBranch;
    if (!api) {
      toast.error("Ação de continuar indisponível neste ambiente.");
      return;
    }
    setIsContinuing(true);
    try {
      const result = await api(comb.id);
      if (!result?.success) {
        toast.error(result?.error ?? "Não foi possível continuar o workspace.");
        return;
      }
      toast.success("Workspace continuado na branch base.");
      onAction();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Falha ao continuar o workspace");
    } finally {
      setIsContinuing(false);
    }
  };

  if (!comb.worktreePath) {
    return (
      <div className="flex h-full items-center justify-center p-8">
        <p className="text-sm text-muted-foreground">
          Worktree ainda não criada.
        </p>
      </div>
    );
  }

  if (resolved.some((r) => !r.mainProjectPath?.trim())) {
    return (
      <div className="flex h-full items-center justify-center p-8">
        <p className="text-sm text-muted-foreground">
          Um ou mais projetos da revisão não foram encontrados no Hive.
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="shrink-0 space-y-2 border-b border-border px-4 py-2.5">
        <div className="space-y-1.5">
          <div className="flex min-w-0 items-start justify-between gap-3">
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2">
                <h3 className="text-base font-semibold leading-tight truncate">
                  {comb.name}
                </h3>
                <Badge variant="outline" className="shrink-0 text-[10px]">
                  {getCombStatusLabel(comb.status)}
                </Badge>
                {multi ? (
                  <Badge variant="secondary" className="gap-1 text-[10px]">
                    <Layers className="h-3 w-3" />
                    {resolved.length} targets
                  </Badge>
                ) : null}
              </div>
            </div>
            <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
              {comb.status === "applied" ? (
                <Button
                  variant="default"
                  size="sm"
                  className="gap-1 whitespace-nowrap"
                  disabled={isContinuing}
                  onClick={() => void handleContinue()}
                >
                  <ChevronsRight className="h-3.5 w-3.5" />
                  {isContinuing ? "Continuando…" : "Continue"}
                </Button>
              ) : null}
              <Button
                variant="outline"
                size="sm"
                className="gap-1 whitespace-nowrap"
                onClick={() => {
                  /* Volta para panes: pai controla aba; apenas sinaliza foco */
                  window.dispatchEvent(new CustomEvent("dcc:hive:goto-panes"));
                }}
              >
                <PanelLeft className="h-3.5 w-3.5" />
                Abrir Panes
              </Button>
              <Button
                variant="secondary"
                size="sm"
                className="gap-1 whitespace-nowrap"
                disabled={addTargetProjectOptions.length === 0}
                onClick={() => setAddTargetOpen(true)}
              >
                <Plus className="h-3.5 w-3.5" />
                Adicionar repo à revisão
              </Button>
            </div>
          </div>
          {comb.description ? (
            <p className="text-xs leading-snug text-muted-foreground line-clamp-2">
              {comb.description}
            </p>
          ) : null}
          <div className="flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
            {resolved.map((r) => (
              <span key={r.id} className="font-mono truncate max-w-[min(100%,240px)]">
                {r.label}: {r.isPrimaryCombWorktree ? (comb.branch ?? comb.baseBranch) : "checkout local"}
              </span>
            ))}
          </div>
          {forgeLink ? (
            <div className="flex flex-wrap items-center gap-2">
              <Badge variant="secondary" className="gap-1 font-mono text-[10px]">
                {forgeLink.forge === "gitlab" ? "MR" : "PR"} #{forgeLink.number}
              </Badge>
              <Button
                variant="outline"
                size="sm"
                className="gap-1 whitespace-nowrap"
                onClick={() => {
                  void openExternal(forgeLink.url);
                }}
              >
                <ExternalLink className="h-3.5 w-3.5" />
                View {forgeLink.forge === "gitlab" ? "MR" : "PR"}
              </Button>
            </div>
          ) : null}
        </div>

        {multi ? (
          <Collapsible defaultOpen={false} className="rounded-md border border-border bg-muted/30">
            <CollapsibleTrigger className="flex w-full items-center justify-between gap-2 px-3 py-2 text-left hover:bg-muted/40">
              <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                Mapa da revisão
              </span>
              <ChevronDown className="h-4 w-4 shrink-0 opacity-70" />
            </CollapsibleTrigger>
            <CollapsibleContent className="border-t border-border/60 px-3 pb-3 pt-0">
              <p className="mt-2 text-xs text-muted-foreground">
                {miniMap.files} arquivo(s) total · {miniMap.ok} OK ·{" "}
                {miniMap.later} rever depois · {miniMap.suspicious} suspeito(s)
              </p>
            </CollapsibleContent>
          </Collapsible>
        ) : null}

        <Collapsible defaultOpen={false} className="rounded-md border border-border bg-muted/20">
          <CollapsibleTrigger className="flex w-full items-center justify-between gap-2 px-3 py-2 text-left hover:bg-muted/30">
            <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
              Padrão recomendado
            </span>
            <ChevronDown className="h-4 w-4 shrink-0 opacity-70" />
          </CollapsibleTrigger>
          <CollapsibleContent className="border-t border-border/60 px-3 pb-3 pt-0 text-xs text-muted-foreground">
            <p className="mt-2 leading-relaxed">
              Use <span className="font-medium">1 Missão por história</span> e
              adicione os outros repositórios com o botão{" "}
              <span className="font-medium">Adicionar repo à revisão</span> acima
              para evitar duplicação de contexto.
            </p>
          </CollapsibleContent>
        </Collapsible>

        {multi &&
        Object.entries(tokenCatalog).some(([_, hits]) => {
          const ids = new Set(hits.map((h) => h.targetId));
          return ids.size >= 2;
        }) ? (
          <div className="rounded-md border border-border bg-muted/20 px-3 py-2">
            <p className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground mb-1 flex items-center gap-1">
              <Link2 className="h-3 w-3" />
              Tokens entre repos
            </p>
            <ScrollArea className="max-h-20">
              <div className="flex flex-wrap gap-1">
                {Object.entries(tokenCatalog)
                  .filter(([_, hits]) => {
                    const ids = new Set(hits.map((h) => h.targetId));
                    return ids.size >= 2;
                  })
                  .slice(0, 24)
                  .map(([tok, hits]) => (
                    <Badge
                      key={tok}
                      variant="outline"
                      className="text-[10px] font-mono max-w-[180px] truncate"
                      title={hits
                        .map((h) => `${h.label}:${h.filePath}`)
                        .join("; ")}
                    >
                      {tok}
                      <span className="ml-1 opacity-70">
                        ({new Set(hits.map((h) => h.targetId)).size} repos)
                      </span>
                    </Badge>
                  ))}
              </div>
            </ScrollArea>
          </div>
        ) : null}
      </div>

      {multi ? (
        <ScrollArea className="min-h-0 flex-1">
          <div className="space-y-6 p-4">
            {resolved.length === 0 ? (
              <div className="flex items-center justify-center p-8 text-sm text-muted-foreground">
                Nenhum target de revisão resolvido.
              </div>
            ) : (
              resolved.map((r) => (
                <RepoReviewSection
                  key={r.id}
                  targetId={r.id}
                  storageKey={`${comb.id}::${r.id}`}
                  title={r.label}
                  subtitle={`${r.mainProjectPath} · ${r.isPrimaryCombWorktree ? (comb.branch ?? comb.baseBranch) : "checkout"}`}
                  missionName={comb.name}
                  mainProjectPath={r.mainProjectPath}
                  worktreePath={r.worktreePath}
                  combId={comb.id}
                  useCombWorktreeApis={r.isPrimaryCombWorktree}
                  onAction={onAction}
                  onStatsChange={(s) => handleStats(r.id, s)}
                  onFileTokens={handleFileTokens}
                  compact
                />
              ))
            )}
          </div>
        </ScrollArea>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
          {resolved.length === 0 ? (
            <div className="flex items-center justify-center p-8 text-sm text-muted-foreground">
              Nenhum target de revisão resolvido.
            </div>
          ) : (
            resolved.map((r) => (
              <RepoReviewSection
                key={r.id}
                targetId={r.id}
                storageKey={`${comb.id}::${r.id}`}
                title={r.label}
                subtitle={`${r.mainProjectPath} · ${r.isPrimaryCombWorktree ? (comb.branch ?? comb.baseBranch) : "checkout"}`}
                missionName={comb.name}
                mainProjectPath={r.mainProjectPath}
                worktreePath={r.worktreePath}
                combId={comb.id}
                useCombWorktreeApis={r.isPrimaryCombWorktree}
                onAction={onAction}
                onStatsChange={(s) => handleStats(r.id, s)}
                compact={false}
              />
            ))
          )}
        </div>
      )}

      <Dialog open={addTargetOpen} onOpenChange={setAddTargetOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Adicionar repositório à revisão</DialogTitle>
            <DialogDescription>
              Inclui outro projeto do Hive na mesma aba Review (diffs no checkout
              local). O target principal da Missão continua usando a worktree.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-3 py-2">
            <div className="space-y-2">
              <Label>Projeto</Label>
              {addTargetProjectOptions.length === 0 ? (
                <p className="text-xs text-muted-foreground">
                  Nenhum projeto extra disponível para adicionar.
                </p>
              ) : (
                <Select
                  value={newTargetProjectId}
                  onValueChange={setNewTargetProjectId}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="Selecione o repositório" />
                  </SelectTrigger>
                  <SelectContent>
                    {addTargetProjectOptions.map((p) => (
                      <SelectItem key={p.id} value={p.id}>
                        {p.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              )}
            </div>
            <div className="space-y-2">
              <Label htmlFor="new-target-label">Rótulo (opcional)</Label>
              <Input
                id="new-target-label"
                placeholder="ex.: API mobile"
                value={newTargetLabel}
                onChange={(e) => setNewTargetLabel(e.target.value)}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setAddTargetOpen(false)}>
              Cancelar
            </Button>
            <Button
              onClick={() => void handleAddReviewTarget()}
              disabled={
                !newTargetProjectId || addTargetProjectOptions.length === 0
              }
            >
              Adicionar
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
