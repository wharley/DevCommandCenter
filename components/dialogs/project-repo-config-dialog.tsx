"use client";

import { useEffect, useMemo, useState } from "react";
import { Clock3, Plus, Trash2, WandSparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { Switch } from "@/components/ui/switch";
import type {
  Project,
  ProjectRepoConfig,
  RepoCommandPreset,
  RepoProcessDefinition,
  RepoTaskDefinition,
  RepoTaskTriggerDefinition,
  Provider,
} from "@/lib/database/types";

type LocalProcess = RepoProcessDefinition;
type LocalPreset = RepoCommandPreset;
type LocalTask = RepoTaskDefinition;
type LocalTaskTrigger = RepoTaskTriggerDefinition;

function makeId(prefix: string) {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function createProcess(): LocalProcess {
  return {
    id: makeId("process"),
    name: "Servidor",
    command: "npm run dev",
    description: "",
    cwdMode: "worktree",
    autoRestart: false,
  };
}

function createPreset(): LocalPreset {
  return {
    id: makeId("preset"),
    name: "Lint",
    command: "npm run lint",
    description: "",
  };
}

function createTaskTrigger(): LocalTaskTrigger {
  return {
    when: "complete",
    prompt: "",
    providerId: null,
  };
}

function createTask(): LocalTask {
  return {
    id: makeId("task"),
    name: "Snapshot",
    command: "npm test",
    schedule: "0 */2 * * * *",
    description: "",
    cwdMode: "worktree",
    enabled: true,
    trigger: createTaskTrigger(),
  };
}

function normalizeConfig(config?: ProjectRepoConfig | null): ProjectRepoConfig {
  return {
    branchPrefix: config?.branchPrefix ?? "dcc-comb",
    defaultAgentProviderId: config?.defaultAgentProviderId ?? null,
    setupCommand: config?.setupCommand ?? "",
    teardownCommand: config?.teardownCommand ?? "",
    processes: (config?.processes ?? []).map((item) => ({
      ...item,
      cwdMode: item.cwdMode ?? "worktree",
      autoRestart: item.autoRestart ?? false,
      description: item.description ?? "",
    })),
    presets: (config?.presets ?? []).map((item) => ({
      ...item,
      description: item.description ?? "",
    })),
    tasks: (config?.tasks ?? []).map((item) => ({
      ...item,
      description: item.description ?? "",
      cwdMode: item.cwdMode ?? "worktree",
      enabled: item.enabled ?? true,
      trigger: item.trigger
        ? {
            when: item.trigger.when ?? "complete",
            prompt: item.trigger.prompt ?? "",
            providerId: item.trigger.providerId ?? null,
          }
        : createTaskTrigger(),
    })),
  };
}

interface ProjectRepoConfigDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  project: Project | null;
  providers: Provider[];
  onSave: (projectId: string, config: ProjectRepoConfig) => Promise<void>;
  onEditToml?: () => void;
}

export function ProjectRepoConfigDialog({
  open,
  onOpenChange,
  project,
  providers,
  onSave,
  onEditToml,
}: ProjectRepoConfigDialogProps) {
  const activeProviders = useMemo(() => providers.filter((provider) => provider.isActive), [providers]);
  const [branchPrefix, setBranchPrefix] = useState("dcc-comb");
  const [defaultAgentProviderId, setDefaultAgentProviderId] = useState<string | null>(null);
  const [setupCommand, setSetupCommand] = useState("");
  const [teardownCommand, setTeardownCommand] = useState("");
  const [processes, setProcesses] = useState<LocalProcess[]>([]);
  const [presets, setPresets] = useState<LocalPreset[]>([]);
  const [tasks, setTasks] = useState<LocalTask[]>([]);
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    if (!open) return;
    const normalized = normalizeConfig(project?.repoConfig ?? null);
    setBranchPrefix(normalized.branchPrefix ?? "dcc-comb");
    setDefaultAgentProviderId(normalized.defaultAgentProviderId ?? null);
    setSetupCommand(normalized.setupCommand ?? "");
    setTeardownCommand(normalized.teardownCommand ?? "");
    setProcesses(normalized.processes?.length ? normalized.processes : [createProcess()]);
    setPresets(normalized.presets?.length ? normalized.presets : [createPreset()]);
    setTasks(normalized.tasks?.length ? normalized.tasks : [createTask()]);
  }, [open, project]);

  const handleSave = async () => {
    if (!project) return;
    setIsSaving(true);
    try {
      const nextConfig: ProjectRepoConfig = {
        branchPrefix: branchPrefix.trim() || "dcc-comb",
        defaultAgentProviderId: defaultAgentProviderId || null,
        setupCommand: setupCommand.trim() || null,
        teardownCommand: teardownCommand.trim() || null,
        processes: processes
          .map((process) => ({
            ...process,
            name: process.name.trim(),
            command: process.command.trim(),
            description: process.description?.trim() || "",
          }))
          .filter((process) => process.name && process.command),
        presets: presets
          .map((preset) => ({
            ...preset,
            name: preset.name.trim(),
            command: preset.command.trim(),
            description: preset.description?.trim() || "",
          }))
          .filter((preset) => preset.name && preset.command),
        tasks: tasks
          .map((task) => ({
            ...task,
            name: task.name.trim(),
            command: task.command.trim(),
            schedule: task.schedule.trim(),
            description: task.description?.trim() || "",
            trigger: task.trigger
              ? {
                  when: task.trigger.when ?? "complete",
                  prompt: task.trigger.prompt?.trim() || "",
                  providerId: task.trigger.providerId ?? null,
                }
              : null,
          }))
          .filter((task) => task.name && task.command && task.schedule),
      };
      await onSave(project.id, nextConfig);
      onOpenChange(false);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-4xl">
        <DialogHeader>
          <DialogTitle>Configuração do repositório</DialogTitle>
          <DialogDescription>
            Define regras de branch, comandos de setup/teardown, processos gerenciados e presets de comando.
            As alterações são salvas em <code>.dcc.toml</code> na raiz do repositório e espelhadas no banco local.
          </DialogDescription>
        </DialogHeader>

        {project ? (
          <div className="space-y-5 py-2">
            <div className="grid gap-4 md:grid-cols-2">
              <div className="space-y-2">
                <Label htmlFor="branch-prefix">Prefixo de branch</Label>
                <Input
                  id="branch-prefix"
                  value={branchPrefix}
                  onChange={(event) => setBranchPrefix(event.target.value)}
                  placeholder="dcc"
                />
                <p className="text-xs text-muted-foreground">
                  Usado para nomes de branch e worktree derivados deste projeto.
                </p>
              </div>
              <div className="space-y-2">
                <Label htmlFor="default-agent">Agente padrão</Label>
                <Select
                  value={defaultAgentProviderId ?? "none"}
                  onValueChange={(value) => setDefaultAgentProviderId(value === "none" ? null : value)}
                >
                  <SelectTrigger id="default-agent">
                    <SelectValue placeholder="Selecione um provedor" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="none">Sem padrão</SelectItem>
                    {activeProviders.map((provider) => (
                      <SelectItem key={provider.id} value={provider.id}>
                        {provider.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div className="grid gap-4 md:grid-cols-2">
              <div className="space-y-2">
                <Label htmlFor="setup-command">Setup do repo</Label>
                <Textarea
                  id="setup-command"
                  value={setupCommand}
                  onChange={(event) => setSetupCommand(event.target.value)}
                  rows={4}
                  placeholder="npm install"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="teardown-command">Teardown do repo</Label>
                <Textarea
                  id="teardown-command"
                  value={teardownCommand}
                  onChange={(event) => setTeardownCommand(event.target.value)}
                  rows={4}
                  placeholder="npm run clean"
                />
              </div>
            </div>

            <section className="space-y-3 rounded-lg border border-border p-3">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <p className="text-sm font-medium">Processos gerenciados</p>
                  <p className="text-xs text-muted-foreground">
                    Cada processo vira um pane executável no workspace.
                  </p>
                </div>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => setProcesses((prev) => [...prev, createProcess()])}
                >
                  <Plus className="mr-1.5 h-3.5 w-3.5" />
                  Adicionar processo
                </Button>
              </div>

              <div className="space-y-3">
                {processes.map((process, index) => (
                  <div key={process.id} className="rounded-md border border-border bg-muted/20 p-3">
                    <div className="grid gap-3 md:grid-cols-[1fr_1.5fr_150px_110px_auto] md:items-end">
                      <div className="space-y-2">
                        <Label>Nome</Label>
                        <Input
                          value={process.name}
                          onChange={(event) =>
                            setProcesses((prev) =>
                              prev.map((item) =>
                                item.id === process.id ? { ...item, name: event.target.value } : item,
                              ),
                            )
                          }
                          placeholder="Servidor"
                        />
                      </div>
                      <div className="space-y-2">
                        <Label>Comando</Label>
                        <Input
                          value={process.command}
                          onChange={(event) =>
                            setProcesses((prev) =>
                              prev.map((item) =>
                                item.id === process.id ? { ...item, command: event.target.value } : item,
                              ),
                            )
                          }
                          placeholder="npm run dev"
                        />
                      </div>
                      <div className="space-y-2">
                        <Label>CWD</Label>
                        <Select
                          value={process.cwdMode ?? "worktree"}
                          onValueChange={(value: "project" | "worktree") =>
                            setProcesses((prev) =>
                              prev.map((item) =>
                                item.id === process.id ? { ...item, cwdMode: value } : item,
                              ),
                            )
                          }
                        >
                          <SelectTrigger>
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="worktree">Worktree</SelectItem>
                            <SelectItem value="project">Projeto</SelectItem>
                          </SelectContent>
                        </Select>
                      </div>
                      <div className="space-y-2">
                        <Label>Auto restart</Label>
                        <div className="flex h-10 items-center">
                          <Switch
                            checked={process.autoRestart ?? false}
                            onCheckedChange={(checked) =>
                              setProcesses((prev) =>
                                prev.map((item) =>
                                  item.id === process.id ? { ...item, autoRestart: checked } : item,
                                ),
                              )
                            }
                          />
                        </div>
                      </div>
                      <div className="flex justify-end">
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          className="h-9 w-9"
                          onClick={() =>
                            setProcesses((prev) => prev.filter((item) => item.id !== process.id))
                          }
                          disabled={processes.length === 1}
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    </div>
                    <div className="mt-3 space-y-2">
                      <Label>Descrição</Label>
                      <Input
                        value={process.description ?? ""}
                        onChange={(event) =>
                          setProcesses((prev) =>
                            prev.map((item) =>
                              item.id === process.id ? { ...item, description: event.target.value } : item,
                            ),
                          )
                        }
                        placeholder="Opcional"
                      />
                    </div>
                    <p className="mt-2 text-[11px] text-muted-foreground">
                      {index + 1} processo configurado.
                    </p>
                  </div>
                ))}
              </div>
            </section>

            <section className="space-y-3 rounded-lg border border-border p-3">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <p className="text-sm font-medium">Presets rápidos</p>
                  <p className="text-xs text-muted-foreground">
                    Atalhos que aparecem na palette global.
                  </p>
                </div>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => setPresets((prev) => [...prev, createPreset()])}
                >
                  <Plus className="mr-1.5 h-3.5 w-3.5" />
                  Adicionar preset
                </Button>
              </div>

              <div className="space-y-3">
                {presets.map((preset, index) => (
                  <div key={preset.id} className="rounded-md border border-border bg-muted/20 p-3">
                    <div className="grid gap-3 md:grid-cols-[1fr_2fr_auto] md:items-end">
                      <div className="space-y-2">
                        <Label>Nome</Label>
                        <Input
                          value={preset.name}
                          onChange={(event) =>
                            setPresets((prev) =>
                              prev.map((item) =>
                                item.id === preset.id ? { ...item, name: event.target.value } : item,
                              ),
                            )
                          }
                          placeholder="Lint"
                        />
                      </div>
                      <div className="space-y-2">
                        <Label>Comando</Label>
                        <Input
                          value={preset.command}
                          onChange={(event) =>
                            setPresets((prev) =>
                              prev.map((item) =>
                                item.id === preset.id ? { ...item, command: event.target.value } : item,
                              ),
                            )
                          }
                          placeholder="npm run lint"
                        />
                      </div>
                      <div className="flex justify-end">
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          className="h-9 w-9"
                          onClick={() =>
                            setPresets((prev) => prev.filter((item) => item.id !== preset.id))
                          }
                          disabled={presets.length === 1}
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    </div>
                    <div className="mt-3 space-y-2">
                      <Label>Descrição</Label>
                      <Input
                        value={preset.description ?? ""}
                        onChange={(event) =>
                          setPresets((prev) =>
                            prev.map((item) =>
                              item.id === preset.id ? { ...item, description: event.target.value } : item,
                            ),
                          )
                        }
                        placeholder="Opcional"
                      />
                    </div>
                    <p className="mt-2 text-[11px] text-muted-foreground">
                      {index + 1} preset configurado.
                    </p>
                  </div>
                ))}
              </div>
            </section>

            <section className="space-y-3 rounded-lg border border-border p-3">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <p className="text-sm font-medium">Tarefas agendadas</p>
                  <p className="text-xs text-muted-foreground">
                    Entradas `[[tasks]]` com cron e um gatilho opcional após a execução.
                  </p>
                </div>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => setTasks((prev) => [...prev, createTask()])}
                >
                  <Plus className="mr-1.5 h-3.5 w-3.5" />
                  Adicionar tarefa
                </Button>
              </div>

              <div className="space-y-3">
                {tasks.map((task, index) => (
                  <div key={task.id} className="rounded-md border border-border bg-muted/20 p-3">
                    <div className="grid gap-3 md:grid-cols-[1fr_1.5fr_1.2fr_130px_auto] md:items-end">
                      <div className="space-y-2">
                        <Label>Nome</Label>
                        <Input
                          value={task.name}
                          onChange={(event) =>
                            setTasks((prev) =>
                              prev.map((item) =>
                                item.id === task.id ? { ...item, name: event.target.value } : item,
                              ),
                            )
                          }
                          placeholder="Snapshot"
                        />
                      </div>
                      <div className="space-y-2">
                        <Label>Comando</Label>
                        <Input
                          value={task.command}
                          onChange={(event) =>
                            setTasks((prev) =>
                              prev.map((item) =>
                                item.id === task.id ? { ...item, command: event.target.value } : item,
                              ),
                            )
                          }
                          placeholder="npm test"
                        />
                      </div>
                      <div className="space-y-2">
                        <Label>Cron</Label>
                        <Input
                          value={task.schedule}
                          onChange={(event) =>
                            setTasks((prev) =>
                              prev.map((item) =>
                                item.id === task.id ? { ...item, schedule: event.target.value } : item,
                              ),
                            )
                          }
                          placeholder="0 */2 * * * *"
                        />
                      </div>
                      <div className="space-y-2">
                        <Label>CWD</Label>
                        <Select
                          value={task.cwdMode ?? "worktree"}
                          onValueChange={(value: "project" | "worktree") =>
                            setTasks((prev) =>
                              prev.map((item) =>
                                item.id === task.id ? { ...item, cwdMode: value } : item,
                              ),
                            )
                          }
                        >
                          <SelectTrigger>
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="worktree">Worktree</SelectItem>
                            <SelectItem value="project">Projeto</SelectItem>
                          </SelectContent>
                        </Select>
                      </div>
                      <div className="flex justify-end">
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          className="h-9 w-9"
                          onClick={() =>
                            setTasks((prev) => prev.filter((item) => item.id !== task.id))
                          }
                          disabled={tasks.length === 1}
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    </div>
                    <div className="mt-3 grid gap-3 md:grid-cols-[1.2fr_1fr]">
                      <div className="space-y-2">
                        <Label>Descrição</Label>
                        <Input
                          value={task.description ?? ""}
                          onChange={(event) =>
                            setTasks((prev) =>
                              prev.map((item) =>
                                item.id === task.id ? { ...item, description: event.target.value } : item,
                              ),
                            )
                          }
                          placeholder="Opcional"
                        />
                      </div>
                      <div className="space-y-2">
                        <Label>Ativa</Label>
                        <div className="flex h-10 items-center">
                          <Switch
                            checked={task.enabled ?? true}
                            onCheckedChange={(checked) =>
                              setTasks((prev) =>
                                prev.map((item) =>
                                  item.id === task.id ? { ...item, enabled: checked } : item,
                                ),
                              )
                            }
                          />
                        </div>
                      </div>
                    </div>
                    <div className="mt-3 rounded-md border border-border/70 bg-background/60 p-3">
                      <div className="flex items-center gap-2 text-xs font-medium text-foreground">
                        <Clock3 className="h-3.5 w-3.5" />
                        Gatilho pós-execução
                      </div>
                      <div className="mt-3 grid gap-3 md:grid-cols-[140px_1fr]">
                        <div className="space-y-2">
                          <Label>Quando</Label>
                          <Select
                            value={task.trigger?.when ?? "complete"}
                            onValueChange={(value: "success" | "failure" | "complete") =>
                              setTasks((prev) =>
                                prev.map((item) =>
                                  item.id === task.id
                                    ? {
                                        ...item,
                                        trigger: {
                                          ...(item.trigger ?? createTaskTrigger()),
                                          when: value,
                                        },
                                      }
                                    : item,
                                ),
                              )
                            }
                          >
                            <SelectTrigger>
                              <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                              <SelectItem value="complete">Ao concluir</SelectItem>
                              <SelectItem value="success">Ao finalizar com sucesso</SelectItem>
                              <SelectItem value="failure">Ao falhar</SelectItem>
                            </SelectContent>
                          </Select>
                        </div>
                        <div className="space-y-2">
                          <Label>Prompt / nota</Label>
                          <Textarea
                            value={task.trigger?.prompt ?? ""}
                            onChange={(event) =>
                              setTasks((prev) =>
                                prev.map((item) =>
                                  item.id === task.id
                                    ? {
                                        ...item,
                                        trigger: {
                                          ...(item.trigger ?? createTaskTrigger()),
                                          prompt: event.target.value,
                                        },
                                      }
                                    : item,
                                ),
                              )
                            }
                            rows={3}
                            placeholder="Revisar saída, abrir agente ou registrar próxima ação."
                          />
                        </div>
                      </div>
                    </div>
                    <p className="mt-2 text-[11px] text-muted-foreground">
                      {index + 1} tarefa configurada.
                    </p>
                  </div>
                ))}
              </div>
            </section>

            <div className="rounded-md border border-dashed border-border bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
              <div className="flex items-center gap-2 font-medium text-foreground">
                <WandSparkles className="h-4 w-4" />
                Dica de fluxo
              </div>
              <p className="mt-1">
                A palette global usa estes presets e processos. O prefixo de branch influencia o nome dos worktrees
                gerados para este projeto. O arquivo <code>.dcc.toml</code> é a fonte de verdade do repositório.
              </p>
            </div>
          </div>
        ) : null}

        <DialogFooter>
          {onEditToml ? (
            <Button type="button" variant="outline" onClick={onEditToml} disabled={!project}>
              Editar .dcc.toml
            </Button>
          ) : null}
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            Cancelar
          </Button>
          <Button type="button" onClick={handleSave} disabled={isSaving || !project}>
            {isSaving ? "Guardando..." : "Guardar configuração"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
