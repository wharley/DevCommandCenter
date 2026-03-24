"use client";

import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Bell,
  Bot,
  ChevronRight,
  FolderGit2,
  GitBranch,
  GitPullRequest,
  Loader2,
  Merge,
  Plus,
  Settings,
  Terminal,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import { formatDistanceToNow } from "date-fns";
import { ptBR } from "date-fns/locale";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { EmbeddedTerminal } from "@/components/embedded-terminal";
import { AddProjectDialog } from "@/components/dialogs/add-project-dialog";
import { useConfirmDialog } from "@/components/providers/confirm-dialog-provider";
import { usePanes, useProjects, useProviders } from "@/hooks/use-data";
import SettingsPage from "@/src/pages/SettingsPage";
import { normalizeComb, normalizeCombs } from "@/lib/database/normalize";
import type { Comb, CreateCombDTO, Pane, Project, Provider } from "@/lib/database/types";
import {
  useTerminalAttentionToasts,
  type TerminalAttentionRecord,
} from "@/hooks/use-terminal-attention-toasts";

const CLI_PROVIDER_TYPES = ["codex", "claude-code", "gemini", "cursor"] as const;

function isCliProviderType(type: string): type is (typeof CLI_PROVIDER_TYPES)[number] {
  return CLI_PROVIDER_TYPES.includes(type as (typeof CLI_PROVIDER_TYPES)[number]);
}

function buildCliCommand(provider: Provider | null): string | undefined {
  if (!provider) return undefined;
  const t = provider.type;
  const cliPath = provider.cliPath?.trim();
  const usePath = cliPath && (cliPath.startsWith("/") || /^[A-Za-z]:\\/.test(cliPath));
  if (t === "codex") return usePath ? cliPath : "codex";
  if (t === "claude-code") return usePath ? cliPath : "claude";
  if (t === "gemini") return usePath ? cliPath : "gemini";
  if (t === "cursor") return usePath ? cliPath : "cursor-agent";
  return undefined;
}

