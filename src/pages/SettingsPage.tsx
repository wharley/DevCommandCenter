import React from "react";
import { useState } from "react";
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
} from "lucide-react";
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
  google: {
    label: "Google AI",
    icon: Bot,
    description: "Modelos Gemini via API",
  },
  cursor: {
    label: "Cursor CLI",
    icon: Terminal,
    description: "Cursor Agent CLI (terminal) — não é o editor Cursor",
  },
  vscode: {
    label: "VS Code",
    icon: Terminal,
    description: "Integração VS Code Copilot",
  },
  custom: {
    label: "Personalizado",
    icon: Bot,
    description: "Configuração de provedor personalizado",
  },
};

export default function SettingsPage() {
  const [addDialogOpen, setAddDialogOpen] = useState(false);
  const [editingProvider, setEditingProvider] = useState<Provider | null>(null);

  const { providers, update, remove } = useProviders();

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
                  const config = providerTypeConfig[provider.type];
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
                          {provider.apiKey && (
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
                          {provider.config?.model && (
                            <div className="flex items-center gap-2 text-muted-foreground">
                              <Bot className="h-4 w-4" />
                              <span>Modelo: {provider.config.model}</span>
                            </div>
                          )}
                          {!provider.apiKey && !provider.cliPath && (
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
              Versão 0.1.0 — Seu hub para agentes de código com IA. Conecte
              vários provedores como Claude Code e OpenAI para criar e gerenciar
              missões de código com facilidade.
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
