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
import { useDesktopShell } from "@/hooks/use-desktop-shell";
import { toast } from "sonner";
import type {
  Provider,
  ProviderType,
  PermissionMode,
} from "@/lib/database/types";
import { PROVIDER_MODEL_REGISTRY } from "../../apps/desktop/src/lib/provider-model-registry";

const PROVIDER_TYPES_WITH_PERMISSION_MODES: ProviderType[] = [
  "claude-code",
  "codex",
  "gemini",
];

const PERMISSION_MODE_OPTIONS: { value: PermissionMode; label: string }[] = [
  { value: "", label: "Padrão (aceitar edições)" },
  { value: "plan", label: "Só planejar (exige aprovação)" },
  { value: "acceptEdits", label: "Aceitar edições automaticamente" },
  { value: "bypass", label: "Bypass (máxima automação)" },
];

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
  cursor: { needsApiKey: false, needsCli: true },
  gemini: { needsApiKey: false, needsCli: true },
  custom: { needsApiKey: true, needsCli: false },
};

const PROVIDER_TYPE_TO_REGISTRY_KEY: Partial<
  Record<ProviderType, keyof typeof PROVIDER_MODEL_REGISTRY>
> = {
  "claude-code": "claude_code",
  codex: "codex",
  cursor: "cursor",
  gemini: "gemini",
};

const OPENAI_MODELS: { value: string; label: string }[] = [
  { value: "gpt-4.1", label: "GPT-4.1" },
  { value: "gpt-4.1-mini", label: "GPT-4.1 Mini" },
  { value: "gpt-4.1-nano", label: "GPT-4.1 Nano" },
  { value: "gpt-4o", label: "GPT-4o" },
  { value: "gpt-4o-mini", label: "GPT-4o Mini" },
  { value: "o3", label: "o3" },
  { value: "o4-mini", label: "o4-mini" },
];

function getModelsForType(
  type: ProviderType,
): { value: string; label: string }[] | null {
  const registryKey = PROVIDER_TYPE_TO_REGISTRY_KEY[type];
  if (registryKey) {
    const models = PROVIDER_MODEL_REGISTRY[registryKey];
    return models.map((m) => ({
      value: m.id === "auto" ? "" : m.id,
      label: m.label,
    }));
  }
  if (type === "openai") return OPENAI_MODELS;
  if (type === "anthropic") {
    return PROVIDER_MODEL_REGISTRY.claude_code.map((m) => ({
      value: m.id,
      label: m.label,
    }));
  }
  return null;
}

const CLI_PROVIDER_TYPES_EDIT: ProviderType[] = [
  "claude-code",
  "codex",
  "cursor",
  "gemini",
];

export function EditProviderDialog({
  open,
  onOpenChange,
  provider,
}: EditProviderDialogProps) {
  const { update } = useProviders();
  const { validateCliPath, detectCliForProvider } = useDesktopShell();
  const [cliStatus, setCliStatus] = useState<{
    valid: boolean;
    message?: string;
  } | null>(null);

  const [isLoading, setIsLoading] = useState(false);
  const [formData, setFormData] = useState({
    name: "",
    apiKey: "",
    cliPath: "",
    model: "",
    permissionMode: "" as PermissionMode | "",
  });

  const config = providerTypeConfig[provider.type];
  const modelOptions = getModelsForType(provider.type);

  useEffect(() => {
    if (provider) {
      setFormData({
        name: provider.name,
        apiKey: "",
        cliPath: provider.cliPath ?? "",
        model: (provider.config?.model as string) ?? "",
        permissionMode:
          (provider.config?.permissionMode as PermissionMode | undefined) ?? "",
      });
    }
  }, [provider]);

  useEffect(() => {
    if (
      !formData.cliPath.trim() ||
      !CLI_PROVIDER_TYPES_EDIT.includes(provider.type)
    ) {
      setCliStatus(null);
      return;
    }
    let cancelled = false;
    validateCliPath(provider.type, formData.cliPath).then((result) => {
      if (!cancelled) setCliStatus(result);
    });
    return () => {
      cancelled = true;
    };
  }, [provider.type, formData.cliPath, validateCliPath]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!formData.name.trim()) {
      toast.error("Preencha o nome");
      return;
    }

    setIsLoading(true);

    try {
      await new Promise((resolve) => setTimeout(resolve, 300));

      const newConfig = {
        ...(provider.config ?? {}),
        ...(formData.model && { model: formData.model }),
        ...(PROVIDER_TYPES_WITH_PERMISSION_MODES.includes(provider.type) &&
          formData.permissionMode !== undefined && {
            permissionMode: formData.permissionMode || null,
          }),
      };
      update(provider.id, {
        name: formData.name.trim(),
        apiKey: formData.apiKey.trim() || undefined,
        cliPath: formData.cliPath.trim() || undefined,
        config: Object.keys(newConfig).length ? newConfig : undefined,
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
                  {(provider.hasApiKey ?? provider.apiKey)
                    ? "Deixe em branco para manter a chave atual."
                    : "Informe a chave de API."}
                </p>
              </div>
            )}

            {config.needsCli && (
              <div className="grid gap-2">
                <div className="flex items-center justify-between gap-2">
                  <Label htmlFor="cliPath" className="flex items-center gap-2">
                    <Terminal className="h-4 w-4" />
                    Caminho do CLI
                  </Label>
                  {CLI_PROVIDER_TYPES_EDIT.includes(provider.type) && (
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="h-8 text-xs"
                      onClick={() => {
                        detectCliForProvider(provider.type).then((path) => {
                          if (path)
                            setFormData((prev) => ({
                              ...prev,
                              cliPath: path,
                            }));
                          else
                            toast.error(
                              "CLI não encontrado. Digite o caminho completo (ex.: /opt/homebrew/bin/claude) ou o nome do comando se estiver no PATH.",
                            );
                        });
                      }}
                    >
                      Detectar automaticamente
                    </Button>
                  )}
                </div>
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
                {cliStatus !== null && (
                  <p
                    className={
                      cliStatus.valid
                        ? "text-xs text-green-600 dark:text-green-500"
                        : "text-xs text-destructive"
                    }
                  >
                    {cliStatus.valid
                      ? "CLI encontrado e funcionando."
                      : `CLI não encontrado${cliStatus.message ? `: ${cliStatus.message}` : "."}`}
                  </p>
                )}
              </div>
            )}

            {PROVIDER_TYPES_WITH_PERMISSION_MODES.includes(provider.type) && (
              <div className="grid gap-2">
                <Label htmlFor="permissionMode">Modo de permissão</Label>
                <Select
                  value={formData.permissionMode || "__default__"}
                  onValueChange={(value) =>
                    setFormData((prev) => ({
                      ...prev,
                      permissionMode:
                        value === "__default__" ? "" : (value as PermissionMode),
                    }))
                  }
                >
                  <SelectTrigger id="permissionMode">
                    <SelectValue placeholder="Modo de permissão" />
                  </SelectTrigger>
                  <SelectContent>
                    {PERMISSION_MODE_OPTIONS.map((opt) => (
                      <SelectItem
                        key={opt.value || "default"}
                        value={opt.value || "__default__"}
                      >
                        {opt.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <p className="text-xs text-muted-foreground">
                  Controla se o agente só planeja, aceita edições automaticamente
                  ou bypass de aprovações.
                </p>
              </div>
            )}

            <div className="grid gap-2">
              <Label htmlFor="model">Modelo padrão</Label>
              {modelOptions ? (
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
                    {modelOptions.map((opt) => (
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
