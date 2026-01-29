import React, { useMemo } from "react";
import { useEffect, useState } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import {
  ArrowLeft,
  CheckCircle2,
  Circle,
  AlertCircle,
  Loader2,
  Play,
  Code2,
  FileText,
  MessageSquare,
  ChevronRight,
  Clock,
  Sparkles,
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { Progress } from "@/components/ui/progress";
import {
  useProjects,
  useMissions,
  useProviders,
  useMissionLogs,
} from "@/hooks/use-data";
import { useAppStore } from "@/hooks/use-app-store";
import { createAIService } from "@/lib/services/ai-service";
import { toast } from "sonner";
import { format } from "date-fns";
import type {
  MissionStatus,
  MissionPlan,
  PlanStep,
  MissionLogType,
  GeneratedCode,
} from "@/lib/database/types";

const statusConfig: Record<MissionStatus, { label: string; color: string }> = {
  created: { label: "Criada", color: "bg-muted text-muted-foreground" },
  planning: {
    label: "Gerando plano...",
    color: "bg-primary text-primary-foreground",
  },
  plan_generated: { label: "Plano pronto", color: "bg-blue-500 text-white" },
  generating_code: {
    label: "Gerando código...",
    color: "bg-primary text-primary-foreground",
  },
  code_ready: { label: "Código pronto", color: "bg-green-500 text-white" },
  applying: {
    label: "Aplicando alterações...",
    color: "bg-primary text-primary-foreground",
  },
  completed: { label: "Concluída", color: "bg-green-600 text-white" },
  failed: {
    label: "Falhou",
    color: "bg-destructive text-destructive-foreground",
  },
  cancelled: { label: "Cancelada", color: "bg-muted text-muted-foreground" },
};

const logTypeConfig: Record<
  MissionLogType,
  { icon: React.ElementType; color: string }
> = {
  info: { icon: Circle, color: "text-muted-foreground" },
  prompt: { icon: MessageSquare, color: "text-blue-500" },
  response: { icon: Sparkles, color: "text-primary" },
  error: { icon: AlertCircle, color: "text-destructive" },
  action: { icon: Play, color: "text-green-500" },
  user_input: { icon: MessageSquare, color: "text-amber-500" },
  warning: { icon: AlertCircle, color: "text-amber-500" },
  debug: { icon: Circle, color: "text-muted-foreground" },
};

export default function MissionPage() {
  const { id: projectId, missionId } = useParams<{
    id: string;
    missionId: string;
  }>();
  const navigate = useNavigate();

  const [isGenerating, setIsGenerating] = useState(false);
  const [activeTab, setActiveTab] = useState("plan");

  const { projects, isLoading: projectsLoading } = useProjects();
  const {
    missions,
    updateStatus,
    setPlan,
    setCode,
    isLoading: missionsLoading,
  } = useMissions(projectId ?? undefined);
  const { providers } = useProviders();
  const { logs, addLog } = useMissionLogs(missionId ?? "");
  const setCurrentMission = useAppStore((s) => s.setCurrentMission);

  // Usar useMemo para estabilizar as referências e evitar re-renders desnecessários
  const project = useMemo(
    () =>
      projectId ? (projects.find((p) => p.id === projectId) ?? null) : null,
    [projectId, projects],
  );

  const mission = useMemo(
    () =>
      missionId ? (missions.find((m) => m.id === missionId) ?? null) : null,
    [missionId, missions],
  );

  const provider = useMemo(
    () =>
      mission?.providerId
        ? (providers.find((p) => p.id === mission.providerId) ?? null)
        : null,
    [mission?.providerId, providers],
  );

  // Usar missionId como dependência em vez do objeto mission inteiro
  useEffect(() => {
    if (missionId) setCurrentMission(missionId);
    return () => setCurrentMission(null);
  }, [missionId, setCurrentMission]);

  const isLoading =
    (projectId && projectsLoading) || (missionId && missionsLoading);

  if (isLoading) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4">
        <Loader2 className="h-12 w-12 animate-spin text-muted-foreground" />
        <p className="text-muted-foreground">Carregando missão...</p>
      </div>
    );
  }

  if (!mission || !project) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4">
        <AlertCircle className="h-12 w-12 text-muted-foreground" />
        <p className="text-muted-foreground">Missão não encontrada</p>
        <Button
          variant="outline"
          onClick={() => navigate(`/project/${projectId}`)}
        >
          <ArrowLeft className="mr-2 h-4 w-4" />
          Voltar ao projeto
        </Button>
      </div>
    );
  }

  const handleGeneratePlan = async () => {
    if (!provider || !missionId) {
      toast.error("Nenhum provedor selecionado para esta missão");
      return;
    }

    setIsGenerating(true);
    updateStatus(missionId, "planning");
    addLog("prompt", "Iniciando geração do plano...", {
      model: provider.config?.model as string,
    });

    try {
      const aiService = createAIService({
        provider,
        mission,
        projectContext: mission.context ?? undefined,
      });

      const response = await aiService.generatePlan();

      if (response.success && response.data) {
        setPlan(missionId, response.data as MissionPlan);
        addLog(
          "response",
          `Plano gerado com ${(response.data as MissionPlan).steps.length} etapas`,
          response.metadata ?? undefined,
        );
        toast.success("Plano gerado com sucesso");
      } else {
        throw new Error(response.error || "Falha ao gerar plano");
      }
    } catch (error) {
      // Volta para "created" para permitir tentar novamente
      updateStatus(missionId, "created");
      addLog(
        "error",
        error instanceof Error ? error.message : "Erro desconhecido",
        undefined,
      );
      toast.error("Falha ao gerar plano. Você pode tentar novamente.");
    } finally {
      setIsGenerating(false);
    }
  };

  const handleGenerateCode = async () => {
    if (!provider || !mission.plan || !missionId) {
      toast.error("O plano é necessário antes de gerar o código");
      return;
    }

    setIsGenerating(true);
    updateStatus(missionId, "generating_code");
    addLog("prompt", "Iniciando geração de código com base no plano...", {
      model: provider.config?.model as string,
    });

    try {
      const aiService = createAIService({
        provider,
        mission,
        projectContext: mission.context ?? undefined,
      });

      const response = await aiService.generateCode();

      if (response.success && response.data) {
        setCode(missionId, response.data as GeneratedCode);
        addLog(
          "response",
          `Código gerado: ${(response.data as { files: { path: string }[] }).files.length} arquivo(s) alterado(s)`,
          response.metadata ?? undefined,
        );
        toast.success("Sugestões de código geradas");
        setActiveTab("code");
      } else {
        throw new Error(response.error || "Falha ao gerar código");
      }
    } catch (error) {
      // Volta para "plan_generated" para permitir tentar novamente
      updateStatus(missionId, "plan_generated");
      addLog(
        "error",
        error instanceof Error ? error.message : "Erro desconhecido",
        undefined,
      );
      toast.error("Falha ao gerar código. Você pode tentar novamente.");
    } finally {
      setIsGenerating(false);
    }
  };

  const completedSteps =
    mission.plan?.steps.filter((s) => s.status === "completed").length ?? 0;
  const totalSteps = mission.plan?.steps.length ?? 0;
  const progress = totalSteps > 0 ? (completedSteps / totalSteps) * 100 : 0;

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <header className="border-b border-border bg-card px-6 py-4">
        <div className="flex items-center gap-4 mb-3">
          <Button
            variant="ghost"
            size="icon"
            onClick={() => navigate(`/project/${projectId}`)}
          >
            <ArrowLeft className="h-4 w-4" />
          </Button>
          <Separator orientation="vertical" className="h-6" />
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Link
              to={`/project/${projectId}`}
              className="hover:text-foreground"
            >
              {project.name}
            </Link>
            <ChevronRight className="h-4 w-4" />
            <span className="text-foreground">Missão</span>
          </div>
        </div>

        <div className="flex items-start justify-between">
          <div className="flex-1">
            <div className="flex items-center gap-3 mb-2">
              <h1 className="text-xl font-semibold text-card-foreground">
                {mission.title}
              </h1>
              <Badge className={statusConfig[mission.status].color}>
                {statusConfig[mission.status].label}
              </Badge>
            </div>
            <p className="text-sm text-muted-foreground max-w-2xl">
              {mission.description}
            </p>
          </div>

          <div className="flex items-center gap-2">
            {mission.status === "created" && (
              <Button onClick={handleGeneratePlan} disabled={isGenerating}>
                {isGenerating ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <Sparkles className="mr-2 h-4 w-4" />
                )}
                Gerar plano
              </Button>
            )}
            {mission.status === "plan_generated" && (
              <Button onClick={handleGenerateCode} disabled={isGenerating}>
                {isGenerating ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <Code2 className="mr-2 h-4 w-4" />
                )}
                Gerar código
              </Button>
            )}
            {mission.status === "code_ready" && (
              <Button
                onClick={() => {
                  toast.info(
                    "Aplicar alterações estará disponível na versão Electron",
                  );
                }}
              >
                <Play className="mr-2 h-4 w-4" />
                Aplicar alterações
              </Button>
            )}
          </div>
        </div>

        {/* Progress bar for missions with plans */}
        {mission.plan && (
          <div className="mt-4 flex items-center gap-4">
            <Progress value={progress} className="flex-1 h-2" />
            <span className="text-sm text-muted-foreground">
              {completedSteps}/{totalSteps} etapas
            </span>
          </div>
        )}
      </header>

      {/* Content */}
      <div className="flex-1 overflow-hidden">
        <Tabs
          value={activeTab}
          onValueChange={setActiveTab}
          className="h-full flex flex-col"
        >
          <div className="border-b border-border px-6">
            <TabsList className="h-12">
              <TabsTrigger value="plan" className="gap-2">
                <FileText className="h-4 w-4" />
                Plano
              </TabsTrigger>
              <TabsTrigger
                value="code"
                className="gap-2"
                disabled={!mission.generatedCode}
              >
                <Code2 className="h-4 w-4" />
                Código
              </TabsTrigger>
              <TabsTrigger value="logs" className="gap-2">
                <MessageSquare className="h-4 w-4" />
                Logs
                {logs.length > 0 && (
                  <Badge variant="secondary" className="ml-1">
                    {logs.length}
                  </Badge>
                )}
              </TabsTrigger>
            </TabsList>
          </div>

          <TabsContent value="plan" className="flex-1 overflow-auto p-6 mt-0">
            <PlanView
              plan={mission.plan}
              status={mission.status}
              provider={provider?.name}
            />
          </TabsContent>

          <TabsContent value="code" className="flex-1 overflow-auto p-6 mt-0">
            <CodeView code={mission.generatedCode} />
          </TabsContent>

          <TabsContent value="logs" className="flex-1 overflow-auto p-6 mt-0">
            <LogsView logs={logs} />
          </TabsContent>
        </Tabs>
      </div>
    </div>
  );
}

