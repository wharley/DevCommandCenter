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
import type { ProviderType, PermissionMode } from "@/lib/database/types";

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

const providerTypeToCliCommand: Partial<Record<ProviderType, string>> = {
  "claude-code": "claude",
  codex: "codex",
  cursor: "agent",
  gemini: "gemini",
};

interface AddProviderDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** Tipos de provedor atualmente implementados (com adapter). Os demais ficam ocultos na UI. */
const IMPLEMENTED_PROVIDER_TYPES: ProviderType[] = [
  "claude-code",
  "codex",
  "openai",
  "anthropic",
  "cursor",
  "gemini",
];

const providerTypesAll: {
  value: ProviderType;
  label: string;
  needsApiKey: boolean;
  needsCli: boolean;
}[] = [
  {
    value: "claude-code",
    label: "Claude Code (CLI)",
    needsApiKey: false,
    needsCli: true,
  },
  {
    value: "codex",
    label: "Codex (CLI)",
    needsApiKey: false,
    needsCli: true,
  },
  { value: "openai", label: "OpenAI", needsApiKey: true, needsCli: false },
  {
    value: "anthropic",
    label: "Anthropic API",
    needsApiKey: true,
    needsCli: false,
  },
  {
    value: "cursor",
    label: "Cursor CLI",
    needsApiKey: false,
    needsCli: true,
  },
  {
    value: "gemini",
    label: "Gemini CLI",
    needsApiKey: false,
    needsCli: true,
  },
  {
    value: "custom",
    label: "Custom Provider",
    needsApiKey: true,
    needsCli: false,
  },
];

const providerTypes = providerTypesAll.filter((t) =>
  IMPLEMENTED_PROVIDER_TYPES.includes(t.value),
);

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
  gemini: [
    { value: "", label: "Padrão do CLI (auto)" },
    { value: "gemini-3.1-pro", label: "Gemini 3.1 Pro" },
    { value: "gemini-3-flash", label: "Gemini 3 Flash" },
    { value: "gemini-3-pro", label: "Gemini 3 Pro" },
    { value: "gemini-2.5-pro", label: "Gemini 2.5 Pro" },
    { value: "gemini-2.5-flash", label: "Gemini 2.5 Flash" },
    { value: "gemini-2.0-flash", label: "Gemini 2.0 Flash" },
  ],
};

const CLI_PROVIDER_TYPES: ProviderType[] = [
  "claude-code",
  "codex",
  "cursor",
  "gemini",
];

