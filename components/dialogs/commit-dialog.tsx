import React, { useCallback, useEffect, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  FileCode,
  Loader2,
  Upload,
} from "lucide-react";
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
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import type { GitStatus } from "@/types/electron";

interface CommitDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  defaultMessage: string;
  onCommit: (message: string) => Promise<void>;
  projectPath: string;
  status: GitStatus | null;
}

function getFilesToCommit(
  status: GitStatus,
): { path: string; untracked: boolean }[] {
  const seen = new Set<string>();
  const result: { path: string; untracked: boolean }[] = [];
  for (const p of status.staged) {
    if (!seen.has(p)) {
      seen.add(p);
      result.push({ path: p, untracked: false });
    }
  }
  for (const p of status.unstaged) {
    if (!seen.has(p)) {
      seen.add(p);
      result.push({ path: p, untracked: false });
    }
  }
  for (const p of status.untracked) {
    if (!seen.has(p)) {
      seen.add(p);
      result.push({ path: p, untracked: true });
    }
  }
  return result;
}

export function CommitDialog({
  open,
  onOpenChange,
  defaultMessage,
  onCommit,
  projectPath,
  status,
}: CommitDialogProps) {
  const [message, setMessage] = useState(defaultMessage);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [showPushAfterCommit, setShowPushAfterCommit] = useState(false);
  const [isPushingInDialog, setIsPushingInDialog] = useState(false);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [diffContent, setDiffContent] = useState<string | null>(null);
  const [diffLoading, setDiffLoading] = useState(false);

  const filesToCommit =
    status && status.isDirty ? getFilesToCommit(status) : [];
  const selectedUntracked =
    selectedPath &&
    filesToCommit.some((f) => f.path === selectedPath && f.untracked);

  const loadDiff = useCallback(
    async (path: string) => {
      if (
        !window.electronAPI?.git?.getFileDiffHead ||
        filesToCommit.some((f) => f.path === path && f.untracked)
      ) {
        setDiffContent(null);
        return;
      }
      setDiffLoading(true);
      setDiffContent(null);
      try {
        const diff = await window.electronAPI.git.getFileDiffHead(
          projectPath,
          path,
        );
        setDiffContent(diff || "(sem diff)");
      } catch {
        setDiffContent("(erro ao carregar diff)");
      } finally {
        setDiffLoading(false);
      }
    },
    [projectPath, filesToCommit],
  );

  useEffect(() => {
    if (open) setMessage(defaultMessage);
  }, [open, defaultMessage]);

  useEffect(() => {
    if (!open) {
      setSelectedPath(null);
      setDiffContent(null);
      setShowPushAfterCommit(false);
    }
  }, [open]);

  const handleSelectFile = (path: string, untracked: boolean) => {
    setSelectedPath(path);
    if (untracked) {
      setDiffContent(null);
      return;
    }
    loadDiff(path);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = message.trim();
    if (!trimmed) return;
    setIsSubmitting(true);
    try {
      await onCommit(trimmed);
      setShowPushAfterCommit(true);
    } finally {
      setIsSubmitting(false);
    }
  };

  const handlePushInDialog = async () => {
    if (!projectPath || !window.electronAPI?.git?.push) {
      toast.error("Push indisponível");
      return;
    }
    setIsPushingInDialog(true);
    try {
      const result = await window.electronAPI.git.push(projectPath);
      if (result.success) {
        toast.success("Push realizado");
        onOpenChange(false);
      } else {
        toast.error(result.error ?? "Falha ao fazer push.");
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Erro desconhecido";
      toast.error(`Falha ao fazer push: ${msg}`);
    } finally {
      setIsPushingInDialog(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[700px] max-h-[90vh] flex flex-col">
        {showPushAfterCommit ? (
          <>
            <DialogHeader>
              <DialogTitle>Commit realizado</DialogTitle>
              <DialogDescription>
                Fazer push agora para enviar ao remoto?
              </DialogDescription>
            </DialogHeader>
            <DialogFooter className="pt-4">
              <Button
                type="button"
                variant="outline"
                onClick={() => onOpenChange(false)}
              >
                Fechar
              </Button>
              <Button
                type="button"
                onClick={handlePushInDialog}
                disabled={isPushingInDialog}
              >
                {isPushingInDialog ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <Upload className="mr-2 h-4 w-4" />
                )}
                Fazer push
              </Button>
            </DialogFooter>
          </>
        ) : (
          <form onSubmit={handleSubmit} className="flex flex-col min-h-0">
            <DialogHeader>
              <DialogTitle>Commitar alterações</DialogTitle>
              <DialogDescription>
                Revise os arquivos e a mensagem. Ao clicar em Commitar, será
                executado git add -A e commit.
              </DialogDescription>
            </DialogHeader>

            <div className="grid gap-4 py-4 min-h-0 flex-1">
              {filesToCommit.length > 0 && (
                <div className="grid gap-2 min-h-0">
                  <Label>Arquivos que serão commitados</Label>
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 min-h-0">
                    <ScrollArea className="h-[180px] rounded-md border bg-muted/30 p-2">
                      <ul className="space-y-0.5">
                        {filesToCommit.map(({ path, untracked }) => (
                          <li key={path}>
                            <button
                              type="button"
                              onClick={() => handleSelectFile(path, untracked)}
                              className={cn(
                                "w-full text-left px-2 py-1.5 rounded text-sm font-mono truncate flex items-center gap-1",
                                selectedPath === path
                                  ? "bg-primary/10 text-primary"
                                  : "hover:bg-muted",
                              )}
                            >
                              {selectedPath === path ? (
                                <ChevronDown className="h-3.5 w-3.5 shrink-0" />
                              ) : (
                                <ChevronRight className="h-3.5 w-3.5 shrink-0" />
                              )}
                              <span className="truncate">{path}</span>
                              {untracked && (
                                <span className="shrink-0 text-xs text-muted-foreground">
                                  (novo)
                                </span>
                              )}
                            </button>
                          </li>
                        ))}
                      </ul>
                    </ScrollArea>
                    <div className="rounded-md border bg-muted/30 p-2 min-h-[180px] flex flex-col">
                      {selectedPath ? (
                        <>
                          <div className="text-xs text-muted-foreground mb-1 font-mono truncate">
                            {selectedPath}
                          </div>
                          <ScrollArea className="flex-1 min-h-0">
                            {selectedUntracked ? (
                              <p className="text-sm text-muted-foreground p-2">
                                Novo arquivo (untracked)
                              </p>
                            ) : diffLoading ? (
                              <div className="flex items-center gap-2 p-2">
                                <Loader2 className="h-4 w-4 animate-spin" />
                                <span className="text-sm">
                                  Carregando diff…
                                </span>
                              </div>
                            ) : diffContent ? (
                              <pre className="text-xs font-mono p-2 whitespace-pre-wrap overflow-x-auto DiffBlock">
                                {diffContent.split("\n").map((line, i) => {
                                  const isAdd =
                                    line.startsWith("+") &&
                                    !line.startsWith("+++");
                                  const isDel =
                                    line.startsWith("-") &&
                                    !line.startsWith("---");
                                  return (
                                    <div
                                      key={i}
                                      className={cn(
                                        isAdd &&
                                          "text-green-600 dark:text-green-400",
                                        isDel &&
                                          "text-red-600 dark:text-red-400",
                                      )}
                                    >
                                      {line || " "}
                                    </div>
                                  );
                                })}
                              </pre>
                            ) : null}
                          </ScrollArea>
                        </>
                      ) : (
                        <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
                          <FileCode className="h-8 w-8 opacity-50 mb-2" />
                          <p>Clique em um arquivo para ver o diff</p>
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              )}

              {status === null && open && (
                <p className="text-sm text-muted-foreground py-2 flex items-center gap-2">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Carregando status do repositório…
                </p>
              )}
              {filesToCommit.length === 0 && status !== null && (
                <p className="text-sm text-muted-foreground py-2">
                  Nenhuma alteração para commitar.
                </p>
              )}

              <div className="grid gap-2">
                <Label htmlFor="commit-message">Mensagem</Label>
                <Textarea
                  id="commit-message"
                  value={message}
                  onChange={(e) => setMessage(e.target.value)}
                  placeholder="DevCommandCenter: título da missão"
                  className="min-h-[100px] resize-none"
                  disabled={isSubmitting}
                  required
                />
              </div>
            </div>

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => onOpenChange(false)}
                disabled={isSubmitting}
              >
                Cancelar
              </Button>
              <Button type="submit" disabled={isSubmitting || !message.trim()}>
                {isSubmitting ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : null}
                Commitar
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}
