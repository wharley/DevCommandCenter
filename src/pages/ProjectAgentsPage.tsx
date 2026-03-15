import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import {
  AlertTriangle,
  ArrowRight,
  Clock,
  Copy,
  ExternalLink,
  GitBranch,
  GitCommit,
  GitMerge,
  Loader2,
  Play,
  Plus,
  Terminal,
  Trash2,
  Upload,
  XCircle,
} from "lucide-react";
import { CommitDialog } from "@/components/dialogs/commit-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
import { EmbeddedTerminal } from "@/components/embedded-terminal";
import { DiffCodeBlock } from "@/components/diff-code-block";
import { useConfirmDialog } from "@/components/providers/confirm-dialog-provider";
import { Collapsible, CollapsibleContent } from "@/components/ui/collapsible";
import { useMissions, useProviders } from "@/hooks/use-data";
import { useAppStore } from "@/hooks/use-app-store";
import { useProjectWorkspaceContext } from "@/src/pages/ProjectWorkspacePage";
import type { Mission, Provider } from "@/lib/database/types";
import type { GitBranchState, GitStatus } from "@/types/electron";
import { formatDistanceToNow } from "date-fns";
import { ptBR } from "date-fns/locale";
import { toast } from "sonner";

const CLI_PROVIDER_TYPES = [
  "codex",
  "claude-code",
  "gemini",
  "cursor",
] as const;
const TERMINAL_RUNNING_STATUSES = [
  "planning",
  "generating_code",
  "applying",
] as const;
const FINAL_STATUSES = ["completed", "failed", "cancelled"] as const;
type AgentPriority = "high" | "normal" | "low";
type QueueUrgency = "ok" | "warning" | "critical";
type AgentsView = "wall" | "queue";
type AgentLane = "running" | "queued" | "review" | "history";
const PRIORITY_SCORE: Record<AgentPriority, number> = {
  high: 3,
  normal: 2,
  low: 1,
};
const SLA_WARNING_OPTIONS = [1, 2, 4, 6, 8, 12] as const;
const SLA_CRITICAL_OPTIONS = [4, 8, 12, 16, 24, 36, 48] as const;
const DEFAULT_SLA_WARNING_HOURS = 4;
const DEFAULT_SLA_CRITICAL_HOURS = 12;

function isCliProviderType(
  type: string,
): type is (typeof CLI_PROVIDER_TYPES)[number] {
  return CLI_PROVIDER_TYPES.includes(
    type as (typeof CLI_PROVIDER_TYPES)[number],
  );
}

function getQueueStatusLabel(
  mission: Mission,
): "Nova" | "Em execução" | "Concluída" | "Falha" | "Cancelada" {
  if (mission.status === "completed") return "Concluída";
  if (mission.status === "failed") return "Falha";
  if (mission.status === "cancelled") return "Cancelada";
  if (mission.context?.agentSession?.status === "running") return "Em execução";
  if (
    TERMINAL_RUNNING_STATUSES.includes(
      mission.status as (typeof TERMINAL_RUNNING_STATUSES)[number],
    )
  ) {
    return "Em execução";
  }
  if (mission.startedAt && !mission.completedAt) return "Em execução";
  return "Nova";
}

function hasActiveSession(mission: Mission): boolean {
  if (
    FINAL_STATUSES.includes(
      mission.status as (typeof FINAL_STATUSES)[number],
    )
  ) {
    return false;
  }
  return (
    mission.context?.agentSession?.status === "running" ||
    getQueueStatusLabel(mission) === "Em execução"
  );
}

function hasReviewableBranch(mission: Mission): boolean {
  if (!mission.worktreePath) return false;
  if (
    !FINAL_STATUSES.includes(mission.status as (typeof FINAL_STATUSES)[number])
  )
    return false;
  return true;
}

function getAgentLane(mission: Mission): AgentLane {
  if (hasActiveSession(mission)) return "running";
  if (hasReviewableBranch(mission)) return "review";
  if (
    FINAL_STATUSES.includes(mission.status as (typeof FINAL_STATUSES)[number])
  )
    return "history";
  return "queued";
}

function formatMergeReadiness(state?: GitBranchState | null): string {
  if (!state) return "Git pendente";
  if (state.mergeReadiness === "ready") return "Pronta para integrar";
  if (state.mergeReadiness === "dirty") return "Commit pendente";
  if (state.mergeReadiness === "behind_default") return "Atualize com a base";
  if (state.mergeReadiness === "diverged") return "Resolver divergência";
  if (state.mergeReadiness === "already_merged") return "Já incorporada";
  return "Sem integração aplicável";
}

function formatPublishStatus(state?: GitBranchState | null): string {
  if (!state) return "Git pendente";
  if (!state.hasUpstream) return "Branch local";
  if (state.aheadCount > 0) return `${state.aheadCount} commit(s) para enviar`;
  if (state.behindCount > 0)
    return `${state.behindCount} commit(s) para atualizar`;
  return "Remoto sincronizado";
}

function normalizeGitRemoteUrl(remoteUrl?: string | null): string | null {
  if (!remoteUrl) return null;
  const trimmed = remoteUrl.trim();
  if (!trimmed) return null;

  if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) {
    return trimmed.replace(/\.git$/, "");
  }

  const sshMatch = trimmed.match(/^git@([^:]+):(.+?)(?:\.git)?$/);
  if (sshMatch) {
    return `https://${sshMatch[1]}/${sshMatch[2]}`.replace(/\.git$/, "");
  }

  return null;
}

function buildCompareUrl(
  remoteUrl?: string | null,
  baseBranch?: string | null,
  headBranch?: string | null,
): string | null {
  const normalized = normalizeGitRemoteUrl(remoteUrl);
  if (!normalized || !baseBranch || !headBranch) return null;

  try {
    const url = new URL(normalized);
    if (url.hostname.includes("github.com")) {
      return `${normalized}/compare/${encodeURIComponent(baseBranch)}...${encodeURIComponent(headBranch)}`;
    }
    if (url.hostname.includes("gitlab")) {
      return `${normalized}/-/compare/${encodeURIComponent(baseBranch)}...${encodeURIComponent(headBranch)}`;
    }
    if (url.hostname.includes("bitbucket")) {
      return `${normalized}/branches/compare/${encodeURIComponent(headBranch)}%0D${encodeURIComponent(baseBranch)}`;
    }
  } catch {
    return normalized;
  }

  return normalized;
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
  criticalHours: number,
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

function formatRelativeIso(iso?: string | null): string | null {
  if (!iso) return null;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return null;
  return formatDistanceToNow(date, { addSuffix: true, locale: ptBR });
}

function getSessionPreview(mission: Mission): string | null {
  const raw = mission.context?.agentSession?.outputPreview?.trim();
  if (!raw) return null;
  const lines = raw.split(/\r?\n/).filter(Boolean);
  if (lines.length === 0) return null;
  return lines.slice(-4).join("\n");
}

/** Shorten worktree path for display. Suporta path local (projeto/.dcc/worktrees/branch) e legado global. */
function shortenWorktreePath(fullPath: string): string {
  const parts = fullPath.replace(/\\/g, "/").split("/");
  const worktreesIdx = parts.indexOf("worktrees");
  if (worktreesIdx >= 0) {
    const isLocalDcc = parts[worktreesIdx - 1] === ".dcc" && worktreesIdx >= 2;
    if (isLocalDcc) {
      const projectName = parts[worktreesIdx - 2];
      const branch = parts[worktreesIdx + 1];
      return `${projectName}/.dcc/worktrees/${branch}`;
    }
    if (worktreesIdx < parts.length - 2) {
      const repoHash = parts[worktreesIdx + 1];
      const branch = parts[worktreesIdx + 2];
      const before = parts.slice(0, worktreesIdx);
      const prefix =
        before[0] === "" || before[0] === "Users" ? "~" : before.join("/");
      return `${prefix}/.../worktrees/${repoHash}/${branch}`;
    }
  }
  if (parts.length > 3)
    return `${parts.slice(0, 2).join("/")}/.../${parts.slice(-2).join("/")}`;
  return fullPath;
}

function getFriendlyBranchLabel(branch?: string | null): string | null {
  if (!branch || !branch.startsWith("dcc-mission-")) return null;
  const raw = branch.replace(/^dcc-mission-/, "");
  const withoutId = raw.replace(/-[a-z0-9]{8}$/i, "");
  if (!withoutId) return null;
  const words = withoutId
    .split("-")
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1));
  return words.length > 0 ? words.join(" ") : null;
}

