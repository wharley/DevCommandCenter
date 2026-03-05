import React from "react";

import { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { Loader2, Pencil, Sparkles } from "lucide-react";
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

export interface InitialMissionForEdit {
  title: string;
  description: string;
  preserveInstructions?: string;
  providerId?: string;
  planProviderId?: string;
  codeProviderId?: string;
}

interface NewMissionDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  projectId: string;
  defaultProviderId?: string;
  /** When set, dialog runs in edit mode: title "Editar missão", submit calls update and does not navigate */
  missionId?: string;
  initialMission?: InitialMissionForEdit;
  /** Optional: called when user clicks "Dicas para acertar mais" to open the tips dialog */
  onOpenTips?: () => void;
}

export function NewMissionDialog({
  open,
  onOpenChange,
  projectId,
  defaultProviderId,
  missionId,
  initialMission,
  onOpenTips,
}: NewMissionDialogProps) {
  const navigate = useNavigate();
  const { providers } = useProviders();
  const { create, update } = useMissions(projectId);
  const activeProviders = providers.filter((p) => p.isActive);

  const isEditMode = Boolean(missionId && initialMission);

  const [isLoading, setIsLoading] = useState(false);
  const [formData, setFormData] = useState({
    title: "",
    description: "",
    preserveInstructions: "",
    providerId: defaultProviderId ?? "",
    planProviderId: "",
    codeProviderId: "",
  });

  // When opening in edit mode, fill form from initialMission
  useEffect(() => {
    if (open && isEditMode && initialMission) {
      setFormData({
        title: initialMission.title ?? "",
        description: initialMission.description ?? "",
        preserveInstructions: initialMission.preserveInstructions ?? "",
        providerId: initialMission.providerId ?? defaultProviderId ?? "",
        planProviderId: initialMission.planProviderId ?? "",
        codeProviderId: initialMission.codeProviderId ?? "",
      });
    }
    if (open && !isEditMode) {
      setFormData({
        title: "",
        description: "",
        preserveInstructions: "",
        providerId: defaultProviderId ?? "",
        planProviderId: "",
        codeProviderId: "",
      });
    }
  }, [open, isEditMode, initialMission, defaultProviderId]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!formData.title.trim() || !formData.description.trim()) {
      toast.error("Preencha título e descrição");
      return;
    }

    if (!formData.providerId) {
      toast.error("Selecione um provedor de IA");
      return;
    }

    setIsLoading(true);

    try {
      await new Promise((resolve) => setTimeout(resolve, 300));

      if (isEditMode && missionId) {
        await update(missionId, {
          title: formData.title.trim(),
          description: formData.description.trim(),
          preserveInstructions: formData.preserveInstructions.trim() || undefined,
          providerId: formData.providerId || undefined,
          planProviderId: formData.planProviderId || undefined,
          codeProviderId: formData.codeProviderId || undefined,
        });
        toast.success("Missão atualizada");
        onOpenChange(false);
        return;
      }

      const mission = await create({
        projectId,
        providerId: formData.providerId,
        planProviderId: formData.planProviderId || undefined,
        codeProviderId: formData.codeProviderId || undefined,
        title: formData.title.trim(),
        description: formData.description.trim(),
        preserveInstructions: formData.preserveInstructions.trim() || undefined,
      });

      toast.success("Missão criada com sucesso");
      onOpenChange(false);
      navigate(`/project/${projectId}/mission/${mission.id}`);

      setFormData({
        title: "",
        description: "",
        preserveInstructions: "",
        providerId: defaultProviderId ?? "",
        planProviderId: "",
        codeProviderId: "",
      });
    } catch {
      toast.error(isEditMode ? "Falha ao atualizar missão" : "Falha ao criar missão");
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[550px] max-h-[90vh] flex flex-col overflow-hidden">
        <form onSubmit={handleSubmit} className="flex flex-col min-h-0 flex-1 overflow-hidden">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              {isEditMode ? (
                <>
                  <Pencil className="h-5 w-5 text-primary" />
                  Editar missão
                </>
              ) : (
                <>
                  <Sparkles className="h-5 w-5 text-primary" />
                  Nova missão
                </>
              )}
            </DialogTitle>
            <DialogDescription>
              {isEditMode
                ? "Altere título, descrição ou provedor da missão."
                : "Descreva a tarefa de código com a qual o agente de IA deve ajudar."}
            </DialogDescription>
          </DialogHeader>

          <div className="overflow-y-auto min-h-0 flex-1">
            <div className="grid gap-4 py-4">
              <div className="grid gap-2">
                <Label htmlFor="title">Título da missão</Label>
                <Input
                  id="title"
                  placeholder="ex.: Migrar checkout para Stripe"
                  value={formData.title}
                  onChange={(e) =>
                    setFormData((prev) => ({ ...prev, title: e.target.value }))
                  }
                />
              </div>

              <div className="grid gap-2">
                <Label htmlFor="description">Descrição</Label>
                <Textarea
                  id="description"
                  placeholder="Descreva em linguagem natural o que você quer realizar. Seja específico sobre requisitos, restrições e resultados esperados..."
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
                <p className="text-xs text-muted-foreground">
                  Quanto mais detalhada a descrição, melhor a IA poderá entender e
                  executar sua solicitação.
                </p>
                {onOpenTips && (
                  <button
                    type="button"
                    onClick={onOpenTips}
                    className="text-xs text-primary hover:text-primary/80 hover:underline focus:outline-none focus:underline"
                  >
                    Dicas para acertar mais
                  </button>
                )}
              </div>

              <div className="grid gap-2">
                <Label htmlFor="preserveInstructions">
                  Preservar / Não alterar (opcional)
                </Label>
                <Textarea
                  id="preserveInstructions"
                  placeholder="Ex.: Mantenha 'Preview ao vivo' e 'Veja as mudanças em tempo real enquanto personaliza sua campanha.' Não use só NPS; a campanha pode ser eventos, NPS, etc."
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
                <p className="text-xs text-muted-foreground">
                  Liste títulos, frases ou trechos que devem permanecer iguais. A
                  IA receberá isso como instrução de não alterar.
                </p>
              </div>

              <div className="grid gap-2">
                <Label htmlFor="provider">Provedor de IA (padrão)</Label>
                <Select
                  value={formData.providerId}
                  onValueChange={(value) =>
                    setFormData((prev) => ({ ...prev, providerId: value }))
                  }
                >
                  <SelectTrigger id="provider">
                    <SelectValue placeholder="Selecione o provedor de IA" />
                  </SelectTrigger>
                  <SelectContent>
                    {activeProviders.length === 0 ? (
                      <SelectItem value="none" disabled>
                        Nenhum provedor configurado - vá em Configurações
                      </SelectItem>
                    ) : (
                      activeProviders.map((provider) => (
                        <SelectItem key={provider.id} value={provider.id}>
                          {provider.name}
                        </SelectItem>
                      ))
                    )}
                  </SelectContent>
                </Select>
                <p className="text-xs text-muted-foreground">
                  Na tela da missão você pode escolher provedores específicos para
                  plano e código, se quiser.
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
              disabled={isLoading || activeProviders.length === 0}
            >
              {isLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {isEditMode ? "Salvar" : "Criar missão"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
