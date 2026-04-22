"use client";

import React, { useState, useMemo } from "react";
import { cn } from "@/lib/utils";
import {
  AlertTriangle,
  ArrowDownToLine,
  Check,
  Clock,
  FileCode,
  GitMerge,
  Loader2,
  Plus,
  RotateCcw,
  Search,
  Trash2,
  Upload,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { DiffFileTree } from "@/components/review/diff-file-tree";
import type { ReviewFlag } from "@/components/review/repo-review-section";

type DiffFile = {
  path: string;
  status: string;
  diff: string;
  insertions?: number;
  deletions?: number;
};

export type GitActionsPanelProps = {
  files: DiffFile[];
  fileFlags: Record<string, ReviewFlag>;
  activeDiffPath: string | null;
  onFileSelect: (path: string) => void;
  isPushing: boolean;
  isPulling: boolean;
  isMerging: boolean;
  worktreeDirty: boolean;
  mainRepoDirty: boolean;
  canMergeComb: boolean;
  mergeUiBlocked: boolean;
  onCommit: () => void;
  onPush: () => void;
  onPull: () => void;
  onDiscard: () => void;
  onMerge: () => void;
  targetBranch: string;
  branchList: string[];
  onTargetBranchChange: (b: string) => void;
  onStageFile?: (path: string) => void;
  onDiscardFile?: (path: string, status: string) => void;
};

const STATUS_STYLES: Record<string, { bg: string; text: string }> = {
  A: { bg: "bg-emerald-500/20", text: "text-emerald-600 dark:text-emerald-400" },
  M: { bg: "bg-amber-500/20", text: "text-amber-600 dark:text-amber-400" },
  D: { bg: "bg-rose-500/20", text: "text-rose-600 dark:text-rose-400" },
  R: { bg: "bg-blue-500/20", text: "text-blue-600 dark:text-blue-400" },
};

function statusStyle(status: string) {
  const key = status.charAt(0).toUpperCase();
  return (
    STATUS_STYLES[key] ?? {
      bg: "bg-muted",
      text: "text-muted-foreground",
    }
  );
}

function fileBasename(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

function fileDir(path: string): string {
  const parts = path.split(/[/\\]/);
  if (parts.length <= 1) return "";
  return parts.slice(0, -1).join("/");
}

export function GitActionsPanel({
  files,
  fileFlags,
  activeDiffPath,
  onFileSelect,
  isPushing,
  isPulling,
  isMerging,
  worktreeDirty,
  mainRepoDirty,
  canMergeComb,
  mergeUiBlocked,
  onCommit,
  onPush,
  onPull,
  onDiscard,
  onMerge,
  targetBranch,
  branchList,
  onTargetBranchChange,
  onStageFile,
  onDiscardFile,
}: GitActionsPanelProps) {
  const [search, setSearch] = useState("");

  const filteredFiles = useMemo(() => {
    if (!search.trim()) return files;
    const q = search.toLowerCase();
    return files.filter((f) => f.path.toLowerCase().includes(q));
  }, [files, search]);

  const gitActionInFlight = isPushing || isPulling || isMerging;

  return (
    <div className="flex h-full min-h-0 flex-col border-l border-border bg-card/20">
      <Tabs defaultValue="changes" className="flex h-full min-h-0 flex-col">
        <div className="shrink-0 border-b border-border px-2 pt-2 pb-0 space-y-2">
          <TabsList className="h-7 w-full">
            <TabsTrigger value="changes" className="flex-1 text-xs gap-1.5">
              Changes
              {files.length > 0 && (
                <Badge
                  variant="secondary"
                  className="h-4 px-1 text-[10px] min-w-[16px] justify-center"
                >
                  {files.length}
                </Badge>
              )}
            </TabsTrigger>
            <TabsTrigger value="files" className="flex-1 text-xs">
              Files
            </TabsTrigger>
          </TabsList>
          <div className="relative pb-2">
            <Search className="absolute left-2 top-1/2 h-3 w-3 -translate-y-1/2 text-muted-foreground pointer-events-none" />
            <Input
              className="h-7 pl-6 text-xs"
              placeholder="Filtrar arquivos…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>
        </div>

        {/* ── CHANGES TAB ─────────────────────────────────────────── */}
        <TabsContent
          value="changes"
          className="mt-0 flex min-h-0 flex-1 flex-col data-[state=inactive]:hidden"
        >
          <div className="shrink-0 border-b border-border/60 px-2 py-2 space-y-1.5">
            <div className="grid grid-cols-3 gap-1.5">
              <Button
                size="sm"
                variant="outline"
                className="h-7 col-span-1 gap-1 text-xs px-2"
                disabled={files.length === 0 || gitActionInFlight}
                onClick={onCommit}
                title="Abrir diálogo de commit"
              >
                <FileCode className="h-3 w-3 shrink-0" />
                Commit
              </Button>
              <Button
                size="sm"
                variant="outline"
                className="h-7 col-span-1 gap-1 text-xs px-2"
                disabled={gitActionInFlight}
                onClick={onPush}
                title="Push do branch para o remoto"
              >
                {isPushing ? (
                  <Loader2 className="h-3 w-3 shrink-0 animate-spin" />
                ) : (
                  <Upload className="h-3 w-3 shrink-0" />
                )}
                Push
              </Button>
              <Button
                size="sm"
                variant="outline"
                className="h-7 col-span-1 gap-1 text-xs px-2"
                disabled={gitActionInFlight}
                onClick={onPull}
                title="Pull do remoto para o branch atual"
              >
                {isPulling ? (
                  <Loader2 className="h-3 w-3 shrink-0 animate-spin" />
                ) : (
                  <ArrowDownToLine className="h-3 w-3 shrink-0" />
                )}
                Pull
              </Button>
            </div>

            <div className={cn("grid gap-1.5", canMergeComb ? "grid-cols-2" : "grid-cols-1")}>
              <Button
                size="sm"
                variant="ghost"
                className="h-7 gap-1 text-xs text-muted-foreground hover:text-destructive hover:border-destructive/40 border border-transparent"
                disabled={files.length === 0 || gitActionInFlight}
                onClick={onDiscard}
                title="Descartar alterações locais (git reset --hard)"
              >
                <Trash2 className="h-3 w-3 shrink-0" />
                Descartar
              </Button>
              {canMergeComb && (
                <Button
                  size="sm"
                  variant="default"
                  className="h-7 gap-1 text-xs"
                  disabled={mergeUiBlocked || gitActionInFlight}
                  onClick={onMerge}
                  title={
                    mergeUiBlocked
                      ? "Limpe a worktree e o repositório principal antes do merge"
                      : "Integrar branch da Missão no repositório principal"
                  }
                >
                  {isMerging ? (
                    <Loader2 className="h-3 w-3 shrink-0 animate-spin" />
                  ) : (
                    <GitMerge className="h-3 w-3 shrink-0" />
                  )}
                  Merge…
                </Button>
              )}
            </div>

            {canMergeComb && (
              <div>
                <Label className="text-[10px] text-muted-foreground uppercase tracking-wide mb-1 block">
                  Branch de destino
                </Label>
                {branchList.length > 0 ? (
                  <Select value={targetBranch} onValueChange={onTargetBranchChange}>
                    <SelectTrigger className="h-7 text-xs">
                      <SelectValue placeholder="Branch" />
                    </SelectTrigger>
                    <SelectContent>
                      {branchList.map((br) => (
                        <SelectItem key={br} value={br} className="text-xs">
                          {br}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                ) : (
                  <Input
                    className="h-7 text-xs"
                    placeholder="main"
                    value={targetBranch}
                    onChange={(e) => onTargetBranchChange(e.target.value)}
                  />
                )}
              </div>
            )}

            {(worktreeDirty || mainRepoDirty) && (
              <div className="flex flex-col gap-1">
                {worktreeDirty && (
                  <div className="flex items-center gap-1.5 rounded-md border border-amber-500/40 bg-amber-500/10 px-2 py-1 text-[10px] text-amber-600 dark:text-amber-400">
                    <AlertTriangle className="h-3 w-3 shrink-0" />
                    Worktree com alterações por commitar
                  </div>
                )}
                {mainRepoDirty && (
                  <div className="flex items-center gap-1.5 rounded-md border border-rose-500/40 bg-rose-500/10 px-2 py-1 text-[10px] text-rose-600 dark:text-rose-400">
                    <AlertTriangle className="h-3 w-3 shrink-0" />
                    Repositório principal com alterações
                  </div>
                )}
              </div>
            )}
          </div>

          <ScrollArea className="min-h-0 flex-1">
            {filteredFiles.length === 0 ? (
              <p className="px-3 py-6 text-center text-xs text-muted-foreground">
                {files.length === 0
                  ? "Nenhuma alteração detectada."
                  : "Nenhum arquivo corresponde ao filtro."}
              </p>
            ) : (
              <ul className="py-1">
                {filteredFiles.map((f) => {
                  const { bg, text } = statusStyle(f.status);
                  const statusLabel = f.status.charAt(0).toUpperCase() || "?";
                  const flag = fileFlags[f.path] ?? "ok";
                  const isActive = f.path === activeDiffPath;
                  const dir = fileDir(f.path);
                  const isUntracked = f.status === "untracked";
                  return (
                    <li key={f.path} className="group">
                      <div
                        role="button"
                        tabIndex={0}
                        className={cn(
                          "relative flex w-full items-center gap-2 px-2 py-1.5 text-left transition-colors hover:bg-muted/40 cursor-pointer",
                          isActive && "bg-primary/10",
                        )}
                        onClick={() => onFileSelect(f.path)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter" || e.key === " ") onFileSelect(f.path);
                        }}
                      >
                        <span
                          className={cn(
                            "shrink-0 rounded px-1 py-0.5 text-[9px] font-bold uppercase",
                            bg,
                            text,
                          )}
                        >
                          {statusLabel}
                        </span>
                        <div className="min-w-0 flex-1">
                          <p className="truncate font-mono text-[11px] leading-tight">
                            {fileBasename(f.path)}
                          </p>
                          {dir && (
                            <p className="truncate text-[10px] leading-tight text-muted-foreground">
                              {dir}
                            </p>
                          )}
                        </div>
                        <div className="shrink-0 flex items-center gap-1 pr-2 font-mono text-[10px] tabular-nums">
                          {(f.insertions ?? 0) > 0 && (
                            <span className="text-emerald-600 dark:text-emerald-400">
                              +{f.insertions}
                            </span>
                          )}
                          {(f.deletions ?? 0) > 0 && (
                            <span className="text-rose-600 dark:text-rose-400">
                              -{f.deletions}
                            </span>
                          )}
                        </div>
                        <div className="shrink-0 flex items-center gap-0.5">
                          {/* Flag de revisão */}
                          <div className="shrink-0 opacity-60">
                            {flag === "ok" && (
                              <Check className="h-3 w-3 text-emerald-500" />
                            )}
                            {flag === "later" && (
                              <Clock className="h-3 w-3 text-amber-500" />
                            )}
                            {flag === "suspicious" && (
                              <AlertTriangle className="h-3 w-3 text-rose-500" />
                            )}
                          </div>
                        </div>
                        {(onStageFile || onDiscardFile) && (
                          <div className="pointer-events-none absolute right-1 top-1/2 z-10 -translate-y-1/2 opacity-0 transition-opacity duration-150 group-hover:opacity-100 group-focus-within:opacity-100">
                            <div className="pointer-events-auto flex items-center gap-0.5 rounded-md border border-border/70 bg-background/95 px-1 py-0.5 shadow-sm backdrop-blur supports-[backdrop-filter]:bg-background/85">
                              {onStageFile && (
                                <button
                                  type="button"
                                  className="h-5 w-5 rounded flex items-center justify-center text-muted-foreground hover:text-emerald-600 hover:bg-emerald-500/15 transition-colors"
                                  title="Adicionar ao stage (git add)"
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    onStageFile(f.path);
                                  }}
                                >
                                  <Plus className="h-3 w-3" />
                                </button>
                              )}
                              {onDiscardFile && (
                                <button
                                  type="button"
                                  className="h-5 w-5 rounded flex items-center justify-center text-muted-foreground hover:text-rose-500 hover:bg-rose-500/15 transition-colors"
                                  title={isUntracked ? "Remover arquivo (git clean)" : "Descartar alterações (git restore)"}
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    onDiscardFile(f.path, f.status);
                                  }}
                                >
                                  <RotateCcw className="h-3 w-3" />
                                </button>
                              )}
                            </div>
                          </div>
                        )}
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
          </ScrollArea>
        </TabsContent>

        {/* ── FILES TAB ───────────────────────────────────────────── */}
        <TabsContent
          value="files"
          className="mt-0 min-h-0 flex-1 overflow-hidden data-[state=inactive]:hidden"
        >
          {filteredFiles.length === 0 ? (
            <p className="px-3 py-6 text-center text-xs text-muted-foreground">
              {files.length === 0
                ? "Nenhuma alteração detectada."
                : "Nenhum arquivo corresponde ao filtro."}
            </p>
          ) : (
            <DiffFileTree
              files={filteredFiles}
              selectedPath={activeDiffPath}
              onFileSelect={onFileSelect}
              compact={false}
            />
          )}
        </TabsContent>
      </Tabs>
    </div>
  );
}
