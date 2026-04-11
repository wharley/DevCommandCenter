"use client";

import { useEffect, useState } from "react";
import { Loader2, RefreshCcw, Save } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import type { Project } from "@/lib/database/types";

interface ProjectRepoTomlDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  project: Project | null;
  onSaved?: () => Promise<void> | void;
}

export function ProjectRepoTomlDialog({
  open,
  onOpenChange,
  project,
  onSaved,
}: ProjectRepoTomlDialogProps) {
  const [content, setContent] = useState("");
  const [filePath, setFilePath] = useState<string | null>(null);
  const [source, setSource] = useState<string>("generated");
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadContent = async () => {
    if (!project) return;
    const api = window.db?.projects;
    if (!api?.getRepoConfigToml) {
      setError("Editor bruto disponível apenas no app desktop.");
      setContent("");
      setFilePath(null);
      return;
    }
    setIsLoading(true);
    setError(null);
    try {
      const result = await api.getRepoConfigToml(project.id);
      setContent(result.content ?? "");
      setFilePath(result.path ?? null);
      setSource(result.source ?? "generated");
    } catch (err) {
      const message = err instanceof Error ? err.message : "Falha ao carregar .dcc.toml";
      setError(message);
      toast.error(message);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    if (!open || !project) return;
    void loadContent();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, project?.id]);

  const handleSave = async () => {
    if (!project) return;
    const api = window.db?.projects;
    if (!api?.saveRepoConfigToml) {
      toast.error("Salvar .dcc.toml está disponível apenas no app desktop.");
      return;
    }
    setIsSaving(true);
    setError(null);
    try {
      const result = await api.saveRepoConfigToml(project.id, content);
      if (!result?.success) {
        throw new Error(result?.error || "Falha ao salvar .dcc.toml");
      }
      toast.success(".dcc.toml salvo");
      await onSaved?.();
      onOpenChange(false);
    } catch (err) {
      const message = err instanceof Error ? err.message : "Falha ao salvar .dcc.toml";
      setError(message);
      toast.error(message);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[92vh] overflow-y-auto sm:max-w-5xl">
        <DialogHeader>
          <DialogTitle>Editar .dcc.toml</DialogTitle>
          <DialogDescription>
            Edição direta da configuração do repositório. O arquivo abaixo é a fonte de verdade do projeto.
            {filePath ? <span className="block pt-1 font-mono text-[11px]">Arquivo: {filePath}</span> : null}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          {error ? (
            <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          ) : null}

          <Textarea
            value={content}
            onChange={(event) => setContent(event.target.value)}
            disabled={isLoading}
            rows={28}
            className="min-h-[520px] font-mono text-[12px] leading-5"
            spellCheck={false}
            placeholder={`[branch]
prefix = "dcc-comb"`}
          />
          <p className="text-xs text-muted-foreground">
            Origem atual: {source}. Validação de TOML acontece antes do salvamento.
          </p>
        </div>

        <DialogFooter className="gap-2 sm:gap-2">
          <Button
            type="button"
            variant="outline"
            onClick={() => void loadContent()}
            disabled={isLoading || isSaving || !project}
          >
            {isLoading ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <RefreshCcw className="mr-2 h-4 w-4" />}
            Recarregar
          </Button>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            Cancelar
          </Button>
          <Button type="button" onClick={handleSave} disabled={isSaving || isLoading || !project}>
            {isSaving ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <Save className="mr-2 h-4 w-4" />}
            Salvar
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
