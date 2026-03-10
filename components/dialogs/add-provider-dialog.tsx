import React from "react";

import { useState } from "react";
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
import { useElectron } from "@/hooks/use-electron";
import { toast } from "sonner";
import type { ProviderType } from "@/lib/database/types";

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

export function AddProviderDialog({
  open,
  onOpenChange,
}: AddProviderDialogProps) {
  const { create } = useProviders();
  const { resolveCliPath } = useElectron();

  const [isLoading, setIsLoading] = useState(false);
  const [formData, setFormData] = useState({
    name: "",
    type: "" as ProviderType | "",
    apiKey: "",
    cliPath: "",
    model: "",
  });

  const selectedType = providerTypes.find((t) => t.value === formData.type);

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

      await create({
        name: formData.name.trim(),
        type: formData.type as ProviderType,
        apiKey: formData.apiKey.trim() || undefined,
        cliPath: formData.cliPath.trim() || undefined,
        config: formData.model ? { model: formData.model } : undefined,
        isActive: true,
      });

      toast.success("Provedor adicionado com sucesso");
      onOpenChange(false);

      // Reset form
      setFormData({
        name: "",
        type: "",
        apiKey: "",
        cliPath: "",
        model: "",
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
                  const needsCli = providerTypes.find(
                    (t) => t.value === value,
                  )?.needsCli;
                  const cmd =
                    needsCli &&
                    providerTypeToCliCommand[
                      value as keyof typeof providerTypeToCliCommand
                    ];
                  setFormData((prev) => ({
                    ...prev,
                    type: value,
                    model: getDefaultModel(value),
                  }));
                  if (cmd) {
                    resolveCliPath(cmd).then((path) => {
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
                      const cmd =
                        providerTypeToCliCommand[
                          formData.type as keyof typeof providerTypeToCliCommand
                        ];
                      if (cmd)
                        resolveCliPath(cmd).then((path) => {
                          if (path)
                            setFormData((prev) => ({
                              ...prev,
                              cliPath: path,
                            }));
                          else toast.error("CLI não encontrado no PATH");
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
