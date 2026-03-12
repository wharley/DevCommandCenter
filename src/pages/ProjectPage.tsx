import React from "react";
import { useEffect, useState, useRef } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import {
  ArrowLeft,
  GitBranch,
  Folder,
  Clock,
  Plus,
  Rocket,
  CheckCircle2,
  AlertCircle,
  Loader2,
  MoreHorizontal,
  Trash2,
  Play,
  FileCode,
  X,
  Pencil,
  Lightbulb,
  LayoutList,
  LayoutGrid,
  Sparkles,
  Terminal,
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
import { Empty } from "@/components/ui/empty";
import { Separator } from "@/components/ui/separator";
import { NewMissionDialog } from "@/components/dialogs/new-mission-dialog";
import { NewTaskDialog } from "@/components/dialogs/new-task-dialog";
import type { InitialTaskForCreate } from "@/components/dialogs/new-task-dialog";
import { WorkflowChoiceDialog } from "@/components/dialogs/workflow-choice-dialog";
import { MissionTipsDialog } from "@/components/dialogs/mission-tips-dialog";
import { useConfirmDialog } from "@/components/providers/confirm-dialog-provider";
import { useProjects, useMissions, useProviders } from "@/hooks/use-data";
import { useIsMobile } from "@/hooks/use-mobile";
import { MissionProgressPipeline } from "@/components/mission-progress-pipeline";
import { MissionBoard } from "@/components/mission-board";
import { useAppStore } from "@/hooks/use-app-store";
import { formatDistanceToNow } from "date-fns";
import { toast } from "sonner";
import { ptBR } from "date-fns/locale";
import type { MissionStatus } from "@/lib/database/types";
import type { InitialMissionForEdit } from "@/components/dialogs/new-mission-dialog";

const statusConfig: Record<
  MissionStatus,
  {
    label: string;
    icon: React.ElementType;
    variant: "default" | "secondary" | "destructive" | "outline";
  }
> = {
  created: { label: "Criada", icon: FileCode, variant: "secondary" },
  planning: { label: "Planejando", icon: Loader2, variant: "default" },
  plan_generated: {
    label: "Plano pronto",
    icon: CheckCircle2,
    variant: "default",
  },
  generating_code: { label: "Gerando", icon: Loader2, variant: "default" },
  code_ready: {
    label: "Código pronto",
    icon: CheckCircle2,
    variant: "default",
  },
  applying: { label: "Aplicando", icon: Loader2, variant: "default" },
  completed: { label: "Concluída", icon: CheckCircle2, variant: "secondary" },
  failed: { label: "Falhou", icon: AlertCircle, variant: "destructive" },
  cancelled: { label: "Cancelada", icon: AlertCircle, variant: "outline" },
};

export default function ProjectPage() {
  const { id: projectId } = useParams<{ id: string }>();
  const navigate = useNavigate();

  const [dialogOpen, setDialogOpen] = useState(false);
  const [showWorkflowChoice, setShowWorkflowChoice] = useState(false);
  const [newTaskDialogOpen, setNewTaskDialogOpen] = useState(false);
  const [newTaskInitial, setNewTaskInitial] = useState<InitialTaskForCreate | null>(null);
  const [tipsDialogOpen, setTipsDialogOpen] = useState(false);
  const [editingMissionId, setEditingMissionId] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<"list" | "board">("board");
  const [missionFilter, setMissionFilter] = useState<"all" | "pipeline" | "agents_cli">("all");
  const [quickCreatePrompt, setQuickCreatePrompt] = useState("");
  const [quickCreateInitial, setQuickCreateInitial] =
    useState<InitialMissionForEdit | null>(null);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "n") {
        e.preventDefault();
        setShowWorkflowChoice(true);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  const { projects, update, isLoading: projectsLoading } = useProjects();
  const {
    missions,
    remove: removeMission,
    cancel: cancelMission,
    isLoading: missionsLoading,
  } = useMissions(projectId ?? undefined);
  const { providers } = useProviders();
  const setCurrentProject = useAppStore((s) => s.setCurrentProject);
  const { confirmDialog } = useConfirmDialog();
  const isMobile = useIsMobile();

  // Ref para evitar múltiplas atualizações de lastOpenedAt
  const hasUpdatedLastOpenedRef = useRef<string | null>(null);

  const project = projectId
    ? (projects.find((p) => p.id === projectId) ?? null)
    : null;
  const editingMission =
    editingMissionId != null
      ? missions.find((m) => m.id === editingMissionId) ?? null
      : null;
  const defaultProvider = project?.defaultProviderId
    ? (providers.find((p) => p.id === project.defaultProviderId) ?? null)
    : null;
  const isLoading = projectsLoading;

  // Sort missions: active first, then by date
  const sortedMissions = [...missions].sort((a, b) => {
    const activeStatuses = [
      "planning",
      "plan_generated",
      "generating_code",
      "code_ready",
      "applying",
    ];
    const aActive = activeStatuses.includes(a.status);
    const bActive = activeStatuses.includes(b.status);

    if (aActive && !bActive) return -1;
    if (!aActive && bActive) return 1;
    return b.updatedAt.getTime() - a.updatedAt.getTime();
  });

  const filteredMissions =
    missionFilter === "all"
      ? sortedMissions
      : missionFilter === "agents_cli"
        ? sortedMissions.filter((m) => m.missionType === "agents_cli")
        : sortedMissions.filter((m) => m.missionType !== "agents_cli");

  const agentsCliMissions = missions.filter((m) => m.missionType === "agents_cli");

  // Efeito para setar o projeto atual na store (apenas quando projectId muda)
  useEffect(() => {
    if (projectId) {
      setCurrentProject(projectId);
    }
    return () => {
      setCurrentProject(null);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId]);

  // Efeito separado para atualizar lastOpenedAt (só roda uma vez por projeto)
  useEffect(() => {
    if (projectId && hasUpdatedLastOpenedRef.current !== projectId) {
      hasUpdatedLastOpenedRef.current = projectId;
      update(projectId, { lastOpenedAt: new Date() });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId]);

  if (projectId && isLoading) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4">
        <Loader2 className="h-12 w-12 animate-spin text-muted-foreground" />
        <p className="text-muted-foreground">Carregando projeto...</p>
      </div>
    );
  }

  if (!project) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4">
        <AlertCircle className="h-12 w-12 text-muted-foreground" />
        <p className="text-muted-foreground">Projeto não encontrado</p>
        <Button variant="outline" onClick={() => navigate("/")}>
          <ArrowLeft className="mr-2 h-4 w-4" />
          Voltar aos projetos
        </Button>
      </div>
    );
  }

  const activeMissions = missions.filter(
    (m) => !["completed", "failed", "cancelled"].includes(m.status),
  ).length;

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <header className="border-b border-border bg-card px-6 py-4">
        <div className="flex items-center gap-4 mb-4">
          <Button variant="ghost" size="icon" className="cursor-pointer" onClick={() => navigate("/")}>
            <ArrowLeft className="h-4 w-4" />
          </Button>
          <Separator orientation="vertical" className="h-6" />
          <div className="flex-1">
            <h1 className="text-xl font-semibold text-card-foreground">
              {project.name}
            </h1>
            {project.description && (
              <p className="text-sm text-muted-foreground">
                {project.description}
              </p>
            )}
          </div>
          <Button variant="outline" onClick={() => setTipsDialogOpen(true)}>
            <Lightbulb className="mr-2 h-4 w-4" />
            Dicas
          </Button>
          <Button
            onClick={() => setShowWorkflowChoice(true)}
            title="Nova missão (⌘N)"
          >
            <Plus className="mr-2 h-4 w-4" />
            Nova missão
          </Button>
        </div>

        {/* Project Info */}
        <div className="flex flex-wrap items-center gap-4 text-sm text-muted-foreground">
          <div className="flex items-center gap-2">
            <Folder className="h-4 w-4" />
            <code className="rounded bg-muted px-2 py-0.5 text-xs">
              {project.path}
            </code>
          </div>
          {project.gitRemoteUrl && (
            <div className="flex items-center gap-2">
              <GitBranch className="h-4 w-4" />
              <span>main</span>
            </div>
          )}
          {defaultProvider && (
            <Badge variant="outline">{defaultProvider.name}</Badge>
          )}
          <div className="flex items-center gap-2">
            <Clock className="h-4 w-4" />
            <span>
              Aberto{" "}
              {formatDistanceToNow(project.lastOpenedAt ?? project.createdAt, {
                addSuffix: true,
                locale: ptBR,
              })}
            </span>
          </div>
        </div>
      </header>

      {/* Content */}
      <div className="flex-1 overflow-auto p-6">
        {/* Quick create */}
        <div className="mb-6 flex flex-col gap-2 sm:flex-row sm:items-center">
          <div className="relative flex-1">
            <Sparkles className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
            <input
              type="text"
              placeholder="O que você quer fazer? Descreva em uma frase e pressione Enter para criar a missão."
              className="w-full rounded-lg border border-input bg-muted/30 py-2.5 pl-10 pr-4 text-sm placeholder:text-muted-foreground focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary"
              value={quickCreatePrompt}
              onChange={(e) => setQuickCreatePrompt(e.target.value)}
              onKeyDown={(e) => {
                if (e.key !== "Enter" || !quickCreatePrompt.trim()) return;
                e.preventDefault();
                const text = quickCreatePrompt.trim();
                const firstLine = text.split(/\n/)[0] ?? text;
                const title =
                  firstLine.length > 80 ? `${firstLine.slice(0, 77)}...` : firstLine;
                setQuickCreateInitial({ title, description: text });
                setQuickCreatePrompt("");
                setShowWorkflowChoice(true);
              }}
              aria-label="Criar missão rapidamente"
            />
          </div>
          <Button
            variant="secondary"
            className="shrink-0"
            onClick={() => {
              if (!quickCreatePrompt.trim()) {
                setShowWorkflowChoice(true);
                return;
              }
              const text = quickCreatePrompt.trim();
              const firstLine = text.split(/\n/)[0] ?? text;
              const title =
                firstLine.length > 80 ? `${firstLine.slice(0, 77)}...` : firstLine;
              setQuickCreateInitial({ title, description: text });
              setQuickCreatePrompt("");
              setShowWorkflowChoice(true);
            }}
          >
            <Plus className="mr-2 h-4 w-4" />
            Nova missão
          </Button>
        </div>

        {/* Stats */}
        <div className="mb-6 grid gap-4 sm:grid-cols-2 md:grid-cols-4">
          <Card>
            <CardHeader className="pb-2">
              <CardDescription>Total de missões</CardDescription>
              <CardTitle className="text-3xl">{missions.length}</CardTitle>
            </CardHeader>
          </Card>
          <Card>
            <CardHeader className="pb-2">
              <CardDescription>Missões ativas</CardDescription>
              <CardTitle className="text-3xl text-primary">
                {activeMissions}
              </CardTitle>
            </CardHeader>
          </Card>
          <Card>
            <CardHeader className="pb-2">
              <CardDescription>Concluídas</CardDescription>
              <CardTitle className="text-3xl text-green-600">
                {missions.filter((m) => m.status === "completed").length}
              </CardTitle>
            </CardHeader>
          </Card>
          <Card>
            <CardHeader className="pb-2">
              <CardDescription>Tarefas (agentes)</CardDescription>
              <CardTitle className="text-3xl text-muted-foreground">
                {agentsCliMissions.length}
              </CardTitle>
            </CardHeader>
          </Card>
        </div>

        {/* Missions List / Board */}
        <div className="mb-4 flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
          <h2 className="text-lg font-semibold">
            {missionFilter === "agents_cli"
              ? "Tarefas (agentes)"
              : missionFilter === "pipeline"
                ? "Missões (pipeline)"
                : "Missões"}
          </h2>
          <div className="flex flex-wrap items-center gap-2">
            <div
              className="flex rounded-md border border-input bg-muted/30 p-0.5"
              role="tablist"
              aria-label="Filtro de tipo"
            >
              <Button
                variant={missionFilter === "all" ? "secondary" : "ghost"}
                size="sm"
                className="h-8 gap-1.5 px-2.5"
                onClick={() => setMissionFilter("all")}
                aria-pressed={missionFilter === "all"}
              >
                Todas
              </Button>
              <Button
                variant={missionFilter === "pipeline" ? "secondary" : "ghost"}
                size="sm"
                className="h-8 gap-1.5 px-2.5"
                onClick={() => setMissionFilter("pipeline")}
                aria-pressed={missionFilter === "pipeline"}
              >
                Pipeline
              </Button>
              <Button
                variant={missionFilter === "agents_cli" ? "secondary" : "ghost"}
                size="sm"
                className="h-8 gap-1.5 px-2.5"
                onClick={() => setMissionFilter("agents_cli")}
                aria-pressed={missionFilter === "agents_cli"}
              >
                <Terminal className="h-4 w-4" />
                Agentes
              </Button>
            </div>
            <div
              className="flex rounded-md border border-input bg-muted/30 p-0.5"
              role="tablist"
              aria-label="Visualização de missões"
            >
              <Button
                variant={viewMode === "list" ? "secondary" : "ghost"}
                size="sm"
                className="h-8 gap-1.5 px-2.5"
                onClick={() => setViewMode("list")}
                aria-pressed={viewMode === "list"}
              >
                <LayoutList className="h-4 w-4" />
                Lista
              </Button>
              <Button
                variant={viewMode === "board" ? "secondary" : "ghost"}
                size="sm"
                className="h-8 gap-1.5 px-2.5"
                onClick={() => setViewMode("board")}
                aria-pressed={viewMode === "board"}
              >
                <LayoutGrid className="h-4 w-4" />
                Board
              </Button>
            </div>
            <Badge variant="secondary">
              {filteredMissions.length} {missionFilter === "all" ? "no total" : "neste filtro"}
            </Badge>
            {missionFilter === "agents_cli" && (
              <Button
                size="sm"
                onClick={() => {
                  setNewTaskInitial(null);
                  setNewTaskDialogOpen(true);
                }}
              >
                <Plus className="mr-2 h-4 w-4" />
                Nova tarefa
              </Button>
            )}
          </div>
        </div>

        {filteredMissions.length === 0 ? (
          <Empty className="mt-12">
            <Empty.Icon>
              {missionFilter === "agents_cli" ? (
                <Terminal className="h-10 w-10" />
              ) : (
                <Rocket className="h-10 w-10" />
              )}
            </Empty.Icon>
            <Empty.Title>
              {missionFilter === "agents_cli"
                ? "Nenhuma tarefa (agente) ainda"
                : missionFilter === "pipeline"
                  ? "Nenhuma missão pipeline"
                  : "Nenhuma missão ainda"}
            </Empty.Title>
            <Empty.Description>
              {missionFilter === "agents_cli"
                ? "Crie uma tarefa para abrir no terminal com um agente (Codex, Claude, Cursor, etc.)."
                : "Crie sua primeira missão de código com IA para este projeto."}
            </Empty.Description>
            <Empty.Actions>
              {missionFilter === "agents_cli" ? (
                <Button
                  onClick={() => {
                    setNewTaskInitial(null);
                    setNewTaskDialogOpen(true);
                  }}
                >
                  <Plus className="mr-2 h-4 w-4" />
                  Nova tarefa
                </Button>
              ) : (
                <Button onClick={() => setShowWorkflowChoice(true)}>
                  <Plus className="mr-2 h-4 w-4" />
                  Nova missão
                </Button>
              )}
            </Empty.Actions>
          </Empty>
        ) : viewMode === "board" && !isMobile ? (
          <MissionBoard
            projectId={projectId ?? project.id}
            missions={filteredMissions}
            providers={providers}
            defaultProvider={defaultProvider}
            onRemove={removeMission}
            onCancel={async (missionId) => {
              try {
                await cancelMission(missionId);
                toast.success("Missão cancelada");
              } catch {
                toast.error("Não foi possível cancelar a missão.");
              }
            }}
            onEdit={setEditingMissionId}
            onOpenNewMission={() => setShowWorkflowChoice(true)}
            confirmDialog={confirmDialog}
          />
        ) : (
          <div className="space-y-3">
            {filteredMissions.map((mission) => {
              const statusInfo = statusConfig[mission.status];
              const StatusIcon = statusInfo.icon;
              const isActive = [
                "planning",
                "generating_code",
                "applying",
              ].includes(mission.status);
              const provider = mission.providerId
                ? (providers.find((p) => p.id === mission.providerId) ??
                  defaultProvider)
                : defaultProvider;

              return (
                <Card
                  key={mission.id}
                  className="group relative transition-shadow hover:shadow-md"
                >
                  <Link
                    to={
                      mission.missionType === "agents_cli"
                        ? `/project/${projectId}/task/${mission.id}`
                        : `/project/${projectId}/mission/${mission.id}`
                    }
                    className="absolute inset-0 z-10"
                  />

                  <CardHeader className="pb-2">
                    <div className="flex items-start justify-between">
                      <div className="flex items-start gap-3">
                        <div
                          className={`mt-0.5 flex h-8 w-8 items-center justify-center rounded-lg ${
                            mission.status === "completed"
                              ? "bg-green-500/10"
                              : mission.status === "failed"
                                ? "bg-destructive/10"
                                : "bg-primary/10"
                          }`}
                        >
                          <StatusIcon
                            className={`h-4 w-4 ${
                              isActive ? "animate-spin" : ""
                            } ${
                              mission.status === "completed"
                                ? "text-green-600"
                                : mission.status === "failed"
                                  ? "text-destructive"
                                  : "text-primary"
                            }`}
                          />
                        </div>
                        <div className="flex-1 min-w-0">
                          <CardTitle className="text-base">
                            {mission.title}
                          </CardTitle>
                          <CardDescription className="line-clamp-1 mt-1">
                            {mission.description}
                          </CardDescription>
                          <div className="mt-2">
                            {mission.missionType === "agents_cli" ? (
                              <span className="text-xs text-muted-foreground">
                                Tarefa · Abra no terminal para executar o agente
                              </span>
                            ) : (
                              <MissionProgressPipeline
                                status={mission.status}
                                missionType={mission.missionType ?? undefined}
                              />
                            )}
                          </div>
                        </div>
                      </div>

                      <div className="flex items-center gap-2 flex-wrap">
                        <Badge variant={statusInfo.variant}>
                          {statusInfo.label}
                        </Badge>
                        {mission.missionType === "agents_cli" && (
                          <Badge variant="outline" className="gap-1">
                            <Terminal className="h-3 w-3" />
                            Agente
                          </Badge>
                        )}
                        {mission.worktreePath && (
                          <Badge variant="secondary">Worktree ativo</Badge>
                        )}
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <Button
                              variant="ghost"
                              size="icon"
                              className="relative z-20 h-8 w-8 opacity-0 transition-opacity group-hover:opacity-100"
                            >
                              <MoreHorizontal className="h-4 w-4" />
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            <DropdownMenuItem asChild>
                              <Link
                                to={
                                  mission.missionType === "agents_cli"
                                    ? `/project/${projectId}/task/${mission.id}`
                                    : `/project/${projectId}/mission/${mission.id}`
                                }
                              >
                                <Play className="mr-2 h-4 w-4" />
                                {mission.missionType === "agents_cli"
                                  ? "Abrir tarefa"
                                  : "Abrir missão"}
                              </Link>
                            </DropdownMenuItem>
                            {!mission.plan &&
                              mission.status !== "planning" && (
                                <DropdownMenuItem
                                  onClick={(e) => {
                                    e.preventDefault();
                                    setEditingMissionId(mission.id);
                                  }}
                                >
                                  <Pencil className="mr-2 h-4 w-4" />
                                  Editar
                                </DropdownMenuItem>
                              )}
                            {[
                              "created",
                              "plan_generated",
                              "code_ready",
                              "completed",
                              "failed",
                            ].includes(mission.status) && (
                              <DropdownMenuItem
                                onClick={async (e) => {
                                  e.preventDefault();
                                  const confirmed = await confirmDialog({
                                    title: "Cancelar missão",
                                    description:
                                      "Tem certeza que deseja cancelar esta missão?",
                                  });
                                  if (!confirmed) return;
                                  try {
                                    await cancelMission(mission.id);
                                    toast.success("Missão cancelada");
                                  } catch {
                                    toast.error(
                                      "Não foi possível cancelar a missão.",
                                    );
                                  }
                                }}
                              >
                                <X className="mr-2 h-4 w-4" />
                                Cancelar missão
                              </DropdownMenuItem>
                            )}
                            <DropdownMenuSeparator />
                            <DropdownMenuItem
                              className="text-destructive focus:text-destructive"
                              onClick={(e) => {
                                e.preventDefault();
                                removeMission(mission.id);
                              }}
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
                    <div className="flex items-center gap-4 text-xs text-muted-foreground">
                      {provider && (
                        <span className="flex items-center gap-1">
                          Provedor: {provider.name}
                        </span>
                      )}
                      {mission.plan && (
                        <span>{mission.plan.steps.length} etapas</span>
                      )}
                      <span>
                        Atualizado{" "}
                        {formatDistanceToNow(mission.updatedAt, {
                          addSuffix: true,
                          locale: ptBR,
                        })}
                      </span>
                    </div>
                  </CardContent>
                </Card>
              );
            })}
          </div>
        )}
      </div>

      {/* Workflow choice: Pipeline vs Terminal (agentes) */}
      <WorkflowChoiceDialog
        open={showWorkflowChoice}
        onOpenChange={setShowWorkflowChoice}
        onSelect={(choice) => {
          if (choice === "pipeline") {
            setDialogOpen(true);
          } else {
            setNewTaskInitial(
              quickCreateInitial
                ? {
                    title: quickCreateInitial.title,
                    description: quickCreateInitial.description,
                    preserveInstructions: quickCreateInitial.preserveInstructions,
                  }
                : null,
            );
            setNewTaskDialogOpen(true);
            setQuickCreateInitial(null);
          }
        }}
      />

      {/* New Mission Dialog (Pipeline) */}
      {projectId && (
        <NewMissionDialog
          open={dialogOpen}
          onOpenChange={(open) => {
            setDialogOpen(open);
            if (!open) setQuickCreateInitial(null);
          }}
          projectId={projectId}
          defaultProviderId={project?.defaultProviderId ?? undefined}
          initialMission={quickCreateInitial ?? undefined}
          onOpenTips={() => setTipsDialogOpen(true)}
        />
      )}

      {/* New Task Dialog (Agents CLI) */}
      {projectId && (
        <NewTaskDialog
          open={newTaskDialogOpen}
          onOpenChange={(open) => {
            setNewTaskDialogOpen(open);
            if (!open) setNewTaskInitial(null);
          }}
          projectId={projectId}
          initialTask={newTaskInitial ?? undefined}
        />
      )}
      <MissionTipsDialog open={tipsDialogOpen} onOpenChange={setTipsDialogOpen} />
      {/* Edit Mission Dialog (when mission has no plan yet) */}
      {projectId && editingMission && (
        <NewMissionDialog
          open={editingMissionId !== null}
          onOpenChange={(open) => {
            if (!open) setEditingMissionId(null);
          }}
          projectId={projectId}
          defaultProviderId={project?.defaultProviderId ?? undefined}
          missionId={editingMission.id}
          initialMission={{
            title: editingMission.title,
            description: editingMission.description,
            preserveInstructions: editingMission.preserveInstructions ?? "",
            providerId: editingMission.providerId ?? undefined,
            planProviderId: editingMission.planProviderId ?? undefined,
            codeProviderId: editingMission.codeProviderId ?? undefined,
            missionType: editingMission.missionType ?? undefined,
          }}
          onOpenTips={() => setTipsDialogOpen(true)}
        />
      )}
    </div>
  );
}
