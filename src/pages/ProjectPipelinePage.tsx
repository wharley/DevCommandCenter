import React, { useEffect, useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import {
  AlertCircle,
  CheckCircle2,
  FileCode,
  LayoutGrid,
  LayoutList,
  Loader2,
  MoreHorizontal,
  Pencil,
  Play,
  Plus,
  Rocket,
  Trash2,
  X,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Empty } from "@/components/ui/empty";
import { NewMissionDialog } from "@/components/dialogs/new-mission-dialog";
import type { InitialMissionForEdit } from "@/components/dialogs/new-mission-dialog";
import { MissionTipsDialog } from "@/components/dialogs/mission-tips-dialog";
import { MissionBoard } from "@/components/mission-board";
import { MissionProgressPipeline } from "@/components/mission-progress-pipeline";
import { useConfirmDialog } from "@/components/providers/confirm-dialog-provider";
import { useMissions } from "@/hooks/use-data";
import { useIsMobile } from "@/hooks/use-mobile";
import { useProjectWorkspaceContext } from "@/src/pages/ProjectWorkspacePage";
import type { MissionStatus } from "@/lib/database/types";
import { formatDistanceToNow } from "date-fns";
import { ptBR } from "date-fns/locale";
import { toast } from "sonner";

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
  plan_generated: { label: "Plano pronto", icon: CheckCircle2, variant: "default" },
  generating_code: { label: "Gerando", icon: Loader2, variant: "default" },
  code_ready: { label: "Código pronto", icon: CheckCircle2, variant: "default" },
  applying: { label: "Aplicando", icon: Loader2, variant: "default" },
  completed: { label: "Concluída", icon: CheckCircle2, variant: "secondary" },
  failed: { label: "Falhou", icon: AlertCircle, variant: "destructive" },
  cancelled: { label: "Cancelada", icon: AlertCircle, variant: "outline" },
};