export function AddProviderDialog({
  open,
  onOpenChange,
}: AddProviderDialogProps) {
  const { create } = useProviders();
  const { detectCliForProvider, validateCliPath } = useDesktopShell();
  const [cliStatus, setCliStatus] = useState<{
    valid: boolean;
    message?: string;
  } | null>(null);

  const [isLoading, setIsLoading] = useState(false);
  const [formData, setFormData] = useState({
    name: "",
    type: "" as ProviderType | "",
    apiKey: "",
    cliPath: "",
    model: "",
    permissionMode: "" as PermissionMode | "",
  });

  const selectedType = providerTypes.find((t) => t.value === formData.type);

  useEffect(() => {
    if (
      !formData.cliPath.trim() ||
      !formData.type ||
      !CLI_PROVIDER_TYPES.includes(formData.type as ProviderType)
    ) {
      setCliStatus(null);
      return;
    }
    let cancelled = false;
    validateCliPath(formData.type, formData.cliPath).then((result) => {
      if (!cancelled) setCliStatus(result);
    });
    return () => {
      cancelled = true;
    };
  }, [formData.type, formData.cliPath, validateCliPath]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!formData.name.trim() || !formData.type) {
      toast.error("Preencha nome e tipo");
      return;
    }

    if (selectedType?.needsApiKey && !formData.apiKey.trim()) {
      toast.error("Chave de API é obrigatória para este provedor");
      return;
    }

    if (selectedType?.needsCli && !formData.cliPath.trim()) {
      toast.error("Caminho do CLI é obrigatório para este provedor");
      return;
    }

    setIsLoading(true);

    try {
      await new Promise((resolve) => setTimeout(resolve, 300));

      const config: Record<string, unknown> = {};
      if (formData.model) config.model = formData.model;
      if (
        PROVIDER_TYPES_WITH_PERMISSION_MODES.includes(
          formData.type as ProviderType,
        ) &&
        formData.permissionMode
      ) {
        config.permissionMode = formData.permissionMode;
      }
      await create({
        name: formData.name.trim(),
        type: formData.type as ProviderType,
        apiKey: formData.apiKey.trim() || undefined,
        cliPath: formData.cliPath.trim() || undefined,
        config: Object.keys(config).length ? config : undefined,
        isActive: true,
      });

      toast.success("Provedor adicionado com sucesso");
      onOpenChange(false);

      setFormData({
        name: "",
        type: "",
        apiKey: "",
        cliPath: "",
        model: "",
        permissionMode: "",
      });
    } catch {
      toast.error("Falha ao adicionar provedor");
    } finally {
      setIsLoading(false);
    }
  };

  const getDefaultModel = (type: ProviderType) => {
    switch (type) {
      case "claude-code":
        return "claude-sonnet-4-5-20250929";
      case "codex":
        return "";
      case "openai":
        return "gpt-4.1";
      case "anthropic":
        return "claude-sonnet-4-5-20250929";
      case "cursor":
        return "";
      default:
        return "";
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[500px]">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Bot className="h-5 w-5 text-primary" />
              Adicionar provedor de IA
            </DialogTitle>
            <DialogDescription>
              Configure um novo provedor de agente de código com IA (traga sua
              própria chave).
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="type">Tipo de provedor</Label>
              <Select
                value={formData.type}
                onValueChange={(value: ProviderType) => {
                  setFormData((prev) => ({
                    ...prev,
                    type: value,
                    model: getDefaultModel(value),
                  }));
                  if (CLI_PROVIDER_TYPES.includes(value)) {
                    detectCliForProvider(value).then((path) => {
                      if (path)
                        setFormData((prev) =>
                          prev.cliPath ? prev : { ...prev, cliPath: path },
                        );
                    });
                  }
                }}
              >
                <SelectTrigger id="type">
                  <SelectValue placeholder="Selecione o tipo de provedor" />
                </SelectTrigger>
                <SelectContent>
                  {providerTypes.map((type) => (
                    <SelectItem key={type.value} value={type.value}>
                      {type.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

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

            {selectedType?.needsApiKey && (
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
                  Sua chave de API é armazenada localmente e nunca enviada a
                  servidores externos.
                </p>
              </div>
            )}

            {selectedType?.needsCli && (
              <div className="grid gap-2">
                <div className="flex items-center justify-between gap-2">
                  <Label htmlFor="cliPath" className="flex items-center gap-2">
                    <Terminal className="h-4 w-4" />
                    Caminho do CLI
                  </Label>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="h-8 text-xs"
                    onClick={() => {
                      detectCliForProvider(formData.type).then((path) => {
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
                </div>
                <Input
                  id="cliPath"
                  placeholder={
                    formData.type === "codex"
                      ? "/usr/local/bin/codex"
                      : formData.type === "cursor"
                        ? "agent"
                        : "/usr/local/bin/claude"
                  }
                  value={formData.cliPath}
                  onChange={(e) =>
                    setFormData((prev) => ({
                      ...prev,
                      cliPath: e.target.value,
                    }))
                  }
                />
                <p className="text-xs text-muted-foreground">
                  Caminho do executável do CLI no seu sistema.
                </p>
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

            {formData.type &&
              PROVIDER_TYPES_WITH_PERMISSION_MODES.includes(
                formData.type as ProviderType,
              ) && (
                <div className="grid gap-2">
                  <Label htmlFor="permissionMode">Modo de permissão</Label>
                  <Select
                    value={formData.permissionMode || "__default__"}
                    onValueChange={(value) =>
                      setFormData((prev) => ({
                        ...prev,
                        permissionMode:
                          value === "__default__"
                            ? ""
                            : (value as PermissionMode),
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
                    Controla se o agente só planeja, aceita edições
                    automaticamente ou bypass de aprovações.
                  </p>
                </div>
              )}

            {formData.type && (
              <div className="grid gap-2">
                <Label htmlFor="model">Modelo padrão</Label>
                {modelsByProviderType[formData.type] ? (
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
                      {modelsByProviderType[formData.type]!.map((opt) => (
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
                      setFormData((prev) => ({
                        ...prev,
                        model: e.target.value,
                      }))
                    }
                  />
                )}
              </div>
            )}
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
              Adicionar provedor
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
