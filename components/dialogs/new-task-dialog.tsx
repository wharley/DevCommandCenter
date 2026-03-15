"use client";

import React, { useState, useEffect, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { GitBranch, Loader2, Terminal } from "lucide-react";
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
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useProviders, useMissions } from "@/hooks/use-data";
import { toast } from "sonner";
import type { Provider } from "@/lib/database/types";

const CLI_PROVIDER_TYPES = ["codex", "claude-code", "gemini", "cursor"] as const;

function isCliProvider(type: string): type is (typeof CLI_PROVIDER_TYPES)[number] {
  return CLI_PROVIDER_TYPES.includes(type as (typeof CLI_PROVIDER_TYPES)[number]);
}

export interface InitialTaskForCreate {
  title?: string;
  description?: string;
  preserveInstructions?: string;
}

interface NewTaskDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  projectId: string;
  /** Path of the project repo; used to show current branch (worktree is created from it) */
  projectPath?: string | null;
  /** Optional prefill when opened from quick create or workflow choice */
  initialTask?: InitialTaskForCreate;
}

export function NewTaskDialog({
  open,
  onOpenChange,
  projectId,
  projectPath,
  initialTask,
}: NewTaskDialogProps) {
  const navigate = useNavigate();
  const { providers } = useProviders();
  const { create } = useMissions(projectId);

  const cliProviders = useMemo(
    () => providers.filter((p) => p.isActive && isCliProvider(p.type)),
    [providers],
  );

  const firstCliProviderId = cliProviders[0]?.id ?? "";

  const [isLoading, setIsLoading] = useState(false);
  const [projectBranch, setProjectBranch] = useState<string | null>(null);
  const [projectBranchLoading, setProjectBranchLoading] = useState(false);
  const [formData, setFormData] = useState({
    title: "",
    description: "",
    preserveInstructions: "",
    providerId: "",
  });

  useEffect(() => {
    if (open) {
      setFormData({
        title: initialTask?.title ?? "",
        description: initialTask?.description ?? "",
        preserveInstructions: initialTask?.preserveInstructions ?? "",
        providerId: firstCliProviderId,
      });
    }
  }, [open, initialTask?.title, initialTask?.description, initialTask?.preserveInstructions, firstCliProviderId]);

  useEffect(() => {
    if (!open || !projectPath?.trim() || typeof window === "undefined" || !window.electronAPI?.git?.getCurrentBranch) {
      setProjectBranch(null);
      setProjectBranchLoading(false);
      return;
    }
    let cancelled = false;
    setProjectBranchLoading(true);
    setProjectBranch(null);
    window.electronAPI.git
      .getCurrentBranch(projectPath.trim())
      .then((branch) => {
        if (!cancelled) {
          setProjectBranch(branch?.trim() || null);
        }
      })
      .catch(() => {
        if (!cancelled) setProjectBranch(null);
      })
      .finally(() => {
        if (!cancelled) setProjectBranchLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, projectPath]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!formData.title.trim() || !formData.description.trim()) {
      toast.error("Preencha título e descrição");
      return;
    }
    if (!formData.providerId) {
      toast.error("Selecione um agente (CLI)");
      return;
    }
    setIsLoading(true);
    try {
      const mission = await create({
        projectId,
        providerId: formData.providerId,
        planProviderId: formData.providerId,
        codeProviderId: formData.providerId,
        title: formData.title.trim(),
        description: formData.description.trim(),
        preserveInstructions: formData.preserveInstructions.trim() || undefined,
        missionType: "agents_cli",
      });
      toast.success("Tarefa criada. Abrindo terminal...");
      onOpenChange(false);
      navigate(`/project/${projectId}/task/${mission.id}`);
      setFormData({
        title: "",
        description: "",
        preserveInstructions: "",
        providerId: cliProviders[0]?.id || "",
      });
    } catch {
      toast.error("Falha ao criar tarefa");
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[550px] max-h-[90vh] flex flex-col overflow-hidden">
        <form
          onSubmit={handleSubmit}
          className="flex flex-col min-h-0 flex-1 overflow-hidden"
        >
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Terminal className="h-5 w-5 text-primary" />
              Nova tarefa (agente no terminal)
            </DialogTitle>
            <DialogDescription>
              Cada tarefa usa um agente e um branch. O contexto (título e descrição) será passado ao terminal ao abrir.
            </DialogDescription>
          </DialogHeader>

          {projectPath?.trim() && (
            <div className="rounded-md border border-border bg-muted/30 px-3 py-2 text-sm">
              <div className="flex items-center gap-2 font-medium text-foreground">
                <GitBranch className="h-4 w-4 shrink-0 text-muted-foreground" />
                {projectBranchLoading ? (
                  <span className="text-muted-foreground">Carregando branch…</span>
                ) : (
                  <span>
                    Branch base do projeto:{" "}
                    <span className="font-mono">
                      {projectBranch ?? "—"}
                    </span>
                  </span>
                )}
              </div>
              <p className="mt-1 text-xs text-muted-foreground">
                A pasta da tarefa (worktree) será criada a partir deste branch quando você abrir o terminal.
              </p>
            </div>
          )}

          <div className="overflow-y-auto min-h-0 flex-1">
            <div className="grid gap-4 py-4">
              <div className="grid gap-2">
                <Label htmlFor="task-title">Título da tarefa</Label>
                <Input
                  id="task-title"
                  placeholder="ex.: Refatorar módulo de pagamento"
                  value={formData.title}
                  onChange={(e) =>
                    setFormData((prev) => ({ ...prev, title: e.target.value }))
                  }
                />
              </div>

              <div className="grid gap-2">
                <Label htmlFor="task-description">Descrição</Label>
                <Textarea
                  id="task-description"
                  placeholder="Descreva o que o agente deve fazer..."
                  value={formData.description}
                  onChange={(e) =>
                    setFormData((prev) => ({
                      ...prev,
                      description: e.target.value,
                    }))
                  }
                  rows={5}
                  className="max-h-48 overflow-y-auto resize-none"
                />
              </div>

              <div className="grid gap-2">
                <Label htmlFor="task-preserve">Preservar / Não alterar (opcional)</Label>
                <Textarea
                  id="task-preserve"
                  placeholder="Trechos ou arquivos que não devem ser alterados..."
                  value={formData.preserveInstructions}
                  onChange={(e) =>
                    setFormData((prev) => ({
                      ...prev,
                      preserveInstructions: e.target.value,
                    }))
                  }
                  rows={2}
                  className="max-h-24 overflow-y-auto resize-none"
                />
              </div>

              <div className="grid gap-2">
                <Label htmlFor="task-agent">Abrir com agente</Label>
                <Select
                  value={formData.providerId}
                  onValueChange={(value) =>
                    setFormData((prev) => ({ ...prev, providerId: value }))
                  }
                >
                  <SelectTrigger id="task-agent">
                    <SelectValue placeholder="Selecione o agente" />
                  </SelectTrigger>
                  <SelectContent>
                    {cliProviders.length === 0 ? (
                      <SelectItem value="none" disabled>
                        Nenhum agente CLI configurado – vá em Configurações
                      </SelectItem>
                    ) : (
                      cliProviders.map((p: Provider) => (
                        <SelectItem key={p.id} value={p.id}>
                          {p.name}
                        </SelectItem>
                      ))
                    )}
                  </SelectContent>
                </Select>
                <p className="text-xs text-muted-foreground">
                  Provedores configurados em Configurações (Codex, Claude, Gemini, Cursor).
                </p>
              </div>
            </div>
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
            >
              Cancelar
            </Button>
            <Button
              type="submit"
              disabled={isLoading || cliProviders.length === 0}
            >
              {isLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              Criar tarefa
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
