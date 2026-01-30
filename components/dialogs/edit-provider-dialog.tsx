"use client";

import React from "react";

import { useState, useEffect } from "react";
import { Loader2, Bot, Terminal, Key } from "lucide-react";
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useProviders } from "@/hooks/use-data";
import { toast } from "sonner";
import type { Provider, ProviderType } from "@/lib/database/types";

interface EditProviderDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  provider: Provider;
}

const providerTypeConfig: Record<
  ProviderType,
  { needsApiKey: boolean; needsCli: boolean }
> = {
  "claude-code": { needsApiKey: false, needsCli: true },
  codex: { needsApiKey: false, needsCli: true },
  openai: { needsApiKey: true, needsCli: false },
  anthropic: { needsApiKey: true, needsCli: false },
  google: { needsApiKey: true, needsCli: false },
  cursor: { needsApiKey: false, needsCli: true },
  vscode: { needsApiKey: false, needsCli: true },
  custom: { needsApiKey: true, needsCli: false },
};

const modelsByProviderType: Partial<
  Record<ProviderType, { value: string; label: string }[]>
> = {
  "claude-code": [
    { value: "claude-opus-4-5-20251101", label: "Claude Opus 4.5" },
    { value: "claude-sonnet-4-5-20250929", label: "Claude Sonnet 4.5" },
    { value: "claude-sonnet-4-20250514", label: "Claude Sonnet 4" },
    { value: "claude-3-5-sonnet-20241022", label: "Claude 3.5 Sonnet" },
    { value: "claude-3-opus-20240229", label: "Claude 3 Opus" },
    { value: "claude-3-haiku-20240307", label: "Claude 3 Haiku" },
  ],
  codex: [{ value: "", label: "Padrão do Codex" }],
  openai: [
    { value: "gpt-4.1", label: "GPT-4.1" },
    { value: "gpt-4.1-mini", label: "GPT-4.1 Mini" },
    { value: "gpt-4.1-nano", label: "GPT-4.1 Nano" },
    { value: "gpt-4o", label: "GPT-4o" },
    { value: "gpt-4o-mini", label: "GPT-4o Mini" },
    { value: "o3", label: "o3" },
    { value: "o4-mini", label: "o4-mini" },
  ],
  anthropic: [
    { value: "claude-opus-4-5-20251101", label: "Claude Opus 4.5" },
    { value: "claude-sonnet-4-5-20250929", label: "Claude Sonnet 4.5" },
    { value: "claude-sonnet-4-20250514", label: "Claude Sonnet 4" },
    { value: "claude-3-5-sonnet-20241022", label: "Claude 3.5 Sonnet" },
    { value: "claude-3-opus-20240229", label: "Claude 3 Opus" },
    { value: "claude-3-haiku-20240307", label: "Claude 3 Haiku" },
  ],
  cursor: [
    { value: "", label: "Padrão (auto)" },
    { value: "gpt-5.2", label: "GPT-5.2" },
    { value: "gpt-5.2-codex", label: "GPT-5.2 Codex" },
    { value: "claude-4.5-opus", label: "Claude 4.5 Opus" },
    { value: "claude-4.5-sonnet", label: "Claude 4.5 Sonnet" },
    { value: "composer-1", label: "Composer 1" },
    { value: "gemini-3-flash", label: "Gemini 3 Flash" },
    { value: "gemini-3-pro", label: "Gemini 3 Pro" },
    { value: "grok-code", label: "Grok Code" },
  ],
};

export function EditProviderDialog({
  open,
  onOpenChange,
  provider,
}: EditProviderDialogProps) {
  const { update } = useProviders();

  const [isLoading, setIsLoading] = useState(false);
  const [formData, setFormData] = useState({
    name: "",
    apiKey: "",
    cliPath: "",
    model: "",
  });

  const config = providerTypeConfig[provider.type];

  useEffect(() => {
    if (provider) {
      setFormData({
        name: provider.name,
        apiKey: provider.apiKey ?? "",
        cliPath: provider.cliPath ?? "",
        model: (provider.config?.model as string) ?? "",
      });
    }
  }, [provider]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!formData.name.trim()) {
      toast.error("Preencha o nome");
      return;
    }

    setIsLoading(true);

    try {
      await new Promise((resolve) => setTimeout(resolve, 300));

      update(provider.id, {
        name: formData.name.trim(),
        apiKey: formData.apiKey.trim() || undefined,
        cliPath: formData.cliPath.trim() || undefined,
        config: formData.model
          ? { ...(provider.config ?? {}), model: formData.model }
          : (provider.config ?? undefined),
      });

      toast.success("Provedor atualizado com sucesso");
      onOpenChange(false);
    } catch {
      toast.error("Falha ao atualizar provedor");
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[500px]">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Bot className="h-5 w-5 text-primary" />
              Editar provedor
            </DialogTitle>
            <DialogDescription>
              Atualize a configuração de {provider.name}.
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="name">Nome de exibição</Label>
              <Input
                id="name"
                placeholder="ex.: Meu Claude Code"
                value={formData.name}
                onChange={(e) =>
                  setFormData((prev) => ({ ...prev, name: e.target.value }))
                }
              />
            </div>

            {config.needsApiKey && (
              <div className="grid gap-2">
                <Label htmlFor="apiKey" className="flex items-center gap-2">
                  <Key className="h-4 w-4" />
                  Chave de API
                </Label>
                <Input
                  id="apiKey"
                  type="password"
                  placeholder="sk-..."
                  value={formData.apiKey}
                  onChange={(e) =>
                    setFormData((prev) => ({ ...prev, apiKey: e.target.value }))
                  }
                />
                <p className="text-xs text-muted-foreground">
                  Deixe em branco para manter a chave atual.
                </p>
              </div>
            )}

            {config.needsCli && (
              <div className="grid gap-2">
                <Label htmlFor="cliPath" className="flex items-center gap-2">
                  <Terminal className="h-4 w-4" />
                  Caminho do CLI
                </Label>
                <Input
                  id="cliPath"
                  placeholder="/usr/local/bin/claude"
                  value={formData.cliPath}
                  onChange={(e) =>
                    setFormData((prev) => ({
                      ...prev,
                      cliPath: e.target.value,
                    }))
                  }
                />
              </div>
            )}

            <div className="grid gap-2">
              <Label htmlFor="model">Modelo padrão</Label>
              {modelsByProviderType[provider.type] ? (
                <Select
                  value={formData.model || "__default__"}
                  onValueChange={(value) =>
                    setFormData((prev) => ({
                      ...prev,
                      model: value === "__default__" ? "" : value,
                    }))
                  }
                >
                  <SelectTrigger id="model">
                    <SelectValue placeholder="Selecione o modelo" />
                  </SelectTrigger>
                  <SelectContent>
                    {modelsByProviderType[provider.type]!.map((opt) => (
                      <SelectItem
                        key={opt.value || "default"}
                        value={opt.value || "__default__"}
                      >
                        {opt.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              ) : (
                <Input
                  id="model"
                  placeholder="ex.: gpt-4-turbo"
                  value={formData.model}
                  onChange={(e) =>
                    setFormData((prev) => ({ ...prev, model: e.target.value }))
                  }
                />
              )}
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
              Salvar alterações
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
