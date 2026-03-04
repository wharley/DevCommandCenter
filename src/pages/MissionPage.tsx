import React, { useMemo, useCallback, memo } from "react";
import { useEffect, useState } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import {
  ArrowLeft,
  CheckCircle2,
  Circle,
  AlertCircle,
  Loader2,
  Pencil,
  Play,
  Code2,
  FileText,
  MessageSquare,
  ChevronRight,
  Clock,
  Sparkles,
  GitBranch,
  GitCommit,
  Upload,
  XCircle,
  RotateCcw,
  Info,
  Lightbulb,
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
import { Alert } from "@/components/ui/alert";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import {
  useProjects,
  useMissions,
  useProviders,
  useMissionLogs,
} from "@/hooks/use-data";
import { useAppStore } from "@/hooks/use-app-store";
import { createAIService } from "@/lib/services/ai-service";
import { CommitDialog } from "@/components/dialogs/commit-dialog";
import { NewMissionDialog } from "@/components/dialogs/new-mission-dialog";
import { RegeneratePlanDialog } from "@/components/dialogs/regenerate-plan-dialog";
import { RegenerateCodeDialog } from "@/components/dialogs/regenerate-code-dialog";
import { MissionTipsDialog } from "@/components/dialogs/mission-tips-dialog";
import { toast } from "sonner";
import { format } from "date-fns";
import { cn } from "@/lib/utils";
import type {
  MissionStatus,
  MissionPlan,
  PlanStep,
  MissionLogType,
  GeneratedCode,
} from "@/lib/database/types";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import Editor from "react-simple-code-editor";
import Prism from "prismjs";
import "prismjs/components/prism-clike";
import "prismjs/components/prism-javascript";
import "prismjs/components/prism-jsx";
import "prismjs/components/prism-typescript";
import "prismjs/components/prism-tsx";
import "prismjs/components/prism-css";
import "prismjs/components/prism-scss";
import "prismjs/components/prism-json";
import "prismjs/components/prism-markdown";
import "prismjs/components/prism-markup";
import "prismjs/components/prism-diff";
import "prismjs/components/prism-bash";
import "prismjs/components/prism-yaml";
import "prismjs/components/prism-python";
import "prismjs/themes/prism-tomorrow.css";

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

const CollapsibleMissionDescription = memo(
  function CollapsibleMissionDescription({
    description,
  }: {
    description: string;
  }) {
    const [isExpanded, setIsExpanded] = useState(false);
    const shouldShowToggle = description.length > 150; // ~3 linhas

    if (!shouldShowToggle) {
      return <p className="text-sm text-muted-foreground">{description}</p>;
    }

    return (
      <div className="space-y-2">
        {!isExpanded ? (
          <div className="relative">
            <p className="text-sm text-muted-foreground line-clamp-3">
              {description}
            </p>
            <div className="absolute bottom-0 right-0 bg-gradient-to-l from-card via-card to-transparent pl-8">
              <button
                type="button"
                onClick={() => setIsExpanded(true)}
                className="text-sm text-primary hover:text-primary/80 font-medium inline-flex items-center gap-1 py-1 px-2 -mr-2 rounded hover:bg-primary/10 transition-colors cursor-pointer"
              >
                Ver completo
                <ChevronRight className="h-3 w-3" />
              </button>
            </div>
          </div>
        ) : (
          <div className="space-y-3">
            <ScrollArea className="h-[180px] rounded-md border border-border bg-muted/30 px-3 py-2">
              <p className="text-sm text-muted-foreground whitespace-pre-wrap pr-3">
                {description}
              </p>
            </ScrollArea>
            <div className="flex justify-end pt-1">
              <button
                type="button"
                onClick={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  setIsExpanded(false);
                }}
                className="relative z-10 text-sm text-primary hover:text-primary/80 font-medium flex items-center gap-1.5 py-2 px-4 rounded-md hover:bg-primary/10 transition-all cursor-pointer select-none"
              >
                <span>Recolher</span>
                <ChevronRight className="h-3.5 w-3.5 -rotate-90" />
              </button>
            </div>
          </div>
        )}
      </div>
    );
  },
);

