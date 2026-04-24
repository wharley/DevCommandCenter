"use client";

import React, { useCallback, useEffect, useState } from "react";
import {
  Play,
  Square,
  RefreshCw,
  Cpu,
  MemoryStick,
  Clock,
  Activity,
  AlertCircle,
  CheckCircle2,
  Loader2,
  XCircle,
  Terminal as TerminalIcon,
} from "lucide-react";
import { formatDistanceToNow } from "date-fns";
import { ptBR } from "date-fns/locale";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import type { Project } from "@/lib/database/types";
import type { DaemonProcessStatus } from "@/types/app";

interface ProcessesPanelProps {
  project: Project;
  refreshInterval?: number;
  /** Classes no contentor (ex.: `p-0` na sidebar). */
  className?: string;
  /** Esconde o título grande; útil quando o painel está dentro de outra secção já titulada. */
  embedded?: boolean;
}

function getStatusColor(status: DaemonProcessStatus["status"]): string {
  switch (status) {
    case "running":
      return "bg-green-500/10 text-green-700 dark:text-green-400 border-green-500/20";
    case "starting":
    case "restarting":
      return "bg-blue-500/10 text-blue-700 dark:text-blue-400 border-blue-500/20";
    case "stopped":
      return "bg-gray-500/10 text-gray-700 dark:text-gray-400 border-gray-500/20";
    case "stopping":
      return "bg-yellow-500/10 text-yellow-700 dark:text-yellow-400 border-yellow-500/20";
    case "crashed":
    case "failed":
      return "bg-red-500/10 text-red-700 dark:text-red-400 border-red-500/20";
    default:
      return "bg-gray-500/10 text-gray-700 dark:text-gray-400 border-gray-500/20";
  }
}

function getStatusIcon(status: DaemonProcessStatus["status"]) {
  switch (status) {
    case "running":
      return <CheckCircle2 className="h-4 w-4" />;
    case "starting":
    case "restarting":
      return <Loader2 className="h-4 w-4 animate-spin" />;
    case "stopped":
      return <Square className="h-4 w-4" />;
    case "crashed":
    case "failed":
      return <XCircle className="h-4 w-4" />;
    default:
      return <AlertCircle className="h-4 w-4" />;
  }
}

function formatBytes(mb: number): string {
  if (mb < 1) return `${(mb * 1024).toFixed(0)} KB`;
  if (mb >= 1024) return `${(mb / 1024).toFixed(2)} GB`;
  return `${mb.toFixed(1)} MB`;
}

