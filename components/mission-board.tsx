import React from "react";
import { Link } from "react-router-dom";
import {
  MoreHorizontal,
  Trash2,
  Play,
  Pencil,
  X,
  Loader2,
  AlertCircle,
  CheckCircle2,
  ClipboardList,
  FileText,
  Code2,
  Send,
  Plus,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  COLUMN_LABELS,
  COLUMN_ORDER,
  groupMissionsByColumn,
  type MissionColumnId,
} from "@/lib/mission-board";

const COLUMN_ICONS: Record<MissionColumnId, React.ComponentType<{ className?: string }>> = {
  todo: ClipboardList,
  plan: FileText,
  code: Code2,
  apply: Send,
  done: CheckCircle2,
};
import type { Mission, MissionStatus } from "@/lib/database/types";
import type { Provider } from "@/lib/database/types";
import { formatDistanceToNow } from "date-fns";
import { ptBR } from "date-fns/locale";

const statusConfig: Record<
  MissionStatus,
  {
    label: string;
    variant: "default" | "secondary" | "destructive" | "outline";
    icon: React.ComponentType<{ className?: string }>;
    /** Para coluna Concluído: estilo igual à lista (ícone + cor) */
    cardClass?: string;
    iconClass?: string;
  }
> = {
  created: { label: "Criada", variant: "secondary", icon: ClipboardList },
  planning: { label: "Gerando…", variant: "default", icon: Loader2 },
  plan_generated: { label: "Pronto", variant: "default", icon: CheckCircle2 },
  generating_code: { label: "Gerando…", variant: "default", icon: Loader2 },
  code_ready: { label: "Pronto", variant: "default", icon: CheckCircle2 },
  applying: { label: "Aplicando…", variant: "default", icon: Loader2 },
  completed: {
    label: "Concluída",
    variant: "secondary",
    icon: CheckCircle2,
    cardClass: "bg-green-500/10",
    iconClass: "text-green-600 dark:text-green-500",
  },
  failed: {
    label: "Falhou",
    variant: "destructive",
    icon: AlertCircle,
    cardClass: "bg-destructive/10",
    iconClass: "text-destructive",
  },
  cancelled: {
    label: "Cancelada",
    variant: "outline",
    icon: AlertCircle,
    cardClass: "bg-muted",
    iconClass: "text-muted-foreground",
  },
};

export interface MissionBoardProps {
  projectId: string;
  missions: Mission[];
  providers: Provider[];
  defaultProvider: Provider | null;
  onRemove: (missionId: string) => void;
  onCancel: (missionId: string) => Promise<void>;
  onEdit: (missionId: string) => void;
  onOpenNewMission?: () => void;
  confirmDialog: (options: {
    title: string;
    description: string;
  }) => Promise<boolean>;
}

export function MissionBoard({
  projectId,
  missions,
  providers,
  defaultProvider,
  onRemove,
  onCancel,
  onEdit,
  onOpenNewMission,
  confirmDialog,
}: MissionBoardProps) {
  const missionsByColumn = React.useMemo(
    () => groupMissionsByColumn(missions),
    [missions],
  );

  return (
    <div
      className="flex gap-4 overflow-x-auto pb-2 md:min-w-0"
      role="region"
      aria-label="Board de missões por estágio"
    >
      {COLUMN_ORDER.map((columnId) => (
        <MissionColumn
          key={columnId}
          columnId={columnId}
          title={COLUMN_LABELS[columnId]}
          icon={COLUMN_ICONS[columnId]}
          missions={missionsByColumn[columnId]}
          projectId={projectId}
          providers={providers}
          defaultProvider={defaultProvider}
          onRemove={onRemove}
          onCancel={onCancel}
          onEdit={onEdit}
          onOpenNewMission={columnId === "todo" ? onOpenNewMission : undefined}
          confirmDialog={confirmDialog}
        />
      ))}
    </div>
  );
}

interface MissionColumnProps {
  columnId: MissionColumnId;
  title: string;
  icon: React.ComponentType<{ className?: string }>;
  missions: Mission[];
  projectId: string;
  providers: Provider[];
  defaultProvider: Provider | null;
  onRemove: (missionId: string) => void;
  onCancel: (missionId: string) => Promise<void>;
  onEdit: (missionId: string) => void;
  onOpenNewMission?: () => void;
  confirmDialog: (options: {
    title: string;
    description: string;
  }) => Promise<boolean>;
}

