"use client";

import React, { useEffect, useRef, useState, useMemo } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import { ArrowLeft, Loader2, Terminal, MessageSquare, Clock } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useProjects, useMissions, useProviders, useMissionLogs } from "@/hooks/use-data";
import { EmbeddedTerminal } from "@/components/embedded-terminal";
import { toast } from "sonner";
import { format } from "date-fns";
import type { MissionLogType } from "@/lib/database/types";

const logTypeConfig: Record<
  MissionLogType,
  { icon: React.ElementType; color: string }
> = {
  info: { icon: MessageSquare, color: "text-muted-foreground" },
  prompt: { icon: MessageSquare, color: "text-blue-500" },
  response: { icon: MessageSquare, color: "text-primary" },
  error: { icon: MessageSquare, color: "text-destructive" },
  action: { icon: MessageSquare, color: "text-green-500" },
  user_input: { icon: MessageSquare, color: "text-amber-500" },
  warning: { icon: MessageSquare, color: "text-amber-500" },
  debug: { icon: MessageSquare, color: "text-muted-foreground" },
};

export default function TaskPage() {
  const { id: projectId, missionId } = useParams<{ id: string; missionId: string }>();
  const navigate = useNavigate();
  const [activeTab, setActiveTab] = useState("terminal");
  const [embeddedTerminalCwd, setEmbeddedTerminalCwd] = useState<string | null>(null);
  const [embeddedTerminalCommand, setEmbeddedTerminalCommand] = useState<string | undefined>(undefined);
  const [embeddedTerminalArgs, setEmbeddedTerminalArgs] = useState<string[]>([]);
  const [isOpeningTerminal, setIsOpeningTerminal] = useState(false);
  const hasAutoOpenedRef = useRef<string | null>(null);

  const { projects, isLoading: projectsLoading } = useProjects();
  const { missions, refresh: refreshMissions, isLoading: missionsLoading } = useMissions(projectId ?? undefined);
  const { providers } = useProviders();
  const { logs } = useMissionLogs(missionId ?? "");

  const project = useMemo(
    () => (projectId ? projects.find((p) => p.id === projectId) ?? null : null),
    [projectId, projects]
  );
  const mission = useMemo(
    () => (missionId ? missions.find((m) => m.id === missionId) ?? null : null),
    [missionId, missions]
  );

  const suggestedCliCommand = useMemo(() => {
    const prov = mission?.providerId
      ? providers.find((p) => p.id === mission.providerId)
      : null;
    if (!prov) return undefined;
    const t = prov.type;
    const cliPath = prov.cliPath?.trim();
    const usePath =
      cliPath && (cliPath.startsWith("/") || /^[A-Za-z]:\\/.test(cliPath));
    if (t === "codex") return usePath ? cliPath : "codex";
    if (t === "claude-code") return usePath ? cliPath : "claude";
    if (t === "gemini") return usePath ? cliPath : "gemini";
    if (t === "cursor") return usePath ? cliPath : "cursor-agent";
    return undefined;
  }, [providers, mission?.providerId]);

  const missionPrompt = useMemo(() => {
    if (!mission) return "";
    const parts = [mission.title, mission.description];
    if (mission.preserveInstructions?.trim()) {
      parts.push(`Não alterar: ${mission.preserveInstructions.trim()}`);
    }
    return parts.join("\n\n");
  }, [mission?.title, mission?.description, mission?.preserveInstructions]);

  const openTerminal = async () => {
    if (
      !missionId ||
      !project?.path ||
      !window.electronAPI?.worktree?.ensureForMission ||
      !window.electronAPI?.terminal?.getOrCreate
    )
      return;
    setIsOpeningTerminal(true);
    try {
      const ensure = await window.electronAPI?.worktree?.ensureForMission(missionId);
      if (!ensure || !ensure.success) {
        toast.error(ensure?.error ?? "Não foi possível criar/obter worktree");
        return;
      }
      const pathToOpen = ensure.worktreePath ?? project.path;
      setEmbeddedTerminalCwd(pathToOpen);
      setEmbeddedTerminalCommand(suggestedCliCommand ?? undefined);
      setEmbeddedTerminalArgs(missionPrompt ? [missionPrompt] : []);
      setActiveTab("terminal");
      if (ensure.worktreePath) refreshMissions();
    } catch (e) {
      toast.error(`Erro: ${e instanceof Error ? e.message : "desconhecido"}`);
    } finally {
      setIsOpeningTerminal(false);
    }
  };

  useEffect(() => {
    if (
      !missionId ||
      !project?.path ||
      !mission ||
      mission.missionType !== "agents_cli" ||
      hasAutoOpenedRef.current === missionId
    )
      return;
    if (!window.electronAPI?.worktree?.ensureForMission || !window.electronAPI?.terminal?.getOrCreate)
      return;
    hasAutoOpenedRef.current = missionId;
    let cancelled = false;
    (async () => {
      try {
        const ensure = await window.electronAPI?.worktree?.ensureForMission(missionId);
        if (cancelled) return;
        if (!ensure?.success) return;
        const pathToOpen = ensure.worktreePath ?? project.path;
        setEmbeddedTerminalCwd(pathToOpen);
        setEmbeddedTerminalCommand(suggestedCliCommand ?? undefined);
        setEmbeddedTerminalArgs(missionPrompt ? [missionPrompt] : []);
        setActiveTab("terminal");
        if (ensure.worktreePath) refreshMissions();
      } catch {
        if (!cancelled) hasAutoOpenedRef.current = null;
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [missionId, project?.path, mission?.missionType, suggestedCliCommand, missionPrompt, refreshMissions]);

  const isLoading = (projectId && projectsLoading) || (missionId && missionsLoading);

  if (isLoading) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4">
        <Loader2 className="h-12 w-12 animate-spin text-muted-foreground" />
        <p className="text-muted-foreground">Carregando tarefa...</p>
      </div>
    );
  }

  if (!mission || !project) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4">
        <p className="text-muted-foreground">Tarefa não encontrada</p>
        <Button variant="outline" onClick={() => navigate(`/project/${projectId}/agents`)}>
          <ArrowLeft className="mr-2 h-4 w-4" />
          Voltar ao projeto
        </Button>
      </div>
    );
  }

  if (mission.missionType !== "agents_cli") {
    navigate(`/project/${projectId}/mission/${missionId}`, { replace: true });
    return null;
  }

  return (
    <div className="flex h-full flex-col">
      <header className="shrink-0 border-b border-border px-6 py-4">
        <div className="flex items-center gap-3">
          <Button variant="ghost" size="icon" asChild>
            <Link to={`/project/${projectId}/agents`}>
              <ArrowLeft className="h-4 w-4" />
            </Link>
          </Button>
          <div className="min-w-0 flex-1">
            <h1 className="truncate text-lg font-semibold">{mission.title}</h1>
            {mission.description && (
              <p className="mt-0.5 line-clamp-2 text-sm text-muted-foreground">
                {mission.description}
              </p>
            )}
          </div>
        </div>
      </header>

      <div className="flex-1 overflow-hidden">
        <Tabs value={activeTab} onValueChange={setActiveTab} className="h-full flex flex-col">
          <div className="border-b border-border px-6">
            <TabsList className="h-12">
              <TabsTrigger value="terminal" className="gap-2">
                <Terminal className="h-4 w-4" />
                Terminal
              </TabsTrigger>
              <TabsTrigger value="logs" className="gap-2">
                <MessageSquare className="h-4 w-4" />
                Logs
                {logs.length > 0 && (
                  <span className="ml-1 rounded bg-muted px-1.5 py-0.5 text-xs">
                    {logs.length}
                  </span>
                )}
              </TabsTrigger>
            </TabsList>
          </div>

          <TabsContent
            value="terminal"
            className="flex-1 flex flex-col overflow-hidden p-0 mt-0 data-[state=active]:flex data-[state=inactive]:hidden"
          >
            {embeddedTerminalCwd ? (
              <div className="flex-1 min-h-0 p-4">
                <EmbeddedTerminal
                  cwd={embeddedTerminalCwd}
                  command={embeddedTerminalCommand}
                  args={embeddedTerminalArgs}
                  onClose={() => setEmbeddedTerminalCwd(null)}
                  title={mission.title}
                  missionId={missionId}
                />
              </div>
            ) : (
              <div className="flex flex-1 flex-col items-center justify-center gap-4 p-6 text-center">
                <Terminal className="h-12 w-12 text-muted-foreground" />
                <p className="text-sm text-muted-foreground">
                  Terminal da tarefa no diretório da worktree. Clique para iniciar.
                </p>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={openTerminal}
                  disabled={isOpeningTerminal}
                >
                  {isOpeningTerminal ? (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  ) : (
                    <Terminal className="mr-2 h-4 w-4" />
                  )}
                  Abrir no terminal
                </Button>
              </div>
            )}
          </TabsContent>

          <TabsContent
            value="logs"
            className="flex-1 overflow-auto p-6 mt-0 data-[state=active]:block data-[state=inactive]:hidden"
          >
            <div className="space-y-2">
              {logs.length === 0 ? (
                <p className="text-sm text-muted-foreground">Nenhum log ainda.</p>
              ) : (
                [...logs]
                  .sort((a, b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime())
                  .map((log) => {
                    const config = logTypeConfig[log.type] ?? logTypeConfig.info;
                    const Icon = config.icon;
                    return (
                      <div
                        key={log.id}
                        className="flex gap-3 rounded-lg border border-border bg-card p-3"
                      >
                        <Icon className={`h-5 w-5 shrink-0 ${config.color}`} />
                        <div className="min-w-0 flex-1">
                          <p className="text-sm">{log.content}</p>
                          <div className="mt-1 flex items-center gap-3 text-xs text-muted-foreground">
                            <span className="flex items-center gap-1">
                              <Clock className="h-3 w-3" />
                              {format(new Date(log.createdAt), "HH:mm:ss")}
                            </span>
                          </div>
                        </div>
                      </div>
                    );
                  })
              )}
            </div>
          </TabsContent>
        </Tabs>
      </div>
    </div>
  );
}