function ProcessCard({ process, onAction }: {
  process: DaemonProcessStatus;
  onAction: (processId: string, action: "start" | "stop" | "restart") => Promise<void>;
}) {
  const [loading, setLoading] = useState<string | null>(null);
  const isRunning = process.status === "running";
  const isTransitioning = ["starting", "stopping", "restarting"].includes(process.status);

  const handleAction = async (action: "start" | "stop" | "restart") => {
    setLoading(action);
    try {
      await onAction(process.processId, action);
    } finally {
      setLoading(null);
    }
  };

  return (
    <Card className="overflow-hidden">
      <CardHeader className="pb-3">
        <div className="flex items-start justify-between">
          <div className="space-y-1">
            <CardTitle className="text-base flex items-center gap-2">
              <TerminalIcon className="h-4 w-4" />
              {process.processName}
            </CardTitle>
            <CardDescription className="text-xs font-mono">
              {process.command}
            </CardDescription>
          </div>
          <Badge variant="outline" className={getStatusColor(process.status)}>
            <span className="flex items-center gap-1.5">
              {getStatusIcon(process.status)}
              {process.status}
            </span>
          </Badge>
        </div>
      </CardHeader>

      <CardContent className="space-y-3">
        {/* Métricas */}
        {isRunning && (
          <div className="grid grid-cols-2 gap-2 text-xs">
            <Tooltip>
              <TooltipTrigger asChild>
                <div className="flex items-center gap-2 p-2 rounded-md bg-muted">
                  <Cpu className="h-3.5 w-3.5 text-muted-foreground" />
                  <span className="font-medium">{process.cpuPercent.toFixed(1)}%</span>
                </div>
              </TooltipTrigger>
              <TooltipContent>
                <p>Uso de CPU</p>
              </TooltipContent>
            </Tooltip>

            <Tooltip>
              <TooltipTrigger asChild>
                <div className="flex items-center gap-2 p-2 rounded-md bg-muted">
                  <MemoryStick className="h-3.5 w-3.5 text-muted-foreground" />
                  <span className="font-medium">{formatBytes(process.memoryMb)}</span>
                </div>
              </TooltipTrigger>
              <TooltipContent>
                <p>Uso de Memória</p>
              </TooltipContent>
            </Tooltip>
          </div>
        )}

        {/* Info adicional */}
        <div className="space-y-1.5 text-xs text-muted-foreground">
          {process.pid && (
            <div className="flex items-center gap-2">
              <Activity className="h-3 w-3" />
              <span>PID: {process.pid}</span>
            </div>
          )}

          {process.restartCount > 0 && (
            <div className="flex items-center gap-2">
              <RefreshCw className="h-3 w-3" />
              <span>Reinicializado {process.restartCount}x</span>
              {process.autoRestart && (
                <Badge variant="outline" className="text-xs py-0">
                  auto-restart
                </Badge>
              )}
            </div>
          )}

          {process.startedAt && isRunning && (
            <div className="flex items-center gap-2">
              <Clock className="h-3 w-3" />
              <span>
                Rodando há{" "}
                {formatDistanceToNow(new Date(process.startedAt), {
                  locale: ptBR,
                  addSuffix: false,
                })}
              </span>
            </div>
          )}

          {process.lastError && (
            <div className="flex items-center gap-2 text-destructive">
              <AlertCircle className="h-3 w-3" />
              <span className="truncate">{process.lastError}</span>
            </div>
          )}
        </div>

        {/* Ações */}
        <div className="flex gap-2 pt-2">
          {!isRunning && (
            <Button
              size="sm"
              variant="default"
              className="flex-1"
              onClick={() => handleAction("start")}
              disabled={loading !== null || isTransitioning}
            >
              {loading === "start" ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Play className="h-3.5 w-3.5" />
              )}
              Iniciar
            </Button>
          )}

          {isRunning && (
            <>
              <Button
                size="sm"
                variant="outline"
                className="flex-1"
                onClick={() => handleAction("restart")}
                disabled={loading !== null}
              >
                {loading === "restart" ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <RefreshCw className="h-3.5 w-3.5" />
                )}
                Reiniciar
              </Button>

              <Button
                size="sm"
                variant="destructive"
                className="flex-1"
                onClick={() => handleAction("stop")}
                disabled={loading !== null}
              >
                {loading === "stop" ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Square className="h-3.5 w-3.5" />
                )}
                Parar
              </Button>
            </>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

export function ProcessesPanel({
  project,
  refreshInterval = 3000,
  className,
  embedded = false,
}: ProcessesPanelProps) {
  const [processes, setProcesses] = useState<DaemonProcessStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchProcesses = useCallback(async () => {
    if (!window.desktopAPI?.daemon) return;

    try {
      const result = await window.desktopAPI.daemon.listProcesses(project.id);
      setProcesses(result);
      setError(null);
    } catch (err) {
      console.error("Failed to fetch processes:", err);
      setError(err instanceof Error ? err.message : "Erro ao carregar processos");
    } finally {
      setLoading(false);
    }
  }, [project.id]);

  useEffect(() => {
    if (typeof window === "undefined") return;
    if (!window.desktopAPI?.daemon) {
      setLoading(false);
      setProcesses([]);
      return;
    }
    void fetchProcesses();
    const interval = setInterval(() => void fetchProcesses(), refreshInterval);
    return () => clearInterval(interval);
  }, [fetchProcesses, refreshInterval]);

  const handleAction = useCallback(
    async (processId: string, action: "start" | "stop" | "restart") => {
      if (!window.desktopAPI?.daemon) return;

      try {
        let result;
        switch (action) {
          case "start":
            result = await window.desktopAPI.daemon.startProcess(project.id, processId);
            break;
          case "stop":
            result = await window.desktopAPI.daemon.stopProcess(project.id, processId);
            break;
          case "restart":
            result = await window.desktopAPI.daemon.restartProcess(project.id, processId);
            break;
        }

        if (result.success) {
          toast.success(`Processo ${action === "start" ? "iniciado" : action === "stop" ? "parado" : "reiniciado"} com sucesso`);
          await fetchProcesses();
        } else {
          toast.error(result.error || "Falha ao executar ação");
        }
      } catch (err) {
        console.error(`Failed to ${action} process:`, err);
        toast.error(err instanceof Error ? err.message : `Erro ao ${action === "start" ? "iniciar" : action === "stop" ? "parar" : "reiniciar"} processo`);
      }
    },
    [project.id, fetchProcesses]
  );

  if (loading) {
    return (
      <div className="flex items-center justify-center p-8">
        <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center p-8 text-center">
        <AlertCircle className="h-8 w-8 text-destructive mb-2" />
        <p className="text-sm text-muted-foreground">{error}</p>
        <Button size="sm" variant="outline" className="mt-4" onClick={fetchProcesses}>
          <RefreshCw className="h-3.5 w-3.5 mr-2" />
          Tentar novamente
        </Button>
      </div>
    );
  }

  if (processes.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center p-8 text-center">
        <TerminalIcon className="h-8 w-8 text-muted-foreground mb-2" />
        <p className="text-sm text-muted-foreground">
          Nenhum processo configurado
        </p>
        <p className="text-xs text-muted-foreground mt-1">
          Adicione processos no arquivo .dcc.toml do projeto
        </p>
      </div>
    );
  }

  return (
    <div className={cn(embedded ? "space-y-2 p-0" : "space-y-3 p-4", className)}>
      {embedded ? (
        <div className="flex justify-end">
          <Button size="sm" variant="ghost" className="h-7 w-7 p-0" onClick={() => void fetchProcesses()}>
            <RefreshCw className="h-3.5 w-3.5" />
          </Button>
        </div>
      ) : (
        <div className="mb-2 flex items-center justify-between">
          <h3 className="text-sm font-medium">Processos Gerenciados ({processes.length})</h3>
          <Button size="sm" variant="ghost" onClick={() => void fetchProcesses()}>
            <RefreshCw className="h-3.5 w-3.5" />
          </Button>
        </div>
      )}

      <div className="grid gap-3">
        {processes.map((process) => (
          <ProcessCard
            key={process.processId}
            process={process}
            onAction={handleAction}
          />
        ))}
      </div>
    </div>
  );
}