function MissionColumn({
  columnId,
  title,
  icon: Icon,
  missions,
  projectId,
  providers,
  defaultProvider,
  onRemove,
  onCancel,
  onEdit,
  onOpenNewMission,
  confirmDialog,
}: MissionColumnProps) {
  return (
    <div
      className="flex w-64 shrink-0 flex-col overflow-hidden rounded-lg border bg-muted/30"
      role="group"
      aria-label={`${title}, ${missions.length} missões`}
    >
      <div className="flex items-center justify-between gap-2 border-b px-3 py-2">
        <span className="flex min-w-0 items-center gap-2 truncate text-sm font-medium">
          <Icon className="h-4 w-4 shrink-0 text-muted-foreground" />
          {title}
        </span>
        <div className="flex shrink-0 items-center gap-1">
          {onOpenNewMission && (
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                onOpenNewMission();
              }}
              aria-label="Nova missão"
              title="Nova missão"
            >
              <Plus className="h-4 w-4" />
            </Button>
          )}
          <Badge variant="secondary" className="text-xs">
            {missions.length}
          </Badge>
        </div>
      </div>
      <ScrollArea className="max-h-[calc(100vh-20rem)] flex-1">
        <div className="space-y-2 p-2">
          {missions.map((mission) => (
            <MissionBoardCard
              key={mission.id}
              mission={mission}
              projectId={projectId}
              providers={providers}
              defaultProvider={defaultProvider}
              showStatusBadge={columnId === "done"}
              onRemove={onRemove}
              onCancel={onCancel}
              onEdit={onEdit}
              confirmDialog={confirmDialog}
            />
          ))}
        </div>
      </ScrollArea>
    </div>
  );
}

interface MissionBoardCardProps {
  mission: Mission;
  projectId: string;
  providers: Provider[];
  defaultProvider: Provider | null;
  showStatusBadge: boolean;
  onRemove: (missionId: string) => void;
  onCancel: (missionId: string) => Promise<void>;
  onEdit: (missionId: string) => void;
  confirmDialog: (options: {
    title: string;
    description: string;
  }) => Promise<boolean>;
}

function MissionBoardCard({
  mission,
  projectId,
  providers,
  defaultProvider,
  showStatusBadge,
  onRemove,
  onCancel,
  onEdit,
  confirmDialog,
}: MissionBoardCardProps) {
  const statusInfo = statusConfig[mission.status];
  const isActive = ["planning", "generating_code", "applying"].includes(
    mission.status,
  );
  const provider = mission.providerId
    ? providers.find((p) => p.id === mission.providerId) ?? defaultProvider
    : defaultProvider;

  return (
    <Card className="group relative min-w-0 overflow-hidden transition-shadow hover:shadow-md">
      <Link
        to={`/project/${projectId}/mission/${mission.id}`}
        className="absolute inset-0 z-10"
      />
      <CardHeader className="p-3 pb-1">
        <div className="flex items-start justify-between gap-1">
          <div className="min-w-0 flex-1 overflow-hidden">
            <p className="truncate text-sm font-medium">{mission.title}</p>
            <p className="mt-0.5 line-clamp-2 text-xs text-muted-foreground wrap-break-word">
              {mission.description}
            </p>
          </div>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="relative z-20 h-7 w-7 shrink-0 opacity-0 transition-opacity group-hover:opacity-100"
              >
                <MoreHorizontal className="h-3.5 w-3.5" />
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
                    onEdit(mission.id);
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
                    if (confirmed) await onCancel(mission.id);
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
                  onRemove(mission.id);
                }}
              >
                <Trash2 className="mr-2 h-4 w-4" />
                Excluir
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </CardHeader>
      <CardContent className="min-w-0 overflow-hidden p-3 pt-0">
        <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 overflow-hidden text-xs text-muted-foreground">
          {isActive && (
            <span className="flex shrink-0 items-center gap-1 text-primary">
              <Loader2 className="h-3 w-3 animate-spin" />
              {statusInfo.label}
            </span>
          )}
          {showStatusBadge && !isActive && (() => {
            const StatusIcon = statusInfo.icon;
            const cardClass = statusInfo.cardClass ?? "bg-primary/10";
            const iconClass = statusInfo.iconClass ?? "text-primary";
            return (
              <span
                className={`inline-flex shrink-0 items-center gap-1 rounded-md px-1.5 py-0.5 ${cardClass}`}
              >
                <StatusIcon className={`h-3 w-3 ${iconClass}`} />
                <span className={iconClass}>{statusInfo.label}</span>
              </span>
            );
          })()}
          {provider && (
            <span className="min-w-0 truncate">Provedor: {provider.name}</span>
          )}
          <span className="shrink-0">
            {formatDistanceToNow(mission.updatedAt, {
              addSuffix: true,
              locale: ptBR,
            })}
          </span>
        </div>
      </CardContent>
    </Card>
  );
}
