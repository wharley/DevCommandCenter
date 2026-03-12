import React, { useRef } from "react";
import { useState, useEffect } from "react";
import {
  Bot,
  Plus,
  Key,
  Terminal,
  MoreHorizontal,
  Trash2,
  Edit,
  Check,
  AlertCircle,
  RefreshCw,
  Loader2,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Switch } from "@/components/ui/switch";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Empty } from "@/components/ui/empty";
import { Separator } from "@/components/ui/separator";
import { AddProviderDialog } from "@/components/dialogs/add-provider-dialog";
import { EditProviderDialog } from "@/components/dialogs/edit-provider-dialog";
import { useProviders } from "@/hooks/use-data";
import type { Provider, ProviderType } from "@/lib/database/types";
import { formatDistanceToNow } from "date-fns";
import { ptBR } from "date-fns/locale";

const providerTypeConfig: Record<
  ProviderType,
  { label: string; icon: React.ElementType; description: string }
> = {
  "claude-code": {
    label: "Claude Code",
    icon: Terminal,
    description: "Claude da Anthropic via CLI",
  },
  codex: {
    label: "Codex",
    icon: Terminal,
    description: "Codex via CLI",
  },
  openai: {
    label: "OpenAI",
    icon: Bot,
    description: "Modelos GPT via API",
  },
  anthropic: {
    label: "Anthropic API",
    icon: Bot,
    description: "Claude via API direta",
  },
  cursor: {
    label: "Cursor CLI",
    icon: Terminal,
    description: "Cursor Agent CLI (terminal) — não é o editor Cursor",
  },
  gemini: {
    label: "Gemini",
    icon: Bot,
    description: "Google Gemini via API",
  },
  custom: {
    label: "Personalizado",
    icon: Bot,
    description: "Configuração de provedor personalizado",
  },
};

/** Rótulo do modelo quando o usuário deixou "padrão" (valor vazio) no diálogo */
const defaultModelLabelByType: Partial<Record<ProviderType, string>> = {
  codex: "Padrão do Codex",
  cursor: "Padrão (auto)",
};