// ============================================
// Sub-components
// ============================================

function PlanView({
  plan,
  status,
  provider,
}: {
  plan: MissionPlan | null | undefined;
  status: MissionStatus;
  provider?: string;
}) {
  if (!plan) {
    return (
      <Card className="border-dashed">
        <CardContent className="flex flex-col items-center justify-center py-12">
          <FileText className="h-12 w-12 text-muted-foreground mb-4" />
          <h3 className="text-lg font-medium mb-2">Nenhum plano ainda</h3>
          <p className="text-sm text-muted-foreground text-center max-w-md">
            {status === "planning"
              ? "Gerando plano... Pode levar alguns instantes."
              : 'Clique em "Gerar plano" para criar um plano de ação para esta missão.'}
          </p>
          {status === "planning" && (
            <Loader2 className="h-6 w-6 animate-spin text-primary mt-4" />
          )}
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-6">
      {/* Summary */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle className="text-base">Resumo do plano</CardTitle>
            {plan.estimatedComplexity && (
              <Badge
                variant={
                  plan.estimatedComplexity === "high"
                    ? "destructive"
                    : plan.estimatedComplexity === "medium"
                      ? "default"
                      : "secondary"
                }
              >
                Complexidade {plan.estimatedComplexity}
              </Badge>
            )}
          </div>
          {provider && <CardDescription>Gerado por {provider}</CardDescription>}
        </CardHeader>
        <CardContent>
          <p className="text-sm">{plan.summary}</p>
        </CardContent>
      </Card>

      {/* Steps */}
      <div>
        <h3 className="text-sm font-medium mb-4">
          Etapas ({plan.steps.length})
        </h3>
        <div className="space-y-3">
          {plan.steps.map((step, index) => (
            <StepCard key={step.id} step={step} index={index} />
          ))}
        </div>
      </div>
    </div>
  );
}

function StepCard({ step, index }: { step: PlanStep; index: number }) {
  const getStatusIcon = () => {
    switch (step.status) {
      case "completed":
        return <CheckCircle2 className="h-5 w-5 text-green-500" />;
      case "in_progress":
        return <Loader2 className="h-5 w-5 text-primary animate-spin" />;
      case "skipped":
        return <Circle className="h-5 w-5 text-muted-foreground" />;
      default:
        return <Circle className="h-5 w-5 text-muted-foreground" />;
    }
  };

  return (
    <Card className={step.status === "in_progress" ? "border-primary" : ""}>
      <CardContent className="py-4">
        <div className="flex gap-4">
          <div className="flex flex-col items-center">
            {getStatusIcon()}
            {index < 4 && <div className="w-px flex-1 bg-border mt-2" />}
          </div>
          <div className="flex-1">
            <div className="flex items-center gap-2 mb-1">
              <span className="text-xs font-medium text-muted-foreground">
                Etapa {step.order}
              </span>
            </div>
            <h4 className="font-medium mb-1">{step.title}</h4>
            <p className="text-sm text-muted-foreground">{step.description}</p>
            {step.files && step.files.length > 0 && (
              <div className="mt-2 flex flex-wrap gap-1">
                {step.files.map((file) => (
                  <code
                    key={file}
                    className="text-xs bg-muted px-1.5 py-0.5 rounded"
                  >
                    {file}
                  </code>
                ))}
              </div>
            )}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function CodeView({
  code,
}: {
  code:
    | {
        files: {
          path: string;
          action: string;
          suggestedContent?: string;
          diff?: string;
        }[];
        summary?: string;
      }
    | null
    | undefined;
}) {
  if (!code) {
    return (
      <Card className="border-dashed">
        <CardContent className="flex flex-col items-center justify-center py-12">
          <Code2 className="h-12 w-12 text-muted-foreground mb-4" />
          <h3 className="text-lg font-medium mb-2">
            Nenhuma sugestão de código ainda
          </h3>
          <p className="text-sm text-muted-foreground text-center max-w-md">
            As sugestões de código aparecerão aqui após você gerá-las.
          </p>
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-6">
      {code.summary && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Resumo</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm">{code.summary}</p>
          </CardContent>
        </Card>
      )}

      <div>
        <h3 className="text-sm font-medium mb-4">
          Arquivos alterados ({code.files.length})
        </h3>
        <div className="space-y-4">
          {code.files.map((file) => (
            <Card key={file.path}>
              <CardHeader className="py-3">
                <div className="flex items-center justify-between">
                  <code className="text-sm font-medium">{file.path}</code>
                  <Badge
                    variant={
                      file.action === "create"
                        ? "default"
                        : file.action === "delete"
                          ? "destructive"
                          : "secondary"
                    }
                  >
                    {file.action}
                  </Badge>
                </div>
              </CardHeader>
              {file.diff && (
                <CardContent className="pt-0">
                  <ScrollArea className="h-64 rounded border bg-muted/50">
                    <pre className="p-4 text-xs font-mono whitespace-pre-wrap">
                      {file.diff}
                    </pre>
                  </ScrollArea>
                </CardContent>
              )}
            </Card>
          ))}
        </div>
      </div>
    </div>
  );
}

function LogsView({
  logs,
}: {
  logs: {
    id: string;
    type: MissionLogType;
    content: string;
    metadata?: {
      tokensUsed?: number;
      durationMs?: number;
      model?: string;
    } | null;
    createdAt: Date;
  }[];
}) {
  if (logs.length === 0) {
    return (
      <Card className="border-dashed">
        <CardContent className="flex flex-col items-center justify-center py-12">
          <MessageSquare className="h-12 w-12 text-muted-foreground mb-4" />
          <h3 className="text-lg font-medium mb-2">Nenhum log ainda</h3>
          <p className="text-sm text-muted-foreground text-center max-w-md">
            Os logs de atividade aparecerão aqui conforme você trabalha nesta
            missão.
          </p>
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-2">
      {logs.map((log) => {
        const config = logTypeConfig[log.type];
        const Icon = config.icon;

        return (
          <div
            key={log.id}
            className="flex gap-3 rounded-lg border border-border bg-card p-3"
          >
            <Icon className={`h-5 w-5 shrink-0 ${config.color}`} />
            <div className="flex-1 min-w-0">
              <p className="text-sm">{log.content}</p>
              <div className="mt-1 flex items-center gap-3 text-xs text-muted-foreground">
                <span className="flex items-center gap-1">
                  <Clock className="h-3 w-3" />
                  {format(log.createdAt, "HH:mm:ss")}
                </span>
                {log.metadata?.tokensUsed && (
                  <span>{log.metadata.tokensUsed} tokens</span>
                )}
                {log.metadata?.durationMs && (
                  <span>{(log.metadata.durationMs / 1000).toFixed(1)}s</span>
                )}
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}
