import React from "react";

import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Loader2, Sparkles } from "lucide-react";
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

interface NewMissionDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  projectId: string;
  defaultProviderId?: string;
}

export function NewMissionDialog({
  open,
  onOpenChange,
  projectId,
  defaultProviderId,
}: NewMissionDialogProps) {
  const navigate = useNavigate();
  const { providers } = useProviders();
  const { create } = useMissions(projectId);
  const activeProviders = providers.filter((p) => p.isActive);

  const [isLoading, setIsLoading] = useState(false);
  const [formData, setFormData] = useState({
    title: "",
    description: "",
    providerId: defaultProviderId ?? "",
  });

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

      const mission = await create({
        projectId,
        providerId: formData.providerId,
        title: formData.title.trim(),
        description: formData.description.trim(),
      });

      toast.success("Missão criada com sucesso");
      onOpenChange(false);
      navigate(`/project/${projectId}/mission/${mission.id}`);

      // Reset form
      setFormData({
        title: "",
        description: "",
        providerId: defaultProviderId ?? "",
      });
    } catch {
      toast.error("Falha ao criar missão");
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
              <Sparkles className="h-5 w-5 text-primary" />
              Nova missão
            </DialogTitle>
            <DialogDescription>
              Descreva a tarefa de código com a qual o agente de IA deve ajudar.
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
              </div>

              <div className="grid gap-2">
                <Label htmlFor="provider">Provedor de IA</Label>
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
              Criar missão
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