export default function SettingsPage() {
  const [addDialogOpen, setAddDialogOpen] = useState(false);
  const [editingProvider, setEditingProvider] = useState<Provider | null>(null);

  const { providers, update, remove } = useProviders();
  const [encryptionAvailable, setEncryptionAvailable] = useState<boolean | null>(null);

  const [appVersion, setAppVersion] = useState<string>("—");
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const gotUpdateEventRef = useRef(false);
  const hasAppUpdateAPI =
    typeof window !== "undefined" && !!window.electronAPI?.app;

  React.useEffect(() => {
    window.db?.providers?.isEncryptionAvailable?.().then(setEncryptionAvailable);
  }, []);

  useEffect(() => {
    if (!hasAppUpdateAPI) return;
    window.electronAPI!.app.getVersion().then(setAppVersion);
  }, [hasAppUpdateAPI]);

  useEffect(() => {
    if (!hasAppUpdateAPI || !window.electronAPI?.app?.onUpdateStatus) return;
    const unsubscribe = window.electronAPI.app.onUpdateStatus((payload) => {
      gotUpdateEventRef.current = true;
      setCheckingUpdate(false);
      switch (payload.type) {
        case "available":
          toast.info(
            `Nova versão ${payload.version ?? ""} disponível. Baixando...`,
            { duration: 4000 }
          );
          break;
        case "not-available":
          toast.success("Você está na versão mais recente.");
          break;
        case "downloaded":
          toast.success("Atualização baixada.", {
            duration: 10000,
            action: {
              label: "Reiniciar agora",
              onClick: () => window.electronAPI?.app?.quitAndInstall(),
            },
          });
          break;
        case "error":
          toast.error(
            payload.message ?? "Erro ao verificar atualização."
          );
          break;
      }
    });
    return () => unsubscribe();
  }, [hasAppUpdateAPI]);

  const handleCheckForUpdates = async () => {
    if (!hasAppUpdateAPI || !window.electronAPI?.app) return;
    gotUpdateEventRef.current = false;
    setCheckingUpdate(true);
    try {
      await window.electronAPI.app.checkForUpdates();
      setTimeout(() => {
        setCheckingUpdate((prev) => {
          if (!prev) return prev;
          if (!gotUpdateEventRef.current) {
            toast.success("Você está na versão mais recente.");
          }
          return false;
        });
      }, 3000);
    } catch {
      setCheckingUpdate(false);
      toast.error("Falha ao verificar atualização.");
    }
  };

  const handleToggleActive = (provider: Provider) => {
    update(provider.id, { isActive: !provider.isActive });
  };

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <header className="border-b border-border bg-card px-6 py-4">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-xl font-semibold text-card-foreground">
              Configurações
            </h1>
            <p className="text-sm text-muted-foreground">
              Configure seus provedores de IA e preferências do app
            </p>
          </div>
        </div>
      </header>

      {/* Content */}
      <div className="flex-1 overflow-auto p-6">
        <div className="w-full">
          {/* Providers Section */}
          <section>
            {encryptionAvailable === false && (
              <Alert variant="destructive" className="mb-4">
                <AlertCircle className="h-4 w-4" />
                <AlertTitle>Criptografia indisponível</AlertTitle>
                <AlertDescription>
                  Neste ambiente, as chaves de API serão armazenadas em texto plano.
                  Recomenda-se não usar em máquinas compartilhadas.
                </AlertDescription>
              </Alert>
            )}

            <div className="flex items-center justify-between mb-4">
              <div>
                <h2 className="text-lg font-semibold">Provedores de IA</h2>
                <p className="text-sm text-muted-foreground">
                  Gerencie seus agentes de código e chaves de API
                </p>
              </div>
              <Button
                onClick={() => setAddDialogOpen(true)}
                className="shrink-0"
              >
                <Plus className="mr-2 h-4 w-4" />
                Adicionar provedor
              </Button>
            </div>

            {providers.length === 0 ? (
              <Empty>
                <Empty.Icon>
                  <Bot className="h-10 w-10" />
                </Empty.Icon>
                <Empty.Title>Nenhum provedor configurado</Empty.Title>
                <Empty.Description>
                  Adicione seu primeiro provedor de IA para criar missões de
                  código.
                </Empty.Description>
                <Empty.Actions>
                  <Button onClick={() => setAddDialogOpen(true)}>
                    <Plus className="mr-2 h-4 w-4" />
                    Adicionar provedor
                  </Button>
                </Empty.Actions>
              </Empty>
            ) : (
              <div className="space-y-4">
                {providers.map((provider) => {
                  const config =
                    providerTypeConfig[provider.type] ??
                    providerTypeConfig.custom;
                  const Icon = config.icon;

                  return (
                    <Card key={provider.id}>
                      <CardHeader className="pb-3">
                        <div className="flex items-start justify-between">
                          <div className="flex items-start gap-4">
                            <div
                              className={`flex h-10 w-10 items-center justify-center rounded-lg ${
                                provider.isActive ? "bg-primary/10" : "bg-muted"
                              }`}
                            >
                              <Icon
                                className={`h-5 w-5 ${
                                  provider.isActive
                                    ? "text-primary"
                                    : "text-muted-foreground"
                                }`}
                              />
                            </div>
                            <div>
                              <CardTitle className="text-base flex items-center gap-2">
                                {provider.name}
                                {!provider.isActive && (
                                  <Badge
                                    variant="outline"
                                    className="text-muted-foreground"
                                  >
                                    Desativado
                                  </Badge>
                                )}
                              </CardTitle>
                              <CardDescription>
                                {config.label} - {config.description}
                              </CardDescription>
                            </div>
                          </div>

                          <div className="flex items-center gap-2">
                            <Switch
                              checked={provider.isActive}
                              onCheckedChange={() =>
                                handleToggleActive(provider)
                              }
                            />
                            <DropdownMenu>
                              <DropdownMenuTrigger asChild>
                                <Button
                                  variant="ghost"
                                  size="icon"
                                  className="h-8 w-8"
                                >
                                  <MoreHorizontal className="h-4 w-4" />
                                </Button>
                              </DropdownMenuTrigger>
                              <DropdownMenuContent align="end">
                                <DropdownMenuItem
                                  onClick={() => setEditingProvider(provider)}
                                >
                                  <Edit className="mr-2 h-4 w-4" />
                                  Editar
                                </DropdownMenuItem>
                                <DropdownMenuSeparator />
                                <DropdownMenuItem
                                  className="text-destructive focus:text-destructive"
                                  onClick={() => remove(provider.id)}
                                >
                                  <Trash2 className="mr-2 h-4 w-4" />
                                  Excluir
                                </DropdownMenuItem>
                              </DropdownMenuContent>
                            </DropdownMenu>
                          </div>
                        </div>
                      </CardHeader>
                      <CardContent>
                        <div className="flex flex-wrap gap-4 text-sm">
                          {(provider.hasApiKey ?? provider.apiKey) && (
                            <div className="flex items-center gap-2 text-muted-foreground">
                              <Key className="h-4 w-4" />
                              <span>Chave de API configurada</span>
                              <Check className="h-4 w-4 text-green-500" />
                            </div>
                          )}
                          {provider.cliPath && (
                            <div className="flex items-center gap-2 text-muted-foreground">
                              <Terminal className="h-4 w-4" />
                              <code className="text-xs bg-muted px-1.5 py-0.5 rounded">
                                {provider.cliPath}
                              </code>
                            </div>
                          )}
                          {(provider.config?.model ||
                            defaultModelLabelByType[provider.type]) && (
                            <div className="flex items-center gap-2 text-muted-foreground">
                              <Bot className="h-4 w-4" />
                              <span>
                                Modelo:{" "}
                                {provider.config?.model ||
                                  defaultModelLabelByType[provider.type]}
                              </span>
                            </div>
                          )}
                          {!provider.hasApiKey && !provider.apiKey && !provider.cliPath && (
                            <div className="flex items-center gap-2 text-amber-500">
                              <AlertCircle className="h-4 w-4" />
                              <span>Não totalmente configurado</span>
                            </div>
                          )}
                        </div>
                        <Separator className="my-3" />
                        <p className="text-xs text-muted-foreground">
                          Adicionado{" "}
                          {formatDistanceToNow(provider.createdAt, {
                            addSuffix: true,
                            locale: ptBR,
                          })}
                        </p>
                      </CardContent>
                    </Card>
                  );
                })}
              </div>
            )}
          </section>

          {/* Atualizações */}
          {hasAppUpdateAPI && (
            <section className="mt-8">
              <Card>
                <CardHeader className="pb-3">
                  <CardTitle className="text-base">Atualizações</CardTitle>
                  <CardDescription>
                    Versão instalada: {appVersion}. Verifique se há uma nova
                    versão disponível.
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleCheckForUpdates}
                    disabled={checkingUpdate}
                  >
                    {checkingUpdate ? (
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    ) : (
                      <RefreshCw className="mr-2 h-4 w-4" />
                    )}
                    Verificar atualizações
                  </Button>
                </CardContent>
              </Card>
            </section>
          )}
        </div>
      </div>

      {/* Footer - Sobre */}
      <footer className="border-t border-border bg-card px-6 py-4 mt-auto">
        <div className="max-w-3xl flex items-center gap-4">
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary">
            <Terminal className="h-5 w-5 text-primary-foreground" />
          </div>
          <div className="min-w-0">
            <h3 className="font-semibold text-sm">Dev Command Center</h3>
            <p className="text-xs text-muted-foreground truncate">
              Versão {appVersion} — Seu hub para agentes de código com IA.
              Conecte vários provedores como Claude Code e OpenAI para criar e
              gerenciar missões de código com facilidade.
            </p>
          </div>
        </div>
      </footer>

      {/* Dialogs */}
      <AddProviderDialog open={addDialogOpen} onOpenChange={setAddDialogOpen} />

      {editingProvider && (
        <EditProviderDialog
          open={!!editingProvider}
          onOpenChange={(open) => !open && setEditingProvider(null)}
          provider={editingProvider}
        />
      )}
    </div>
  );
}
