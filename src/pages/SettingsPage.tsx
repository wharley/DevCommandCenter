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
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Empty } from "@/components/ui/empty";
import { AddProviderDialog } from "@/components/dialogs/add-provider-dialog";
import { EditProviderDialog } from "@/components/dialogs/edit-provider-dialog";
import { useProviders } from "@/hooks/use-data";
import type { Provider, ProviderType } from "@/lib/database/types";
import {
  areNotificationsEnabled,
  ensureOsNotificationPermission,
  isTauriRuntime,
  setNotificationsEnabled,
} from "@/lib/notifications";
import { formatDistanceToNow } from "date-fns";
import { ptBR } from "date-fns/locale";
import {
  loadTerminalAppearance,
  saveTerminalAppearance,
  type TerminalAppearancePreferences,
} from "@/lib/terminal/terminal-preferences";

const TERMINAL_FONT_PRESETS: { label: string; value: string }[] = [
  { label: "Geist / sistema (padrão)", value: "var(--font-geist-mono, 'Menlo', 'Monaco', monospace)" },
  { label: "Menlo / Monaco", value: "Menlo, Monaco, 'Courier New', monospace" },
  { label: "JetBrains Mono", value: "'JetBrains Mono', Menlo, monospace" },
  { label: "Fira Code", value: "'Fira Code', Menlo, monospace" },
  { label: "SF Mono", value: "'SF Mono', Menlo, monospace" },
];

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
  const [notificationsEnabled, setNotificationsEnabledState] = useState(
    () => areNotificationsEnabled()
  );

  const { providers, update, remove } = useProviders();
  const [encryptionAvailable, setEncryptionAvailable] = useState<boolean | null>(null);

  const [appVersion, setAppVersion] = useState<string>("—");
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [terminalAppearance, setTerminalAppearance] = useState<TerminalAppearancePreferences>(() =>
    loadTerminalAppearance(),
  );
  const gotUpdateEventRef = useRef(false);
  const hasAppUpdateAPI =
    typeof window !== "undefined" && !!window.desktopAPI?.app;

  React.useEffect(() => {
    window.db?.providers?.isEncryptionAvailable?.().then(setEncryptionAvailable);
  }, []);

  useEffect(() => {
    if (!hasAppUpdateAPI) return;
    window.desktopAPI!.app.getVersion().then(setAppVersion);
  }, [hasAppUpdateAPI]);

  useEffect(() => {
    if (!hasAppUpdateAPI || !window.desktopAPI?.app?.onUpdateStatus) return;
    const unsubscribe = window.desktopAPI.app.onUpdateStatus((payload) => {
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
              onClick: () => window.desktopAPI?.app?.quitAndInstall(),
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
    if (!hasAppUpdateAPI || !window.desktopAPI?.app) return;
    gotUpdateEventRef.current = false;
    setCheckingUpdate(true);
    try {
      await window.desktopAPI.app.checkForUpdates();
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
    <div className="flex h-full min-h-0 flex-col bg-background">
      {/* Header */}
      <header className="shrink-0 border-b border-border bg-card/80 px-6 py-3 backdrop-blur supports-backdrop-filter:bg-card/60">
        <div className="mx-auto flex max-w-3xl items-center justify-between gap-4">
          <div>
            <h1 className="text-lg font-semibold tracking-tight text-foreground">Providers</h1>
            <p className="text-xs text-muted-foreground">
              Configuração dos agentes e notificações essenciais
            </p>
          </div>
        </div>
      </header>

      {/* Content */}
      <div className="flex-1 overflow-auto px-6 py-5">
        <div className="mx-auto max-w-3xl space-y-6">
          {/* Providers Section */}
          <section>
            {encryptionAvailable === false && (
              <Alert variant="destructive" className="mb-4 py-3">
                <AlertCircle className="h-4 w-4" />
                <AlertTitle>Criptografia indisponível</AlertTitle>
                <AlertDescription className="text-xs">
                  Neste ambiente, as chaves de API serão armazenadas em texto plano.
                  Recomenda-se não usar em máquinas compartilhadas.
                </AlertDescription>
              </Alert>
            )}

            <div className="mb-3 flex flex-wrap items-end justify-between gap-3">
              <div>
                <h2 className="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
                  Provedores de IA
                </h2>
                <p className="mt-0.5 text-xs text-muted-foreground">
                  Agentes de código e chaves de API
                </p>
              </div>
              <Button
                size="sm"
                onClick={() => setAddDialogOpen(true)}
                className="shrink-0"
              >
                <Plus className="mr-1.5 h-3.5 w-3.5" />
                Adicionar
              </Button>
            </div>

            {providers.length === 0 ? (
              <div className="overflow-hidden rounded-xl border border-dashed border-border bg-muted/20 px-6 py-10">
                <Empty>
                  <Empty.Icon>
                    <Bot className="h-8 w-8 opacity-80" />
                  </Empty.Icon>
                  <Empty.Title className="text-base">
                    Nenhum provedor configurado
                  </Empty.Title>
                  <Empty.Description className="text-sm">
                    Adicione seu primeiro provedor de IA para criar missões de
                    código.
                  </Empty.Description>
                  <Empty.Actions>
                    <Button size="sm" onClick={() => setAddDialogOpen(true)}>
                      <Plus className="mr-1.5 h-3.5 w-3.5" />
                      Adicionar provedor
                    </Button>
                  </Empty.Actions>
                </Empty>
              </div>
            ) : (
              <div className="overflow-hidden rounded-xl border border-border bg-card shadow-sm">
                {providers.map((provider, index) => {
                  const config =
                    providerTypeConfig[provider.type] ??
                    providerTypeConfig.custom;
                  const Icon = config.icon;

                  return (
                    <div
                      key={provider.id}
                      className={`border-border px-3 py-2.5 transition-colors hover:bg-muted/40 sm:px-4 ${
                        index > 0 ? "border-t" : ""
                      }`}
                    >
                      <div className="flex items-start gap-3">
                        <div
                          className={`mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-md ${
                            provider.isActive ? "bg-primary/10" : "bg-muted"
                          }`}
                        >
                          <Icon
                            className={`h-4 w-4 ${
                              provider.isActive
                                ? "text-primary"
                                : "text-muted-foreground"
                            }`}
                          />
                        </div>
                        <div className="min-w-0 flex-1 space-y-1.5">
                          <div className="flex flex-wrap items-center gap-2">
                            <span className="text-sm font-medium leading-none">
                              {provider.name}
                            </span>
                            {!provider.isActive && (
                              <Badge
                                variant="outline"
                                className="h-5 px-1.5 text-[10px] text-muted-foreground"
                              >
                                Desativado
                              </Badge>
                            )}
                            <span className="text-[11px] text-muted-foreground">
                              {config.label}
                            </span>
                          </div>
                          <p className="line-clamp-1 text-[11px] text-muted-foreground/90">
                            {config.description}
                          </p>
                          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
                            {(provider.hasApiKey ?? provider.apiKey) && (
                              <span className="inline-flex items-center gap-1">
                                <Key className="h-3 w-3" />
                                API ok
                                <Check className="h-3 w-3 text-green-600 dark:text-green-500" />
                              </span>
                            )}
                            {provider.cliPath && (
                              <span className="inline-flex min-w-0 max-w-full items-center gap-1">
                                <Terminal className="h-3 w-3 shrink-0" />
                                <code className="truncate rounded bg-muted px-1 py-0.5 font-mono text-[10px]">
                                  {provider.cliPath}
                                </code>
                              </span>
                            )}
                            {(provider.config?.model ||
                              defaultModelLabelByType[provider.type]) && (
                              <span className="inline-flex items-center gap-1">
                                <Bot className="h-3 w-3" />
                                {provider.config?.model ||
                                  defaultModelLabelByType[provider.type]}
                              </span>
                            )}
                            {!provider.hasApiKey &&
                              !provider.apiKey &&
                              !provider.cliPath && (
                                <span className="inline-flex items-center gap-1 text-amber-600 dark:text-amber-500">
                                  <AlertCircle className="h-3 w-3" />
                                  Incompleto
                                </span>
                              )}
                          </div>
                          <p className="text-[10px] text-muted-foreground/70">
                            Adicionado{" "}
                            {formatDistanceToNow(provider.createdAt, {
                              addSuffix: true,
                              locale: ptBR,
                            })}
                          </p>
                        </div>
                        <div className="flex shrink-0 items-center gap-0.5">
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
                    </div>
                  );
                })}
              </div>
            )}
          </section>

          {/* Notificações + Atualizações — painel único estilo lista */}
          <section className="overflow-hidden rounded-xl border border-border bg-card shadow-sm">
            <div
              className={`flex items-center justify-between gap-4 px-4 py-3 ${
                hasAppUpdateAPI ? "border-b border-border" : ""
              }`}
            >
              <div className="min-w-0">
                <p className="text-sm font-medium">Notificações nativas</p>
                <p className="text-[11px] text-muted-foreground">
                  Avisos do sistema quando um agente ou terminal precisa de atenção (outro workspace, janela em
                  segundo plano, etc.). O toast dentro do app continua sempre.
                </p>
              </div>
              <Switch
                checked={notificationsEnabled}
                onCheckedChange={async (checked) => {
                  setNotificationsEnabled(checked);
                  setNotificationsEnabledState(checked);
                  if (checked && isTauriRuntime()) {
                    const granted = await ensureOsNotificationPermission();
                    if (!granted) {
                      toast.info(
                        "Permita notificações para o Dev Command Center nas definições do sistema, se quiser banners fora da janela.",
                        { duration: 7000 },
                      );
                    }
                  }
                }}
              />
            </div>
            {hasAppUpdateAPI && (
              <div className="flex flex-col gap-2 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
                <div className="min-w-0">
                  <p className="text-sm font-medium">Atualizações</p>
                  <p className="text-[11px] text-muted-foreground">
                    Versão {appVersion}
                  </p>
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  className="shrink-0"
                  onClick={handleCheckForUpdates}
                  disabled={checkingUpdate}
                >
                  {checkingUpdate ? (
                    <Loader2 className="mr-2 h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <RefreshCw className="mr-2 h-3.5 w-3.5" />
                  )}
                  Verificar
                </Button>
              </div>
            )}
          </section>

          <section className="overflow-hidden rounded-xl border border-border bg-card shadow-sm">
            <div className="border-b border-border px-4 py-3">
              <p className="text-sm font-medium">Terminal embutido</p>
              <p className="text-[11px] text-muted-foreground">
                Aparência do xterm no workspace.                 Atalhos: Cmd+1–9 foco no pane, Cmd+K limpar scrollback,
                Cmd+Plus/Cmd+Minus zoom, Cmd+0 reset do zoom (Ctrl no Windows/Linux).
              </p>
            </div>
            <div className="space-y-4 px-4 py-3">
              <div className="grid gap-2 sm:grid-cols-[140px_1fr] sm:items-center">
                <Label htmlFor="term-font-size">Tamanho da fonte</Label>
                <Input
                  id="term-font-size"
                  type="number"
                  min={8}
                  max={32}
                  value={terminalAppearance.fontSize}
                  onChange={(e) => {
                    const v = Number(e.target.value);
                    if (Number.isNaN(v)) return;
                    const fontSize = Math.min(32, Math.max(8, v));
                    const next = { ...terminalAppearance, fontSize };
                    setTerminalAppearance(next);
                    saveTerminalAppearance(next);
                  }}
                  className="max-w-[120px]"
                />
              </div>
              <div className="grid gap-2 sm:grid-cols-[140px_1fr] sm:items-center">
                <Label htmlFor="term-font-family">Fonte</Label>
                <Select
                  value={terminalAppearance.fontFamily}
                  onValueChange={(value) => {
                    const next = { ...terminalAppearance, fontFamily: value };
                    setTerminalAppearance(next);
                    saveTerminalAppearance(next);
                  }}
                >
                  <SelectTrigger id="term-font-family" className="max-w-md">
                    <SelectValue placeholder="Fonte" />
                  </SelectTrigger>
                  <SelectContent>
                    {TERMINAL_FONT_PRESETS.every((p) => p.value !== terminalAppearance.fontFamily) ? (
                      <SelectItem value={terminalAppearance.fontFamily}>Valor guardado</SelectItem>
                    ) : null}
                    {TERMINAL_FONT_PRESETS.map((p) => (
                      <SelectItem key={p.value} value={p.value}>
                        {p.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <p className="text-sm font-medium">Cores do tema do app</p>
                  <p className="text-[11px] text-muted-foreground">
                    Quando desligado, usa paleta escura fixa no terminal.
                  </p>
                </div>
                <Switch
                  checked={terminalAppearance.useAppThemeColors}
                  onCheckedChange={(checked) => {
                    const next = { ...terminalAppearance, useAppThemeColors: checked };
                    setTerminalAppearance(next);
                    saveTerminalAppearance(next);
                  }}
                />
              </div>
              <p className="text-[11px] text-muted-foreground">
                Métricas de throughput (dev):{" "}
                <code className="rounded bg-muted px-1">localStorage.setItem(&apos;dcc.debugTerminalMetrics&apos;, &apos;1&apos;)</code>
              </p>
            </div>
          </section>
        </div>
      </div>

      {/* Footer - Sobre */}
      <footer className="mt-auto shrink-0 border-t border-border bg-card/50 px-6 py-3">
        <div className="mx-auto flex max-w-3xl items-center gap-3">
          <img src="/dcc-mark.svg" alt="DCC" className="h-8 w-8 shrink-0 rounded-lg" />
          <div className="min-w-0">
            <h3 className="text-xs font-semibold">Dev Command Center</h3>
            <p className="line-clamp-2 text-[11px] text-muted-foreground">
              Versao {appVersion} - Hub para agentes de codigo e terminal.
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