function BranchAccessRow({
  branch,
  worktreePath,
}: {
  branch?: string | null;
  worktreePath?: string | null;
}) {
  const branchLabel =
    branch ?? (worktreePath ? "preparando..." : "branch não criada");
  const friendlyLabel = getFriendlyBranchLabel(branch);

  const copyBranch = async () => {
    if (!branch) {
      toast.error("A branch ainda não está disponível");
      return;
    }
    try {
      await navigator.clipboard.writeText(branch);
      toast.success("Nome da branch copiado");
    } catch {
      toast.error("Não foi possível copiar a branch");
    }
  };

  return (
    <div className="rounded-lg border bg-muted/10 p-2">
      <div className="flex flex-wrap items-start gap-2">
        <div className="min-w-0 flex-1">
          <p className="text-[11px] uppercase tracking-wide text-muted-foreground">
            Branch real
          </p>
          <p className="mt-1 break-all font-mono text-xs">{branchLabel}</p>
          {friendlyLabel && (
            <p className="mt-1 text-[11px] text-muted-foreground">
              Nome amigável: {friendlyLabel}
            </p>
          )}
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            variant="outline"
            className="h-7 px-2"
            onClick={() => void copyBranch()}
            disabled={!branch}
          >
            <Copy className="mr-1 h-3.5 w-3.5" />
            Copiar
          </Button>
        </div>
      </div>
      {worktreePath && (
        <p className="mt-2 break-all font-mono text-[11px] text-muted-foreground">
          {worktreePath}
        </p>
      )}
    </div>
  );
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
        {formatDistanceToNow(mission.updatedAt, {
          addSuffix: true,
          locale: ptBR,
        })}
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
      {!FINAL_STATUSES.includes(
        mission.status as (typeof FINAL_STATUSES)[number],
      ) && (
        <Button size="sm" variant="ghost" onClick={() => onArchive(mission)}>
          <XCircle className="mr-2 h-4 w-4" />
          Arquivar
        </Button>
      )}
    </div>
  );
}

function AgentGitFlowStatus({
  mission,
  gitState,
}: {
  mission: Mission;
  gitState?: GitBranchState | null;
}) {
  const steps = [
    {
      key: "branch",
      label: mission.worktreePath ? "Branch pronta" : "Branch pendente",
      done: Boolean(mission.worktreePath),
    },
    {
      key: "commit",
      label: gitState
        ? gitState.isDirty
          ? "Commit pendente"
          : gitState.changedFiles.length > 0
            ? "Alteracoes prontas"
            : "Sem alteracoes locais"
        : mission.isCommitted
          ? "Commitado"
          : "Commit pendente",
      done: gitState ? !gitState.isDirty : Boolean(mission.isCommitted),
    },
    {
      key: "push",
      label: gitState
        ? gitState.hasUpstream
          ? gitState.aheadCount > 0
            ? "Push pendente"
            : "Remoto sincronizado"
          : "Sem branch remota"
        : mission.isPushed
          ? "Enviado"
          : "Push pendente",
      done: gitState
        ? gitState.hasUpstream && gitState.aheadCount === 0
        : Boolean(mission.isPushed),
    },
  ] as const;

  return (
    <div className="mt-2 flex flex-wrap gap-1.5">
      {steps.map((step) => (
        <span
          key={step.key}
          className={`rounded-full border px-2 py-0.5 text-[10px] font-medium ${
            step.done
              ? "border-emerald-500/40 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
              : "border-border bg-muted/40 text-muted-foreground"
          }`}
        >
          {step.label}
        </span>
      ))}
    </div>
  );
}

function ReviewMissionCard({
  mission,
  projectId,
  provider,
  gitState,
  compareUrl,
  isExpanded,
  diffState,
  isWorktreeAction,
  onMerge,
  onDiscard,
  onToggleExpanded,
  onDuplicate,
}: {
  mission: Mission;
  projectId: string;
  provider: Provider | null;
  gitState?: GitBranchState | null;
  compareUrl?: string | null;
  isExpanded: boolean;
  diffState?: {
    loading: boolean;
    error?: string;
    files: Array<{ path: string; diff: string }>;
  };
  isWorktreeAction: boolean;
  onMerge: (mission: Mission) => void;
  onDiscard: (mission: Mission) => void;
  onToggleExpanded: (mission: Mission) => void;
  onDuplicate: (mission: Mission) => void;
}) {
  const reviewHighlights = [
    {
      label: "Integração",
      value: formatMergeReadiness(gitState),
    },
    {
      label: "Publicação",
      value: formatPublishStatus(gitState),
    },
    {
      label: "Arquivos",
      value: String(gitState?.changedFiles.length ?? 0),
    },
  ];

  return (
    <Card className="overflow-hidden border-primary/10 shadow-sm">
      <CardHeader className="pb-3">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0 flex-1">
            <CardTitle className="line-clamp-1 text-base">
              {mission.title}
            </CardTitle>
            <CardDescription className="line-clamp-2">
              {provider ? provider.name : "Sem agente"} ·{" "}
              {formatMergeReadiness(gitState)}
            </CardDescription>
          </div>
          <Badge variant={getStatusVariant(getQueueStatusLabel(mission))}>
            {getQueueStatusLabel(mission)}
          </Badge>
        </div>
        <AgentMissionMeta mission={mission} provider={provider} />
        <BranchAccessRow
          branch={gitState?.branch ?? mission.worktreeBranch ?? null}
          worktreePath={mission.worktreePath ?? null}
        />
        <AgentGitFlowStatus mission={mission} gitState={gitState} />
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="grid gap-2 md:grid-cols-3">
          {reviewHighlights.map((item) => (
            <div
              key={item.label}
              className="rounded-lg border bg-muted/20 px-3 py-2"
            >
              <p className="text-[11px] uppercase tracking-wide text-muted-foreground">
                {item.label}
              </p>
              <p className="mt-1 text-sm font-medium">{item.value}</p>
            </div>
          ))}
        </div>

        <div className="flex flex-wrap gap-2 rounded-lg border bg-muted/10 p-2">
          <Button asChild size="sm">
            <Link to={`/project/${projectId}/task/${mission.id}`}>
              Continuar na mesma branch
            </Link>
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={() => onToggleExpanded(mission)}
          >
            {isExpanded ? "Ocultar diff" : "Mostrar diff"}
          </Button>
          {compareUrl && (
            <Button
              size="sm"
              variant="outline"
              onClick={() =>
                void window.electronAPI?.shell?.openExternal(compareUrl)
              }
            >
              <ExternalLink className="mr-2 h-4 w-4" />
              Abrir comparação
            </Button>
          )}
          <Button
            size="sm"
            variant="outline"
            disabled={isWorktreeAction}
            onClick={() => onMerge(mission)}
          >
            <GitMerge className="mr-2 h-4 w-4" />
            Incorporar ao principal
          </Button>
          {!(mission.isCommitted && mission.isPushed) && (
            <Button
              size="sm"
              variant="ghost"
              disabled={isWorktreeAction}
              onClick={() => onDiscard(mission)}
            >
              <Trash2 className="mr-2 h-4 w-4" />
              Descartar branch
            </Button>
          )}
          <Button
            size="sm"
            variant="ghost"
            onClick={() => onDuplicate(mission)}
          >
            <Copy className="mr-2 h-4 w-4" />
            Criar tarefa derivada
          </Button>
        </div>

        <Collapsible open={isExpanded}>
          <CollapsibleContent className="space-y-3">
            {diffState?.loading ? (
              <div className="flex items-center gap-2 rounded-md border px-3 py-2 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                Carregando diffs da branch...
              </div>
            ) : diffState?.error ? (
              <div className="rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-sm text-destructive">
                {diffState.error}
              </div>
            ) : diffState?.files.length ? (
              diffState.files.map((file) => (
                <div key={file.path} className="rounded-md border">
                  <div className="border-b bg-muted/30 px-3 py-2 font-mono text-xs">
                    {file.path}
                  </div>
                  <DiffCodeBlock content={file.diff} />
                </div>
              ))
            ) : (
              <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
                Nenhum diff encontrado nessa branch.
              </div>
            )}
          </CollapsibleContent>
        </Collapsible>
      </CardContent>
    </Card>
  );
}

