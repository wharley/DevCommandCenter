import React, { useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import {
  AlertTriangle,
  ArrowRight,
  Clock,
  Copy,
  Loader2,
  Play,
  Plus,
  Terminal,
  XCircle,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Empty } from "@/components/ui/empty";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Checkbox } from "@/components/ui/checkbox";
import { NewTaskDialog } from "@/components/dialogs/new-task-dialog";
import type { InitialTaskForCreate } from "@/components/dialogs/new-task-dialog";
import { useMissions, useProviders } from "@/hooks/use-data";
import { useProjectWorkspaceContext } from "@/src/pages/ProjectWorkspacePage";
import type { Mission, Provider } from "@/lib/database/types";
import { formatDistanceToNow } from "date-fns";
import { ptBR } from "date-fns/locale";
import { toast } from "sonner";

const CLI_PROVIDER_TYPES = ["codex", "claude-code", "gemini", "cursor"] as const;
const TERMINAL_RUNNING_STATUSES = ["planning", "generating_code", "applying"] as const;
const FINAL_STATUSES = ["completed", "failed", "cancelled"] as const;
type AgentPriority = "high" | "normal" | "low";
type QueueUrgency = "ok" | "warning" | "critical";
const PRIORITY_SCORE: Record<AgentPriority, number> = {
  high: 3,
  normal: 2,
  low: 1,
};
const SLA_WARNING_OPTIONS = [1, 2, 4, 6, 8, 12] as const;
const SLA_CRITICAL_OPTIONS = [4, 8, 12, 16, 24, 36, 48] as const;
const DEFAULT_SLA_WARNING_HOURS = 4;
const DEFAULT_SLA_CRITICAL_HOURS = 12;

function isCliProviderType(type: string): type is (typeof CLI_PROVIDER_TYPES)[number] {
  return CLI_PROVIDER_TYPES.includes(type as (typeof CLI_PROVIDER_TYPES)[number]);
}

function getQueueStatusLabel(mission: Mission): "Nova" | "Em execução" | "Concluída" | "Falha" | "Cancelada" {
  if (mission.status === "completed") return "Concluída";
  if (mission.status === "failed") return "Falha";
  if (mission.status === "cancelled") return "Cancelada";
  if (TERMINAL_RUNNING_STATUSES.includes(mission.status as (typeof TERMINAL_RUNNING_STATUSES)[number])) {
    return "Em execução";
  }
  if (mission.startedAt && !mission.completedAt) return "Em execução";
  return "Nova";
}

function getStatusVariant(statusLabel: ReturnType<typeof getQueueStatusLabel>) {
  if (statusLabel === "Falha") return "destructive" as const;
  if (statusLabel === "Concluída") return "secondary" as const;
  if (statusLabel === "Cancelada") return "outline" as const;
  if (statusLabel === "Em execução") return "default" as const;
  return "secondary" as const;
}

function getQueueWaitMs(mission: Mission): number {
  const base = mission.startedAt ?? mission.createdAt;
  return Math.max(0, Date.now() - base.getTime());
}

function getQueueUrgency(
  waitMs: number,
  warningHours: number,
  criticalHours: number
): QueueUrgency {
  const hours = waitMs / (1000 * 60 * 60);
  if (hours >= criticalHours) return "critical";
  if (hours >= warningHours) return "warning";
  return "ok";
}

function formatWaitLabel(waitMs: number): string {
  const totalMinutes = Math.floor(waitMs / (1000 * 60));
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours <= 0) return `${minutes}m`;
  return `${hours}h ${minutes}m`;
}

function AgentMissionMeta({
  mission,
  provider,
}: {
  mission: Mission;
  provider: Provider | null;
}) {
  return (
    <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
      <span className="inline-flex items-center gap-1">
        <Clock className="h-3.5 w-3.5" />
        {formatDistanceToNow(mission.updatedAt, { addSuffix: true, locale: ptBR })}
      </span>
      {provider && <span>Agente: {provider.name}</span>}
      {mission.worktreePath && <Badge variant="outline">Worktree</Badge>}
    </div>
  );
}

function AgentMissionActions({
  mission,
  projectId,
  onDuplicate,
  onArchive,
}: {
  mission: Mission;
  projectId: string;
  onDuplicate: (mission: Mission) => void;
  onArchive: (mission: Mission) => Promise<void>;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2">
      <Button asChild size="sm">
        <Link to={`/project/${projectId}/task/${mission.id}`}>
          <Play className="mr-2 h-4 w-4" />
          Abrir terminal
        </Link>
      </Button>
      <Button size="sm" variant="outline" onClick={() => onDuplicate(mission)}>
        <Copy className="mr-2 h-4 w-4" />
        Duplicar contexto
      </Button>
      {!FINAL_STATUSES.includes(mission.status as (typeof FINAL_STATUSES)[number]) && (
        <Button size="sm" variant="ghost" onClick={() => onArchive(mission)}>
          <XCircle className="mr-2 h-4 w-4" />
          Arquivar
        </Button>
      )}
    </div>
  );
}

