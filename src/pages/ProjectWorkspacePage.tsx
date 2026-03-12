import React, { useEffect, useMemo, useRef, useState } from "react";
import { Link, Outlet, useLocation, useNavigate, useOutletContext, useParams } from "react-router-dom";
import { ArrowLeft, Clock, Folder, GitBranch, Loader2, Plus, Terminal } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { WorkflowChoiceDialog } from "@/components/dialogs/workflow-choice-dialog";
import { useProjects, useProviders } from "@/hooks/use-data";
import { useAppStore } from "@/hooks/use-app-store";
import { formatDistanceToNow } from "date-fns";
import { ptBR } from "date-fns/locale";
import type { Project, Provider } from "@/lib/database/types";

export interface ProjectWorkspaceContextValue {
  projectId: string;
  project: Project;
  providers: Provider[];
  defaultProvider: Provider | null;
}

export function useProjectWorkspaceContext() {
  return useOutletContext<ProjectWorkspaceContextValue>();
}

export default function ProjectWorkspacePage() {
  const { id: projectId } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const location = useLocation();
  const [showWorkflowChoice, setShowWorkflowChoice] = useState(false);
  const hasUpdatedLastOpenedRef = useRef<string | null>(null);

  const { projects, update, isLoading: projectsLoading } = useProjects();
  const { providers } = useProviders();
  const setCurrentProject = useAppStore((s) => s.setCurrentProject);

  const project = useMemo(
    () => (projectId ? projects.find((p) => p.id === projectId) ?? null : null),
    [projectId, projects]
  );

  const defaultProvider = useMemo(
    () =>
      project?.defaultProviderId
        ? (providers.find((p) => p.id === project.defaultProviderId) ?? null)
        : null,
    [project?.defaultProviderId, providers]
  );

  const activeWorkspace = useMemo<"pipeline" | "agents" | null>(() => {
    if (location.pathname.includes("/agents")) return "agents";
    if (location.pathname.includes("/pipeline")) return "pipeline";
    return null;
  }, [location.pathname]);

  const handleContextualCreate = () => {
    if (activeWorkspace === "agents") {
      navigate(`/project/${projectId}/agents?new=agents`);
      return;
    }
    navigate(`/project/${projectId}/pipeline?new=pipeline`);
  };

  useEffect(() => {
    if (!projectId) return;
    setCurrentProject(projectId);
    return () => setCurrentProject(null);
  }, [projectId, setCurrentProject]);

  useEffect(() => {
    if (!projectId || hasUpdatedLastOpenedRef.current === projectId) return;
    hasUpdatedLastOpenedRef.current = projectId;
    update(projectId, { lastOpenedAt: new Date() });
  }, [projectId, update]);

  useEffect(() => {
    if (!projectId || !activeWorkspace) return;
    localStorage.setItem(`dcc:project:${projectId}:workspace`, activeWorkspace);
  }, [projectId, activeWorkspace]);

  if (projectId && projectsLoading) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4">
        <Loader2 className="h-12 w-12 animate-spin text-muted-foreground" />
        <p className="text-muted-foreground">Carregando projeto...</p>
      </div>
    );
  }

  if (!project || !projectId) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4">
        <p className="text-muted-foreground">Projeto não encontrado</p>
        <Button variant="outline" onClick={() => navigate("/")}>
          <ArrowLeft className="mr-2 h-4 w-4" />
          Voltar aos projetos
        </Button>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <header className="border-b border-border bg-card px-6 py-4">
        <div className="mb-4 flex items-center gap-4">
          <Button variant="ghost" size="icon" className="cursor-pointer" onClick={() => navigate("/")}>
            <ArrowLeft className="h-4 w-4" />
          </Button>
          <Separator orientation="vertical" className="h-6" />
          <div className="min-w-0 flex-1">
            <h1 className="truncate text-xl font-semibold text-card-foreground">{project.name}</h1>
            {project.description && <p className="text-sm text-muted-foreground">{project.description}</p>}
          </div>
          <Button onClick={handleContextualCreate}>
            <Plus className="mr-2 h-4 w-4" />
            {activeWorkspace === "agents" ? "Nova tarefa de agente" : "Nova missão pipeline"}
          </Button>
          <Button variant="outline" onClick={() => setShowWorkflowChoice(true)}>
            Escolher fluxo
          </Button>
        </div>

        <div className="mb-3 flex flex-wrap items-center gap-4 text-sm text-muted-foreground">
          <div className="flex items-center gap-2">
            <Folder className="h-4 w-4" />
            <code className="rounded bg-muted px-2 py-0.5 text-xs">{project.path}</code>
          </div>
          {project.gitRemoteUrl && (
            <div className="flex items-center gap-2">
              <GitBranch className="h-4 w-4" />
              <span>main</span>
            </div>
          )}
          {defaultProvider && <Badge variant="outline">{defaultProvider.name}</Badge>}
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

        <div className="flex items-center gap-2">
          <Button asChild variant={activeWorkspace === "pipeline" ? "secondary" : "ghost"}>
            <Link to={`/project/${projectId}/pipeline`}>Pipeline</Link>
          </Button>
          <Button asChild variant={activeWorkspace === "agents" ? "secondary" : "ghost"} className="gap-2">
            <Link to={`/project/${projectId}/agents`}>
              <Terminal className="h-4 w-4" />
              Agentes
            </Link>
          </Button>
        </div>
      </header>

      <div className="flex-1 overflow-auto">
        <Outlet
          context={{
            projectId,
            project,
            providers,
            defaultProvider,
          }}
        />
      </div>

      <WorkflowChoiceDialog
        open={showWorkflowChoice}
        onOpenChange={setShowWorkflowChoice}
        onSelect={(choice) => {
          if (choice === "pipeline") {
            navigate(`/project/${projectId}/pipeline?new=pipeline`);
            return;
          }
          navigate(`/project/${projectId}/agents?new=agents`);
        }}
      />
    </div>
  );
}