export default function ProjectAgentsPage() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const { projectId, project } = useProjectWorkspaceContext();
  const { confirmDialog } = useConfirmDialog();
  const setSidebarCollapsed = useAppStore((s) => s.setSidebarCollapsed);

  const { providers } = useProviders();
  const {
    missions,
    create,
    getById,
    update,
    remove: removeMission,
    cancel: cancelMission,
    complete: completeMission,
    fail: failMission,
    refresh: refreshMissions,
    isLoading,
  } = useMissions(projectId);

  const [quickTitle, setQuickTitle] = useState("");
  const [quickDescription, setQuickDescription] = useState("");
  const [quickProviderId, setQuickProviderId] = useState("");
  const [agentsView, setAgentsView] = useState<AgentsView>("wall");
  const [isQuickCreating, setIsQuickCreating] = useState(false);
  const [newTaskDialogOpen, setNewTaskDialogOpen] = useState(false);
  const [newTaskInitial, setNewTaskInitial] =
    useState<InitialTaskForCreate | null>(null);
  const [selectedMissionIds, setSelectedMissionIds] = useState<Set<string>>(
    new Set(),
  );
  const [batchConcurrency, setBatchConcurrency] = useState<string>("2");
  const [isBatchLaunching, setIsBatchLaunching] = useState(false);
  const [batchCancelRequested, setBatchCancelRequested] = useState(false);
  const [isAutoRunEnabled, setIsAutoRunEnabled] = useState(false);
  const [priorityByMissionId, setPriorityByMissionId] = useState<
    Record<string, AgentPriority>
  >({});
  const [slaWarningHours, setSlaWarningHours] = useState<string>(
    String(DEFAULT_SLA_WARNING_HOURS),
  );
  const [slaCriticalHours, setSlaCriticalHours] = useState<string>(
    String(DEFAULT_SLA_CRITICAL_HOURS),
  );
  const [batchProgress, setBatchProgress] = useState<{
    total: number;
    started: number;
    succeeded: number;
    failed: number;
  } | null>(null);
  const [commitDialogOpen, setCommitDialogOpen] = useState(false);
  const [commitDialogMission, setCommitDialogMission] =
    useState<Mission | null>(null);
  const [commitDialogStatus, setCommitDialogStatus] =
    useState<GitStatus | null>(null);
  const [isFinishingMissionId, setIsFinishingMissionId] = useState<
    string | null
  >(null);
  const [isPushingMissionId, setIsPushingMissionId] = useState<string | null>(
    null,
  );
  const [lastFailedMissionIds, setLastFailedMissionIds] = useState<string[]>(
    [],
  );
  const [postFinishMissionId, setPostFinishMissionId] = useState<string | null>(
    null,
  );
  const [isWorktreeActionMissionId, setIsWorktreeActionMissionId] = useState<
    string | null
  >(null);
  const [gitStateByMissionId, setGitStateByMissionId] = useState<
    Record<string, GitBranchState | null>
  >({});
  const [projectBranch, setProjectBranch] = useState<string | null>(null);
  const [projectBranchLoading, setProjectBranchLoading] = useState(false);
  const [reviewExpandedByMissionId, setReviewExpandedByMissionId] = useState<
    Record<string, boolean>
  >({});
  const [reviewDiffsByMissionId, setReviewDiffsByMissionId] = useState<
    Record<
      string,
      {
        loading: boolean;
        error?: string;
        files: Array<{ path: string; diff: string }>;
      }
    >
  >({});
  const batchRunIdRef = useRef(0);
  const autoLaunchInFlightRef = useRef<Set<string>>(new Set());

  const cliProviders = useMemo(
    () => providers.filter((p) => p.isActive && isCliProviderType(p.type)),
    [providers],
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
    const saved = localStorage.getItem(
      `dcc:project:${projectId}:agents:batchConcurrency`,
    );
    if (saved && ["1", "2", "3", "4"].includes(saved)) {
      setBatchConcurrency(saved);
    }
    const autoRunSaved = localStorage.getItem(
      `dcc:project:${projectId}:agents:autoRun`,
    );
    if (autoRunSaved === "1") {
      setIsAutoRunEnabled(true);
    }
    const savedPriorities = localStorage.getItem(
      `dcc:project:${projectId}:agents:priorities`,
    );
    if (savedPriorities) {
      try {
        const parsed = JSON.parse(savedPriorities) as Record<
          string,
          AgentPriority
        >;
        setPriorityByMissionId(parsed);
      } catch {
        setPriorityByMissionId({});
      }
    }
    const savedSla = localStorage.getItem(
      `dcc:project:${projectId}:agents:sla`,
    );
    if (savedSla) {
      try {
        const parsed = JSON.parse(savedSla) as {
          warningHours?: number;
          criticalHours?: number;
        };
        if (typeof parsed.warningHours === "number")
          setSlaWarningHours(String(parsed.warningHours));
        if (typeof parsed.criticalHours === "number")
          setSlaCriticalHours(String(parsed.criticalHours));
      } catch {
        setSlaWarningHours(String(DEFAULT_SLA_WARNING_HOURS));
        setSlaCriticalHours(String(DEFAULT_SLA_CRITICAL_HOURS));
      }
    }
    const savedView = localStorage.getItem(
      `dcc:project:${projectId}:agents:view`,
    );
    if (savedView === "wall" || savedView === "queue") {
      setAgentsView(savedView);
    }
  }, [projectId]);

  useEffect(() => {
    if (!projectId || typeof window === "undefined") return;
    localStorage.setItem(
      `dcc:project:${projectId}:agents:batchConcurrency`,
      batchConcurrency,
    );
  }, [projectId, batchConcurrency]);

  useEffect(() => {
    if (!projectId || typeof window === "undefined") return;
    localStorage.setItem(
      `dcc:project:${projectId}:agents:autoRun`,
      isAutoRunEnabled ? "1" : "0",
    );
  }, [projectId, isAutoRunEnabled]);

  useEffect(() => {
    if (!projectId || typeof window === "undefined") return;
    localStorage.setItem(`dcc:project:${projectId}:agents:view`, agentsView);
  }, [agentsView, projectId]);

  useEffect(() => {
    if (agentsView === "wall") {
      setSidebarCollapsed(true);
      return () => setSidebarCollapsed(false);
    }
  }, [agentsView, setSidebarCollapsed]);

  useEffect(() => {
    const targetPath = commitDialogMission?.worktreePath ?? project?.path;
    if (!commitDialogOpen || !targetPath || !window.electronAPI?.git) {
      if (!commitDialogOpen) setCommitDialogStatus(null);
      return;
    }

    let cancelled = false;
    setCommitDialogStatus(null);

    window.electronAPI.git
      .getStatus(targetPath)
      .then((status) => {
        if (!cancelled) setCommitDialogStatus(status);
      })
      .catch(() => {
        if (!cancelled) setCommitDialogStatus(null);
      });

    return () => {
      cancelled = true;
    };
  }, [commitDialogMission?.worktreePath, commitDialogOpen, project?.path]);

  useEffect(() => {
    const path = project?.path;
    if (!path?.trim() || typeof window === "undefined" || !window.electronAPI?.git?.getCurrentBranch) {
      setProjectBranch(null);
      setProjectBranchLoading(false);
      return;
    }
    let cancelled = false;
    setProjectBranchLoading(true);
    setProjectBranch(null);
    window.electronAPI.git
      .getCurrentBranch(path.trim())
      .then((branch) => {
        if (!cancelled) setProjectBranch(branch?.trim() || null);
      })
      .catch(() => {
        if (!cancelled) setProjectBranch(null);
      })
      .finally(() => {
        if (!cancelled) setProjectBranchLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [project?.path]);

  useEffect(() => {
    if (!projectId || typeof window === "undefined") return;
    localStorage.setItem(
      `dcc:project:${projectId}:agents:priorities`,
      JSON.stringify(priorityByMissionId),
    );
  }, [projectId, priorityByMissionId]);

  const normalizedSlaWarningHours = useMemo(() => {
    const raw = Number(slaWarningHours);
    if (!Number.isFinite(raw)) return DEFAULT_SLA_WARNING_HOURS;
    return Math.max(1, Math.min(72, Math.floor(raw)));
  }, [slaWarningHours]);

  const normalizedSlaCriticalHours = useMemo(() => {
    const raw = Number(slaCriticalHours);
    const base = Number.isFinite(raw)
      ? Math.floor(raw)
      : DEFAULT_SLA_CRITICAL_HOURS;
    return Math.max(normalizedSlaWarningHours + 1, Math.min(240, base));
  }, [normalizedSlaWarningHours, slaCriticalHours]);

  const availableCriticalOptions = useMemo(
    () =>
      SLA_CRITICAL_OPTIONS.filter((value) => value > normalizedSlaWarningHours),
    [normalizedSlaWarningHours],
  );

  useEffect(() => {
    const current = Number(slaCriticalHours);
    if (
      availableCriticalOptions.includes(
        current as (typeof availableCriticalOptions)[number],
      )
    )
      return;
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
      }),
    );
  }, [normalizedSlaCriticalHours, normalizedSlaWarningHours, projectId]);

  const agentMissions = useMemo(
    () =>
      missions
        .filter((m) => m.missionType === "agents_cli")
        .sort((a, b) => b.updatedAt.getTime() - a.updatedAt.getTime()),
    [missions],
  );

  useEffect(() => {
    if (!window.electronAPI?.git?.getBranchState) return;
    const targets = agentMissions.filter((mission) => mission.worktreePath);
    if (targets.length === 0) {
      setGitStateByMissionId({});
      return;
    }

    let cancelled = false;
    Promise.all(
      targets.map(async (mission) => {
        try {
          const gitState = await window.electronAPI!.git.getBranchState(
            mission.worktreePath!,
          );
          return [mission.id, gitState] as const;
        } catch {
          return [mission.id, null] as const;
        }
      }),
    ).then((entries) => {
      if (cancelled) return;
      setGitStateByMissionId((prev) => {
        const next: Record<string, GitBranchState | null> = {};
        for (const [missionId, gitState] of entries) next[missionId] = gitState;
        for (const missionId of Object.keys(prev)) {
          if (!(missionId in next) && reviewExpandedByMissionId[missionId]) {
            next[missionId] = prev[missionId];
          }
        }
        return next;
      });
    });

    return () => {
      cancelled = true;
    };
  }, [agentMissions, reviewExpandedByMissionId]);

  const getMissionPriority = (missionId: string): AgentPriority =>
    priorityByMissionId[missionId] ?? "normal";

  const running = useMemo(
    () => agentMissions.filter((m) => getAgentLane(m) === "running"),
    [agentMissions],
  );

  const queued = useMemo(() => {
    return agentMissions
      .filter((m) => getAgentLane(m) === "queued")
      .sort((a, b) => {
        const pa = PRIORITY_SCORE[getMissionPriority(a.id)];
        const pb = PRIORITY_SCORE[getMissionPriority(b.id)];
        if (pa !== pb) return pb - pa;
        const ua = getQueueUrgency(
          getQueueWaitMs(a),
          normalizedSlaWarningHours,
          normalizedSlaCriticalHours,
        );
        const ub = getQueueUrgency(
          getQueueWaitMs(b),
          normalizedSlaWarningHours,
          normalizedSlaCriticalHours,
        );
        const urgencyScore: Record<QueueUrgency, number> = {
          critical: 3,
          warning: 2,
          ok: 1,
        };
        if (urgencyScore[ua] !== urgencyScore[ub])
          return urgencyScore[ub] - urgencyScore[ua];
        return a.createdAt.getTime() - b.createdAt.getTime();
      });
  }, [
    agentMissions,
    normalizedSlaCriticalHours,
    normalizedSlaWarningHours,
    priorityByMissionId,
  ]);

  const readyToReview = useMemo(
    () => agentMissions.filter((m) => getAgentLane(m) === "review"),
    [agentMissions],
  );
  const history = useMemo(
    () => agentMissions.filter((m) => getAgentLane(m) === "history"),
    [agentMissions],
  );

  const nowMission = running[0] ?? queued[0] ?? readyToReview[0] ?? null;
  const nextMissions = useMemo(
    () => queued.filter((m) => m.id !== nowMission?.id).slice(0, 6),
    [nowMission?.id, queued],
  );
  const recentMissions = useMemo(() => history.slice(0, 8), [history]);
  const getMissionUrgency = (mission: Mission): QueueUrgency =>
    getQueueUrgency(
      getQueueWaitMs(mission),
      normalizedSlaWarningHours,
      normalizedSlaCriticalHours,
    );

  const urgentQueued = useMemo(
    () => queued.filter((mission) => getMissionUrgency(mission) !== "ok"),
    [queued, normalizedSlaCriticalHours, normalizedSlaWarningHours],
  );
  const criticalQueued = useMemo(
    () => queued.filter((mission) => getMissionUrgency(mission) === "critical"),
    [queued, normalizedSlaCriticalHours, normalizedSlaWarningHours],
  );
  const wallQueuedPreview = useMemo(
    () => queued.slice(0, Math.max(0, 6 - running.length)),
    [queued, running.length],
  );
  const wallMissions = useMemo(() => running, [running]);

  const handleQuickCreate = async () => {
    if (!quickTitle.trim()) {
      toast.error("Descreva rapidamente a tarefa para iniciar o agente");
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
        description: quickDescription.trim() || quickTitle.trim(),
        missionType: "agents_cli",
      });
      await launchMissionSession(mission);
      await refreshMissions();
      toast.success("Agente iniciado no Wall");
      setQuickTitle("");
      setQuickDescription("");
    } catch {
      toast.error("Não foi possível iniciar o agente");
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
        (m) =>
          !FINAL_STATUSES.includes(m.status as (typeof FINAL_STATUSES)[number]),
      ),
    [agentMissions],
  );

  const selectedLaunchable = useMemo(
    () => launchableMissions.filter((m) => selectedMissionIds.has(m.id)),
    [launchableMissions, selectedMissionIds],
  );

  const buildSuggestedCliCommand = (
    provider: Provider | null,
  ): string | undefined => {
    if (!provider) return undefined;
    const t = provider.type;
    const cliPath = provider.cliPath?.trim();
    const usePath =
      cliPath && (cliPath.startsWith("/") || /^[A-Za-z]:\\/.test(cliPath));
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

  const loadReviewDiffs = useCallback(async (mission: Mission) => {
    if (
      !mission.worktreePath ||
      !window.electronAPI?.git?.getBranchState ||
      !window.electronAPI?.git?.getFileDiffHead
    ) {
      return;
    }

    setReviewDiffsByMissionId((prev) => ({
      ...prev,
      [mission.id]: {
        loading: true,
        files: prev[mission.id]?.files ?? [],
      },
    }));

    try {
      const gitState = await window.electronAPI.git.getBranchState(
        mission.worktreePath,
      );
      const changedFiles = gitState.changedFiles.slice(0, 6);
      const localChangedFiles = new Set([
        ...gitState.untracked,
        ...gitState.staged,
        ...gitState.unstaged,
      ]);
      const canCompareAgainstBase = Boolean(
        gitState.defaultBranch && window.electronAPI?.git?.getFileDiffAgainstBase,
      );
      const files = await Promise.all(
        changedFiles.map(async (filePath) => {
          let diff = "";
          if (canCompareAgainstBase && gitState.defaultBranch) {
            diff = await window.electronAPI!.git.getFileDiffAgainstBase(
              mission.worktreePath!,
              filePath,
              gitState.defaultBranch,
            );
          }
          if (!diff.trim() && (localChangedFiles.has(filePath) || !canCompareAgainstBase)) {
            diff = await window.electronAPI!.git.getFileDiffHead(
              mission.worktreePath!,
              filePath,
            );
          }
          return {
            path: filePath,
            diff: diff || "Sem diff textual disponivel para este arquivo.",
          };
        }),
      );
      setGitStateByMissionId((prev) => ({ ...prev, [mission.id]: gitState }));
      setReviewDiffsByMissionId((prev) => ({
        ...prev,
        [mission.id]: {
          loading: false,
          files,
        },
      }));
    } catch (error) {
      setReviewDiffsByMissionId((prev) => ({
        ...prev,
        [mission.id]: {
          loading: false,
          files: prev[mission.id]?.files ?? [],
          error:
            error instanceof Error ? error.message : "Falha ao carregar diff",
        },
      }));
    }
  }, []);

  const launchMissionSession = async (mission: Mission): Promise<void> => {
    if (
      !window.electronAPI?.worktree?.ensureForMission ||
      !window.electronAPI?.terminal?.getOrCreate
    ) {
      throw new Error("Terminal embarcado não disponível");
    }
    const ensure = await window.electronAPI.worktree.ensureForMission(
      mission.id,
    );
    if (!ensure?.success) {
      throw new Error(
        ensure?.error ??
          `Falha ao preparar worktree da tarefa "${mission.title}"`,
      );
    }
    const provider = mission.providerId
      ? (providerById.get(mission.providerId) ?? null)
      : null;
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
  };

  const openCommitDialog = (mission: Mission) => {
    setCommitDialogMission(mission);
    setCommitDialogOpen(true);
  };

  const handleCommitFromWall = async (message: string) => {
    const mission = commitDialogMission;
    const targetPath = mission?.worktreePath ?? project?.path;

    if (
      !mission ||
      !targetPath ||
      typeof window === "undefined" ||
      !window.electronAPI?.git
    ) {
      toast.error("Commit indisponível");
      throw new Error("Commit indisponível");
    }

    try {
      const ok = await window.electronAPI.git.commit(targetPath, message);
      if (!ok) {
        toast.error("Falha ao commitar. Verifique o status do repositório.");
        throw new Error("Falha ao commitar");
      }

      await update(mission.id, { isCommitted: true });
      await refreshMissions();
      toast.success("Commit realizado");
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Erro desconhecido";
      toast.error(`Falha ao commitar: ${msg}`);
      throw e;
    }
  };

  const handlePushFromWall = async (mission: Mission) => {
    const targetPath = mission.worktreePath ?? project?.path;
    if (
      !targetPath ||
      typeof window === "undefined" ||
      !window.electronAPI?.git?.push
    ) {
      toast.error("Push indisponível");
      return;
    }

    setIsPushingMissionId(mission.id);
    try {
      const result = await window.electronAPI.git.push(targetPath);
      if (result.success) {
        await update(mission.id, { isPushed: true });
        await refreshMissions();
        toast.success("Push realizado");
      } else {
        toast.error(result.error ?? "Falha ao fazer push.");
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Erro desconhecido";
      toast.error(`Falha ao fazer push: ${msg}`);
    } finally {
      setIsPushingMissionId(null);
    }
  };

  const handleMergeWorktreeFromWall = async (mission: Mission) => {
    if (!mission.worktreePath || !window.electronAPI?.worktree?.mergeIntoMain)
      return;
    const confirmed = await confirmDialog({
      title: "Incorporar alterações no branch principal?",
      description:
        "O branch da missão será feito merge no branch principal (main/master) e o worktree será removido.",
      confirmLabel: "Incorporar",
      cancelLabel: "Cancelar",
    });
    if (!confirmed) return;
    setIsWorktreeActionMissionId(mission.id);
    try {
      const result = await window.electronAPI.worktree.mergeIntoMain(
        mission.id,
      );
      if (result?.success) {
        toast.success("Alterações incorporadas ao branch principal");
        await refreshMissions();
      } else {
        toast.error(result?.error ?? "Erro ao incorporar");
      }
    } catch (e) {
      toast.error(`Erro: ${e instanceof Error ? e.message : "desconhecido"}`);
    } finally {
      setIsWorktreeActionMissionId(null);
    }
  };

  const handleDiscardWorktreeFromWall = async (mission: Mission) => {
    if (!mission.worktreePath || !window.electronAPI?.worktree?.discard) return;
    const confirmed = await confirmDialog({
      title: "Descartar worktree?",
      description:
        "O worktree e o branch da missão serão removidos. As alterações não commitadas serão perdidas.",
      confirmLabel: "Descartar",
      cancelLabel: "Cancelar",
    });
    if (!confirmed) return;
    setIsWorktreeActionMissionId(mission.id);
    try {
      const result = await window.electronAPI.worktree.discard(mission.id);
      if (result?.success) {
        toast.success("Worktree descartado");
        await refreshMissions();
      } else {
        toast.error(result?.error ?? "Erro ao descartar");
      }
    } catch (e) {
      toast.error(`Erro: ${e instanceof Error ? e.message : "desconhecido"}`);
    } finally {
      setIsWorktreeActionMissionId(null);
    }
  };

  const finishMissionIfActive = async (
    missionId: string,
    result: "completed" | "failed",
    message?: string,
  ): Promise<boolean> => {
    const latestMission = await getById(missionId);
    if (!latestMission || latestMission.missionType !== "agents_cli")
      return false;
    if (
      FINAL_STATUSES.includes(
        latestMission.status as (typeof FINAL_STATUSES)[number],
      )
    ) {
      return false;
    }

    if (result === "completed") {
      await completeMission(missionId, message);
    } else {
      await failMission(missionId, message ?? "Agent finalizado com falha");
    }

    return true;
  };

  const handleAgentTerminalExit = async (mission: Mission, code: number) => {
    try {
      toast[code === 0 ? "success" : "error"](
        code === 0
          ? `Agent finalizado: ${mission.title}`
          : `Agent falhou: ${mission.title}`,
      );
    } catch (e) {
      const msg = e instanceof Error ? e.message : "erro desconhecido";
      toast.error(
        `Não foi possível sincronizar o término de "${mission.title}": ${msg}`,
      );
    } finally {
      await refreshMissions();
    }
  };

  const handleManualFinish = async (mission: Mission) => {
    const asCompleted = await confirmDialog({
      title: "Finalizar agente",
      description: `Como deseja encerrar "${mission.title}"? Clique em Concluir se o agent terminou bem.`,
      confirmLabel: "Concluir",
      cancelLabel: "Outras opções",
    });

    if (asCompleted) {
      setIsFinishingMissionId(mission.id);
      try {
        const didUpdate = await finishMissionIfActive(
          mission.id,
          "completed",
          "Finalizado manualmente pelo usuário",
        );
        if (didUpdate) {
          toast.success("Agent marcado como concluído");
          if (mission.worktreePath) setPostFinishMissionId(mission.id);
        } else {
          toast.info("A missão já estava finalizada.");
        }
      } catch (e) {
        const msg = e instanceof Error ? e.message : "erro desconhecido";
        toast.error(`Não foi possível finalizar: ${msg}`);
      } finally {
        setIsFinishingMissionId(null);
        await refreshMissions();
      }
      return;
    }

    const asFailed = await confirmDialog({
      title: "Finalizar agente",
      description: `Marcar "${mission.title}" como falha?`,
      confirmLabel: "Falha",
      cancelLabel: "Cancelar",
    });

    if (!asFailed) return;

    setIsFinishingMissionId(mission.id);
    try {
      const didUpdate = await finishMissionIfActive(
        mission.id,
        "failed",
        "Marcado manualmente como falha",
      );
      if (didUpdate) toast.success("Agent marcado como falha");
      else toast.info("A missão já estava finalizada.");
    } catch (e) {
      const msg = e instanceof Error ? e.message : "erro desconhecido";
      toast.error(`Não foi possível finalizar: ${msg}`);
    } finally {
      setIsFinishingMissionId(null);
      await refreshMissions();
    }
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
            : prev,
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
              : prev,
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
              : prev,
          );
          const message = e instanceof Error ? e.message : "erro desconhecido";
          toast.error(`Falha ao iniciar "${mission.title}": ${message}`);
        }
      }
    };

    try {
      await Promise.all(
        Array.from({ length: Math.min(maxWorkers, launchTargets.length) }, () =>
          worker(),
        ),
      );
      if (batchRunIdRef.current !== runId) return;
      await refreshMissions();
      setLastFailedMissionIds(failedMissionIds);
      if (failed === 0) {
        toast.success(`Lote iniciado com sucesso (${succeeded} tarefa(s))`);
      } else if (succeeded > 0) {
        toast.warning(
          `Lote parcial: ${succeeded} iniciadas, ${failed} falharam`,
        );
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
    if (
      !window.electronAPI?.worktree?.ensureForMission ||
      !window.electronAPI?.terminal?.getOrCreate
    )
      return;

    const maxWorkers = Math.max(1, Math.min(4, Number(batchConcurrency) || 1));
    const availableSlots =
      maxWorkers - running.length - autoLaunchInFlightRef.current.size;
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
        <p className="text-muted-foreground">
          Carregando tarefas de agentes...
        </p>
      </div>
    );
  }

  if (agentsView === "wall") {
    return (
      <div className="p-6 space-y-5">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-xl font-semibold">Agents Wall</h2>
            <p className="text-sm text-muted-foreground">
              Acompanhe execuções em tempo real. A fila continua disponível como
              apoio operacional.
            </p>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setAgentsView("wall")}
            >
              Wall
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setAgentsView("queue")}
            >
              Fila
            </Button>
          </div>
        </div>

        <Card>
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between gap-2">
              <CardTitle className="text-base">
                Novo agente (start direto)
              </CardTitle>
              {project?.path && (
                <div className="flex items-center gap-2 rounded-md border border-border bg-muted/30 px-3 py-1.5 text-sm shrink-0">
                  <GitBranch className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                  {projectBranchLoading ? (
                    <span className="text-muted-foreground text-xs">Carregando…</span>
                  ) : (
                    <span className="font-mono font-medium text-foreground">
                      {projectBranch ?? "—"}
                    </span>
                  )}
                </div>
              )}
            </div>
            <CardDescription>
              Digite a missão, escolha o agente e inicie. A tarefa é criada
              automaticamente.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            <Input
              placeholder="Ex.: refatorar checkout e adicionar testes de regressão"
              value={quickTitle}
              onChange={(e) => setQuickTitle(e.target.value)}
            />
            <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
              <div className="min-w-[220px] flex-1">
                <Select
                  value={quickProviderId}
                  onValueChange={setQuickProviderId}
                >
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
              <Button
                onClick={handleQuickCreate}
                disabled={isQuickCreating || cliProviders.length === 0}
              >
                {isQuickCreating ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <Play className="mr-2 h-4 w-4" />
                )}
                Iniciar agente
              </Button>
            </div>
          </CardContent>
        </Card>

        {wallMissions.length === 0 ? (
          <Empty className="mt-8">
            <Empty.Icon>
              <Terminal className="h-10 w-10" />
            </Empty.Icon>
            <Empty.Title>Sem sessões ativas</Empty.Title>
            <Empty.Description>
              Inicie uma tarefa para acompanhar a execução em tempo real.
            </Empty.Description>
          </Empty>
        ) : (
          <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
            {wallMissions.map((mission) => {
              const gitState = gitStateByMissionId[mission.id] ?? null;
              const session = mission.context?.agentSession ?? null;
              const sessionPreview = getSessionPreview(mission);
              const lastActivityLabel = formatRelativeIso(
                session?.lastActivityAt,
              );
              const isRunningMission = getAgentLane(mission) === "running";
              const isFinalMission = FINAL_STATUSES.includes(
                mission.status as (typeof FINAL_STATUSES)[number],
              );
              const canFinalizeFromWall = isRunningMission;
              const canCommitFromWall = Boolean(
                mission.worktreePath &&
                isFinalMission &&
                gitState &&
                (gitState.isDirty || gitState.changedFiles.length > 0),
              );
              const canPushFromWall = Boolean(
                mission.worktreePath &&
                isFinalMission &&
                gitState &&
                !gitState.isDirty &&
                (gitState.aheadCount > 0 || !gitState.hasUpstream),
              );
              const provider = mission.providerId
                ? (providerById.get(mission.providerId) ?? null)
                : null;
              const suggestedCommand = buildSuggestedCliCommand(provider);
              const prompt = buildMissionPrompt(mission);
              return (
                <Card
                  key={mission.id}
                  className="flex min-h-[360px] flex-col overflow-hidden border-primary/10 text-left shadow-sm"
                >
                  <CardHeader className="min-w-0 pb-2 text-left">
                    <div className="flex min-w-0 items-start justify-between gap-2">
                      <div className="min-w-0 flex-1">
                        <CardTitle className="line-clamp-2 max-w-full wrap-break-word text-sm">
                          {mission.title}
                        </CardTitle>
                        <CardDescription className="line-clamp-1 wrap-break-word">
                          {provider ? provider.name : "Sem agente"}
                        </CardDescription>
                      </div>
                      <Badge
                        variant={getStatusVariant(getQueueStatusLabel(mission))}
                        className="shrink-0"
                      >
                        {getQueueStatusLabel(mission)}
                      </Badge>
                    </div>
                    <div className="mt-2 flex flex-wrap gap-2 text-[11px] text-muted-foreground">
                      <span className="rounded-full border bg-muted/20 px-2 py-0.5">
                        {session?.status === "running"
                          ? "Sessão ao vivo"
                          : "Sessão pronta para retomar"}
                      </span>
                      {lastActivityLabel && (
                        <span className="rounded-full border bg-muted/20 px-2 py-0.5">
                          Última atividade {lastActivityLabel}
                        </span>
                      )}
                      <span className="rounded-full border bg-muted/20 px-2 py-0.5">
                        Em andamento ha{" "}
                        {formatWaitLabel(getQueueWaitMs(mission))}
                      </span>
                    </div>
                    <div className="mt-2">
                      <BranchAccessRow
                        branch={gitState?.branch ?? mission.worktreeBranch ?? null}
                        worktreePath={mission.worktreePath ?? null}
                      />
                    </div>
                    <AgentGitFlowStatus mission={mission} gitState={gitState} />
                  </CardHeader>
                  <CardContent className="flex min-h-0 flex-1 flex-col gap-3">
                    <div className="rounded-lg border bg-muted/10 p-3">
                      <p className="text-[11px] uppercase tracking-wide text-muted-foreground">
                        Objetivo
                      </p>
                      <p className="mt-1 line-clamp-3 text-sm text-foreground/90">
                        {mission.description}
                      </p>
                    </div>
                    {isRunningMission ? (
                      <div className="min-h-0 flex-1">
                        <EmbeddedTerminal
                          cwd={mission.worktreePath ?? project.path}
                          command={suggestedCommand}
                          args={prompt ? [prompt] : []}
                          onExit={(code) => {
                            void handleAgentTerminalExit(mission, code);
                          }}
                          title={mission.title}
                          missionId={mission.id}
                        />
                      </div>
                    ) : (
                      <div className="rounded-md border bg-muted/20 p-3 text-left text-sm text-muted-foreground whitespace-pre-wrap wrap-break-word">
                        Sessão pausada. Reabra a mesma branch para continuar do
                        ponto em que parou.
                      </div>
                    )}
                    {sessionPreview && (
                      <div className="rounded-lg border bg-background p-3">
                        <div className="mb-2 flex items-center justify-between gap-2">
                          <p className="text-[11px] uppercase tracking-wide text-muted-foreground">
                            Última saída capturada
                          </p>
                          {session?.outputLineCount ? (
                            <span className="text-[11px] text-muted-foreground">
                              {session.outputLineCount} linhas
                            </span>
                          ) : null}
                        </div>
                        <pre className="max-h-28 overflow-auto whitespace-pre-wrap text-xs text-muted-foreground">
                          {sessionPreview}
                        </pre>
                      </div>
                    )}
                    <div className="grid grid-cols-2 gap-2">
                      {canFinalizeFromWall && (
                        <Button
                          size="sm"
                          variant="secondary"
                          className="w-full justify-center"
                          onClick={() => void handleManualFinish(mission)}
                          disabled={isFinishingMissionId === mission.id}
                        >
                          {isFinishingMissionId === mission.id ? (
                            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                          ) : null}
                          Finalizar
                        </Button>
                      )}
                      {!isRunningMission && (
                        <Button
                          size="sm"
                          className="w-full justify-center"
                          onClick={async () => {
                            try {
                              await launchMissionSession(mission);
                              await refreshMissions();
                              toast.success(
                                `Agente iniciado: ${mission.title}`,
                              );
                            } catch (e) {
                              toast.error(
                                e instanceof Error
                                  ? e.message
                                  : `Falha ao iniciar ${mission.title}`,
                              );
                            }
                          }}
                        >
                          <Play className="mr-2 h-4 w-4" />
                          Iniciar
                        </Button>
                      )}
                      {!isFinalMission && canCommitFromWall && (
                        <Button
                          size="sm"
                          variant="secondary"
                          className="w-full justify-center"
                          onClick={() => openCommitDialog(mission)}
                        >
                          <GitCommit className="mr-2 h-4 w-4" />
                          Commitar
                        </Button>
                      )}
                      {!isFinalMission && canPushFromWall && (
                        <Button
                          size="sm"
                          variant="secondary"
                          className="w-full justify-center"
                          onClick={() => void handlePushFromWall(mission)}
                          disabled={isPushingMissionId === mission.id}
                        >
                          {isPushingMissionId === mission.id ? (
                            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                          ) : (
                            <Upload className="mr-2 h-4 w-4" />
                          )}
                          Push
                        </Button>
                      )}
                      {!isFinalMission && mission.worktreePath && isFinalMission && (
                        <Button
                          size="sm"
                          variant="outline"
                          className="w-full justify-center"
                          onClick={() =>
                            void handleMergeWorktreeFromWall(mission)
                          }
                          disabled={isWorktreeActionMissionId === mission.id}
                        >
                          {isWorktreeActionMissionId === mission.id ? (
                            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                          ) : (
                            <GitMerge className="mr-2 h-4 w-4" />
                          )}
                          Incorporar
                        </Button>
                      )}
                      {!isFinalMission && mission.worktreePath && isFinalMission && (
                        <Button
                          size="sm"
                          variant="outline"
                          className="w-full justify-center hover:bg-destructive/10 hover:text-destructive"
                          onClick={() =>
                            void handleDiscardWorktreeFromWall(mission)
                          }
                          disabled={isWorktreeActionMissionId === mission.id}
                        >
                          <Trash2 className="mr-2 h-4 w-4" />
                          Descartar
                        </Button>
                      )}
                      <Button
                        size="sm"
                        variant="outline"
                        className="w-full justify-center"
                        asChild
                      >
                        <Link to={`/project/${projectId}/task/${mission.id}`}>
                          Abrir detalhe
                        </Link>
                      </Button>
                      {!FINAL_STATUSES.includes(
                        mission.status as (typeof FINAL_STATUSES)[number],
                      ) && (
                        <Button
                          size="sm"
                          variant="ghost"
                          className="w-full justify-center"
                          onClick={async () => {
                            try {
                              await cancelMission(mission.id);
                              await refreshMissions();
                              toast.success("Tarefa arquivada");
                            } catch {
                              toast.error("Não foi possível arquivar a tarefa");
                            }
                          }}
                        >
                          Arquivar
                        </Button>
                      )}
                      <Button
                        size="sm"
                        variant="destructive"
                        className="w-full justify-center"
                        onClick={async () => {
                          const confirmed = await confirmDialog({
                            title: "Excluir tarefa de agente?",
                            description:
                              `Esta ação removerá a tarefa "${mission.title}" da lista. ` +
                              "Se houver terminal/worktree ativo, ele também será encerrado e descartado.",
                            confirmLabel: "Excluir",
                            cancelLabel: "Cancelar",
                          });
                          if (!confirmed) return;
                          try {
                            if (window.electronAPI?.terminal?.killByMissionId) {
                              await window.electronAPI.terminal.killByMissionId(
                                mission.id,
                              );
                            }
                            if (
                              mission.worktreePath &&
                              window.electronAPI?.worktree?.discard
                            ) {
                              await window.electronAPI.worktree.discard(
                                mission.id,
                              );
                            }
                            await removeMission(mission.id);
                            await refreshMissions();
                            toast.success("Tarefa excluída");
                          } catch {
                            toast.error("Não foi possível excluir a tarefa");
                          }
                        }}
                      >
                        Excluir
                      </Button>
                    </div>
                  </CardContent>
                </Card>
              );
            })}
          </div>
        )}
        {wallQueuedPreview.length > 0 && (
          <Card className="mt-6">
            <CardHeader className="pb-3">
              <CardTitle className="text-base">Próximas da fila</CardTitle>
              <CardDescription>
                Tarefas prontas para iniciar sem misturar com as sessões que já
                estão em andamento.
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
                {wallQueuedPreview.map((mission) => (
                  <div
                    key={mission.id}
                    className="rounded-lg border bg-background p-3"
                  >
                    <div className="mb-2 flex items-start justify-between gap-2">
                      <p className="line-clamp-1 text-sm font-medium">
                        {mission.title}
                      </p>
                      <Badge variant="outline">
                        {getMissionPriority(mission.id) === "high"
                          ? "Alta"
                          : "Fila"}
                      </Badge>
                    </div>
                    <p className="line-clamp-2 text-sm text-muted-foreground">
                      {mission.description}
                    </p>
                    <div className="mt-3 flex items-center gap-2">
                      <Button
                        size="sm"
                        onClick={async () => {
                          try {
                            await launchMissionSession(mission);
                            await refreshMissions();
                            toast.success(`Agente iniciado: ${mission.title}`);
                          } catch (e) {
                            toast.error(
                              e instanceof Error
                                ? e.message
                                : `Falha ao iniciar ${mission.title}`,
                            );
                          }
                        }}
                      >
                        <Play className="mr-2 h-4 w-4" />
                        Iniciar
                      </Button>
                      <Button asChild size="sm" variant="outline">
                        <Link to={`/project/${projectId}/task/${mission.id}`}>
                          Abrir
                        </Link>
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>
        )}
        {readyToReview.length > 0 && (
          <Card className="mt-6">
            <CardHeader className="pb-3">
              <CardTitle className="text-base">Prontas para revisão</CardTitle>
              <CardDescription>
                Quando a execução termina, a branch sai do mural e vem para esta
                etapa de revisão.
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="grid gap-4">
                {readyToReview.map((mission) => (
                  <ReviewMissionCard
                    key={mission.id}
                    mission={mission}
                    projectId={projectId}
                    provider={
                      mission.providerId
                        ? (providerById.get(mission.providerId) ?? null)
                        : null
                    }
                    gitState={gitStateByMissionId[mission.id] ?? null}
                    compareUrl={buildCompareUrl(
                      project?.gitRemoteUrl ?? null,
                      gitStateByMissionId[mission.id]?.defaultBranch ?? null,
                      gitStateByMissionId[mission.id]?.branch ??
                        mission.worktreeBranch ??
                        null,
                    )}
                    isExpanded={Boolean(reviewExpandedByMissionId[mission.id])}
                    diffState={reviewDiffsByMissionId[mission.id]}
                    isWorktreeAction={isWorktreeActionMissionId === mission.id}
                    onMerge={(targetMission) =>
                      void handleMergeWorktreeFromWall(targetMission)
                    }
                    onDiscard={(targetMission) =>
                      void handleDiscardWorktreeFromWall(targetMission)
                    }
                    onToggleExpanded={(targetMission) => {
                      setReviewExpandedByMissionId((prev) => {
                        const nextValue = !prev[targetMission.id];
                        const next = { ...prev, [targetMission.id]: nextValue };
                        if (
                          nextValue &&
                          !reviewDiffsByMissionId[targetMission.id]
                        ) {
                          void loadReviewDiffs(targetMission);
                        }
                        return next;
                      });
                    }}
                    onDuplicate={(baseMission) => {
                      setNewTaskInitial({
                        title: `${baseMission.title} (cópia)`,
                        description: baseMission.description,
                        preserveInstructions:
                          baseMission.preserveInstructions ?? "",
                      });
                      setNewTaskDialogOpen(true);
                    }}
                  />
                ))}
              </div>
            </CardContent>
          </Card>
        )}
        {recentMissions.length > 0 && (
          <Card className="mt-6">
            <CardHeader className="pb-3">
              <CardTitle className="text-base">Histórico</CardTitle>
              <CardDescription>
                Tarefas encerradas sem branch operacional pendente de revisão.
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
                {recentMissions.map((mission) => (
                  <div
                    key={mission.id}
                    className="rounded-lg border bg-background p-3"
                  >
                    <div className="mb-2 flex items-start justify-between gap-2">
                      <p className="line-clamp-1 text-sm font-medium">
                        {mission.title}
                      </p>
                      <Badge
                        variant={getStatusVariant(getQueueStatusLabel(mission))}
                      >
                        {getQueueStatusLabel(mission)}
                      </Badge>
                    </div>
                    <AgentMissionMeta
                      mission={mission}
                      provider={
                        mission.providerId
                          ? (providerById.get(mission.providerId) ?? null)
                          : null
                      }
                    />
                    <div className="mt-3 flex items-center gap-2">
                      <Button asChild size="sm" variant="outline">
                        <Link to={`/project/${projectId}/task/${mission.id}`}>
                          Reabrir
                        </Link>
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => {
                          setNewTaskInitial({
                            title: `${mission.title} (cópia)`,
                            description: mission.description,
                            preserveInstructions:
                              mission.preserveInstructions ?? "",
                          });
                          setNewTaskDialogOpen(true);
                        }}
                      >
                        Duplicar
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>
        )}
        <NewTaskDialog
          open={newTaskDialogOpen}
          onOpenChange={(open) => {
            setNewTaskDialogOpen(open);
            if (!open) setNewTaskInitial(null);
          }}
          projectId={projectId}
          projectPath={project?.path ?? undefined}
          initialTask={newTaskInitial ?? undefined}
        />
        <CommitDialog
          open={commitDialogOpen}
          onOpenChange={(open) => {
            setCommitDialogOpen(open);
            if (!open) {
              setCommitDialogMission(null);
              setCommitDialogStatus(null);
            }
          }}
          defaultMessage={
            commitDialogMission
              ? `DevCommandCenter: ${commitDialogMission.title}`
              : "DevCommandCenter:"
          }
          onCommit={handleCommitFromWall}
          projectPath={commitDialogMission?.worktreePath ?? project?.path ?? ""}
          status={commitDialogStatus}
          onPushComplete={async () => {
            if (!commitDialogMission) return;
            await update(commitDialogMission.id, { isPushed: true });
            await refreshMissions();
          }}
        />
        {(() => {
          const postFinishMission = postFinishMissionId
            ? (readyToReview.find((m) => m.id === postFinishMissionId) ??
              agentMissions.find((m) => m.id === postFinishMissionId) ??
              null)
            : null;
          return (
            <Dialog
              open={Boolean(postFinishMissionId)}
              onOpenChange={(open) => !open && setPostFinishMissionId(null)}
            >
              <DialogContent>
                <DialogHeader>
                  <DialogTitle>Próximo passo</DialogTitle>
                  <DialogDescription>
                    Tarefa concluída. Próximo passo: commitar as alterações no
                    card (botão Commitar) ou abrir a tarefa para Incorporar ao
                    main / Descartar worktree.
                  </DialogDescription>
                </DialogHeader>
                <DialogFooter className="gap-2 sm:gap-0">
                  {postFinishMission && (
                    <Button
                      size="sm"
                      onClick={() => {
                        openCommitDialog(postFinishMission);
                        setPostFinishMissionId(null);
                      }}
                    >
                      <GitCommit className="mr-2 h-4 w-4" />
                      Abrir diálogo de commit
                    </Button>
                  )}
                  {postFinishMissionId && projectId && (
                    <Button size="sm" variant="outline" asChild>
                      <Link
                        to={`/project/${projectId}/task/${postFinishMissionId}`}
                        onClick={() => setPostFinishMissionId(null)}
                      >
                        Ver detalhe da tarefa
                      </Link>
                    </Button>
                  )}
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => setPostFinishMissionId(null)}
                  >
                    Fechar
                  </Button>
                </DialogFooter>
              </DialogContent>
            </Dialog>
          );
        })()}
      </div>
    );
  }

  return (
    <div className="p-6">
      <div className="mb-4 flex justify-end gap-2">
        <Button
          variant="outline"
          size="sm"
          onClick={() => setAgentsView("wall")}
        >
          Wall
        </Button>
        <Button
          variant="secondary"
          size="sm"
          onClick={() => setAgentsView("queue")}
        >
          Fila
        </Button>
      </div>
      <div className="mb-6 grid gap-4 lg:grid-cols-[1.7fr_1fr]">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Terminal className="h-5 w-5 text-primary" />
              Nova tarefa de agente
            </CardTitle>
            <CardDescription className="space-y-1">
              <span className="block">
                Fluxo rápido para chegar ao terminal com contexto: 1 tarefa = 1
                agente = 1 branch.
              </span>
              <span className="block text-muted-foreground/90">
                Cada tarefa usa uma pasta separada (worktree). As alterações
                ficam lá até você commitar e incorporar ao main. No terminal do
                projeto (main) você não vê essas alterações.
              </span>
            </CardDescription>
            {project?.path && (
              <div className="flex items-center gap-2 rounded-md border border-border bg-muted/30 px-3 py-2 text-sm mt-2">
                <GitBranch className="h-4 w-4 shrink-0 text-muted-foreground" />
                {projectBranchLoading ? (
                  <span className="text-muted-foreground">Carregando branch…</span>
                ) : (
                  <span className="text-foreground">
                    Branch base do projeto:{" "}
                    <span className="font-mono font-medium">
                      {projectBranch ?? "—"}
                    </span>
                  </span>
                )}
              </div>
            )}
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
                <Select
                  value={quickProviderId}
                  onValueChange={setQuickProviderId}
                >
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
              <Button
                onClick={handleQuickCreate}
                disabled={isQuickCreating || cliProviders.length === 0}
              >
                {isQuickCreating ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <Plus className="mr-2 h-4 w-4" />
                )}
                Criar e abrir terminal
              </Button>
              <Button
                variant="outline"
                onClick={() => {
                  setNewTaskInitial(
                    quickTitle || quickDescription
                      ? { title: quickTitle, description: quickDescription }
                      : null,
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
            <CardDescription>
              Fluxo por contexto: agora, próximas execuções e resultados.
            </CardDescription>
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
              <span>Prontas para revisão</span>
              <Badge variant="outline">{readyToReview.length}</Badge>
            </div>
            <div className="flex items-center justify-between rounded-md border px-3 py-2">
              <span>Histórico</span>
              <Badge variant="outline">{history.length}</Badge>
            </div>
            <div className="flex items-center justify-between rounded-md border px-3 py-2">
              <span className="inline-flex items-center gap-1">
                <AlertTriangle className="h-3.5 w-3.5 text-amber-500" />
                SLA em risco
              </span>
              <Badge
                variant={criticalQueued.length > 0 ? "destructive" : "outline"}
              >
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
          <Empty.Description>
            Crie uma tarefa para abrir no terminal com Codex, Claude, Gemini ou
            Cursor.
          </Empty.Description>
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
              <CardDescription>
                A tarefa mais urgente para continuar imediatamente.
              </CardDescription>
            </CardHeader>
            <CardContent>
              {nowMission ? (
                <div className="space-y-4">
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-lg font-semibold">
                        {nowMission.title}
                      </p>
                      <p className="mt-1 whitespace-pre-wrap text-sm text-muted-foreground">
                        {nowMission.description}
                      </p>
                    </div>
                    <Badge
                      variant={getStatusVariant(
                        getQueueStatusLabel(nowMission),
                      )}
                    >
                      {getQueueStatusLabel(nowMission)}
                    </Badge>
                  </div>
                  <AgentMissionMeta
                    mission={nowMission}
                    provider={
                      nowMission.providerId
                        ? (providerById.get(nowMission.providerId) ?? null)
                        : null
                    }
                  />
                  <BranchAccessRow
                    branch={
                      gitStateByMissionId[nowMission.id]?.branch ??
                      nowMission.worktreeBranch ??
                      null
                    }
                    worktreePath={nowMission.worktreePath ?? null}
                  />
                  <AgentMissionActions
                    mission={nowMission}
                    projectId={projectId}
                    onDuplicate={(baseMission) => {
                      setNewTaskInitial({
                        title: `${baseMission.title} (cópia)`,
                        description: baseMission.description,
                        preserveInstructions:
                          baseMission.preserveInstructions ?? "",
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
              <CardDescription>
                Ordem sugerida para abrir no terminal em seguida.
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="mb-3 flex flex-wrap items-center gap-2 rounded-md border border-border bg-muted/20 p-2">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() =>
                    setSelectedMissionIds(
                      new Set(nextMissions.map((m) => m.id)),
                    )
                  }
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
                      for (const mission of selectedLaunchable)
                        next[mission.id] = "high";
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
                        if ((next[mission.id] ?? "normal") !== "high")
                          next[mission.id] = "high";
                      }
                      return next;
                    });
                    toast.success(
                      `Promovidas ${urgentQueued.length} tarefa(s) urgentes para prioridade alta`,
                    );
                  }}
                >
                  Promover urgentes ({urgentQueued.length})
                </Button>
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <span>Concorrência</span>
                  <Select
                    value={batchConcurrency}
                    onValueChange={setBatchConcurrency}
                  >
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
                  <Select
                    value={slaWarningHours}
                    onValueChange={setSlaWarningHours}
                  >
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
                  <Select
                    value={slaCriticalHours}
                    onValueChange={setSlaCriticalHours}
                  >
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
                  disabled={
                    lastFailedMissionIds.length === 0 || isBatchLaunching
                  }
                  onClick={() => {
                    const retryTargets = launchableMissions.filter((m) =>
                      lastFailedMissionIds.includes(m.id),
                    );
                    runBatchLaunch(retryTargets);
                  }}
                >
                  Retry falhas ({lastFailedMissionIds.length})
                </Button>
                {batchProgress && (
                  <span className="text-xs text-muted-foreground">
                    {batchProgress.started}/{batchProgress.total} iniciadas ·{" "}
                    {batchProgress.succeeded} ok · {batchProgress.failed} falhas
                    {batchCancelRequested && " · parando..."}
                  </span>
                )}
                {!batchProgress && isAutoRunEnabled && (
                  <span className="text-xs text-muted-foreground">
                    Auto-run mantendo ativas ate{" "}
                    {Math.max(1, Math.min(4, Number(batchConcurrency) || 1))}{" "}
                    tarefa(s)
                  </span>
                )}
                <span className="text-xs text-muted-foreground">
                  Ordem: prioridade alta &gt; media &gt; baixa
                </span>
                <span className="text-xs text-muted-foreground">
                  SLA: alerta em {normalizedSlaWarningHours}h, critico em{" "}
                  {normalizedSlaCriticalHours}h
                </span>
              </div>
              {nextMissions.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  Nenhuma tarefa aguardando na fila.
                </p>
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
                        onClick={() =>
                          navigate(`/project/${projectId}/task/${mission.id}`)
                        }
                      >
                        <div className="mb-1 flex items-center justify-between gap-2">
                          <span className="truncate text-sm font-medium">
                            {mission.title}
                          </span>
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
                                {getMissionUrgency(mission) === "critical"
                                  ? "Critico"
                                  : "Alerta"}
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
                        <p className="line-clamp-2 text-xs text-muted-foreground">
                          {mission.description}
                        </p>
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
              <CardTitle className="text-base">Prontas para revisão</CardTitle>
              <CardDescription>
                Branches concluídas aguardando revisão, publicação e decisão de
                integração.
              </CardDescription>
            </CardHeader>
            <CardContent>
              {readyToReview.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  Nenhuma branch aguardando revisão.
                </p>
              ) : (
                <div className="grid gap-4">
                  {readyToReview.map((mission) => (
                    <ReviewMissionCard
                      key={mission.id}
                      mission={mission}
                      projectId={projectId}
                      provider={
                        mission.providerId
                          ? (providerById.get(mission.providerId) ?? null)
                          : null
                      }
                      gitState={gitStateByMissionId[mission.id] ?? null}
                      compareUrl={buildCompareUrl(
                        project?.gitRemoteUrl ?? null,
                        gitStateByMissionId[mission.id]?.defaultBranch ?? null,
                        gitStateByMissionId[mission.id]?.branch ??
                          mission.worktreeBranch ??
                          null,
                      )}
                      isExpanded={Boolean(
                        reviewExpandedByMissionId[mission.id],
                      )}
                      diffState={reviewDiffsByMissionId[mission.id]}
                      isWorktreeAction={
                        isWorktreeActionMissionId === mission.id
                      }
                      onMerge={(targetMission) =>
                        void handleMergeWorktreeFromWall(targetMission)
                      }
                      onDiscard={(targetMission) =>
                        void handleDiscardWorktreeFromWall(targetMission)
                      }
                      onToggleExpanded={(targetMission) => {
                        setReviewExpandedByMissionId((prev) => {
                          const nextValue = !prev[targetMission.id];
                          const next = {
                            ...prev,
                            [targetMission.id]: nextValue,
                          };
                          if (
                            nextValue &&
                            !reviewDiffsByMissionId[targetMission.id]
                          ) {
                            void loadReviewDiffs(targetMission);
                          }
                          return next;
                        });
                      }}
                      onDuplicate={(baseMission) => {
                        setNewTaskInitial({
                          title: `${baseMission.title} (cópia)`,
                          description: baseMission.description,
                          preserveInstructions:
                            baseMission.preserveInstructions ?? "",
                        });
                        setNewTaskDialogOpen(true);
                      }}
                    />
                  ))}
                </div>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-base">Histórico</CardTitle>
              <CardDescription>
                Histórico curto para retomar contexto ou abrir uma nova
                ramificação.
              </CardDescription>
            </CardHeader>
            <CardContent>
              {recentMissions.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  Ainda não há tarefas no histórico.
                </p>
              ) : (
                <div className="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
                  {recentMissions.map((mission) => {
                    const statusLabel = getQueueStatusLabel(mission);
                    return (
                      <div
                        key={mission.id}
                        className="rounded-lg border bg-background p-3"
                      >
                        <div className="mb-2 flex items-start justify-between gap-2">
                          <p className="line-clamp-1 text-sm font-medium">
                            {mission.title}
                          </p>
                          <Badge variant={getStatusVariant(statusLabel)}>
                            {statusLabel}
                          </Badge>
                        </div>
                        <AgentMissionMeta
                          mission={mission}
                          provider={
                            mission.providerId
                              ? (providerById.get(mission.providerId) ?? null)
                              : null
                          }
                        />
                        <BranchAccessRow
                          branch={
                            gitStateByMissionId[mission.id]?.branch ??
                            mission.worktreeBranch ??
                            null
                          }
                          worktreePath={mission.worktreePath ?? null}
                        />
                        <div className="mt-3 flex items-center gap-2">
                          <Button asChild size="sm" variant="outline">
                            <Link
                              to={`/project/${projectId}/task/${mission.id}`}
                            >
                              Reabrir
                            </Link>
                          </Button>
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() => {
                              setNewTaskInitial({
                                title: `${mission.title} (cópia)`,
                                description: mission.description,
                                preserveInstructions:
                                  mission.preserveInstructions ?? "",
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
        projectPath={project?.path ?? undefined}
        initialTask={newTaskInitial ?? undefined}
      />
      <CommitDialog
        open={commitDialogOpen}
        onOpenChange={(open) => {
          setCommitDialogOpen(open);
          if (!open) {
            setCommitDialogMission(null);
            setCommitDialogStatus(null);
          }
        }}
        defaultMessage={
          commitDialogMission
            ? `DevCommandCenter: ${commitDialogMission.title}`
            : "DevCommandCenter:"
        }
        onCommit={handleCommitFromWall}
        projectPath={commitDialogMission?.worktreePath ?? project?.path ?? ""}
        status={commitDialogStatus}
        onPushComplete={async () => {
          if (!commitDialogMission) return;
          await update(commitDialogMission.id, { isPushed: true });
          await refreshMissions();
        }}
      />
      {(() => {
        const postFinishMission = postFinishMissionId
          ? (readyToReview.find((m) => m.id === postFinishMissionId) ??
            agentMissions.find((m) => m.id === postFinishMissionId) ??
            null)
          : null;
        return (
          <Dialog
            open={Boolean(postFinishMissionId)}
            onOpenChange={(open) => !open && setPostFinishMissionId(null)}
          >
            <DialogContent>
              <DialogHeader>
                <DialogTitle>Próximo passo</DialogTitle>
                <DialogDescription>
                  Tarefa concluída. Próximo passo: commitar as alterações no
                  card (botão Commitar) ou abrir a tarefa para Incorporar ao
                  main / Descartar worktree.
                </DialogDescription>
              </DialogHeader>
              <DialogFooter className="gap-2 sm:gap-0">
                {postFinishMission && (
                  <Button
                    size="sm"
                    onClick={() => {
                      openCommitDialog(postFinishMission);
                      setPostFinishMissionId(null);
                    }}
                  >
                    <GitCommit className="mr-2 h-4 w-4" />
                    Abrir diálogo de commit
                  </Button>
                )}
                {postFinishMissionId && projectId && (
                  <Button size="sm" variant="outline" asChild>
                    <Link
                      to={`/project/${projectId}/task/${postFinishMissionId}`}
                      onClick={() => setPostFinishMissionId(null)}
                    >
                      Ver detalhe da tarefa
                    </Link>
                  </Button>
                )}
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => setPostFinishMissionId(null)}
                >
                  Fechar
                </Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>
        );
      })()}
    </div>
  );
}