function NewWorkspaceDialog({
  open,
  onOpenChange,
  projects,
  selectedProjectId,
  onCreate,
  createComb,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  projects: Project[];
  selectedProjectId: string | null;
  onCreate: (comb: Comb) => void;
  createComb: (data: CreateCombDTO) => Promise<Comb>;
}) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [baseBranch, setBaseBranch] = useState("main");
  const [projectId, setProjectId] = useState("");
  const [isCreating, setIsCreating] = useState(false);

  useEffect(() => {
    if (!open) return;
    setName("");
    setDescription("");
    setBaseBranch("main");
    setProjectId(selectedProjectId ?? projects[0]?.id ?? "");
  }, [open, selectedProjectId, projects]);

  const handleCreate = async () => {
    if (!name.trim() || !projectId) {
      toast.error("Preencha projeto e nome do workspace.");
      return;
    }
    setIsCreating(true);
    try {
      const comb = await createComb({
        projectId,
        name: name.trim(),
        description: description.trim() || undefined,
        baseBranch: baseBranch.trim() || "main",
      });
      onCreate(comb);
      onOpenChange(false);
      toast.success("Workspace criado");
    } catch {
      toast.error("Falha ao criar workspace");
    } finally {
      setIsCreating(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Novo Workspace</DialogTitle>
          <DialogDescription>Cria uma sessão isolada para terminais e agents.</DialogDescription>
        </DialogHeader>
        <div className="rounded-md border border-border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
          <WorkflowTip />
        </div>
        <div className="space-y-3 py-2">
          <div className="space-y-2">
            <label className="text-sm font-medium">Projeto</label>
            <Select value={projectId} onValueChange={setProjectId}>
              <SelectTrigger>
                <SelectValue placeholder="Selecione o projeto" />
              </SelectTrigger>
              <SelectContent>
                {projects.map((p) => (
                  <SelectItem key={p.id} value={p.id}>
                    {p.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Nome</label>
            <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="ex.: auth-refactor" />
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Branch base</label>
            <Input value={baseBranch} onChange={(e) => setBaseBranch(e.target.value)} placeholder="main" />
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Descrição (opcional)</label>
            <Textarea value={description} onChange={(e) => setDescription(e.target.value)} rows={2} />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancelar
          </Button>
          <Button onClick={handleCreate} disabled={isCreating}>
            {isCreating && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            Criar
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function NewAgentPaneDialog({
  open,
  onOpenChange,
  combId,
  providers,
  onCreate,
  ensureCombWorktree,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  combId: string;
  providers: Provider[];
  onCreate: (pane: Pane) => void;
  ensureCombWorktree: () => Promise<boolean>;
}) {
  const [providerId, setProviderId] = useState("");
  const [prompt, setPrompt] = useState("");
  const [title, setTitle] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const { create } = usePanes(combId);
  const cliProviders = useMemo(
    () => providers.filter((p) => p.isActive && isCliProviderType(p.type)),
    [providers],
  );

  useEffect(() => {
    if (!open) return;
    setProviderId(cliProviders[0]?.id ?? "");
    setPrompt("");
    setTitle("");
  }, [open, cliProviders]);

  const handleCreate = async () => {
    setIsCreating(true);
    try {
      const wtOk = await ensureCombWorktree();
      if (!wtOk) return;
      const pane = await create({
        combId,
        type: "agent",
        providerId: providerId || undefined,
        title: title.trim() || undefined,
        initialPrompt: prompt.trim() || undefined,
      });
      onCreate(pane);
      onOpenChange(false);
      toast.success("Agent pane criado");
    } catch {
      toast.error("Falha ao criar pane");
    } finally {
      setIsCreating(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Novo Agent Pane</DialogTitle>
          <DialogDescription>Abre um agente CLI no workspace atual.</DialogDescription>
        </DialogHeader>
        <div className="space-y-4 py-2">
          <div className="space-y-2">
            <label className="text-sm font-medium">Agente CLI</label>
            <Select value={providerId} onValueChange={setProviderId}>
              <SelectTrigger>
                <SelectValue placeholder="Selecione agente..." />
              </SelectTrigger>
              <SelectContent>
                {cliProviders.map((p) => (
                  <SelectItem key={p.id} value={p.id}>
                    {p.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Titulo (opcional)</label>
            <Input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="ex.: Fix lint" />
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Prompt inicial (opcional)</label>
            <Textarea value={prompt} onChange={(e) => setPrompt(e.target.value)} rows={3} />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancelar
          </Button>
          <Button onClick={handleCreate} disabled={isCreating}>
            {isCreating && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            Criar Agent
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

const WorkspaceListItem = React.memo(function WorkspaceListItem({
  comb,
  isActive,
  projectName,
  attentionExcerpt,
  hasAttention,
  onSelect,
  onRemove,
}: {
  comb: Comb;
  isActive: boolean;
  projectName: string;
  attentionExcerpt: string | null;
  hasAttention: boolean;
  onSelect: (comb: Comb) => void;
  onRemove: (combId: string) => void;
}) {
  const handleSelect = useCallback(() => onSelect(comb), [onSelect, comb]);
  const handleRemove = useCallback(
    (e: React.MouseEvent<HTMLButtonElement>) => {
      e.stopPropagation();
      onRemove(comb.id);
    },
    [onRemove, comb.id],
  );

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={handleSelect}
      className={`titlebar-no-drag group rounded-lg border px-2.5 py-2 transition-colors ${
        isActive ? "border-primary bg-sidebar-accent text-sidebar-accent-foreground" : "border-transparent hover:bg-sidebar-accent/50"
      }`}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="flex items-center gap-1.5">
            <span className="truncate text-sm font-medium">{comb.name}</span>
            {hasAttention ? <span className="h-2 w-2 rounded-full bg-sky-400" /> : null}
          </div>
          <div className="mt-0.5 flex items-center gap-1 text-[11px] text-sidebar-foreground/60">
            <GitBranch className="h-3 w-3" />
            <span className="truncate">{comb.branch ?? comb.baseBranch}</span>
          </div>
          <p className="mt-0.5 line-clamp-1 text-[10px] text-sidebar-foreground/50">{projectName}</p>
          {attentionExcerpt ? <p className="mt-1 line-clamp-1 text-[11px] text-sidebar-foreground/70">{attentionExcerpt}</p> : null}
        </div>
        <div className="flex items-center gap-1">
          <Button variant="ghost" size="icon" className="h-6 w-6 opacity-0 group-hover:opacity-100" onClick={handleRemove}>
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>
    </div>
  );
});

const PaneCard = React.memo(function PaneCard({
  pane,
  worktreePath,
  provider,
  onRemovePane,
}: {
  pane: Pane;
  worktreePath: string;
  provider: Provider | null;
  onRemovePane: (paneId: string) => void;
}) {
  const command = pane.type === "agent" ? buildCliCommand(provider) : undefined;
  const label = pane.type === "agent" ? (pane.title ?? provider?.name ?? "Agent") : (pane.title ?? "Terminal");
  const args = pane.initialPrompt ? [pane.initialPrompt] : [];
  const handleRemove = useCallback(() => onRemovePane(pane.id), [onRemovePane, pane.id]);

  return (
    <div data-pane-id={pane.id} className="flex h-full min-h-0 flex-col overflow-hidden">
      <div className="mb-1 flex items-center justify-between rounded border border-border px-2 py-1">
        <span className="truncate text-xs">{label}</span>
        <Button variant="ghost" size="icon" className="h-6 w-6" onClick={handleRemove}>
          <Trash2 className="h-3.5 w-3.5" />
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-hidden">
        <EmbeddedTerminal cwd={worktreePath} command={command} args={args} paneId={pane.id} title={label} />
      </div>
    </div>
  );
});

function WorkflowTip({ centered = false }: { centered?: boolean }) {
  return (
    <div className={`flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground ${centered ? "justify-center" : ""}`}>
      <FolderGit2 className="h-3.5 w-3.5" />
      <span>Projeto</span>
      <ChevronRight className="h-3.5 w-3.5 opacity-60" />
      <GitBranch className="h-3.5 w-3.5" />
      <span>Workspace</span>
      <ChevronRight className="h-3.5 w-3.5 opacity-60" />
      <Terminal className="h-3.5 w-3.5" />
      <span>Panes</span>
      <ChevronRight className="h-3.5 w-3.5 opacity-60" />
      <GitPullRequest className="h-3.5 w-3.5" />
      <span>Git/PR</span>
      <ChevronRight className="h-3.5 w-3.5 opacity-60" />
      <Merge className="h-3.5 w-3.5" />
      <span>Merge</span>
    </div>
  );
}

export default function CmuxWorkspacePage() {
  const { projects, isLoading: projectsLoading } = useProjects();
  const { providers } = useProviders();
  const { confirmDialog } = useConfirmDialog();

  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [activeCombId, setActiveCombId] = useState<string | null>(null);
  const [showProviders, setShowProviders] = useState(false);
  const [newCombOpen, setNewCombOpen] = useState(false);
  const [newAgentOpen, setNewAgentOpen] = useState(false);
  const [addProjectOpen, setAddProjectOpen] = useState(false);
  const [attentionOpen, setAttentionOpen] = useState(false);
  const [activePaneId, setActivePaneId] = useState<string | null>(null);
  const [attentionRecords, setAttentionRecords] = useState<TerminalAttentionRecord[]>([]);
  const [initializingBasePaneIds, setInitializingBasePaneIds] = useState<Set<string>>(new Set());
  const hydratedAttentionRef = useRef(false);

  const sortedProjects = useMemo(
    () => [...projects].sort((a, b) => +new Date(b.updatedAt) - +new Date(a.updatedAt)),
    [projects],
  );
  const [combs, setCombs] = useState<Comb[]>([]);
  const [combsLoading, setCombsLoading] = useState(true);
  const refreshCombs = useCallback(async () => {
    if (!window.db?.combs || sortedProjects.length === 0) {
      setCombs([]);
      setCombsLoading(false);
      return;
    }
    setCombsLoading(true);
    try {
      const chunks = await Promise.all(
        sortedProjects.map((project) => window.db!.combs.findByProject(project.id)),
      );
      const flat = normalizeCombs(chunks.flat()) as Comb[];
      flat.sort((a, b) => +new Date(b.updatedAt) - +new Date(a.updatedAt));
      setCombs(flat);
    } finally {
      setCombsLoading(false);
    }
  }, [sortedProjects]);

  const createComb = useCallback(async (data: CreateCombDTO) => {
    if (!window.db?.combs) throw new Error("Combs indisponivel");
    const created = await window.db.combs.create(data);
    const comb = normalizeComb(created as Record<string, unknown>) as Comb;
    await refreshCombs();
    return comb;
  }, [refreshCombs]);
  const combsRef = useRef(combs);
  combsRef.current = combs;
  const activeComb = useMemo(
    () => (activeCombId ? (combs.find((c) => c.id === activeCombId) ?? null) : null),
    [activeCombId, combs],
  );
  const activeProject = useMemo(() => {
    if (!activeComb) return null;
    return sortedProjects.find((project) => project.id === activeComb.projectId) ?? null;
  }, [activeComb, sortedProjects]);
  const {
    panes,
    refresh: refreshPanes,
    create: createPane,
    update: updatePane,
    remove: removePane,
  } = usePanes(activeCombId ?? undefined);
  useEffect(() => {
    void refreshCombs();
  }, [refreshCombs]);

  const providerById = useMemo(() => {
    const map = new Map<string, Provider>();
    for (const p of providers) map.set(p.id, p);
    return map;
  }, [providers]);
  const cliProviders = useMemo(
    () => providers.filter((p) => p.isActive && isCliProviderType(p.type)),
    [providers],
  );
  const projectNameById = useMemo(() => {
    const map = new Map<string, string>();
    for (const p of sortedProjects) map.set(p.id, p.name);
    return map;
  }, [sortedProjects]);
  const unreadAttentionByCombId = useMemo(() => {
    const map = new Map<string, TerminalAttentionRecord>();
    for (const record of attentionRecords) {
      if (!record.read && !map.has(record.combId)) {
        map.set(record.combId, record);
      }
    }
    return map;
  }, [attentionRecords]);
  const hasUnreadAttentionByPaneId = useMemo(() => {
    const set = new Set<string>();
    for (const record of attentionRecords) {
      if (!record.read) set.add(record.paneId);
    }
    return set;
  }, [attentionRecords]);
  const unreadCount = useMemo(() => {
    let count = 0;
    for (const r of attentionRecords) {
      if (!r.read) count += 1;
    }
    return count;
  }, [attentionRecords]);

  useTerminalAttentionToasts({
    onNavigateToPane: ({ projectId, combId, paneId }) => {
      setSelectedProjectId(projectId);
      setActiveCombId(combId);
      setActivePaneId(paneId);
    },
    onAttentionRecord: (record) => {
      setAttentionRecords((prev) => {
        if (prev.length > 0 && prev[0].paneId === record.paneId && prev[0].excerpt === record.excerpt) {
          return prev;
        }
        const next = [record, ...prev].slice(0, 120);
        return next;
      });
    },
  });

  useEffect(() => {
    if (hydratedAttentionRef.current) return;
    hydratedAttentionRef.current = true;
    const raw = localStorage.getItem("dcc:attention:records");
    if (!raw) return;
    try {
      const parsed = JSON.parse(raw) as TerminalAttentionRecord[];
      if (Array.isArray(parsed)) setAttentionRecords(parsed.slice(0, 120));
    } catch {
      // ignore malformed storage
    }
  }, []);

  useEffect(() => {
    localStorage.setItem("dcc:attention:records", JSON.stringify(attentionRecords.slice(0, 120)));
  }, [attentionRecords]);

  useEffect(() => {
    if (selectedProjectId && projects.some((p) => p.id === selectedProjectId)) return;
    const stored = localStorage.getItem("dcc:workspace:selectedProjectId");
    if (stored && projects.some((p) => p.id === stored)) {
      setSelectedProjectId(stored);
      return;
    }
    if (projects.length > 0) setSelectedProjectId(sortedProjects[0].id);
  }, [projects, selectedProjectId, sortedProjects]);

  useEffect(() => {
    if (selectedProjectId) localStorage.setItem("dcc:workspace:selectedProjectId", selectedProjectId);
  }, [selectedProjectId]);

  useEffect(() => {
    if (!selectedProjectId) {
      setActiveCombId(null);
      return;
    }
    if (activeCombId && combs.some((c) => c.id === activeCombId)) return;
    const stored = localStorage.getItem(`dcc:workspace:${selectedProjectId}:activeComb`);
    if (stored && combs.some((c) => c.id === stored)) {
      setActiveCombId(stored);
      return;
    }
    const firstProjectComb = combs.find((c) => c.projectId === selectedProjectId);
    setActiveCombId(firstProjectComb?.id ?? null);
  }, [combs, activeCombId, selectedProjectId]);

  useEffect(() => {
    if (activeCombId && selectedProjectId) {
      localStorage.setItem(`dcc:workspace:${selectedProjectId}:activeComb`, activeCombId);
      setAttentionRecords((prev) => {
        let changed = false;
        const next = prev.map((item) => {
          if (item.combId !== activeCombId || item.read) return item;
          changed = true;
          return { ...item, read: true };
        });
        return changed ? next : prev;
      });
    }
  }, [activeCombId, selectedProjectId]);

  const visiblePanes = useMemo(
    () => panes.filter((pane) => !initializingBasePaneIds.has(pane.id)),
    [panes, initializingBasePaneIds],
  );
  const activePane = useMemo(
    () => (activePaneId ? (visiblePanes.find((pane) => pane.id === activePaneId) ?? null) : null),
    [activePaneId, visiblePanes],
  );
  useEffect(() => {
    if (visiblePanes.length === 0) {
      if (activePaneId !== null) setActivePaneId(null);
      return;
    }
    if (!activePaneId || !visiblePanes.some((pane) => pane.id === activePaneId)) {
      setActivePaneId(visiblePanes[0].id);
    }
  }, [visiblePanes, activePaneId]);

  const markPaneAttentionAsRead = useCallback((paneId: string) => {
    setAttentionRecords((prev) => {
      let changed = false;
      const next = prev.map((record) => {
        if (record.paneId !== paneId || record.read) return record;
        changed = true;
        return { ...record, read: true };
      });
      return changed ? next : prev;
    });
  }, []);
  const handleSelectPaneTab = useCallback((paneId: string) => {
    setActivePaneId(paneId);
    markPaneAttentionAsRead(paneId);
  }, [markPaneAttentionAsRead]);

  useEffect(() => {
    const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/i.test(navigator.platform);
    const onKeyDown = (event: KeyboardEvent) => {
      if (!event.shiftKey) return;
      if (isMac ? !event.metaKey : !event.ctrlKey) return;
      if (visiblePanes.length <= 1) return;
      const key = event.key;
      if (key !== "]" && key !== "[") return;
      event.preventDefault();
      const currentIdx = visiblePanes.findIndex((pane) => pane.id === activePaneId);
      const baseIdx = currentIdx >= 0 ? currentIdx : 0;
      const nextIdx =
        key === "]"
          ? (baseIdx + 1) % visiblePanes.length
          : (baseIdx - 1 + visiblePanes.length) % visiblePanes.length;
      const nextPaneId = visiblePanes[nextIdx]?.id;
      if (!nextPaneId) return;
      setActivePaneId(nextPaneId);
      markPaneAttentionAsRead(nextPaneId);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [activePaneId, markPaneAttentionAsRead, visiblePanes]);

  const ensureActiveCombWorktree = useCallback(async (): Promise<boolean> => {
    if (!activeCombId) return false;
    const comb = combsRef.current.find((c) => c.id === activeCombId);
    if (!comb) return false;
    if (comb.worktreePath) return true;
    const api = window.desktopAPI?.comb?.ensureWorktree;
    if (!api) return false;
    try {
      const result = await api(activeCombId);
      if (result.success) {
        await refreshCombs();
        return true;
      }
      if (result.error) toast.error(`Worktree: ${result.error}`);
      return false;
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : "Falha ao preparar worktree");
      return false;
    }
  }, [activeCombId, refreshCombs]);
  const handleSelectWorkspace = useCallback((comb: Comb) => {
    setSelectedProjectId(comb.projectId);
    setActiveCombId(comb.id);
  }, []);

  const handleAddTerminal = async () => {
    if (!activeCombId) return;
    try {
      const ok = await ensureActiveCombWorktree();
      if (!ok) return;
      const pane = await createPane({ combId: activeCombId, type: "term" });
      refreshPanes();
      setActivePaneId(pane.id);
    } catch {
      toast.error("Falha ao abrir terminal");
    }
  };
  const handleOpenBaseTerminal = async () => {
    if (!activeCombId) return;
    const basePath = activeProject?.path?.trim();
    if (!basePath) {
      toast.error("Projeto base indisponivel para este workspace.");
      return;
    }
    try {
      const ok = await ensureActiveCombWorktree();
      if (!ok) return;
      const pane = await createPane({
        combId: activeCombId,
        type: "term",
        title: "Base",
      });
      setInitializingBasePaneIds((prev) => {
        const next = new Set(prev);
        next.add(pane.id);
        return next;
      });
      await updatePane(pane.id, { cwd: basePath });
      try {
        await window.desktopAPI?.terminal?.killByPaneId?.(pane.id);
      } catch {
        // best effort: if no previous PTY exists, ignore
      }
      refreshPanes();
      setActiveCombId(activeCombId);
      setActivePaneId(pane.id);
      setInitializingBasePaneIds((prev) => {
        const next = new Set(prev);
        next.delete(pane.id);
        return next;
      });
    } catch (e: unknown) {
      setInitializingBasePaneIds(new Set());
      toast.error(
        e instanceof Error ? e.message : "Falha ao abrir terminal base no workspace.",
      );
    }
  };

  const handleRemovePane = async (paneId: string) => {
    const confirmed = await confirmDialog({
      title: "Remover pane?",
      description: "O terminal/agent será fechado.",
      confirmLabel: "Remover",
      cancelLabel: "Cancelar",
    });
    if (!confirmed) return;
    const nextVisiblePanes = visiblePanes.filter((pane) => pane.id !== paneId);
    if (activePaneId === paneId) {
      const currentIdx = visiblePanes.findIndex((pane) => pane.id === paneId);
      const fallback =
        nextVisiblePanes[currentIdx] ??
        nextVisiblePanes[currentIdx - 1] ??
        nextVisiblePanes[0] ??
        null;
      setActivePaneId(fallback?.id ?? null);
    }
    await removePane(paneId);
    refreshPanes();
    setAttentionRecords((prev) => prev.filter((item) => item.paneId !== paneId));
  };

  const handleRemoveWorkspace = async (combId: string) => {
    const confirmed = await confirmDialog({
      title: "Remover workspace?",
      description: "Worktree e panes serão removidos.",
      confirmLabel: "Remover",
      cancelLabel: "Cancelar",
    });
    if (!confirmed) return;
    if (window.desktopAPI?.comb?.discard) {
      const result = await window.desktopAPI.comb.discard(combId);
      if (!result.success && result.error) toast.error(result.error);
    }
    if (window.db?.combs) await window.db.combs.delete(combId);
    if (activeCombId === combId) setActiveCombId(null);
    refreshCombs();
    setAttentionRecords((prev) => prev.filter((item) => item.combId !== combId));
    toast.success("Workspace removido");
  };
  const handleRemoveWorkspaceById = useCallback((combId: string) => {
    void handleRemoveWorkspace(combId);
  }, [handleRemoveWorkspace]);
  const handleRemovePaneById = useCallback((paneId: string) => {
    void handleRemovePane(paneId);
  }, [handleRemovePane]);

  if (projectsLoading) {
    return (
      <div className="flex h-screen items-center justify-center bg-background">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="flex h-screen overflow-hidden bg-background">
      <div className="titlebar-drag-region fixed top-0 left-0 right-0 h-8 z-50" />
      <aside className="flex w-72 shrink-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground">
        <div className="border-b border-sidebar-border px-3 pt-10 pb-3">
          <div className="mb-3 flex items-center justify-between">
            <div className="flex items-center gap-2">
              <img src="/dcc-mark.svg" alt="DCC" className="h-7 w-7 rounded-lg" />
              <span className="text-sm font-semibold tracking-tight">Dev Command</span>
            </div>
            <Button variant="ghost" size="icon" className="titlebar-no-drag h-7 w-7" onClick={() => setNewCombOpen(true)}>
              <Plus className="h-4 w-4" />
            </Button>
          </div>
          <Button variant="outline" className="titlebar-no-drag w-full justify-start gap-2" onClick={() => setAddProjectOpen(true)}>
            <FolderGit2 className="h-4 w-4" />
            Adicionar projeto
          </Button>
        </div>

        <div className="flex-1 overflow-y-auto p-2">
          {combsLoading ? (
            <div className="flex justify-center py-8">
              <Loader2 className="h-5 w-5 animate-spin text-sidebar-foreground/40" />
            </div>
          ) : combs.length === 0 ? (
            <p className="px-3 py-4 text-center text-xs text-sidebar-foreground/40">Nenhum workspace neste projeto.</p>
          ) : (
            <div className="space-y-1">
              {combs.map((comb) => {
                const attentionForComb = unreadAttentionByCombId.get(comb.id);
                const isActive = comb.id === activeCombId;
                const projectName = projectNameById.get(comb.projectId) ?? "Projeto";
                return (
                  <WorkspaceListItem
                    key={comb.id}
                    comb={comb}
                    isActive={isActive}
                    projectName={projectName}
                    hasAttention={!!attentionForComb}
                    attentionExcerpt={attentionForComb?.excerpt ?? null}
                    onSelect={handleSelectWorkspace}
                    onRemove={handleRemoveWorkspaceById}
                  />
                );
              })}
            </div>
          )}
        </div>

        <div className="border-t border-sidebar-border p-2 space-y-1">
          <Button
            variant="ghost"
            size="sm"
            className="titlebar-no-drag w-full justify-start gap-2"
            onClick={() => setAttentionOpen(true)}
          >
            <Bell className="h-4 w-4" />
            Notificacoes
            {unreadCount > 0 ? <Badge className="ml-auto">{unreadCount}</Badge> : null}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="titlebar-no-drag w-full justify-start gap-2"
            onClick={() => setShowProviders((prev) => !prev)}
          >
            <Settings className="h-4 w-4" />
            Providers
          </Button>
        </div>
      </aside>

      <main className="flex min-h-0 flex-1 flex-col">
        {showProviders ? (
          <div className="flex-1 overflow-auto mt-8">
            <SettingsPage />
          </div>
        ) : activeComb ? (
          <>
            <div className="mt-8 flex items-center justify-between border-b border-border px-4 py-2">
              <div className="flex min-w-0 items-center gap-3">
                <h3 className="truncate text-sm font-semibold">{activeComb.name}</h3>
                <Badge variant="outline" className="gap-1">
                  <GitBranch className="h-3 w-3" />
                  {activeComb.branch ?? activeComb.baseBranch}
                </Badge>
              </div>
              <div className="flex items-center gap-2">
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button variant="outline" size="sm" onClick={handleOpenBaseTerminal}>
                      <FolderGit2 className="mr-1 h-3.5 w-3.5" />
                      Base
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Terminal no repositorio principal</TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button variant="outline" size="sm" onClick={handleAddTerminal}>
                      <Terminal className="mr-1 h-3.5 w-3.5" />
                      Workspace
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Terminal no worktree atual</TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button variant="outline" size="sm" onClick={() => setNewAgentOpen(true)} disabled={cliProviders.length === 0}>
                      <Bot className="mr-1 h-3.5 w-3.5" />
                      Agent
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Abrir agente CLI no workspace</TooltipContent>
                </Tooltip>
              </div>
            </div>
            <div className="min-h-0 flex-1 overflow-hidden">
              {visiblePanes.length === 0 ? (
                <div className="flex h-full flex-col items-center justify-center gap-4 p-8">
                  <Terminal className="h-12 w-12 text-muted-foreground/30" />
                  <p className="text-sm text-muted-foreground">Nenhum pane aberto neste workspace</p>
                </div>
              ) : (
                <div className="flex h-full min-h-0 flex-col overflow-hidden p-1">
                  <div role="tablist" aria-label="Panes do workspace" className="mb-1 flex shrink-0 gap-1 overflow-x-auto">
                    {visiblePanes.map((pane) => {
                      const provider = pane.providerId ? (providerById.get(pane.providerId) ?? null) : null;
                      const label = pane.type === "agent" ? (pane.title ?? provider?.name ?? "Agent") : (pane.title ?? "Terminal");
                      const selected = pane.id === activePane?.id;
                      const hasUnreadAttention = hasUnreadAttentionByPaneId.has(pane.id);
                      return (
                        <div
                          key={pane.id}
                          role="tab"
                          aria-selected={selected}
                          onClick={() => handleSelectPaneTab(pane.id)}
                          className={`group flex min-w-[170px] max-w-[260px] cursor-pointer items-center gap-2 rounded-md border px-2 py-1.5 ${
                            selected ? "border-primary bg-primary/10" : "border-border bg-muted/30 hover:bg-muted/50"
                          }`}
                        >
                          {pane.type === "agent" ? <Bot className="h-3.5 w-3.5 shrink-0" /> : <Terminal className="h-3.5 w-3.5 shrink-0" />}
                          <span className="truncate text-xs">{label}</span>
                          {hasUnreadAttention ? <span className="h-2 w-2 shrink-0 rounded-full bg-sky-400" /> : null}
                          <Button
                            variant="ghost"
                            size="icon"
                            className="ml-auto h-5 w-5 shrink-0 opacity-70 hover:opacity-100"
                            onClick={(event) => {
                              event.stopPropagation();
                              handleRemovePaneById(pane.id);
                            }}
                          >
                            <Trash2 className="h-3 w-3" />
                          </Button>
                        </div>
                      );
                    })}
                  </div>
                  <div className="min-h-0 flex-1 overflow-hidden">
                    {activePane ? (
                      <PaneCard
                        pane={activePane}
                        worktreePath={activePane.cwd?.trim() || activeComb.worktreePath || ""}
                        provider={activePane.providerId ? (providerById.get(activePane.providerId) ?? null) : null}
                        onRemovePane={handleRemovePaneById}
                      />
                    ) : null}
                  </div>
                </div>
              )}
            </div>
          </>
        ) : (
          <div className="mt-8 flex h-full flex-col items-center justify-center gap-4 p-8">
            <FolderGit2 className="h-16 w-16 text-muted-foreground/20" />
            <div className="text-center">
              <h3 className="text-lg font-medium">Crie ou selecione um workspace</h3>
              <p className="mt-1 text-sm text-muted-foreground">Fluxo simplificado: workspaces, panes e providers.</p>
              <div className="mt-2">
                <WorkflowTip centered />
              </div>
            </div>
            <Button onClick={() => setNewCombOpen(true)}>
              <Plus className="mr-2 h-4 w-4" />
              Novo Workspace
            </Button>
          </div>
        )}
      </main>

      <Dialog open={attentionOpen} onOpenChange={setAttentionOpen}>
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>Notificacoes</DialogTitle>
            <DialogDescription>Eventos recentes de atencao dos agentes e terminais.</DialogDescription>
          </DialogHeader>
          <div className="max-h-[60vh] space-y-2 overflow-auto">
            {attentionRecords.length === 0 ? (
              <p className="text-sm text-muted-foreground">Sem notificacoes.</p>
            ) : (
              attentionRecords.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className="w-full rounded border border-border p-3 text-left hover:bg-muted/50"
                  onClick={() => {
                    setSelectedProjectId(item.projectId);
                    setActiveCombId(item.combId);
                    setAttentionRecords((prev) => prev.map((r) => (r.id === item.id ? { ...r, read: true } : r)));
                    setAttentionOpen(false);
                  }}
                >
                  <div className="flex items-center justify-between gap-3">
                    <p className="text-sm font-medium">
                      {item.projectName} · {item.workspaceName}
                    </p>
                    {!item.read ? <Badge>Novo</Badge> : null}
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">{item.excerpt ?? "Agente aguardando interacao."}</p>
                  <p className="mt-1 text-[11px] text-muted-foreground">
                    {formatDistanceToNow(new Date(item.createdAt), { addSuffix: true, locale: ptBR })}
                  </p>
                </button>
              ))
            )}
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setAttentionRecords((prev) => prev.map((item) => ({ ...item, read: true })))}
            >
              Marcar tudo como lido
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <NewWorkspaceDialog
        open={newCombOpen}
        onOpenChange={setNewCombOpen}
        projects={sortedProjects}
        selectedProjectId={selectedProjectId}
        onCreate={(comb) => {
          setSelectedProjectId(comb.projectId);
          setActiveCombId(comb.id);
          refreshCombs();
        }}
        createComb={createComb}
      />

      {activeCombId ? (
        <NewAgentPaneDialog
          open={newAgentOpen}
          onOpenChange={setNewAgentOpen}
          combId={activeCombId}
          providers={providers}
          ensureCombWorktree={ensureActiveCombWorktree}
          onCreate={(pane) => {
            refreshPanes();
            setActivePaneId(pane.id);
          }}
        />
      ) : null}

      <AddProjectDialog open={addProjectOpen} onOpenChange={setAddProjectOpen} />
    </div>
  );
}