export default function ProjectPipelinePage() {
  const { projectId, project, providers, defaultProvider } = useProjectWorkspaceContext();
  const [searchParams, setSearchParams] = useSearchParams();

  const [dialogOpen, setDialogOpen] = useState(false);
  const [tipsDialogOpen, setTipsDialogOpen] = useState(false);
  const [editingMissionId, setEditingMissionId] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<"list" | "board">("board");
  const [quickCreatePrompt, setQuickCreatePrompt] = useState("");
  const [quickCreateInitial, setQuickCreateInitial] = useState<InitialMissionForEdit | null>(null);

  const { missions, remove: removeMission, cancel: cancelMission, isLoading } = useMissions(projectId);
  const { confirmDialog } = useConfirmDialog();
  const isMobile = useIsMobile();

  useEffect(() => {
    if (searchParams.get("new") !== "pipeline") return;
    setDialogOpen(true);
    const next = new URLSearchParams(searchParams);
    next.delete("new");
    setSearchParams(next, { replace: true });
  }, [searchParams, setSearchParams]);

  const pipelineMissions = useMemo(
    () => missions.filter((m) => m.missionType !== "agents_cli"),
    [missions]
  );

  const sortedMissions = useMemo(() => {
    const activeStatuses = ["planning", "plan_generated", "generating_code", "code_ready", "applying"];
    return [...pipelineMissions].sort((a, b) => {
      const aActive = activeStatuses.includes(a.status);
      const bActive = activeStatuses.includes(b.status);
      if (aActive && !bActive) return -1;
      if (!aActive && bActive) return 1;
      return b.updatedAt.getTime() - a.updatedAt.getTime();
    });
  }, [pipelineMissions]);

  const editingMission =
    editingMissionId != null ? sortedMissions.find((m) => m.id === editingMissionId) ?? null : null;

  const activeMissions = pipelineMissions.filter(
    (m) => !["completed", "failed", "cancelled"].includes(m.status)
  ).length;

  const analysisCount = pipelineMissions.filter((m) => m.missionType === "analysis").length;

  if (isLoading) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4">
        <Loader2 className="h-12 w-12 animate-spin text-muted-foreground" />
        <p className="text-muted-foreground">Carregando missões pipeline...</p>
      </div>
    );
  }

  return (
    <div className="p-6">
      <div className="mb-6 flex flex-col gap-2 sm:flex-row sm:items-center">
        <div className="relative flex-1">
          <input
            type="text"
            placeholder="Descreva em uma frase e pressione Enter para criar missão pipeline."
            className="w-full rounded-lg border border-input bg-muted/30 px-4 py-2.5 text-sm placeholder:text-muted-foreground focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary"
            value={quickCreatePrompt}
            onChange={(e) => setQuickCreatePrompt(e.target.value)}
            onKeyDown={(e) => {
              if (e.key !== "Enter" || !quickCreatePrompt.trim()) return;
              e.preventDefault();
              const text = quickCreatePrompt.trim();
              const firstLine = text.split(/\n/)[0] ?? text;
              const title = firstLine.length > 80 ? `${firstLine.slice(0, 77)}...` : firstLine;
              setQuickCreateInitial({ title, description: text });
              setQuickCreatePrompt("");
              setDialogOpen(true);
            }}
            aria-label="Criar missão pipeline rapidamente"
          />
        </div>
        <Button
          className="shrink-0"
          onClick={() => {
            if (!quickCreatePrompt.trim()) {
              setDialogOpen(true);
              return;
            }
            const text = quickCreatePrompt.trim();
            const firstLine = text.split(/\n/)[0] ?? text;
            const title = firstLine.length > 80 ? `${firstLine.slice(0, 77)}...` : firstLine;
            setQuickCreateInitial({ title, description: text });
            setQuickCreatePrompt("");
            setDialogOpen(true);
          }}
        >
          <Plus className="mr-2 h-4 w-4" />
          Nova missão pipeline
        </Button>
      </div>

      <div className="mb-6 grid gap-4 sm:grid-cols-2 md:grid-cols-4">
        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Total pipeline</CardDescription>
            <CardTitle className="text-3xl">{pipelineMissions.length}</CardTitle>
          </CardHeader>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Missões ativas</CardDescription>
            <CardTitle className="text-3xl text-primary">{activeMissions}</CardTitle>
          </CardHeader>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Concluídas</CardDescription>
            <CardTitle className="text-3xl text-green-600">
              {pipelineMissions.filter((m) => m.status === "completed").length}
            </CardTitle>
          </CardHeader>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Análise (apenas plano)</CardDescription>
            <CardTitle className="text-3xl text-muted-foreground">{analysisCount}</CardTitle>
          </CardHeader>
        </Card>
      </div>

      <div className="mb-4 flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <h2 className="text-lg font-semibold">Missões de Pipeline</h2>
        <div className="flex flex-wrap items-center gap-2">
          <div className="flex rounded-md border border-input bg-muted/30 p-0.5" role="tablist" aria-label="Visualização">
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
          <Badge variant="secondary">{sortedMissions.length} neste fluxo</Badge>
        </div>
      </div>

      {sortedMissions.length === 0 ? (
        <Empty className="mt-12">
          <Empty.Icon>
            <Rocket className="h-10 w-10" />
          </Empty.Icon>
          <Empty.Title>Nenhuma missão pipeline ainda</Empty.Title>
          <Empty.Description>
            Crie sua primeira missão de implementação ou análise para este projeto.
          </Empty.Description>
          <Empty.Actions>
            <Button onClick={() => setDialogOpen(true)}>
              <Plus className="mr-2 h-4 w-4" />
              Nova missão pipeline
            </Button>
          </Empty.Actions>
        </Empty>
      ) : viewMode === "board" && !isMobile ? (
        <MissionBoard
          projectId={projectId}
          missions={sortedMissions}
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
          onOpenNewMission={() => setDialogOpen(true)}
          confirmDialog={confirmDialog}
        />
      ) : (
        <div className="space-y-3">
          {sortedMissions.map((mission) => {
            const statusInfo = statusConfig[mission.status];
            const StatusIcon = statusInfo.icon;
            const isActive = ["planning", "generating_code", "applying"].includes(mission.status);
            const provider = mission.providerId
              ? providers.find((p) => p.id === mission.providerId) ?? defaultProvider
              : defaultProvider;

            return (
              <Card key={mission.id} className="group relative transition-shadow hover:shadow-md">
                <Link to={`/project/${projectId}/mission/${mission.id}`} className="absolute inset-0 z-10" />

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
                          className={`h-4 w-4 ${isActive ? "animate-spin" : ""} ${
                            mission.status === "completed"
                              ? "text-green-600"
                              : mission.status === "failed"
                                ? "text-destructive"
                                : "text-primary"
                          }`}
                        />
                      </div>
                      <div className="min-w-0 flex-1">
                        <CardTitle className="text-base">{mission.title}</CardTitle>
                        <CardDescription className="mt-1 line-clamp-1">{mission.description}</CardDescription>
                        <div className="mt-2">
                          <MissionProgressPipeline status={mission.status} missionType={mission.missionType ?? undefined} />
                        </div>
                      </div>
                    </div>

                    <div className="flex flex-wrap items-center gap-2">
                      <Badge variant={statusInfo.variant}>{statusInfo.label}</Badge>
                      {mission.worktreePath && <Badge variant="secondary">Worktree ativo</Badge>}
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
                            <Link to={`/project/${projectId}/mission/${mission.id}`}>
                              <Play className="mr-2 h-4 w-4" />
                              Abrir missão
                            </Link>
                          </DropdownMenuItem>
                          {!mission.plan && mission.status !== "planning" && (
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
                          {["created", "plan_generated", "code_ready", "completed", "failed"].includes(
                            mission.status
                          ) && (
                            <DropdownMenuItem
                              onClick={async (e) => {
                                e.preventDefault();
                                const confirmed = await confirmDialog({
                                  title: "Cancelar missão",
                                  description: "Tem certeza que deseja cancelar esta missão?",
                                });
                                if (!confirmed) return;
                                try {
                                  await cancelMission(mission.id);
                                  toast.success("Missão cancelada");
                                } catch {
                                  toast.error("Não foi possível cancelar a missão.");
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
                    {provider && <span className="flex items-center gap-1">Provedor: {provider.name}</span>}
                    {mission.plan && <span>{mission.plan.steps.length} etapas</span>}
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

      <NewMissionDialog
        open={dialogOpen}
        onOpenChange={(open) => {
          setDialogOpen(open);
          if (!open) setQuickCreateInitial(null);
        }}
        projectId={projectId}
        defaultProviderId={project.defaultProviderId ?? undefined}
        initialMission={quickCreateInitial ?? undefined}
        onOpenTips={() => setTipsDialogOpen(true)}
      />

      <MissionTipsDialog open={tipsDialogOpen} onOpenChange={setTipsDialogOpen} />

      {editingMission && (
        <NewMissionDialog
          open={editingMissionId !== null}
          onOpenChange={(open) => {
            if (!open) setEditingMissionId(null);
          }}
          projectId={projectId}
          defaultProviderId={project.defaultProviderId ?? undefined}
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