export default function ProjectAgentsPage() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const { projectId, project } = useProjectWorkspaceContext();

  const { providers } = useProviders();
  const {
    missions,
    create,
    cancel: cancelMission,
    start: startMission,
    refresh: refreshMissions,
    isLoading,
  } = useMissions(projectId);

  const [quickTitle, setQuickTitle] = useState("");
  const [quickDescription, setQuickDescription] = useState("");
  const [quickProviderId, setQuickProviderId] = useState("");
  const [isQuickCreating, setIsQuickCreating] = useState(false);
  const [newTaskDialogOpen, setNewTaskDialogOpen] = useState(false);
  const [newTaskInitial, setNewTaskInitial] = useState<InitialTaskForCreate | null>(null);
  const [selectedMissionIds, setSelectedMissionIds] = useState<Set<string>>(new Set());
  const [batchConcurrency, setBatchConcurrency] = useState<string>("2");
  const [isBatchLaunching, setIsBatchLaunching] = useState(false);
  const [batchCancelRequested, setBatchCancelRequested] = useState(false);
  const [isAutoRunEnabled, setIsAutoRunEnabled] = useState(false);
  const [priorityByMissionId, setPriorityByMissionId] = useState<Record<string, AgentPriority>>({});
  const [slaWarningHours, setSlaWarningHours] = useState<string>(String(DEFAULT_SLA_WARNING_HOURS));
  const [slaCriticalHours, setSlaCriticalHours] = useState<string>(String(DEFAULT_SLA_CRITICAL_HOURS));
  const [batchProgress, setBatchProgress] = useState<{
    total: number;
    started: number;
    succeeded: number;
    failed: number;
  } | null>(null);
  const [lastFailedMissionIds, setLastFailedMissionIds] = useState<string[]>([]);
  const batchRunIdRef = useRef(0);
  const autoLaunchInFlightRef = useRef<Set<string>>(new Set());

  const cliProviders = useMemo(
    () => providers.filter((p) => p.isActive && isCliProviderType(p.type)),
    [providers]
  );

  useEffect(() => {
    if (!quickProviderId && cliProviders.length > 0) {
      setQuickProviderId(cliProviders[0].id);
    }
  }, [cliProviders, quickProviderId]);

  useEffect(() => {
    if (searchParams.get("new") !== "agents") return;
    setNewTaskDialogOpen(true);
    const next = new URLSearchParams(searchParams);
    next.delete("new");
    setSearchParams(next, { replace: true });
  }, [searchParams, setSearchParams]);

  useEffect(() => {
    if (!projectId || typeof window === "undefined") return;
    const saved = localStorage.getItem(`dcc:project:${projectId}:agents:batchConcurrency`);
    if (saved && ["1", "2", "3", "4"].includes(saved)) {
      setBatchConcurrency(saved);
    }
    const autoRunSaved = localStorage.getItem(`dcc:project:${projectId}:agents:autoRun`);
    if (autoRunSaved === "1") {
      setIsAutoRunEnabled(true);
    }
    const savedPriorities = localStorage.getItem(`dcc:project:${projectId}:agents:priorities`);
    if (savedPriorities) {
      try {
        const parsed = JSON.parse(savedPriorities) as Record<string, AgentPriority>;
        setPriorityByMissionId(parsed);
      } catch {
        setPriorityByMissionId({});
      }
    }
    const savedSla = localStorage.getItem(`dcc:project:${projectId}:agents:sla`);
    if (savedSla) {
      try {
        const parsed = JSON.parse(savedSla) as { warningHours?: number; criticalHours?: number };
        if (typeof parsed.warningHours === "number") setSlaWarningHours(String(parsed.warningHours));
        if (typeof parsed.criticalHours === "number") setSlaCriticalHours(String(parsed.criticalHours));
      } catch {
        setSlaWarningHours(String(DEFAULT_SLA_WARNING_HOURS));
        setSlaCriticalHours(String(DEFAULT_SLA_CRITICAL_HOURS));
      }
    }
  }, [projectId]);

  useEffect(() => {
    if (!projectId || typeof window === "undefined") return;
    localStorage.setItem(`dcc:project:${projectId}:agents:batchConcurrency`, batchConcurrency);
  }, [projectId, batchConcurrency]);

  useEffect(() => {
    if (!projectId || typeof window === "undefined") return;
    localStorage.setItem(`dcc:project:${projectId}:agents:autoRun`, isAutoRunEnabled ? "1" : "0");
  }, [projectId, isAutoRunEnabled]);

  useEffect(() => {
    if (!projectId || typeof window === "undefined") return;
    localStorage.setItem(
      `dcc:project:${projectId}:agents:priorities`,
      JSON.stringify(priorityByMissionId)
    );
  }, [projectId, priorityByMissionId]);

  const normalizedSlaWarningHours = useMemo(() => {
    const raw = Number(slaWarningHours);
    if (!Number.isFinite(raw)) return DEFAULT_SLA_WARNING_HOURS;
    return Math.max(1, Math.min(72, Math.floor(raw)));
  }, [slaWarningHours]);

  const normalizedSlaCriticalHours = useMemo(() => {
    const raw = Number(slaCriticalHours);
    const base = Number.isFinite(raw) ? Math.floor(raw) : DEFAULT_SLA_CRITICAL_HOURS;
    return Math.max(normalizedSlaWarningHours + 1, Math.min(240, base));
  }, [normalizedSlaWarningHours, slaCriticalHours]);

  const availableCriticalOptions = useMemo(
    () => SLA_CRITICAL_OPTIONS.filter((value) => value > normalizedSlaWarningHours),
    [normalizedSlaWarningHours]
  );

  useEffect(() => {
    const current = Number(slaCriticalHours);
    if (availableCriticalOptions.includes(current as (typeof availableCriticalOptions)[number])) return;
    const fallback = availableCriticalOptions[0] ?? DEFAULT_SLA_CRITICAL_HOURS;
    setSlaCriticalHours(String(fallback));
  }, [availableCriticalOptions, slaCriticalHours]);

  useEffect(() => {
    if (!projectId || typeof window === "undefined") return;
    localStorage.setItem(
      `dcc:project:${projectId}:agents:sla`,
      JSON.stringify({
        warningHours: normalizedSlaWarningHours,
        criticalHours: normalizedSlaCriticalHours,
      })
    );
  }, [normalizedSlaCriticalHours, normalizedSlaWarningHours, projectId]);

  const agentMissions = useMemo(
    () =>
      missions
        .filter((m) => m.missionType === "agents_cli")
        .sort((a, b) => b.updatedAt.getTime() - a.updatedAt.getTime()),
    [missions]
  );

  const getMissionPriority = (missionId: string): AgentPriority =>
    priorityByMissionId[missionId] ?? "normal";

  const running = useMemo(
    () => agentMissions.filter((m) => getQueueStatusLabel(m) === "Em execução"),
    [agentMissions]
  );

  const queued = useMemo(() => {
    return agentMissions
      .filter((m) => getQueueStatusLabel(m) === "Nova")
      .sort((a, b) => {
        const pa = PRIORITY_SCORE[getMissionPriority(a.id)];
        const pb = PRIORITY_SCORE[getMissionPriority(b.id)];
        if (pa !== pb) return pb - pa;
        const ua = getQueueUrgency(
          getQueueWaitMs(a),
          normalizedSlaWarningHours,
          normalizedSlaCriticalHours
        );
        const ub = getQueueUrgency(
          getQueueWaitMs(b),
          normalizedSlaWarningHours,
          normalizedSlaCriticalHours
        );
        const urgencyScore: Record<QueueUrgency, number> = { critical: 3, warning: 2, ok: 1 };
        if (urgencyScore[ua] !== urgencyScore[ub]) return urgencyScore[ub] - urgencyScore[ua];
        return a.createdAt.getTime() - b.createdAt.getTime();
      });
  }, [agentMissions, normalizedSlaCriticalHours, normalizedSlaWarningHours, priorityByMissionId]);

  const done = useMemo(() => {
    return agentMissions.filter((m) => {
      const label = getQueueStatusLabel(m);
      return label === "Concluída" || label === "Falha" || label === "Cancelada";
    });
  }, [agentMissions]);

  const nowMission = running[0] ?? queued[0] ?? null;
  const nextMissions = useMemo(
    () => queued.filter((m) => m.id !== nowMission?.id).slice(0, 6),
    [nowMission?.id, queued]
  );
  const recentMissions = useMemo(() => done.slice(0, 8), [done]);
  const getMissionUrgency = (mission: Mission): QueueUrgency =>
    getQueueUrgency(
      getQueueWaitMs(mission),
      normalizedSlaWarningHours,
      normalizedSlaCriticalHours
    );

  const urgentQueued = useMemo(
    () => queued.filter((mission) => getMissionUrgency(mission) !== "ok"),
    [queued, normalizedSlaCriticalHours, normalizedSlaWarningHours]
  );
  const criticalQueued = useMemo(
    () => queued.filter((mission) => getMissionUrgency(mission) === "critical"),
    [queued, normalizedSlaCriticalHours, normalizedSlaWarningHours]
  );

  const handleQuickCreate = async () => {
    if (!quickTitle.trim() || !quickDescription.trim()) {
      toast.error("Preencha título e descrição para criar a tarefa");
      return;
    }
    if (!quickProviderId) {
      toast.error("Selecione um agente CLI");
      return;
    }
    setIsQuickCreating(true);
    try {
      const mission = await create({
        projectId,
        providerId: quickProviderId,
        planProviderId: quickProviderId,
        codeProviderId: quickProviderId,
        title: quickTitle.trim(),
        description: quickDescription.trim(),
        missionType: "agents_cli",
      });
      toast.success("Tarefa criada. Abrindo terminal...");
      setQuickTitle("");
      setQuickDescription("");
      navigate(`/project/${projectId}/task/${mission.id}`);
    } catch {
      toast.error("Não foi possível criar a tarefa");
    } finally {
      setIsQuickCreating(false);
    }
  };

  const providerById = useMemo(() => {
    const map = new Map<string, Provider>();
    for (const provider of providers) map.set(provider.id, provider);
    return map;
  }, [providers]);

  const launchableMissions = useMemo(
    () =>
      agentMissions.filter(
        (m) => !FINAL_STATUSES.includes(m.status as (typeof FINAL_STATUSES)[number])
      ),
    [agentMissions]
  );

  const selectedLaunchable = useMemo(
    () => launchableMissions.filter((m) => selectedMissionIds.has(m.id)),
    [launchableMissions, selectedMissionIds]
  );

  const buildSuggestedCliCommand = (provider: Provider | null): string | undefined => {
    if (!provider) return undefined;
    const t = provider.type;
    const cliPath = provider.cliPath?.trim();
    const usePath = cliPath && (cliPath.startsWith("/") || /^[A-Za-z]:\\/.test(cliPath));
    if (t === "codex") return usePath ? cliPath : "codex";
    if (t === "claude-code") return usePath ? cliPath : "claude";
    if (t === "gemini") return usePath ? cliPath : "gemini";
    if (t === "cursor") return usePath ? cliPath : "cursor-agent";
    return undefined;
  };

  const buildMissionPrompt = (mission: Mission): string => {
    const parts = [mission.title, mission.description];
    if (mission.preserveInstructions?.trim()) {
      parts.push(`Não alterar: ${mission.preserveInstructions.trim()}`);
    }
    return parts.join("\n\n");
  };

  const launchMissionSession = async (mission: Mission): Promise<void> => {
    if (!window.electronAPI?.worktree?.ensureForMission || !window.electronAPI?.terminal?.getOrCreate) {
      throw new Error("Terminal embarcado não disponível");
    }
    const ensure = await window.electronAPI.worktree.ensureForMission(mission.id);
    if (!ensure?.success) {
      throw new Error(ensure?.error ?? `Falha ao preparar worktree da tarefa "${mission.title}"`);
    }
    const provider = mission.providerId ? providerById.get(mission.providerId) ?? null : null;
    const command = buildSuggestedCliCommand(provider);
    const prompt = buildMissionPrompt(mission);
    const result = await window.electronAPI.terminal.getOrCreate(mission.id, {
      cwd: ensure.worktreePath ?? mission.worktreePath ?? project.path,
      command,
      args: prompt ? [prompt] : [],
      cols: 120,
      rows: 36,
    });
    if (result.error) {
      throw new Error(result.error);
    }
    await startMission(mission.id);
  };

  const runBatchLaunch = async (targets?: Mission[]) => {
    const launchTargets = targets ?? selectedLaunchable;
    if (launchTargets.length === 0) {
      toast.error("Selecione pelo menos uma tarefa para executar");
      return;
    }
    const maxWorkers = Math.max(1, Math.min(4, Number(batchConcurrency) || 1));
    const runId = Date.now();
    batchRunIdRef.current = runId;
    setIsBatchLaunching(true);
    setBatchCancelRequested(false);
    setLastFailedMissionIds([]);
    setBatchProgress({
      total: launchTargets.length,
      started: 0,
      succeeded: 0,
      failed: 0,
    });

    let cursor = 0;
    let succeeded = 0;
    let failed = 0;
    const failedMissionIds: string[] = [];

    const worker = async () => {
      while (true) {
        if (batchRunIdRef.current !== runId || batchCancelRequested) return;
        const index = cursor;
        cursor += 1;
        if (index >= launchTargets.length) return;
        const mission = launchTargets[index];
        setBatchProgress((prev) =>
          prev
            ? {
                ...prev,
                started: prev.started + 1,
              }
            : prev
        );
        try {
          await launchMissionSession(mission);
          if (batchRunIdRef.current !== runId) return;
          succeeded += 1;
          setBatchProgress((prev) =>
            prev
              ? {
                  ...prev,
                  succeeded: prev.succeeded + 1,
                }
              : prev
          );
        } catch (e) {
          if (batchRunIdRef.current !== runId) return;
          failed += 1;
          failedMissionIds.push(mission.id);
          setBatchProgress((prev) =>
            prev
              ? {
                  ...prev,
                  failed: prev.failed + 1,
                }
              : prev
          );
          const message = e instanceof Error ? e.message : "erro desconhecido";
          toast.error(`Falha ao iniciar "${mission.title}": ${message}`);
        }
      }
    };

    try {
      await Promise.all(Array.from({ length: Math.min(maxWorkers, launchTargets.length) }, () => worker()));
      if (batchRunIdRef.current !== runId) return;
      await refreshMissions();
      setLastFailedMissionIds(failedMissionIds);
      if (failed === 0) {
        toast.success(`Lote iniciado com sucesso (${succeeded} tarefa(s))`);
      } else if (succeeded > 0) {
        toast.warning(`Lote parcial: ${succeeded} iniciadas, ${failed} falharam`);
      } else {
        toast.error("Nenhuma tarefa do lote foi iniciada");
      }
      setSelectedMissionIds(new Set());
    } finally {
      if (batchRunIdRef.current === runId) {
        setIsBatchLaunching(false);
        setBatchCancelRequested(false);
        setTimeout(() => {
          setBatchProgress((current) => (current ? null : current));
        }, 1200);
      }
    }
  };

  const stopBatchLaunch = () => {
    if (!isBatchLaunching) return;
    setBatchCancelRequested(true);
    batchRunIdRef.current = 0;
    setIsBatchLaunching(false);
    toast.info("Lote interrompido. Tarefas já iniciadas continuam ativas.");
  };

  useEffect(() => {
    if (!isAutoRunEnabled || isBatchLaunching) return;
    if (!window.electronAPI?.worktree?.ensureForMission || !window.electronAPI?.terminal?.getOrCreate) return;

    const maxWorkers = Math.max(1, Math.min(4, Number(batchConcurrency) || 1));
    const availableSlots = maxWorkers - running.length - autoLaunchInFlightRef.current.size;
    if (availableSlots <= 0) return;

    const candidates = queued
      .filter((mission) => !autoLaunchInFlightRef.current.has(mission.id))
      .slice(0, availableSlots);

    if (candidates.length === 0) return;

    for (const mission of candidates) {
      autoLaunchInFlightRef.current.add(mission.id);
      void (async () => {
        try {
          await launchMissionSession(mission);
          toast.success(`Auto-run iniciou: ${mission.title}`);
        } catch (e) {
          const message = e instanceof Error ? e.message : "erro desconhecido";
          toast.error(`Auto-run falhou em "${mission.title}": ${message}`);
        } finally {
          autoLaunchInFlightRef.current.delete(mission.id);
          await refreshMissions();
        }
      })();
    }
  }, [
    batchConcurrency,
    isAutoRunEnabled,
    isBatchLaunching,
    queued,
    refreshMissions,
    running.length,
  ]);

  if (isLoading) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4">
        <Loader2 className="h-12 w-12 animate-spin text-muted-foreground" />
        <p className="text-muted-foreground">Carregando tarefas de agentes...</p>
      </div>
    );
  }

  return (
    <div className="p-6">
      <div className="mb-6 grid gap-4 lg:grid-cols-[1.7fr_1fr]">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Terminal className="h-5 w-5 text-primary" />
              Nova tarefa de agente
            </CardTitle>
            <CardDescription>
              Fluxo rápido para chegar ao terminal com contexto: 1 tarefa = 1 agente = 1 branch.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            <Input
              placeholder="Título da tarefa (ex.: refatorar checkout)"
              value={quickTitle}
              onChange={(e) => setQuickTitle(e.target.value)}
            />
            <Textarea
              placeholder="Descreva o que o agente deve executar..."
              value={quickDescription}
              onChange={(e) => setQuickDescription(e.target.value)}
              rows={4}
            />
            <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
              <div className="min-w-[220px] flex-1">
                <Select value={quickProviderId} onValueChange={setQuickProviderId}>
                  <SelectTrigger>
                    <SelectValue placeholder="Selecione um agente" />
                  </SelectTrigger>
                  <SelectContent>
                    {cliProviders.length === 0 ? (
                      <SelectItem value="none" disabled>
                        Nenhum agente CLI ativo em Configurações
                      </SelectItem>
                    ) : (
                      cliProviders.map((provider) => (
                        <SelectItem key={provider.id} value={provider.id}>
                          {provider.name}
                        </SelectItem>
                      ))
                    )}
                  </SelectContent>
                </Select>
              </div>
              <Button onClick={handleQuickCreate} disabled={isQuickCreating || cliProviders.length === 0}>
                {isQuickCreating ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <Plus className="mr-2 h-4 w-4" />}
                Criar e abrir terminal
              </Button>
              <Button
                variant="outline"
                onClick={() => {
                  setNewTaskInitial(
                    quickTitle || quickDescription ? { title: quickTitle, description: quickDescription } : null
                  );
                  setNewTaskDialogOpen(true);
                }}
              >
                Abrir formulário completo
              </Button>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Visão operacional</CardTitle>
            <CardDescription>Fluxo por contexto: agora, próximas execuções e resultados.</CardDescription>
          </CardHeader>
          <CardContent className="grid gap-2 text-sm">
            <div className="flex items-center justify-between rounded-md border px-3 py-2">
              <span>Em execução</span>
              <Badge>{running.length}</Badge>
            </div>
            <div className="flex items-center justify-between rounded-md border px-3 py-2">
              <span>Na fila</span>
              <Badge variant="secondary">{queued.length}</Badge>
            </div>
            <div className="flex items-center justify-between rounded-md border px-3 py-2">
              <span>Finalizadas</span>
              <Badge variant="outline">{done.length}</Badge>
            </div>
            <div className="flex items-center justify-between rounded-md border px-3 py-2">
              <span className="inline-flex items-center gap-1">
                <AlertTriangle className="h-3.5 w-3.5 text-amber-500" />
                SLA em risco
              </span>
              <Badge variant={criticalQueued.length > 0 ? "destructive" : "outline"}>
                {urgentQueued.length}
              </Badge>
            </div>
          </CardContent>
        </Card>
      </div>

      {agentMissions.length === 0 ? (
        <Empty className="mt-12">
          <Empty.Icon>
            <Terminal className="h-10 w-10" />
          </Empty.Icon>
          <Empty.Title>Nenhuma tarefa de agente ainda</Empty.Title>
          <Empty.Description>Crie uma tarefa para abrir no terminal com Codex, Claude, Gemini ou Cursor.</Empty.Description>
          <Empty.Actions>
            <Button onClick={() => setNewTaskDialogOpen(true)}>
              <Plus className="mr-2 h-4 w-4" />
              Nova tarefa de agente
            </Button>
          </Empty.Actions>
        </Empty>
      ) : (
        <div className="space-y-5">
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-base">Agora</CardTitle>
              <CardDescription>A tarefa mais urgente para continuar imediatamente.</CardDescription>
            </CardHeader>
            <CardContent>
              {nowMission ? (
                <div className="space-y-4">
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-lg font-semibold">{nowMission.title}</p>
                      <p className="mt-1 whitespace-pre-wrap text-sm text-muted-foreground">
                        {nowMission.description}
                      </p>
                    </div>
                    <Badge variant={getStatusVariant(getQueueStatusLabel(nowMission))}>
                      {getQueueStatusLabel(nowMission)}
                    </Badge>
                  </div>
                  <AgentMissionMeta
                    mission={nowMission}
                    provider={nowMission.providerId ? providerById.get(nowMission.providerId) ?? null : null}
                  />
                  <AgentMissionActions
                    mission={nowMission}
                    projectId={projectId}
                    onDuplicate={(baseMission) => {
                      setNewTaskInitial({
                        title: `${baseMission.title} (cópia)`,
                        description: baseMission.description,
                        preserveInstructions: baseMission.preserveInstructions ?? "",
                      });
                      setNewTaskDialogOpen(true);
                    }}
                    onArchive={async (targetMission) => {
                      try {
                        await cancelMission(targetMission.id);
                        toast.success("Tarefa arquivada");
                      } catch {
                        toast.error("Não foi possível arquivar a tarefa");
                      }
                    }}
                  />
                </div>
              ) : (
                <p className="text-sm text-muted-foreground">
                  Sem tarefa em execução ou fila neste momento.
                </p>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-base">Próximas execuções</CardTitle>
              <CardDescription>Ordem sugerida para abrir no terminal em seguida.</CardDescription>
            </CardHeader>
            <CardContent>
              <div className="mb-3 flex flex-wrap items-center gap-2 rounded-md border border-border bg-muted/20 p-2">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => setSelectedMissionIds(new Set(nextMissions.map((m) => m.id)))}
                  disabled={nextMissions.length === 0 || isBatchLaunching}
                >
                  Selecionar próximas
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => setSelectedMissionIds(new Set())}
                  disabled={selectedMissionIds.size === 0 || isBatchLaunching}
                >
                  Limpar seleção
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={selectedLaunchable.length === 0 || isBatchLaunching}
                  onClick={() => {
                    setPriorityByMissionId((prev) => {
                      const next = { ...prev };
                      for (const mission of selectedLaunchable) next[mission.id] = "high";
                      return next;
                    });
                  }}
                >
                  Prioridade alta nas selecionadas
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={urgentQueued.length === 0 || isBatchLaunching}
                  onClick={() => {
                    setPriorityByMissionId((prev) => {
                      const next = { ...prev };
                      for (const mission of urgentQueued) {
                        if ((next[mission.id] ?? "normal") !== "high") next[mission.id] = "high";
                      }
                      return next;
                    });
                    toast.success(`Promovidas ${urgentQueued.length} tarefa(s) urgentes para prioridade alta`);
                  }}
                >
                  Promover urgentes ({urgentQueued.length})
                </Button>
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <span>Concorrência</span>
                  <Select value={batchConcurrency} onValueChange={setBatchConcurrency}>
                    <SelectTrigger className="h-8 w-[80px]">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="1">1x</SelectItem>
                      <SelectItem value="2">2x</SelectItem>
                      <SelectItem value="3">3x</SelectItem>
                      <SelectItem value="4">4x</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <span>SLA alerta</span>
                  <Select value={slaWarningHours} onValueChange={setSlaWarningHours}>
                    <SelectTrigger className="h-8 w-[86px]">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {SLA_WARNING_OPTIONS.map((value) => (
                        <SelectItem key={value} value={String(value)}>
                          {value}h
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <span>SLA critico</span>
                  <Select value={slaCriticalHours} onValueChange={setSlaCriticalHours}>
                    <SelectTrigger className="h-8 w-[86px]">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {availableCriticalOptions.map((value) => (
                        <SelectItem key={value} value={String(value)}>
                          {value}h
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <Button
                  size="sm"
                  onClick={() => runBatchLaunch()}
                  disabled={selectedLaunchable.length === 0 || isBatchLaunching}
                >
                  {isBatchLaunching ? (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  ) : (
                    <Play className="mr-2 h-4 w-4" />
                  )}
                  Executar lote ({selectedLaunchable.length})
                </Button>
                <Button
                  size="sm"
                  variant="destructive"
                  onClick={stopBatchLaunch}
                  disabled={!isBatchLaunching}
                >
                  Parar lote
                </Button>
                <Button
                  size="sm"
                  variant={isAutoRunEnabled ? "secondary" : "outline"}
                  onClick={() => setIsAutoRunEnabled((prev) => !prev)}
                  disabled={isBatchLaunching}
                >
                  {isAutoRunEnabled ? "Auto-run ligado" : "Auto-run desligado"}
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={lastFailedMissionIds.length === 0 || isBatchLaunching}
                  onClick={() => {
                    const retryTargets = launchableMissions.filter((m) =>
                      lastFailedMissionIds.includes(m.id)
                    );
                    runBatchLaunch(retryTargets);
                  }}
                >
                  Retry falhas ({lastFailedMissionIds.length})
                </Button>
                {batchProgress && (
                  <span className="text-xs text-muted-foreground">
                    {batchProgress.started}/{batchProgress.total} iniciadas · {batchProgress.succeeded} ok ·{" "}
                    {batchProgress.failed} falhas
                    {batchCancelRequested && " · parando..."}
                  </span>
                )}
                {!batchProgress && isAutoRunEnabled && (
                  <span className="text-xs text-muted-foreground">
                    Auto-run mantendo ativas ate {Math.max(1, Math.min(4, Number(batchConcurrency) || 1))} tarefa(s)
                  </span>
                )}
                <span className="text-xs text-muted-foreground">
                  Ordem: prioridade alta &gt; media &gt; baixa
                </span>
                <span className="text-xs text-muted-foreground">
                  SLA: alerta em {normalizedSlaWarningHours}h, critico em {normalizedSlaCriticalHours}h
                </span>
              </div>
              {nextMissions.length === 0 ? (
                <p className="text-sm text-muted-foreground">Nenhuma tarefa aguardando na fila.</p>
              ) : (
                <div className="grid gap-2 md:grid-cols-2">
                  {nextMissions.map((mission) => (
                    <div
                      key={mission.id}
                      className={`flex items-start gap-2 rounded-lg border bg-background p-3 ${
                        getMissionUrgency(mission) === "critical"
                          ? "border-destructive/50 bg-destructive/5"
                          : getMissionUrgency(mission) === "warning"
                            ? "border-amber-500/40 bg-amber-500/5"
                            : ""
                      }`}
                    >
                      <Checkbox
                        checked={selectedMissionIds.has(mission.id)}
                        disabled={isBatchLaunching}
                        onCheckedChange={(checked) => {
                          setSelectedMissionIds((prev) => {
                            const next = new Set(prev);
                            if (checked) next.add(mission.id);
                            else next.delete(mission.id);
                            return next;
                          });
                        }}
                        aria-label={`Selecionar ${mission.title}`}
                        className="mt-1"
                      />
                      <button
                        type="button"
                        className="min-w-0 flex-1 text-left transition-colors hover:bg-muted/40"
                        onClick={() => navigate(`/project/${projectId}/task/${mission.id}`)}
                      >
                        <div className="mb-1 flex items-center justify-between gap-2">
                          <span className="truncate text-sm font-medium">{mission.title}</span>
                          <div className="flex items-center gap-2">
                            {getMissionUrgency(mission) !== "ok" && (
                              <Badge
                                variant={
                                  getMissionUrgency(mission) === "critical"
                                    ? "destructive"
                                    : "outline"
                                }
                                className="text-[10px]"
                              >
                                {getMissionUrgency(mission) === "critical" ? "Critico" : "Alerta"}
                              </Badge>
                            )}
                            <Badge variant="outline" className="text-[10px]">
                              {getMissionPriority(mission.id) === "high"
                                ? "Alta"
                                : getMissionPriority(mission.id) === "low"
                                  ? "Baixa"
                                  : "Media"}
                            </Badge>
                            <ArrowRight className="h-4 w-4 text-muted-foreground" />
                          </div>
                        </div>
                        <p className="line-clamp-2 text-xs text-muted-foreground">{mission.description}</p>
                        <p className="mt-1 text-[11px] text-muted-foreground">
                          Aguardando {formatWaitLabel(getQueueWaitMs(mission))}
                        </p>
                      </button>
                      <div className="w-[110px] shrink-0">
                        <Select
                          value={getMissionPriority(mission.id)}
                          onValueChange={(value: AgentPriority) =>
                            setPriorityByMissionId((prev) => ({
                              ...prev,
                              [mission.id]: value,
                            }))
                          }
                        >
                          <SelectTrigger className="h-7 text-xs">
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="high">Alta</SelectItem>
                            <SelectItem value="normal">Media</SelectItem>
                            <SelectItem value="low">Baixa</SelectItem>
                          </SelectContent>
                        </Select>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-base">Resultados recentes</CardTitle>
              <CardDescription>Histórico curto para reaproveitar contexto rapidamente.</CardDescription>
            </CardHeader>
            <CardContent>
              {recentMissions.length === 0 ? (
                <p className="text-sm text-muted-foreground">Ainda não há tarefas finalizadas.</p>
              ) : (
                <div className="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
                  {recentMissions.map((mission) => {
                    const statusLabel = getQueueStatusLabel(mission);
                    return (
                      <div key={mission.id} className="rounded-lg border bg-background p-3">
                        <div className="mb-2 flex items-start justify-between gap-2">
                          <p className="line-clamp-1 text-sm font-medium">{mission.title}</p>
                          <Badge variant={getStatusVariant(statusLabel)}>{statusLabel}</Badge>
                        </div>
                        <AgentMissionMeta
                          mission={mission}
                          provider={mission.providerId ? providerById.get(mission.providerId) ?? null : null}
                        />
                        <div className="mt-3 flex items-center gap-2">
                          <Button asChild size="sm" variant="outline">
                            <Link to={`/project/${projectId}/task/${mission.id}`}>Reabrir</Link>
                          </Button>
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() => {
                              setNewTaskInitial({
                                title: `${mission.title} (cópia)`,
                                description: mission.description,
                                preserveInstructions: mission.preserveInstructions ?? "",
                              });
                              setNewTaskDialogOpen(true);
                            }}
                          >
                            Duplicar
                          </Button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      )}

      <NewTaskDialog
        open={newTaskDialogOpen}
        onOpenChange={(open) => {
          setNewTaskDialogOpen(open);
          if (!open) setNewTaskInitial(null);
        }}
        projectId={projectId}
        initialTask={newTaskInitial ?? undefined}
      />
    </div>
  );
}
