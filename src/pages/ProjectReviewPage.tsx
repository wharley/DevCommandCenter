import React, { useCallback, useEffect, useMemo, useState } from "react";
import {
  Link,
  useNavigate,
  useParams,
  useSearchParams,
} from "react-router-dom";
import {
  ArrowLeft,
  Check,
  GitBranch,
  Loader2,
  Trash2,
  FileCode,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Checkbox } from "@/components/ui/checkbox";
import { ScrollArea } from "@/components/ui/scroll-area";
import { DiffCodeBlock } from "@/components/diff-code-block";
import { useConfirmDialog } from "@/components/providers/confirm-dialog-provider";
import { useMissions, useProjects } from "@/hooks/use-data";
import { toast } from "sonner";

const REVIEW_WALL_STATUSES = ["ready_for_review", "apply_failed"] as const;

export default function ProjectReviewPage() {
  const { id: projectId } = useParams<{ id: string }>();
  const [searchParams] = useSearchParams();
  const missionIdFromQuery = searchParams.get("missionId");
  const navigate = useNavigate();
  const { projects } = useProjects();
  const { missions } = useMissions(projectId ?? undefined);
  const { confirmDialog } = useConfirmDialog();

  const project = useMemo(
    () =>
      projectId ? (projects.find((p) => p.id === projectId) ?? null) : null,
    [projectId, projects],
  );

  const reviewableMissions = useMemo(
    () =>
      missions.filter(
        (m) =>
          m.missionType === "agents_cli" &&
          m.wallStatus &&
          REVIEW_WALL_STATUSES.includes(
            m.wallStatus as (typeof REVIEW_WALL_STATUSES)[number],
          ),
      ),
    [missions],
  );

  const [selectedMissionId, setSelectedMissionId] = useState<string | null>(
    missionIdFromQuery,
  );
  const [diffs, setDiffs] = useState<{
    loading: boolean;
    error: string | null;
    files: Array<{ path: string; status: string; diff: string }>;
    summary: {
      changedFiles: number;
      insertions: number;
      deletions: number;
    } | null;
  }>({ loading: false, error: null, files: [], summary: null });
  const [includedFiles, setIncludedFiles] = useState<Set<string>>(new Set());
  const [targetBranch, setTargetBranch] = useState<string>("");
  const [localBranches, setLocalBranches] = useState<string[]>([]);
  const [commitMessage, setCommitMessage] = useState("");
  const [isApplying, setIsApplying] = useState(false);
  const [isDiscarding, setIsDiscarding] = useState(false);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);

  const selectedMission = useMemo(
    () =>
      selectedMissionId
        ? (reviewableMissions.find((m) => m.id === selectedMissionId) ?? null)
        : null,
    [selectedMissionId, reviewableMissions],
  );

  useEffect(() => {
    if (
      missionIdFromQuery &&
      reviewableMissions.some((m) => m.id === missionIdFromQuery)
    ) {
      setSelectedMissionId(missionIdFromQuery);
    }
  }, [missionIdFromQuery, reviewableMissions]);

  useEffect(() => {
    if (!selectedMissionId) {
      setDiffs({ loading: false, error: null, files: [], summary: null });
      setIncludedFiles(new Set());
      setSelectedFile(null);
      return;
    }
    if (!window.electronAPI?.worktree?.getDiffs) return;
    setDiffs((prev) => ({ ...prev, loading: true, error: null }));
    window.electronAPI.worktree
      .getDiffs(selectedMissionId)
      .then((res) => {
        if (res.success && res.files) {
          setDiffs({
            loading: false,
            error: res.error ?? null,
            files: res.files,
            summary: res.summary ?? null,
          });
          setIncludedFiles(new Set(res.files.map((f) => f.path)));
          setSelectedFile(res.files[0]?.path ?? null);
        } else {
          setDiffs({
            loading: false,
            error: res.error ?? "Falha ao carregar diffs",
            files: [],
            summary: null,
          });
        }
      })
      .catch(() => {
        setDiffs({
          loading: false,
          error: "Erro ao carregar diffs",
          files: [],
          summary: null,
        });
      });
  }, [selectedMissionId]);

  useEffect(() => {
    if (!project?.path || !window.electronAPI?.git?.getLocalBranches) return;
    window.electronAPI.git.getLocalBranches(project.path).then((branches) => {
      setLocalBranches(branches ?? []);
      if (branches?.length && !targetBranch) {
        window.electronAPI?.git?.getCurrentBranch(project.path).then((cur) => {
          setTargetBranch(cur?.trim() || branches[0] || "");
        });
      }
    });
  }, [project?.path]);

  const toggleFile = useCallback((path: string) => {
    setIncludedFiles((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const selectedFileDiff = useMemo(() => {
    if (!selectedFile || !diffs.files.length) return null;
    return diffs.files.find((f) => f.path === selectedFile) ?? null;
  }, [selectedFile, diffs.files]);

  const handleApply = useCallback(
    async (withCommit: boolean) => {
      if (
        !selectedMissionId ||
        !project?.path ||
        !window.electronAPI?.worktree?.applyMissionPatch
      )
        return;
      setIsApplying(true);
      try {
        let branchToUse = targetBranch?.trim();
        if (!branchToUse && window.electronAPI?.git?.getCurrentBranch) {
          branchToUse =
            (await window.electronAPI.git.getCurrentBranch(project.path)) ||
            "main";
        }
        if (!branchToUse) branchToUse = "main";
        const files = Array.from(includedFiles);
        const result = await window.electronAPI.worktree.applyMissionPatch(
          selectedMissionId,
          branchToUse,
          {
            includeFiles: files.length > 0 ? files : undefined,
            commit: withCommit,
            message: withCommit
              ? commitMessage || "Apply mission patch"
              : undefined,
          },
        );
        if (result?.success) {
          toast.success(
            withCommit ? "Patch aplicado e commitado" : "Patch aplicado",
          );
          setSelectedMissionId(null);
          if (projectId) navigate(`/project/${projectId}/agents`);
        } else {
          toast.error(result?.error ?? "Erro ao aplicar");
        }
      } catch (e) {
        toast.error(e instanceof Error ? e.message : "Erro ao aplicar");
      } finally {
        setIsApplying(false);
      }
    },
    [
      selectedMissionId,
      targetBranch,
      includedFiles,
      commitMessage,
      projectId,
      project?.path,
      navigate,
    ],
  );

  const handleDiscard = useCallback(async () => {
    if (!selectedMissionId) return;
    const confirmed = await confirmDialog({
      title: "Descartar alterações da missão?",
      description:
        "O worktree será removido e as alterações não aplicadas serão perdidas.",
      confirmLabel: "Descartar",
      cancelLabel: "Cancelar",
    });
    if (!confirmed) return;
    setIsDiscarding(true);
    try {
      const result =
        await window.electronAPI?.worktree?.discard(selectedMissionId);
      if (result?.success) {
        toast.success("Worktree descartado");
        setSelectedMissionId(null);
        if (projectId) navigate(`/project/${projectId}/agents`);
      } else {
        toast.error(result?.error ?? "Erro ao descartar");
      }
    } catch (e) {
      toast.error(e instanceof Error ? e.message : "Erro ao descartar");
    } finally {
      setIsDiscarding(false);
    }
  }, [selectedMissionId, confirmDialog, projectId, navigate]);

  if (!project || !projectId) {
    return (
      <div className="p-6">
        <p className="text-muted-foreground">Projeto não encontrado</p>
        <Button variant="outline" className="mt-4" asChild>
          <Link to="/">Voltar</Link>
        </Button>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col p-6">
      <div className="mb-4 flex items-center gap-4">
        <Button
          variant="ghost"
          size="icon"
          onClick={() => navigate(`/project/${projectId}/agents`)}
        >
          <ArrowLeft className="h-5 w-5" />
        </Button>
        <div>
          <h1 className="text-xl font-semibold">Revisar mudanças</h1>
          <p className="text-sm text-muted-foreground">
            Selecione uma missão, revise os diffs e aplique ou descarte no
            repositório principal.
          </p>
        </div>
      </div>

      <div className="grid flex-1 gap-4 overflow-hidden md:grid-cols-[280px_1fr]">
        <Card className="flex flex-col overflow-hidden">
          <CardHeader className="pb-2">
            <CardTitle className="text-sm">Missão</CardTitle>
            <CardDescription>
              Prontas para revisão ou com falha de apply
            </CardDescription>
          </CardHeader>
          <CardContent className="flex-1 overflow-hidden p-0">
            <Select
              value={selectedMissionId ?? "__none__"}
              onValueChange={(v) =>
                setSelectedMissionId(v === "__none__" ? null : v)
              }
            >
              <SelectTrigger className="mx-3 mb-2">
                <SelectValue placeholder="Selecione uma missão" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__none__">— Nenhuma —</SelectItem>
                {reviewableMissions.map((m) => (
                  <SelectItem key={m.id} value={m.id}>
                    <span className="truncate">{m.title}</span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {reviewableMissions.length === 0 && (
              <p className="px-3 pb-2 text-xs text-muted-foreground">
                Nenhuma missão pronta para revisão.
              </p>
            )}
          </CardContent>
        </Card>

        <div className="flex min-h-0 flex-col gap-4 overflow-hidden">
          {!selectedMission ? (
            <Card className="flex flex-1 items-center justify-center">
              <p className="text-muted-foreground">
                Selecione uma missão para ver os diffs.
              </p>
            </Card>
          ) : (
            <>
              <Card>
                <CardHeader className="pb-2">
                  <div className="flex flex-wrap items-center gap-3">
                    <div className="flex items-center gap-2">
                      <GitBranch className="h-4 w-4 text-muted-foreground" />
                      <span className="text-sm font-medium">
                        Branch de destino
                      </span>
                    </div>
                    <Select
                      value={targetBranch || "__current__"}
                      onValueChange={(v) =>
                        setTargetBranch(v === "__current__" ? "" : v)
                      }
                    >
                      <SelectTrigger className="w-[200px]">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="__current__">Atual</SelectItem>
                        {localBranches.map((b) => (
                          <SelectItem key={b} value={b}>
                            {b}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                </CardHeader>
              </Card>

              <div className="grid min-h-0 flex-1 gap-4 overflow-hidden md:grid-cols-[240px_1fr]">
                <Card className="flex flex-col overflow-hidden">
                  <CardHeader className="pb-2">
                    <CardTitle className="text-sm">Arquivos</CardTitle>
                  </CardHeader>
                  <CardContent className="flex-1 overflow-hidden p-0">
                    {diffs.loading ? (
                      <div className="flex items-center justify-center p-4">
                        <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                      </div>
                    ) : diffs.error ? (
                      <p className="p-3 text-sm text-destructive">
                        {diffs.error}
                      </p>
                    ) : diffs.files.length === 0 ? (
                      <p className="p-3 text-sm text-muted-foreground">
                        Nenhuma alteração detectada.
                      </p>
                    ) : (
                      <ScrollArea className="h-full">
                        <div className="space-y-1 p-2">
                          {diffs.files.map((f) => (
                            <div
                              key={f.path}
                              className="flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-muted/50"
                            >
                              <Checkbox
                                checked={includedFiles.has(f.path)}
                                onCheckedChange={() => toggleFile(f.path)}
                              />
                              <button
                                type="button"
                                className={`flex min-w-0 flex-1 items-center gap-2 text-left text-sm ${
                                  selectedFile === f.path
                                    ? "font-medium text-foreground"
                                    : "text-muted-foreground"
                                }`}
                                onClick={() => setSelectedFile(f.path)}
                              >
                                <FileCode className="h-4 w-4 shrink-0" />
                                <span className="truncate">{f.path}</span>
                              </button>
                            </div>
                          ))}
                        </div>
                      </ScrollArea>
                    )}
                  </CardContent>
                </Card>

                <Card className="flex flex-col overflow-hidden">
                  <CardHeader className="pb-2">
                    <CardTitle className="text-sm">Diff</CardTitle>
                    {selectedFile && (
                      <CardDescription className="truncate font-mono text-xs">
                        {selectedFile}
                      </CardDescription>
                    )}
                  </CardHeader>
                  <CardContent className="min-h-0 flex-1 overflow-hidden p-0">
                    {selectedFileDiff?.diff ? (
                      <ScrollArea className="h-full">
                        <div className="p-3">
                          <DiffCodeBlock content={selectedFileDiff.diff} />
                        </div>
                      </ScrollArea>
                    ) : (
                      <p className="p-4 text-sm text-muted-foreground">
                        Selecione um arquivo para ver o diff.
                      </p>
                    )}
                  </CardContent>
                </Card>
              </div>

              <Card>
                <CardContent className="flex flex-wrap items-center gap-3 pt-4">
                  <input
                    type="text"
                    placeholder="Mensagem de commit (se Aplicar + Commit)"
                    className="flex-1 min-w-[200px] rounded-md border bg-background px-3 py-2 text-sm"
                    value={commitMessage}
                    onChange={(e) => setCommitMessage(e.target.value)}
                  />
                  <div className="flex gap-2">
                    <Button
                      onClick={() => void handleApply(false)}
                      disabled={
                        isApplying || isDiscarding || diffs.files.length === 0
                      }
                    >
                      {isApplying ? (
                        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                      ) : (
                        <Check className="mr-2 h-4 w-4" />
                      )}
                      Aplicar patch
                    </Button>
                    <Button
                      variant="secondary"
                      onClick={() => void handleApply(true)}
                      disabled={
                        isApplying || isDiscarding || diffs.files.length === 0
                      }
                    >
                      {isApplying ? (
                        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                      ) : null}
                      Aplicar + Commit
                    </Button>
                    <Button
                      variant="destructive"
                      onClick={() => void handleDiscard()}
                      disabled={isApplying || isDiscarding}
                    >
                      {isDiscarding ? (
                        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                      ) : (
                        <Trash2 className="mr-2 h-4 w-4" />
                      )}
                      Descartar
                    </Button>
                  </div>
                </CardContent>
              </Card>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
