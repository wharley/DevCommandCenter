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
import { useProjects, useMissions, useProviders } from "@/hooks/use-data";
import { useAppStore } from "@/hooks/use-app-store";
import { formatDistanceToNow } from "date-fns";
import { toast } from "sonner";
import { ptBR } from "date-fns/locale";
import type { MissionStatus } from "@/lib/database/types";

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

  const { projects, update, isLoading: projectsLoading } = useProjects();
  const {
    missions,
    remove: removeMission,
    cancel: cancelMission,
    isLoading: missionsLoading,
  } = useMissions(projectId ?? undefined);
  const { providers } = useProviders();
  const setCurrentProject = useAppStore((s) => s.setCurrentProject);

  // Ref para evitar múltiplas atualizações de lastOpenedAt
  const hasUpdatedLastOpenedRef = useRef<string | null>(null);

  const project = projectId
    ? projects.find((p) => p.id === projectId) ?? null
    : null;
  const defaultProvider = project?.defaultProviderId
    ? providers.find((p) => p.id === project.defaultProviderId) ?? null
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
    (m) => !["completed", "failed", "cancelled"].includes(m.status)
  ).length;

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <header className="border-b border-border bg-card px-6 py-4">
        <div className="flex items-center gap-4 mb-4">
          <Button variant="ghost" size="icon" onClick={() => navigate("/")}>
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
          <Button onClick={() => setDialogOpen(true)}>
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
        {/* Stats */}
        <div className="mb-6 grid gap-4 sm:grid-cols-3">
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
        </div>

        {/* Missions List */}
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-lg font-semibold">Missões</h2>
          <Badge variant="secondary">{missions.length} no total</Badge>
        </div>

        {sortedMissions.length === 0 ? (
          <Empty className="mt-12">
            <Empty.Icon>
              <Rocket className="h-10 w-10" />
            </Empty.Icon>
            <Empty.Title>Nenhuma missão ainda</Empty.Title>
            <Empty.Description>
              Crie sua primeira missão de código com IA para este projeto.
            </Empty.Description>
            <Empty.Actions>
              <Button onClick={() => setDialogOpen(true)}>
                <Plus className="mr-2 h-4 w-4" />
                Nova missão
              </Button>
            </Empty.Actions>
          </Empty>
        ) : (
          <div className="space-y-3">
            {sortedMissions.map((mission) => {
              const statusInfo = statusConfig[mission.status];
              const StatusIcon = statusInfo.icon;
              const isActive = [
                "planning",
                "generating_code",
                "applying",
              ].includes(mission.status);
              const provider = mission.providerId
                ? providers.find((p) => p.id === mission.providerId) ??
                  defaultProvider
                : defaultProvider;

              return (
                <Card
                  key={mission.id}
                  className="group relative transition-shadow hover:shadow-md"
                >
                  <Link
                    to={`/project/${projectId}/mission/${mission.id}`}
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
                        <div>
                          <CardTitle className="text-base">
                            {mission.title}
                          </CardTitle>
                          <CardDescription className="line-clamp-1 mt-1">
                            {mission.description}
                          </CardDescription>
                        </div>
                      </div>

                      <div className="flex items-center gap-2">
                        <Badge variant={statusInfo.variant}>
                          {statusInfo.label}
                        </Badge>

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
                                to={`/project/${projectId}/mission/${mission.id}`}
                              >
                                <Play className="mr-2 h-4 w-4" />
                                Abrir missão
                              </Link>
                            </DropdownMenuItem>
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
                                  if (
                                    !window.confirm(
                                      "Tem certeza que deseja cancelar esta missão?"
                                    )
                                  )
                                    return;
                                  try {
                                    await cancelMission(mission.id);
                                    toast.success("Missão cancelada");
                                  } catch {
                                    toast.error(
                                      "Não foi possível cancelar a missão."
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

      {/* New Mission Dialog */}
      {projectId && (
        <NewMissionDialog
          open={dialogOpen}
          onOpenChange={setDialogOpen}
          projectId={projectId}
          defaultProviderId={project.defaultProviderId ?? undefined}
        />
      )}
    </div>
  );
}