export default function MissionPage() {
  const { id: projectId, missionId } = useParams<{
    id: string;
    missionId: string;
  }>();
  const navigate = useNavigate();

  const [isGenerating, setIsGenerating] = useState(false);
  const [isApplying, setIsApplying] = useState(false);
  const [activeTab, setActiveTab] = useState("plan");
  const [previousStatus, setPreviousStatus] = useState<MissionStatus | null>(
    null,
  );
  const [commitDialogOpen, setCommitDialogOpen] = useState(false);
  const [regeneratePlanDialogOpen, setRegeneratePlanDialogOpen] =
    useState(false);
  const [commitDialogStatus, setCommitDialogStatus] = useState<
    import("@/types/electron").GitStatus | null
  >(null);
  const [currentBranch, setCurrentBranch] = useState<string | null>(null);
  const [isWorktree, setIsWorktree] = useState(false);
  const [isPushing, setIsPushing] = useState(false);
  const [createBranchForMission, setCreateBranchForMission] = useState(false);
  const [branchNameForMission, setBranchNameForMission] = useState("");
  const [usageStats, setUsageStats] = useState<{
    totalTokens: number;
    totalDurationMs: number;
  }>({ totalTokens: 0, totalDurationMs: 0 });
  const [selectedFilePaths, setSelectedFilePaths] = useState<Set<string>>(
    new Set(),
  );
  const [editedSuggestions, setEditedSuggestions] = useState<
    Record<string, string>
  >({});
  const [cliParseErrorWithRepoChanges, setCliParseErrorWithRepoChanges] =
    useState(false);
  const [recoveryCodeFromGit, setRecoveryCodeFromGit] =
    useState<GeneratedCode | null>(null);
  const [regenerateCodeDialogOpen, setRegenerateCodeDialogOpen] =
    useState(false);
  const [codeFeedback, setCodeFeedback] = useState("");
  const [editMissionDialogOpen, setEditMissionDialogOpen] = useState(false);
  const [tipsDialogOpen, setTipsDialogOpen] = useState(false);

  const { projects, isLoading: projectsLoading } = useProjects();
  const {
    missions,
    update,
    updateStatus,
    setPlan,
    setCode,
    cancel: cancelMission,
    refresh: refreshMissions,
    isLoading: missionsLoading,
  } = useMissions(projectId ?? undefined);
  const { providers } = useProviders();
  const {
    logs,
    addLog,
    refresh: refreshLogs,
  } = useMissionLogs(missionId ?? "");
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

  const planProviderId =
    mission?.planProviderId ??
    mission?.providerId ??
    project?.defaultProviderId ??
    "";
  const codeProviderId =
    mission?.codeProviderId ??
    mission?.providerId ??
    project?.defaultProviderId ??
    "";

  const planProvider = useMemo(
    () =>
      planProviderId
        ? (providers.find((p) => p.id === planProviderId) ?? null)
        : null,
    [planProviderId, providers],
  );

  const codeProvider = useMemo(
    () =>
      codeProviderId
        ? (providers.find((p) => p.id === codeProviderId) ?? null)
        : null,
    [codeProviderId, providers],
  );

  const canEditMission =
    mission &&
    !mission.plan &&
    mission.status !== "planning";

  // Usar missionId como dependência em vez do objeto mission inteiro
  useEffect(() => {
    if (missionId) setCurrentMission(missionId);
    return () => setCurrentMission(null);
  }, [missionId, setCurrentMission]);

  // Sincronizar seleção de arquivos e limpar edições quando o código gerado mudar
  useEffect(() => {
    const paths = mission?.generatedCode?.files?.map((f) => f.path) ?? [];
    setSelectedFilePaths(new Set(paths));
    setEditedSuggestions({});
  }, [mission?.generatedCode?.files]);

  // Limpar estado de recovery quando a missão tiver generatedCode preenchido
  useEffect(() => {
    if (mission?.generatedCode?.files?.length) {
      setCliParseErrorWithRepoChanges(false);
      setRecoveryCodeFromGit(null);
    }
  }, [mission?.generatedCode?.files?.length]);

  // Branch e worktree (Electron)
  useEffect(() => {
    const path = project?.path;
    const git = typeof window !== "undefined" ? window.electronAPI?.git : null;
    if (!path || !git) {
      setCurrentBranch(null);
      setIsWorktree(false);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const [branch, wt] = await Promise.all([
          git.getCurrentBranch(path),
          git.getWorktreeInfo(path),
        ]);
        if (!cancelled) {
          setCurrentBranch(branch ?? null);
          setIsWorktree(wt?.isWorktree ?? false);
        }
      } catch {
        if (!cancelled) {
          setCurrentBranch(null);
          setIsWorktree(false);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [project?.path]);

  // Status do repositório ao abrir o diálogo de commit
  useEffect(() => {
    if (!commitDialogOpen || !project?.path || !window.electronAPI?.git) {
      if (!commitDialogOpen) setCommitDialogStatus(null);
      return;
    }
    let cancelled = false;
    window.electronAPI.git
      .getStatus(project.path)
      .then((s) => {
        if (!cancelled) setCommitDialogStatus(s);
      })
      .catch(() => {
        if (!cancelled) setCommitDialogStatus(null);
      });
    return () => {
      cancelled = true;
    };
  }, [commitDialogOpen, project?.path]);

  // Total tokens e duração da missão (para exibição na aba Logs)
  useEffect(() => {
    if (!missionId || !window.db?.missionLogs.getUsageStats) {
      setUsageStats({ totalTokens: 0, totalDurationMs: 0 });
      return;
    }
    let cancelled = false;
    window.db.missionLogs
      .getUsageStats(missionId)
      .then((stats) => {
        if (!cancelled)
          setUsageStats({
            totalTokens: stats.totalTokens ?? 0,
            totalDurationMs: stats.totalDurationMs ?? 0,
          });
      })
      .catch(() => {
        if (!cancelled) setUsageStats({ totalTokens: 0, totalDurationMs: 0 });
      });
    return () => {
      cancelled = true;
    };
  }, [missionId, logs]);

  // Poll logs while generating code so backend progress messages appear in the Logs tab
  useEffect(() => {
    if (!isGenerating || !missionId) return;
    const interval = setInterval(refreshLogs, 2500);
    return () => clearInterval(interval);
  }, [isGenerating, missionId, refreshLogs]);

  // Auto-navegação inteligente entre abas baseada no status da missão
  useEffect(() => {
    if (!mission || previousStatus === mission.status) return;

    const status = mission.status;

    // Navegação automática: quando começar a executar, vai para Logs
    if (status === "planning" && previousStatus !== "planning") {
      setActiveTab("logs");
      toast.info("📋 Gerando plano...", {
        description: "Acompanhe o progresso em tempo real na aba Logs",
        duration: 3000,
      });
    } else if (
      status === "generating_code" &&
      previousStatus !== "generating_code"
    ) {
      setActiveTab("logs");
      toast.info("💻 Gerando código...", {
        description: "O modelo de IA está criando as alterações necessárias",
        duration: 3000,
      });
    } else if (status === "applying" && previousStatus !== "applying") {
      setActiveTab("logs");
      toast.info("⚙️ Aplicando alterações...", {
        description: "Gravando os arquivos no projeto",
        duration: 3000,
      });
    }
    // Quando terminar, volta para a aba relevante
    else if (status === "plan_generated" && previousStatus === "planning") {
      setTimeout(() => setActiveTab("plan"), 200); // Pequeno delay para transição suave
      // toast.success já é exibido no handleGeneratePlan
    } else if (
      status === "code_ready" &&
      previousStatus === "generating_code"
    ) {
      setTimeout(() => setActiveTab("code"), 200); // Pequeno delay para transição suave
      // toast.success já é exibido no handleGenerateCode
    } else if (status === "completed" && previousStatus === "applying") {
      // Mantém na aba logs para mostrar resultado final
      toast.success("✅ Missão concluída!", {
        description: "Todas as alterações foram aplicadas com sucesso",
        duration: 5000,
      });
    }

    setPreviousStatus(status);
  }, [mission?.status, previousStatus, mission]);

  // Inicializar previousStatus
  useEffect(() => {
    if (mission && previousStatus === null) {
      setPreviousStatus(mission.status);
    }
  }, [mission, previousStatus]);

  // Last progress message from logs (info/prompt) for the Code tab progress screen
  const lastProgressMessage = useMemo(() => {
    const progressLogs = logs.filter(
      (l) => l.type === "info" || l.type === "prompt",
    );
    if (progressLogs.length === 0) return "Iniciando geração de código...";
    const sorted = [...progressLogs].sort((a, b) => {
      const ta = new Date(a.createdAt).getTime();
      const tb = new Date(b.createdAt).getTime();
      return tb - ta;
    });
    return sorted[0].content;
  }, [logs]);

  // Determinar quais abas devem estar desabilitadas
  const isTabDisabled = useCallback(
    (tab: "plan" | "code" | "logs") => {
      if (tab === "logs") return false; // Logs sempre acessível

      const status = mission?.status;
      if (!status) return false;

      // Durante execução, desabilita a aba sendo processada
      if (status === "planning" && tab === "plan") return true;
      if (status === "generating_code" && tab === "code") return true;
      if (status === "applying" && tab === "code") return true;

      // Code tab só fica disponível se tiver código gerado ou estiver gerando
      if (
        tab === "code" &&
        !mission?.generatedCode &&
        !isGenerating &&
        !cliParseErrorWithRepoChanges
      ) {
        return true;
      }

      return false;
    },
    [
      mission?.status,
      mission?.generatedCode,
      isGenerating,
      cliParseErrorWithRepoChanges,
    ],
  );

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
    if (!planProvider || !missionId) {
      toast.error("Nenhum provedor selecionado para o plano");
      return;
    }

    setIsGenerating(true);
    updateStatus(missionId, "planning");
    addLog("prompt", "Iniciando geração do plano...", {
      model: planProvider.config?.model as string,
    });

    try {
      const aiService = createAIService({
        provider: planProvider,
        mission,
        projectContext: mission.context ?? undefined,
      });

      const response = await aiService.generatePlan();

      if (response.success && response.data) {
        setPlan(missionId, response.data as MissionPlan);
        addLog(
          "response",
          `Plano gerado com ${
            (response.data as MissionPlan).steps.length
          } etapas`,
          response.metadata ?? undefined,
        );
        toast.success("Plano gerado com sucesso");
      } else {
        throw new Error(response.error || "Falha ao gerar plano");
      }
    } catch (error) {
      // Volta para "created" para permitir tentar novamente
      updateStatus(missionId, "created");
      const msg = error instanceof Error ? error.message : "Erro desconhecido";
      addLog("error", msg, undefined);
      toast.error(msg);
    } finally {
      setIsGenerating(false);
    }
  };

  const handleRegeneratePlan = async (feedback: string) => {
    if (!planProvider || !missionId) {
      toast.error("Nenhum provedor selecionado para o plano");
      throw new Error("Plan provider or missionId missing");
    }

    setIsGenerating(true);
    updateStatus(missionId, "planning");
    addLog("prompt", "Regenerando plano com feedback do usuário...", {
      model: planProvider.config?.model as string,
    });

    try {
      const aiService = createAIService({
        provider: planProvider,
        mission,
        projectContext: mission.context ?? undefined,
      });

      const response = await aiService.generatePlan(feedback);

      if (response.success && response.data) {
        setPlan(missionId, response.data as MissionPlan);
        addLog(
          "response",
          `Plano regenerado com ${
            (response.data as MissionPlan).steps.length
          } etapas`,
          response.metadata ?? undefined,
        );
        toast.success("Plano regenerado com sucesso");
      } else {
        throw new Error(response.error || "Falha ao regenerar plano");
      }
    } catch (error) {
      updateStatus(missionId, "plan_generated");
      const msg = error instanceof Error ? error.message : "Erro desconhecido";
      addLog("error", msg, undefined);
      toast.error(msg);
      throw error;
    } finally {
      setIsGenerating(false);
    }
  };

  const handleGenerateCode = async () => {
    console.log("[DevCommandCenter] handleGenerateCode called", {
      hasProvider: !!codeProvider,
      providerName: codeProvider?.name,
      providerType: codeProvider?.type,
      hasPlan: !!mission.plan,
      planSteps: mission.plan?.steps?.length,
      missionId,
      missionStatus: mission.status,
      timestamp: new Date().toISOString(),
    });

    // Validações explícitas com mensagens específicas
    if (!missionId) {
      console.error("[DevCommandCenter] Mission ID is missing");
      toast.error("ID da missão não encontrado");
      return;
    }

    if (!codeProvider) {
      console.error("[DevCommandCenter] No code provider configured");
      toast.error("Nenhum provedor de IA configurado para gerar código", {
        description: "Selecione um provedor em \"Gerar código com\" ou nas configurações do projeto",
      });
      return;
    }

    if (!codeProvider.isActive) {
      console.error("[DevCommandCenter] Code provider is not active");
      toast.error(`O provedor ${codeProvider.name} está inativo`, {
        description: "Ative o provedor nas configurações",
      });
      return;
    }

    if (!mission.plan) {
      console.error("[DevCommandCenter] Mission has no plan");
      toast.error("O plano é necessário antes de gerar o código", {
        description: "Gere um plano primeiro clicando em 'Gerar plano'",
      });
      return;
    }

    if (mission.status !== "plan_generated") {
      console.error(
        "[DevCommandCenter] Invalid mission status:",
        mission.status,
      );
      toast.error(`Status da missão inválido: ${mission.status}`, {
        description:
          "A missão deve estar no status 'Plano pronto' para gerar código",
      });
      return;
    }

    console.log("[DevCommandCenter] All validations passed");

    setCliParseErrorWithRepoChanges(false);
    setRecoveryCodeFromGit(null);

    const git = typeof window !== "undefined" ? window.electronAPI?.git : null;
    if (createBranchForMission && project?.path && git) {
      let baseBranch: string;
      try {
        baseBranch = await git.getDefaultBranch(project.path);
      } catch {
        toast.error(
          "Não foi possível identificar o branch base (main/master). Configure o repositório e tente novamente.",
        );
        return;
      }
      const branchName = branchNameForMission.trim() || `mission/${missionId}`;
      const created = await git.createBranch(
        project.path,
        branchName,
        baseBranch,
      );
      if (!created) {
        toast.error(
          "Não foi possível criar o branch. Verifique se o repositório está em um estado válido.",
        );
        return;
      }
      addLog(
        "action",
        `Branch criado: ${branchName} (a partir de ${baseBranch})`,
        undefined,
      );
      const newBranch = await git.getCurrentBranch(project.path);
      setCurrentBranch(newBranch ?? null);
    }

    // Feedback imediato ANTES de qualquer operação assíncrona
    setIsGenerating(true);

    // Atualizar UI imediatamente
    toast.info("Iniciando geração de código...", { duration: 2000 });

    // Mudar status e logar ANTES de operações lentas
    updateStatus(missionId, "generating_code");
    console.log("[DevCommandCenter] Status updated to generating_code");

    // Incrementar contador de tentativas
    const attempts = (mission.codeGenerationAttempts ?? 0) + 1;
    await update(missionId, { codeGenerationAttempts: attempts });

    // Log inicial visível
    addLog(
      "prompt",
      `Iniciando geração de código com base no plano... (tentativa ${attempts})`,
      {
        model: codeProvider.config?.model as string,
      },
    );

    // Auto-navegação para logs é feita pelo useEffect

    try {
      const aiService = createAIService({
        provider: codeProvider,
        mission,
        projectContext: mission.context ?? undefined,
      });

      const response = await aiService.generateCode();

      if (response.success && response.data) {
        setCode(missionId, response.data as GeneratedCode);
        addLog(
          "response",
          `Código gerado: ${
            (response.data as { files: { path: string }[] }).files.length
          } arquivo(s) alterado(s)`,
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

      const errorMessage =
        error instanceof Error ? error.message : "Erro desconhecido";
      const detailedError = `Erro ao gerar código: ${errorMessage}`;

      addLog("error", detailedError, undefined);

      // Toast com informação mais útil
      toast.error(detailedError, {
        description:
          "Verifique os logs para mais detalhes. O console do navegador (F12) pode ter informações adicionais.",
        duration: 8000,
      });

      const isParseError =
        typeof errorMessage === "string" &&
        (errorMessage.includes("Failed to parse code") ||
          errorMessage.includes("Could not parse JSON"));
      const isCliProvider =
        codeProvider?.type === "cursor" ||
        codeProvider?.type === "claude-code" ||
        codeProvider?.type === "codex";
      if (
        isParseError &&
        isCliProvider &&
        project?.path &&
        window.electronAPI?.git
      ) {
        try {
          const status = await window.electronAPI.git.getStatus(project.path);
          const hasChanges =
            status.isDirty ||
            status.staged.length > 0 ||
            status.unstaged.length > 0 ||
            status.untracked.length > 0;
          if (hasChanges) {
            const allPaths = [
              ...new Set([
                ...status.staged,
                ...status.unstaged,
                ...status.untracked,
              ]),
            ];
            const git = window.electronAPI?.git;
            const files: GeneratedCode["files"] = await Promise.all(
              allPaths.map(async (filePath) => {
                const diff =
                  (git
                    ? await git.getFileDiffHead(project.path, filePath)
                    : "") ?? "";
                const isUntracked = status.untracked.includes(filePath);
                return {
                  path: filePath,
                  action: isUntracked ? "create" : "modify",
                  diff: diff || undefined,
                };
              }),
            );
            if (files.length > 0) {
              // Logging interno para desenvolvedores
              console.warn(
                "[DevCommandCenter] Failed to parse CLI response, recovered from git",
                {
                  error: errorMessage,
                  filesRecovered: files.length,
                  provider: codeProvider?.type,
                  timestamp: new Date().toISOString(),
                },
              );

              setRecoveryCodeFromGit({
                summary: "Código aplicado com sucesso",
                files,
              });
              setCliParseErrorWithRepoChanges(true);
              setActiveTab("code");
              toast.success("Código aplicado com sucesso", {
                description: `${files.length} arquivo(s) alterado(s). Clique na aba Código para revisar.`,
                duration: 5000,
              });
            }
          }
        } catch {
          // ignore recovery failure
        }
      }
    } finally {
      setIsGenerating(false);
    }
  };

  const handleApplyChanges = async () => {
    if (
      !missionId ||
      !mission ||
      !project ||
      !provider ||
      !mission.generatedCode?.files?.length
    ) {
      toast.error("Dados insuficientes para aplicar alterações");
      return;
    }
    if (mission.status === "code_ready" && selectedFilePaths.size === 0) {
      toast.error("Selecione pelo menos um arquivo para aplicar.");
      return;
    }

    setIsApplying(true);
    addLog("action", "Aplicando alterações...");

    try {
      const aiService = createAIService({
        provider,
        mission,
        projectContext: mission.context ?? undefined,
      });
      const filePaths =
        selectedFilePaths.size > 0 ? Array.from(selectedFilePaths) : undefined;
      const editedContent: Record<string, string> = {};
      for (const p of selectedFilePaths) {
        if (editedSuggestions[p] !== undefined)
          editedContent[p] = editedSuggestions[p];
      }
      const result = await aiService.applyChanges({
        createBackup: true,
        filePaths,
        editedContent:
          Object.keys(editedContent).length > 0 ? editedContent : undefined,
      });

      if (result.success) {
        toast.success(
          `Alterações aplicadas: ${result.appliedFiles.length} arquivo(s)`,
        );
        try {
          await refreshMissions();
          await refreshLogs();
          setTimeout(() => setActiveTab("logs"), 0);
        } catch {
          toast.warning(
            "Alterações aplicadas, mas a interface pode estar desatualizada. Atualize a página se necessário.",
          );
        }
      } else if (result.appliedFiles.length > 0) {
        const failedList = result.failedFiles
          .map((f) => (f.path ? `${f.path}: ${f.error}` : f.error))
          .join("; ");
        toast.warning(
          `Aplicados ${result.appliedFiles.length} arquivo(s). ${result.failedFiles.length} falharam: ${failedList}. Verifique os logs.`,
        );
        try {
          await refreshMissions();
          await refreshLogs();
        } catch {
          /* ignore */
        }
      } else {
        const firstError = result.failedFiles[0];
        const detail =
          firstError?.error && !firstError?.path
            ? firstError.error
            : result.failedFiles.length > 0
              ? result.failedFiles
                  .map((f) => (f.path ? `${f.path}: ${f.error}` : f.error))
                  .join("; ")
              : "";
        toast.error(detail || "Falha ao aplicar alterações.");
        try {
          await refreshMissions();
          await refreshLogs();
        } catch {
          /* ignore */
        }
      }
    } catch (error) {
      const msg = error instanceof Error ? error.message : "Erro desconhecido";
      toast.error(`Falha ao aplicar alterações: ${msg}`);
      addLog("error", msg);
      try {
        await refreshMissions();
        await refreshLogs();
      } catch {
        /* ignore */
      }
    } finally {
      setIsApplying(false);
    }
  };

  const handleCommit = async (message: string) => {
    if (
      !project?.path ||
      typeof window === "undefined" ||
      !window.electronAPI?.git
    ) {
      toast.error("Commit indisponível");
      throw new Error("Commit indisponível");
    }
    try {
      const ok = await window.electronAPI.git.commit(project.path, message);
      if (ok) {
        toast.success("Commit realizado");
        // Marca como committed no banco
        if (missionId) {
          await update(missionId, { isCommitted: true });
        }
      } else {
        toast.error("Falha ao commitar. Verifique o status do repositório.");
        throw new Error("Falha ao commitar");
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Erro desconhecido";
      toast.error(`Falha ao commitar: ${msg}`);
      throw e;
    }
  };

  const handlePush = async () => {
    if (
      !project?.path ||
      typeof window === "undefined" ||
      !window.electronAPI?.git?.push
    ) {
      toast.error("Push indisponível");
      return;
    }
    setIsPushing(true);
    try {
      const result = await window.electronAPI.git.push(project.path);
      if (result.success) {
        toast.success("Push realizado");
        // Marca como pushed no banco
        if (missionId) {
          await update(missionId, { isPushed: true });
        }
      } else {
        toast.error(result.error ?? "Falha ao fazer push.");
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Erro desconhecido";
      toast.error(`Falha ao fazer push: ${msg}`);
    } finally {
      setIsPushing(false);
    }
  };

  const handleReject = async () => {
    if (!missionId || !project?.path || !window.electronAPI?.git?.reset) {
      toast.error("Rejeitar indisponível");
      return;
    }
    const status = await window.electronAPI.git.getStatus(project.path);
    if (!status.isRepo) {
      toast.error("Repositório Git não encontrado");
      return;
    }
    const ref = status.isDirty ? "HEAD" : "HEAD~1";
    const msg = status.isDirty
      ? "Descartar alterações não commitadas e cancelar esta missão?"
      : "Reverter o último commit e cancelar esta missão? O commit será removido.";
    if (!window.confirm(msg)) return;
    try {
      const result = await window.electronAPI.git.reset(project.path, ref);
      if (result.success) {
        await cancelMission(missionId);
        toast.success("Missão rejeitada e alterações descartadas");
        navigate(`/project/${projectId}`);
      } else {
        toast.error(result.error ?? "Falha ao reverter alterações.");
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Erro desconhecido";
      toast.error(`Falha ao rejeitar: ${msg}`);
    }
  };

  const handleRetryAfterParseError = async () => {
    const confirmed = window.confirm(
      "Descartar todas as alterações e gerar código novamente?\n\n" +
        "Isso irá:\n" +
        "• Reverter todas as mudanças no repositório\n" +
        "• Gerar código novamente do zero\n" +
        "• Pode levar alguns minutos",
    );

    if (!confirmed) return;

    try {
      setIsGenerating(true);

      // Reverter mudanças
      if (project?.path && window.electronAPI?.git?.reset) {
        const result = await window.electronAPI.git.reset(project.path, "HEAD");
        if (!result.success) {
          toast.error("Falha ao reverter alterações");
          return;
        }
      }

      // Limpar estado
      setCliParseErrorWithRepoChanges(false);
      setRecoveryCodeFromGit(null);
      setSelectedFilePaths(new Set());
      setEditedSuggestions({});

      // Tentar gerar novamente
      toast.info("Gerando código novamente...");
      await handleGenerateCode();
    } catch (e) {
      toast.error(`Erro: ${e instanceof Error ? e.message : "desconhecido"}`);
    } finally {
      setIsGenerating(false);
    }
  };

  /**
   * Descartar alterações e voltar ao plano.
   * Se veio de recovery (parse error + git), faz git reset e limpa estado.
   * Senão, apenas limpa generatedCode e status no DB.
   */
  const handleDiscardChanges = async () => {
    const confirmed = window.confirm(
      "Descartar alterações e voltar ao plano?\n\n" +
        "Você poderá revisar o plano e gerar código de novo quando quiser.",
    );

    if (!confirmed) return;

    const wasRecovery = cliParseErrorWithRepoChanges;

    try {
      if (wasRecovery && project?.path && window.electronAPI?.git?.reset) {
        const result = await window.electronAPI.git.reset(project.path, "HEAD");
        if (!result.success) {
          toast.error(result.error ?? "Falha ao reverter");
          return;
        }
      }

      setCliParseErrorWithRepoChanges(false);
      setRecoveryCodeFromGit(null);
      setSelectedFilePaths(new Set());
      setEditedSuggestions({});
      setActiveTab("plan");

      if (missionId) {
        if (wasRecovery) {
          updateStatus(missionId, "plan_generated");
        } else {
          await update(missionId, {
            generatedCode: undefined,
            status: "plan_generated",
            codeGenerationAttempts: 0,
          });
        }
      }

      toast.success("Alterações descartadas. Revise o plano quando quiser.");
    } catch (e) {
      toast.error(`Erro: ${e instanceof Error ? e.message : "desconhecido"}`);
    }
  };

  const handleCancelMission = async () => {
    if (!missionId) return;

    const confirmed = window.confirm(
      "Cancelar esta missão?\n\n" +
        "A missão será marcada como cancelada e você voltará à página do projeto.",
    );

    if (!confirmed) return;

    try {
      await cancelMission(missionId);
      toast.success("Missão cancelada");
      navigate(`/project/${projectId}`);
    } catch (e) {
      toast.error(
        `Erro ao cancelar: ${e instanceof Error ? e.message : "desconhecido"}`,
      );
    }
  };

  const handleResetToPlanning = async () => {
    if (!missionId) return;

    const confirmed = window.confirm(
      "Resetar missão para o estado de plano gerado?\n\n" +
        "Isso permitirá que você tente gerar o código novamente.\n" +
        "Use isso se a geração de código ficou travada.",
    );

    if (!confirmed) return;

    try {
      setIsGenerating(false);
      updateStatus(missionId, "plan_generated");
      toast.success("Missão resetada para 'Plano pronto'");
    } catch (e) {
      toast.error(
        `Erro ao resetar: ${e instanceof Error ? e.message : "desconhecido"}`,
      );
    }
  };

  const handleRegenerateCodeWithFeedback = async (feedback: string) => {
    if (!codeProvider || !mission.plan || !missionId) {
      toast.error("Selecione um provedor para gerar código e tenha um plano");
      return;
    }

    setIsGenerating(true);
    updateStatus(missionId, "generating_code");
    // Auto-navegação para logs é feita pelo useEffect

    // Incrementar contador de tentativas
    const attempts = (mission.codeGenerationAttempts ?? 0) + 1;
    await update(missionId, { codeGenerationAttempts: attempts });

    const feedbackMsg = feedback.trim()
      ? `Regenerando código (tentativa ${attempts}) com feedback do usuário...`
      : `Regenerando código (tentativa ${attempts})...`;

    addLog("prompt", feedbackMsg, {
      model: codeProvider.config?.model as string,
    });

    try {
      const aiService = createAIService({
        provider: codeProvider,
        mission,
        projectContext: mission.context ?? undefined,
        codeFeedback: feedback.trim() || undefined,
      });

      const response = await aiService.generateCode();

      if (response.success && response.data) {
        setCode(missionId, response.data as GeneratedCode);
        addLog(
          "response",
          `Código gerado: ${
            (response.data as { files: { path: string }[] }).files.length
          } arquivo(s) alterado(s)`,
          response.metadata ?? undefined,
        );
        toast.success("Sugestões de código geradas");
        setActiveTab("code");
      } else {
        throw new Error(response.error || "Falha ao gerar código");
      }
    } catch (error) {
      // Volta para "code_ready" se havia código anterior, senão para "plan_generated"
      const fallbackStatus = mission.generatedCode
        ? "code_ready"
        : "plan_generated";
      updateStatus(missionId, fallbackStatus);
      const errorMessage =
        error instanceof Error ? error.message : "Erro desconhecido";
      addLog("error", errorMessage, undefined);
      toast.error(errorMessage);
    } finally {
      setIsGenerating(false);
      setRegenerateCodeDialogOpen(false);
      setCodeFeedback("");
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
        <div className="flex items-center justify-between gap-4 mb-3">
          <div className="flex items-center gap-4 min-w-0">
            <Button
              variant="ghost"
              size="icon"
              className="cursor-pointer"
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
          {(currentBranch != null || isWorktree) && (
            <div className="flex items-center gap-2 shrink-0 rounded-md border border-border bg-muted/50 px-3 py-1.5 text-sm">
              {currentBranch != null && (
                <span className="flex items-center gap-1.5 font-medium text-foreground">
                  <GitBranch className="h-3.5 w-3.5 text-muted-foreground" />
                  {currentBranch}
                </span>
              )}
              {isWorktree && (
                <>
                  {currentBranch != null && (
                    <Separator orientation="vertical" className="h-4" />
                  )}
                  <Badge variant="secondary" className="font-normal text-xs">
                    Worktree
                  </Badge>
                </>
              )}
            </div>
          )}
        </div>

        <div className="flex items-start justify-between gap-6">
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-3 mb-2 flex-wrap">
              <h1 className="text-xl font-semibold text-card-foreground">
                {mission.title}
              </h1>
              <Badge className={statusConfig[mission.status].color}>
                {statusConfig[mission.status].label}
              </Badge>
              {mission.preserveInstructions?.trim() && (
                <Badge
                  variant="outline"
                  className="font-normal text-muted-foreground"
                  title={mission.preserveInstructions}
                >
                  Instruções de preservação ativas
                </Badge>
              )}
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="text-muted-foreground hover:text-foreground -ml-1"
                onClick={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  setTipsDialogOpen(true);
                }}
              >
                <Lightbulb className="mr-1.5 h-3.5 w-3.5" />
                Dicas
              </Button>
              {canEditMission && (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="text-muted-foreground hover:text-foreground -ml-1"
                  onClick={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    setEditMissionDialogOpen(true);
                  }}
                >
                  <Pencil className="mr-1.5 h-3.5 w-3.5" />
                  Editar
                </Button>
              )}
            </div>
            <div className="max-w-2xl">
              <CollapsibleMissionDescription
                description={mission.description}
              />
            </div>
          </div>

          <div className="flex items-center gap-2 shrink-0">
            {cliParseErrorWithRepoChanges && (
              <Card className="border-blue-500/30 bg-gradient-to-br from-blue-50 to-blue-100/50 dark:from-blue-950/20 dark:to-blue-900/10">
                <CardContent className="pt-6">
                  <div className="flex gap-4">
                    <div className="shrink-0">
                      <div className="w-10 h-10 rounded-full bg-blue-500 flex items-center justify-center">
                        <CheckCircle2 className="h-6 w-6 text-white" />
                      </div>
                    </div>

                    <div className="flex-1 space-y-4">
                      <div>
                        <h3 className="text-lg font-semibold text-blue-900 dark:text-blue-100">
                          Código aplicado com sucesso
                        </h3>
                        <p className="text-sm text-blue-700 dark:text-blue-300 mt-1">
                          As alterações foram aplicadas no repositório. Revise
                          os arquivos na aba Código, edite se necessário, e
                          commite quando estiver pronto.
                        </p>
                      </div>

                      <div className="flex flex-wrap gap-2">
                        <Button
                          onClick={() => setCommitDialogOpen(true)}
                          disabled={isGenerating}
                          variant="default"
                          size="default"
                        >
                          <GitCommit className="mr-2 h-4 w-4" />
                          Revisar e Commitar
                        </Button>

                        <Button
                          onClick={handleRetryAfterParseError}
                          disabled={isGenerating}
                          variant="outline"
                          size="default"
                        >
                          {isGenerating ? (
                            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                          ) : (
                            <RotateCcw className="mr-2 h-4 w-4" />
                          )}
                          Tentar novamente
                        </Button>

                        <Button
                          onClick={handleDiscardChanges}
                          disabled={isGenerating}
                          variant="ghost"
                          size="default"
                          className="text-muted-foreground hover:text-muted-foreground/80"
                        >
                          <ArrowLeft className="mr-2 h-4 w-4" />
                          Descartar alterações
                        </Button>
                      </div>

                      <p className="text-xs text-blue-600 dark:text-blue-400 flex items-start gap-1.5">
                        <Info className="h-3.5 w-3.5 mt-0.5 shrink-0" />
                        <span>
                          Você pode editar o código sugerido diretamente na aba
                          Código antes de commitar.
                        </span>
                      </p>
                    </div>
                  </div>
                </CardContent>
              </Card>
            )}

            {/* Botão de resetar para estado applying travado */}
            {mission.status === "applying" && !isGenerating && (
              <Button
                variant="outline"
                onClick={() => handleResetToPlanning()}
                className="border-amber-500/50 hover:bg-amber-500/10"
              >
                <RotateCcw className="mr-2 h-4 w-4" />
                Voltar ao plano
              </Button>
            )}

            {/* Botões para estado generating_code */}
            {mission.status === "generating_code" && (
              <div className="flex items-center gap-2">
                {isGenerating ? (
                  <>
                    <Button
                      variant="destructive"
                      onClick={async () => {
                        if (
                          window.confirm(
                            "Cancelar geração de código e voltar ao plano?\n\nVocê poderá tentar gerar novamente depois.",
                          )
                        ) {
                          console.log(
                            "[DevCommandCenter] User cancelled code generation",
                          );
                          setIsGenerating(false);
                          if (missionId) {
                            updateStatus(missionId, "plan_generated");
                          }
                          toast.info("Geração de código cancelada");
                        }
                      }}
                    >
                      <XCircle className="mr-2 h-4 w-4" />
                      Cancelar geração
                    </Button>
                    <span className="text-sm text-muted-foreground flex items-center gap-2">
                      <Loader2 className="h-4 w-4 animate-spin" />
                      Gerando código...
                    </span>
                  </>
                ) : (
                  <Button
                    variant="outline"
                    onClick={() => handleResetToPlanning()}
                    className="border-amber-500/50 hover:bg-amber-500/10 hover:text-muted-foreground cursor-pointer"
                  >
                    <RotateCcw className="mr-2 h-4 w-4" />
                    Voltar ao plano
                  </Button>
                )}
              </div>
            )}
            {mission.status === "code_ready" && (
              <div className="flex flex-col gap-3">
                {/* Alerta inteligente para múltiplas tentativas */}
                {(mission.codeGenerationAttempts ?? 0) >= 2 && (
                  <Alert
                    variant={
                      (mission.codeGenerationAttempts ?? 0) >= 3
                        ? "destructive"
                        : "default"
                    }
                    className="max-w-2xl"
                  >
                    <AlertCircle className="h-4 w-4" />
                    <div>
                      <h4 className="font-semibold mb-1">
                        Código gerado {mission.codeGenerationAttempts}x
                      </h4>
                      <p className="text-sm">
                        {(mission.codeGenerationAttempts ?? 0) >= 3 ? (
                          <>
                            Múltiplas tentativas detectadas. Se continuar
                            errado, o problema pode estar no{" "}
                            <strong>plano</strong>, não no código. Considere
                            voltar ao plano e regenerá-lo com feedback.
                          </>
                        ) : (
                          <>
                            Se continuar errado, o problema pode estar no{" "}
                            <strong>plano</strong>, não no código. Considere
                            voltar ao plano e regenerá-lo com feedback.
                          </>
                        )}
                      </p>
                      <Button
                        variant="link"
                        onClick={handleDiscardChanges}
                        className="mt-2 p-0 h-auto text-sm cursor-pointer"
                      >
                        Descartar alterações →
                      </Button>
                    </div>
                  </Alert>
                )}
              </div>
            )}
          </div>
        </div>

        {/* Barra de ações em largura total: não espreme título/descrição */}
        {!cliParseErrorWithRepoChanges && mission.status === "created" && (
          <div className="mt-4 pt-4 border-t border-border flex flex-wrap items-center gap-3">
            <span className="text-sm text-muted-foreground">Gerar plano com:</span>
            <Select
              value={planProviderId || ""}
              onValueChange={(value) => {
                if (missionId && value) update(missionId, { planProviderId: value });
              }}
              disabled={isGenerating}
            >
              <SelectTrigger className="w-[200px]">
                <SelectValue placeholder="Selecione o provedor" />
              </SelectTrigger>
              <SelectContent>
                {providers
                  .filter((p) => p.isActive)
                  .map((p) => (
                    <SelectItem key={p.id} value={p.id}>
                      {p.name}
                    </SelectItem>
                  ))}
              </SelectContent>
            </Select>
            <Button
              onClick={handleGeneratePlan}
              disabled={isGenerating || !planProvider}
            >
              {isGenerating ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : (
                <Sparkles className="mr-2 h-4 w-4" />
              )}
              Gerar plano
            </Button>
          </div>
        )}
        {!cliParseErrorWithRepoChanges && mission.status === "plan_generated" && (
          <div className="mt-4 pt-4 border-t border-border space-y-3">
            <div className="flex flex-wrap items-center gap-x-6 gap-y-3">
              <div className="flex items-center gap-2 flex-wrap">
                <span className="text-sm text-muted-foreground">Plano:</span>
                <Select
                  value={planProviderId || ""}
                  onValueChange={(value) => {
                    if (missionId && value) update(missionId, { planProviderId: value });
                  }}
                  disabled={isGenerating}
                >
                  <SelectTrigger className="w-[180px]">
                    <SelectValue placeholder="Provedor" />
                  </SelectTrigger>
                  <SelectContent>
                    {providers
                      .filter((p) => p.isActive)
                      .map((p) => (
                        <SelectItem key={p.id} value={p.id}>
                          {p.name}
                        </SelectItem>
                      ))}
                  </SelectContent>
                </Select>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setRegeneratePlanDialogOpen(true)}
                  disabled={isGenerating || !planProvider}
                >
                  <Sparkles className="mr-1.5 h-3.5 w-3.5" />
                  Regenerar plano
                </Button>
              </div>
              <div className="flex items-center gap-2 flex-wrap">
                <span className="text-sm text-muted-foreground">Código:</span>
                <Select
                  value={codeProviderId || ""}
                  onValueChange={(value) => {
                    if (missionId && value) update(missionId, { codeProviderId: value });
                  }}
                  disabled={isGenerating}
                >
                  <SelectTrigger className="w-[180px]">
                    <SelectValue placeholder="Provedor" />
                  </SelectTrigger>
                  <SelectContent>
                    {providers
                      .filter((p) => p.isActive)
                      .map((p) => (
                        <SelectItem key={p.id} value={p.id}>
                          {p.name}
                        </SelectItem>
                      ))}
                  </SelectContent>
                </Select>
                <Button
                  size="sm"
                  onClick={handleGenerateCode}
                  disabled={isGenerating || !codeProvider}
                >
                  {isGenerating ? (
                    <Loader2 className="mr-1.5 h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <Code2 className="mr-1.5 h-3.5 w-3.5" />
                  )}
                  {isGenerating ? "Gerando..." : "Gerar código"}
                </Button>
              </div>
            </div>
            {project?.path &&
              typeof window !== "undefined" &&
              window.electronAPI?.git && (
                <div className="flex flex-wrap items-center gap-4">
                  <label className="flex items-center gap-2 text-sm text-muted-foreground cursor-pointer">
                    <Checkbox
                      checked={createBranchForMission}
                      onCheckedChange={(checked) =>
                        setCreateBranchForMission(checked === true)
                      }
                      disabled={isGenerating}
                    />
                    <span>Criar branch para esta missão</span>
                  </label>
                  {createBranchForMission && (
                    <div className="flex items-center gap-2">
                      <span className="text-sm text-muted-foreground">Nome:</span>
                      <Input
                        className="w-[200px]"
                        placeholder={missionId ? `mission/${missionId}` : "mission/..."}
                        value={branchNameForMission}
                        onChange={(e) => setBranchNameForMission(e.target.value)}
                        disabled={isGenerating}
                      />
                    </div>
                  )}
                </div>
              )}
          </div>
        )}

        {!cliParseErrorWithRepoChanges && mission.status === "code_ready" && (
          <div className="mt-4 pt-4 border-t border-border flex flex-wrap items-center gap-2">
            <Button
              onClick={handleApplyChanges}
              disabled={isApplying || isGenerating}
              className="cursor-pointer disabled:cursor-not-allowed"
            >
              {isApplying ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : (
                <Play className="mr-2 h-4 w-4" />
              )}
              Aplicar alterações
            </Button>
            <Button
              onClick={() => setRegenerateCodeDialogOpen(true)}
              variant="secondary"
              disabled={isApplying || isGenerating}
              className="cursor-pointer disabled:cursor-not-allowed"
            >
              <RotateCcw className="mr-2 h-4 w-4" />
              Tentar novamente
            </Button>
            <Button
              onClick={handleDiscardChanges}
              variant="outline"
              disabled={isApplying || isGenerating}
              className="cursor-pointer disabled:cursor-not-allowed hover:bg-muted hover:text-muted-foreground"
            >
              <ArrowLeft className="mr-2 h-4 w-4" />
              Descartar alterações
            </Button>
            <Button
              onClick={handleCancelMission}
              variant="ghost"
              size="sm"
              className="ml-auto text-muted-foreground hover:text-destructive cursor-pointer"
            >
              <XCircle className="mr-2 h-4 w-4" />
              Cancelar
            </Button>
          </div>
        )}

        {!cliParseErrorWithRepoChanges && mission.status === "completed" && (
          <div className="mt-4 pt-4 border-t border-border flex flex-col gap-1.5">
            {!mission.isCommitted && (
              <>
                <p className="text-xs text-muted-foreground">
                  As alterações foram aplicadas mas ainda não foram commitadas
                </p>
                <div className="flex flex-wrap gap-2">
                  <Button
                    onClick={() => setCommitDialogOpen(true)}
                    variant="default"
                    className="border-primary/50 hover:bg-primary/10"
                  >
                    <GitCommit className="mr-2 h-4 w-4" />
                    Commitar
                  </Button>
                  <Button
                    onClick={handleReject}
                    variant="outline"
                    className="border-destructive/50 hover:bg-destructive/10 hover:text-destructive"
                  >
                    <XCircle className="mr-2 h-4 w-4" />
                    Rejeitar
                  </Button>
                </div>
              </>
            )}
            {mission.isCommitted && !mission.isPushed && (
              <>
                <p className="text-xs text-muted-foreground">
                  Alterações commitadas localmente · Push envia ao remoto
                </p>
                <div className="flex flex-wrap gap-2">
                  <Button
                    onClick={handlePush}
                    disabled={isPushing}
                    variant="default"
                  >
                    {isPushing ? (
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    ) : (
                      <Upload className="mr-2 h-4 w-4" />
                    )}
                    Enviar ao remoto
                  </Button>
                  <Button
                    onClick={handleReject}
                    variant="outline"
                    className="border-destructive/50 hover:bg-destructive/10 hover:text-destructive"
                  >
                    <XCircle className="mr-2 h-4 w-4" />
                    Rejeitar
                  </Button>
                </div>
              </>
            )}
            {mission.isCommitted && mission.isPushed && (
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <CheckCircle2 className="h-4 w-4 text-green-500" />
                <span>Missão concluída e sincronizada com o remoto</span>
              </div>
            )}
          </div>
        )}

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
              <TabsTrigger
                value="plan"
                className="gap-2 cursor-pointer disabled:cursor-not-allowed disabled:opacity-50"
                disabled={isTabDisabled("plan")}
              >
                <FileText className="h-4 w-4" />
                Plano
                {mission.status === "planning" && (
                  <Loader2 className="h-3 w-3 ml-1 animate-spin text-blue-500" />
                )}
              </TabsTrigger>
              <TabsTrigger
                value="code"
                className="gap-2 cursor-pointer disabled:cursor-not-allowed disabled:opacity-50"
                disabled={isTabDisabled("code")}
              >
                <Code2 className="h-4 w-4" />
                Código
                {(mission.status === "generating_code" ||
                  mission.status === "applying") && (
                  <Loader2 className="h-3 w-3 ml-1 animate-spin text-blue-500" />
                )}
              </TabsTrigger>
              <TabsTrigger
                value="logs"
                className="gap-2 cursor-pointer"
                disabled={isTabDisabled("logs")}
              >
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

          <TabsContent
            value="plan"
            className="flex-1 overflow-auto p-6 mt-0 data-[state=active]:animate-in data-[state=active]:fade-in-0 data-[state=active]:zoom-in-95 data-[state=inactive]:animate-out data-[state=inactive]:fade-out-0 data-[state=inactive]:zoom-out-95 duration-200"
          >
            <PlanView
              plan={mission.plan}
              status={mission.status}
              provider={planProvider?.name}
            />
          </TabsContent>

          <TabsContent
            value="code"
            className="flex-1 overflow-auto p-6 mt-0 data-[state=active]:animate-in data-[state=active]:fade-in-0 data-[state=active]:zoom-in-95 data-[state=inactive]:animate-out data-[state=inactive]:fade-out-0 data-[state=inactive]:zoom-out-95 duration-200"
          >
            <CodeView
              code={recoveryCodeFromGit ?? mission.generatedCode}
              isGenerating={isGenerating}
              lastProgressMessage={lastProgressMessage}
              selectedFilePaths={
                mission.status === "code_ready" || cliParseErrorWithRepoChanges
                  ? selectedFilePaths
                  : undefined
              }
              onSelectionChange={
                mission.status === "code_ready" || cliParseErrorWithRepoChanges
                  ? (paths) => setSelectedFilePaths(new Set(paths))
                  : undefined
              }
              editedSuggestions={
                mission.status === "code_ready" || cliParseErrorWithRepoChanges
                  ? editedSuggestions
                  : undefined
              }
              onEditedSuggestionsChange={
                mission.status === "code_ready" || cliParseErrorWithRepoChanges
                  ? (path, content) =>
                      setEditedSuggestions((prev) => ({
                        ...prev,
                        [path]: content,
                      }))
                  : undefined
              }
            />
          </TabsContent>

          <TabsContent
            value="logs"
            className="flex-1 overflow-auto p-6 mt-0 data-[state=active]:animate-in data-[state=active]:fade-in-0 data-[state=active]:zoom-in-95 data-[state=inactive]:animate-out data-[state=inactive]:fade-out-0 data-[state=inactive]:zoom-out-95 duration-200"
          >
            <div className="space-y-3">
              {(usageStats.totalTokens > 0 ||
                usageStats.totalDurationMs > 0) && (
                <p className="text-sm text-muted-foreground">
                  Total:{" "}
                  {usageStats.totalTokens > 0
                    ? `${usageStats.totalTokens} tokens`
                    : "—"}
                  {usageStats.totalDurationMs > 0 && (
                    <>
                      {" · "}
                      {(usageStats.totalDurationMs / 60000).toFixed(1)} min
                    </>
                  )}
                </p>
              )}
              <LogsView logs={logs} />
            </div>
          </TabsContent>
        </Tabs>
      </div>

      <CommitDialog
        open={commitDialogOpen}
        onOpenChange={setCommitDialogOpen}
        defaultMessage={`DevCommandCenter: ${mission.title}`}
        onCommit={handleCommit}
        projectPath={project?.path ?? ""}
        status={commitDialogStatus}
        onPushComplete={async () => {
          // Marca como pushed no banco quando push é feito pelo diálogo
          if (missionId) {
            await update(missionId, { isPushed: true });
          }
        }}
      />
      <RegeneratePlanDialog
        open={regeneratePlanDialogOpen}
        onOpenChange={setRegeneratePlanDialogOpen}
        onSubmit={handleRegeneratePlan}
        isLoading={isGenerating}
      />
      <RegenerateCodeDialog
        open={regenerateCodeDialogOpen}
        onOpenChange={setRegenerateCodeDialogOpen}
        onSubmit={handleRegenerateCodeWithFeedback}
        isLoading={isGenerating}
        attempts={mission?.codeGenerationAttempts ?? 1}
      />
      <MissionTipsDialog open={tipsDialogOpen} onOpenChange={setTipsDialogOpen} />
      {projectId && mission && (
        <NewMissionDialog
          open={editMissionDialogOpen}
          onOpenChange={setEditMissionDialogOpen}
          projectId={projectId}
          defaultProviderId={project?.defaultProviderId ?? undefined}
          missionId={missionId ?? undefined}
          initialMission={{
            title: mission.title,
            description: mission.description,
            preserveInstructions: mission.preserveInstructions ?? "",
            providerId: mission.providerId ?? undefined,
            planProviderId: mission.planProviderId ?? undefined,
            codeProviderId: mission.codeProviderId ?? undefined,
          }}
          onOpenTips={() => setTipsDialogOpen(true)}
        />
      )}
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

/** Só trata como mensagem (callout) frases curtas de "nenhuma alteração"; resto mostra como código */
function isLikelyMessage(content: string): boolean {
  const trimmed = content.trim();
  const lines = trimmed.split(/\r?\n/).length;
  if (lines > 1) return false; // multi-line = código
  const lower = trimmed.toLowerCase();
  const noChangePhrases =
    /^(already|no further|no changes|nenhuma alteração|no edit|skip|unchanged)/i;
  return noChangePhrases.test(lower) || trimmed.length < 120;
}

/** Detecta se o conteúdo parece diff unificado (linhas com +, -, ---, +++ ou contexto com espaço). */
function looksLikeUnifiedDiff(content: string): boolean {
  const trimmed = content.trim();
  if (!trimmed) return false;
  const lines = trimmed.split(/\r?\n/);
  return lines.some(
    (line) =>
      line.startsWith("+") ||
      line.startsWith("-") ||
      line.startsWith("--- ") ||
      line.startsWith("+++ "),
  );
}

/** Mapa de extensão de arquivo para linguagem do Prism/SyntaxHighlighter */
const EXT_TO_LANG: Record<string, string> = {
  ts: "typescript",
  tsx: "typescript",
  js: "javascript",
  jsx: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  css: "css",
  scss: "scss",
  sass: "sass",
  html: "html",
  htm: "html",
  json: "json",
  md: "markdown",
  mdx: "markdown",
  py: "python",
  rb: "ruby",
  go: "go",
  rs: "rust",
  java: "java",
  kt: "kotlin",
  sql: "sql",
  sh: "bash",
  bash: "bash",
  yaml: "yaml",
  yml: "yaml",
  xml: "xml",
  svg: "xml",
  diff: "diff",
};

function getLanguageFromPath(filePath: string, isDiff = false): string {
  if (isDiff) return "diff";
  const ext = filePath.split(".").pop()?.toLowerCase() ?? "";
  return EXT_TO_LANG[ext] ?? "text";
}

/** Linguagem para Prism (react-simple-code-editor); Prism usa 'markup' para HTML, 'tsx' para .tsx, etc. */
function getPrismLanguage(filePath: string): string {
  const ext = filePath.split(".").pop()?.toLowerCase() ?? "";
  const prismMap: Record<string, string> = {
    tsx: "tsx",
    ts: "typescript",
    jsx: "jsx",
    js: "javascript",
    mjs: "javascript",
    cjs: "javascript",
    css: "css",
    scss: "scss",
    sass: "sass",
    html: "markup",
    htm: "markup",
    json: "json",
    md: "markdown",
    mdx: "markdown",
    py: "python",
    yaml: "yaml",
    yml: "yaml",
    sh: "bash",
    bash: "bash",
    diff: "diff",
  };
  return prismMap[ext] ?? "markup";
}

/**
 * Memoized EditableCodeBlock - prevents re-renders when props haven't changed
 */
const EditableCodeBlock = memo(function EditableCodeBlock({
  filePath,
  value,
  onChange,
}: {
  filePath: string;
  value: string;
  onChange: (value: string) => void;
}) {
  const lang = getPrismLanguage(filePath);
  const grammar = Prism.languages[lang] ?? Prism.languages.markup;
  return (
    <Editor
      value={value}
      onValueChange={onChange}
      highlight={(code: string) => Prism.highlight(code, grammar, lang)}
      padding={16}
      style={{
        fontFamily: "var(--font-mono), ui-monospace, monospace",
        fontSize: "0.75rem",
        minHeight: "360px",
        background: "hsl(var(--muted) / 0.3)",
      }}
      textareaClassName="focus:outline-none"
    />
  );
});

/**
 * Memoized CodeBlock - prevents expensive SyntaxHighlighter re-renders
 */
const CodeBlock = memo(function CodeBlock({
  filePath,
  content,
  isDiff,
}: {
  filePath: string;
  content: string;
  isDiff?: boolean;
}) {
  const lang = getLanguageFromPath(filePath, isDiff);
  return (
    <SyntaxHighlighter
      language={lang}
      style={oneDark}
      showLineNumbers={false}
      wrapLongLines
      customStyle={{
        margin: 0,
        padding: "1rem",
        fontSize: "0.75rem",
        background: "hsl(var(--muted) / 0.3)",
        borderRadius: "var(--radius)",
      }}
      codeTagProps={{ style: { fontFamily: "inherit" } }}
    >
      {content}
    </SyntaxHighlighter>
  );
});

const CODE_GENERATION_STEPS = [
  { id: "prep", label: "Preparando ambiente" },
  { id: "connect", label: "Conectando ao modelo" },
  { id: "generate", label: "Gerando alterações" },
  { id: "process", label: "Processando resposta" },
] as const;

/**
 * Memoized FileAccordionItem - only renders content when open (lazy rendering)
 * This prevents expensive SyntaxHighlighter processing for closed items
 */
const FileAccordionItem = memo(function FileAccordionItem({
  file,
  isOpen,
  isSelected,
  showCheckbox,
  isSuggestedEditable,
  getSuggestedContent,
  toggleFileSelection,
  onEditedSuggestionsChange,
}: {
  file: {
    path: string;
    action: string;
    originalContent?: string;
    suggestedContent?: string;
    diff?: string;
  };
  isOpen: boolean;
  isSelected: boolean;
  showCheckbox: boolean;
  isSuggestedEditable: boolean;
  getSuggestedContent: (path: string, fallback: string) => string;
  toggleFileSelection: (path: string) => void;
  onEditedSuggestionsChange?: (path: string, content: string) => void;
}) {
  const hasOriginal = (file.originalContent ?? "").trim().length > 0;
  const hasSuggested = (file.suggestedContent ?? "").trim().length > 0;
  const hasDiff = (file.diff ?? "").trim().length > 0;
  const suggestedIsMessage =
    hasSuggested && isLikelyMessage(file.suggestedContent!);
  const diffIsMessage = hasDiff && !looksLikeUnifiedDiff(file.diff ?? "");
  const tabCount = [hasOriginal, hasSuggested, hasDiff].filter(Boolean).length;
  const showTabs = tabCount > 1;

  return (
    <AccordionItem value={file.path}>
      <AccordionTrigger className="py-3 hover:no-underline [&[data-state=open]>svg]:rotate-180">
        <div className="flex items-center justify-between gap-2 w-full pr-2">
          <div className="flex items-center gap-2 min-w-0 flex-1">
            {showCheckbox && (
              <div
                role="button"
                tabIndex={0}
                onClick={(e) => e.stopPropagation()}
                onKeyDown={(e) => e.stopPropagation()}
                className="shrink-0"
              >
                <Checkbox
                  id={`file-${file.path}`}
                  checked={isSelected}
                  onCheckedChange={() => toggleFileSelection(file.path)}
                />
              </div>
            )}
            <code className="text-sm font-medium truncate">{file.path}</code>
          </div>
          <Badge
            variant={
              file.action === "create"
                ? "default"
                : file.action === "delete"
                  ? "destructive"
                  : "secondary"
            }
            className="shrink-0"
          >
            {file.action}
          </Badge>
        </div>
      </AccordionTrigger>
      {(hasOriginal || hasSuggested || hasDiff) && (
        <AccordionContent>
          {/* LAZY RENDERING: Only render expensive content when accordion is open */}
          {isOpen && (
            <>
              {showTabs ? (
                <Tabs
                  defaultValue={
                    hasOriginal
                      ? "original"
                      : hasSuggested
                        ? "suggested"
                        : "diff"
                  }
                  className="w-full"
                >
                  <TabsList className="flex w-full">
                    {hasOriginal && (
                      <TabsTrigger value="original" className="flex-1">
                        Original
                      </TabsTrigger>
                    )}
                    {hasSuggested && (
                      <TabsTrigger value="suggested" className="flex-1">
                        Sugerido
                      </TabsTrigger>
                    )}
                    {hasDiff && (
                      <TabsTrigger value="diff" className="flex-1">
                        Diff
                      </TabsTrigger>
                    )}
                  </TabsList>
                  {hasOriginal && (
                    <TabsContent value="original" className="mt-3">
                      <ScrollArea className="h-[360px] rounded-md border bg-muted/30">
                        <CodeBlock
                          filePath={file.path}
                          content={file.originalContent ?? ""}
                        />
                      </ScrollArea>
                    </TabsContent>
                  )}
                  {hasSuggested && (
                    <TabsContent value="suggested" className="mt-3">
                      {isSuggestedEditable ? (
                        <>
                          <p className="flex items-center gap-2 text-sm text-muted-foreground mb-2">
                            <Pencil className="h-4 w-4 shrink-0" />
                            Ajustes leves aqui; para refatorações maiores use
                            sua IDE.
                          </p>
                          <div className="rounded-md border border-border overflow-hidden">
                            <EditableCodeBlock
                              filePath={file.path}
                              value={getSuggestedContent(
                                file.path,
                                file.suggestedContent ?? "",
                              )}
                              onChange={(value) =>
                                onEditedSuggestionsChange!(file.path, value)
                              }
                            />
                          </div>
                        </>
                      ) : suggestedIsMessage ? (
                        <Alert className="text-muted-foreground">
                          <p className="text-sm whitespace-pre-wrap">
                            {getSuggestedContent(
                              file.path,
                              file.suggestedContent ?? "",
                            )}
                          </p>
                        </Alert>
                      ) : (
                        <ScrollArea className="h-[360px] rounded-md border bg-muted/30">
                          <CodeBlock
                            filePath={file.path}
                            content={getSuggestedContent(
                              file.path,
                              file.suggestedContent ?? "",
                            )}
                          />
                        </ScrollArea>
                      )}
                    </TabsContent>
                  )}
                  {hasDiff && (
                    <TabsContent value="diff" className="mt-3">
                      {diffIsMessage ? (
                        <Alert className="text-muted-foreground">
                          <p className="text-xs font-medium text-muted-foreground mb-1">
                            Nota do assistente
                          </p>
                          <p className="text-sm whitespace-pre-wrap">
                            {file.diff}
                          </p>
                        </Alert>
                      ) : (
                        <ScrollArea className="h-[360px] rounded-md border bg-muted/30">
                          <CodeBlock
                            filePath={file.path}
                            content={file.diff ?? ""}
                            isDiff
                          />
                        </ScrollArea>
                      )}
                    </TabsContent>
                  )}
                </Tabs>
              ) : (
                <div className="space-y-4 mt-2">
                  {hasOriginal && (
                    <div>
                      <h4 className="text-xs font-medium text-muted-foreground mb-2">
                        Conteúdo atual
                      </h4>
                      <ScrollArea className="h-[360px] rounded-md border bg-muted/30">
                        <CodeBlock
                          filePath={file.path}
                          content={file.originalContent ?? ""}
                        />
                      </ScrollArea>
                    </div>
                  )}
                  {hasSuggested && (
                    <div>
                      <h4 className="text-xs font-medium text-muted-foreground mb-2">
                        Código sugerido
                        {isSuggestedEditable && (
                          <span className="font-normal text-muted-foreground ml-1">
                            (editável)
                          </span>
                        )}
                      </h4>
                      {isSuggestedEditable ? (
                        <>
                          <p className="flex items-center gap-2 text-sm text-muted-foreground mb-2">
                            <Pencil className="h-4 w-4 shrink-0" />
                            Ajustes leves aqui; para refatorações maiores use
                            sua IDE.
                          </p>
                          <div className="rounded-md border border-border overflow-hidden">
                            <EditableCodeBlock
                              filePath={file.path}
                              value={getSuggestedContent(
                                file.path,
                                file.suggestedContent ?? "",
                              )}
                              onChange={(value) =>
                                onEditedSuggestionsChange!(file.path, value)
                              }
                            />
                          </div>
                        </>
                      ) : suggestedIsMessage ? (
                        <Alert className="text-muted-foreground">
                          <p className="text-sm whitespace-pre-wrap">
                            {getSuggestedContent(
                              file.path,
                              file.suggestedContent ?? "",
                            )}
                          </p>
                        </Alert>
                      ) : (
                        <ScrollArea className="h-[360px] rounded-md border bg-muted/30">
                          <CodeBlock
                            filePath={file.path}
                            content={getSuggestedContent(
                              file.path,
                              file.suggestedContent ?? "",
                            )}
                          />
                        </ScrollArea>
                      )}
                    </div>
                  )}
                  {hasDiff && (
                    <div>
                      <h4 className="text-xs font-medium text-muted-foreground mb-2">
                        Alterações (diff)
                      </h4>
                      {diffIsMessage ? (
                        <Alert className="text-muted-foreground">
                          <p className="text-xs font-medium text-muted-foreground mb-1">
                            Nota do assistente
                          </p>
                          <p className="text-sm whitespace-pre-wrap">
                            {file.diff}
                          </p>
                        </Alert>
                      ) : (
                        <ScrollArea className="h-[360px] rounded-md border bg-muted/30">
                          <CodeBlock
                            filePath={file.path}
                            content={file.diff ?? ""}
                            isDiff
                          />
                        </ScrollArea>
                      )}
                    </div>
                  )}
                </div>
              )}
            </>
          )}
        </AccordionContent>
      )}
    </AccordionItem>
  );
});

function CodeView({
  code,
  isGenerating = false,
  lastProgressMessage,
  selectedFilePaths,
  onSelectionChange,
  editedSuggestions,
  onEditedSuggestionsChange,
}: {
  code:
    | {
        files: {
          path: string;
          action: string;
          originalContent?: string;
          suggestedContent?: string;
          diff?: string;
        }[];
        summary?: string;
      }
    | null
    | undefined;
  isGenerating?: boolean;
  lastProgressMessage?: string;
  selectedFilePaths?: Set<string>;
  onSelectionChange?: (paths: string[]) => void;
  editedSuggestions?: Record<string, string>;
  onEditedSuggestionsChange?: (path: string, content: string) => void;
}) {
  // Track which accordion items are open for lazy rendering
  const [openItems, setOpenItems] = useState<string[]>([]);

  const getSuggestedContent = useCallback(
    (path: string, fallback: string) => editedSuggestions?.[path] ?? fallback,
    [editedSuggestions],
  );

  const isSuggestedEditable = Boolean(onEditedSuggestionsChange);

  const toggleFileSelection = useCallback(
    (path: string) => {
      if (!selectedFilePaths || !onSelectionChange) return;
      const next = new Set(selectedFilePaths);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      onSelectionChange(Array.from(next));
    },
    [selectedFilePaths, onSelectionChange],
  );

  const selectAll = useCallback(() => {
    if (!code?.files.length || !onSelectionChange) return;
    onSelectionChange(code.files.map((f) => f.path));
  }, [code?.files, onSelectionChange]);

  const selectNone = useCallback(() => {
    onSelectionChange?.([]);
  }, [onSelectionChange]);

  // Handler for accordion value changes (tracks open items for lazy rendering)
  const handleAccordionValueChange = useCallback((value: string[]) => {
    setOpenItems(value);
  }, []);

  if (isGenerating && !code) {
    return (
      <Card className="border-dashed">
        <CardContent className="flex flex-col items-center justify-center py-12 px-8 max-w-lg mx-auto">
          <div className="flex items-center gap-3 mb-6">
            <Loader2 className="h-10 w-10 text-primary animate-spin shrink-0" />
            <h3 className="text-lg font-medium">Gerando código...</h3>
          </div>
          <Progress variant="indeterminate" className="w-full mb-6 h-2" />
          <ul className="w-full space-y-3 mb-6">
            {CODE_GENERATION_STEPS.map((step, index) => (
              <li
                key={step.id}
                className="flex items-center gap-3 text-sm text-muted-foreground"
              >
                {index < 2 ? (
                  <CheckCircle2 className="h-4 w-4 text-green-500 shrink-0" />
                ) : index === 2 ? (
                  <Loader2 className="h-4 w-4 text-primary animate-spin shrink-0" />
                ) : (
                  <Circle className="h-4 w-4 text-muted-foreground/50 shrink-0" />
                )}
                <span
                  className={
                    index === 2 ? "text-foreground font-medium" : undefined
                  }
                >
                  {step.label}
                </span>
              </li>
            ))}
          </ul>
          <p className="text-sm text-muted-foreground text-center w-full bg-muted/50 rounded-md px-4 py-3">
            {lastProgressMessage ?? "Iniciando geração de código..."}
          </p>
        </CardContent>
      </Card>
    );
  }

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
        <div className="flex items-center justify-between gap-4 mb-2">
          <h3 className="text-sm font-medium">
            Arquivos alterados ({code.files.length})
          </h3>
          {selectedFilePaths !== undefined && onSelectionChange && (
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <button
                type="button"
                onClick={selectAll}
                className="hover:text-foreground underline"
              >
                Selecionar todos
              </button>
              <span>·</span>
              <button
                type="button"
                onClick={selectNone}
                className="hover:text-foreground underline"
              >
                Desmarcar todos
              </button>
            </div>
          )}
        </div>
        {selectedFilePaths !== undefined && onSelectionChange && (
          <p className="text-xs text-muted-foreground mb-4">
            Marque os arquivos que deseja aplicar ao projeto. Ao clicar em{" "}
            <strong>Aplicar alterações</strong>, apenas os selecionados serão
            gravados.
          </p>
        )}
        {isSuggestedEditable && (
          <p className="text-xs text-muted-foreground mb-4">
            Edite pequenos ajustes aqui; para mudanças maiores, copie o código
            para sua IDE.
          </p>
        )}
        <Accordion
          type="multiple"
          value={openItems}
          onValueChange={handleAccordionValueChange}
          className="w-full rounded-md border border-border"
        >
          {code.files.map((file) => (
            <FileAccordionItem
              key={file.path}
              file={file}
              isOpen={openItems.includes(file.path)}
              isSelected={
                selectedFilePaths === undefined ||
                selectedFilePaths.has(file.path)
              }
              showCheckbox={
                selectedFilePaths !== undefined &&
                onSelectionChange !== undefined
              }
              isSuggestedEditable={isSuggestedEditable}
              getSuggestedContent={getSuggestedContent}
              toggleFileSelection={toggleFileSelection}
              onEditedSuggestionsChange={onEditedSuggestionsChange}
            />
          ))}
        </Accordion>
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

  // Ordenação cronológica: mais recente no topo. Mesmo horário: nosso resumo (response) acima de info do Claude.
  const getTimestamp = (log: (typeof logs)[0]) => {
    const t = new Date(log.createdAt as string | Date).getTime();
    return Number.isNaN(t) ? 0 : t;
  };
  const safeFormatTime = (val: unknown) => {
    if (val == null) return "—";
    const d = val instanceof Date ? val : new Date(val as string | number);
    return Number.isNaN(d.getTime()) ? "—" : format(d, "HH:mm:ss");
  };
  // Desempate: nossos resumos (response e error) acima de info do Claude (ex.: Recebendo resposta...)
  const typeOrderForTie = (type: MissionLogType) =>
    type === "response" ? 0 : type === "error" ? 1 : type === "info" ? 2 : 3;
  const sortedLogs = [...logs].sort((a, b) => {
    const ta = getTimestamp(a);
    const tb = getTimestamp(b);
    if (tb !== ta) return tb - ta; // mais recente primeiro
    return typeOrderForTie(a.type) - typeOrderForTie(b.type); // response (nosso) acima de info (Claude)
  });

  return (
    <div className="space-y-2">
      {sortedLogs.map((log) => {
        const config = logTypeConfig[log.type] ?? logTypeConfig.info;
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
                  {safeFormatTime(log.createdAt)}
                </span>
                {typeof log.metadata?.tokensUsed === "number" && (
                  <span>{log.metadata.tokensUsed} tokens</span>
                )}
                {typeof log.metadata?.durationMs === "number" && (
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
