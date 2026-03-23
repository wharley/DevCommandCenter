import React from "react";

import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { FolderOpen, Loader2 } from "lucide-react";
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
import { useDesktopShell } from "@/hooks/use-desktop-shell";
import { useProjects, useProviders } from "@/hooks/use-data";
import { toast } from "sonner";

interface AddProjectDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function AddProjectDialog({
  open,
  onOpenChange,
}: AddProjectDialogProps) {
  const navigate = useNavigate();
  const { selectDirectory, isDesktop } = useDesktopShell();
  const { create } = useProjects();
  const { providers } = useProviders();
  const activeProviders = providers.filter((p) => p.isActive);

  const [isLoading, setIsLoading] = useState(false);
  const [isBrowsing, setIsBrowsing] = useState(false);
  const [formData, setFormData] = useState({
    name: "",
    path: "",
    description: "",
    defaultProviderId: "",
  });

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!formData.name.trim() || !formData.path.trim()) {
      toast.error("Preencha nome e caminho");
      return;
    }

    setIsLoading(true);

    try {
      await new Promise((resolve) => setTimeout(resolve, 500));

      const project = await create({
        name: formData.name.trim(),
        path: formData.path.trim(),
        description: formData.description.trim() || undefined,
        defaultProviderId: formData.defaultProviderId || undefined,
        gitRemoteUrl: undefined,
      });

      toast.success("Projeto adicionado com sucesso");
      onOpenChange(false);
      localStorage.setItem("dcc:hive:selectedProject", project.id);
      navigate("/", { replace: true });

      // Reset form
      setFormData({
        name: "",
        path: "",
        description: "",
        defaultProviderId: "",
      });
    } catch {
      toast.error("Falha ao adicionar projeto");
    } finally {
      setIsLoading(false);
    }
  };

  const handleBrowse = async () => {
    setIsBrowsing(true);
    try {
      const path = await selectDirectory();
      if (path) {
        setFormData((prev) => ({ ...prev, path }));
      } else if (!isDesktop) {
        toast.info(
          "Seletor de pasta estará disponível no app desktop. Digite o caminho manualmente.",
        );
      }
    } finally {
      setIsBrowsing(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[500px]">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle>Adicionar projeto</DialogTitle>
            <DialogDescription>
              Adicione um repositório local para gerenciar com missões de código
              com IA.
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="name">Nome do projeto</Label>
              <Input
                id="name"
                placeholder="Meu projeto incrível"
                value={formData.name}
                onChange={(e) =>
                  setFormData((prev) => ({ ...prev, name: e.target.value }))
                }
              />
            </div>

            <div className="grid gap-2">
              <Label htmlFor="path">Caminho do repositório</Label>
              <div className="flex gap-2">
                <Input
                  id="path"
                  placeholder="/Users/you/projects/my-project"
                  value={formData.path}
                  onChange={(e) =>
                    setFormData((prev) => ({ ...prev, path: e.target.value }))
                  }
                  className="flex-1"
                />
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  onClick={handleBrowse}
                  disabled={isBrowsing}
                >
                  {isBrowsing ? (
                    <Loader2 className="h-4 w-4 animate-spin" />
                  ) : (
                    <FolderOpen className="h-4 w-4" />
                  )}
                </Button>
              </div>
              <p className="text-xs text-muted-foreground">
                A pasta local que contém seu repositório git
              </p>
            </div>

            <div className="grid gap-2">
              <Label htmlFor="description">Descrição (opcional)</Label>
              <Textarea
                id="description"
                placeholder="Breve descrição do projeto..."
                value={formData.description}
                onChange={(e) =>
                  setFormData((prev) => ({
                    ...prev,
                    description: e.target.value,
                  }))
                }
                rows={2}
              />
            </div>

            <div className="grid gap-2">
              <Label htmlFor="provider">Provedor de IA padrão (opcional)</Label>
              <Select
                value={formData.defaultProviderId}
                onValueChange={(value) =>
                  setFormData((prev) => ({ ...prev, defaultProviderId: value }))
                }
              >
                <SelectTrigger id="provider">
                  <SelectValue placeholder="Selecione um provedor" />
                </SelectTrigger>
                <SelectContent>
                  {activeProviders.length === 0 ? (
                    <SelectItem value="none" disabled>
                      Nenhum provedor configurado
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

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
            >
              Cancelar
            </Button>
            <Button type="submit" disabled={isLoading}>
              {isLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              Adicionar projeto
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
