"use client";

/**
 * Hook unificado para acesso a dados
 *
 * - No shell desktop (Tauri): usa SQLite via IPC (window.db), com datas normalizadas
 * - No Browser: usa o Zustand store (estado em memória, sem mock)
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { useAppStore } from "./use-app-store";
import {
  normalizeProject,
  normalizeProjects,
  normalizeProvider,
  normalizeProviders,
  normalizeComb,
  normalizeCombs,
  normalizePane,
  normalizePanes,
} from "@/lib/database/normalize";
import type {
  Project,
  CreateProjectDTO,
  UpdateProjectDTO,
  Provider,
  CreateProviderDTO,
  UpdateProviderDTO,
  Comb,
  CreateCombDTO,
  UpdateCombDTO,
  Pane,
  CreatePaneDTO,
  UpdatePaneDTO,
} from "@/lib/database/types";

// Shell desktop com bridge DB (Tauri)
const hasDesktopDb = () => typeof window !== "undefined" && !!window.db;

const IPC_TIMEOUT_MS = 10_000;

/** Wraps a Tauri invoke promise with a timeout so the UI never hangs forever. */
function ipcCall<T>(promise: Promise<T>, label = "ipc"): Promise<T> {
  return Promise.race([
    promise,
    new Promise<never>((_, reject) =>
      setTimeout(
        () => reject(new Error(`[${label}] IPC timeout after ${IPC_TIMEOUT_MS}ms`)),
        IPC_TIMEOUT_MS,
      ),
    ),
  ]);
}

// ============================================
// Event Emitter para sincronização entre hooks
// ============================================
type DataEventType = "projects" | "providers" | "combs" | "panes";
type DataEventListener = () => void;

const dataEventListeners: Record<DataEventType, Set<DataEventListener>> = {
  projects: new Set(),
  providers: new Set(),
  combs: new Set(),
  panes: new Set(),
};

function emitDataChange(type: DataEventType) {
  dataEventListeners[type].forEach((listener) => listener());
}

function subscribeToDataChange(
  type: DataEventType,
  listener: DataEventListener,
) {
  dataEventListeners[type].add(listener);
  return () => {
    dataEventListeners[type].delete(listener);
  };
}

// ============================================
// Hook para Projects
// ============================================

export function useProjects() {
  const storeProjects = useAppStore((s) => s.projects);
  const addProject = useAppStore((s) => s.addProject);
  const updateProject = useAppStore((s) => s.updateProject);
  const deleteProject = useAppStore((s) => s.deleteProject);
  const getProjectById = useAppStore((s) => s.getProjectById);

  const [projects, setProjects] = useState<Project[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const isRefreshingRef = useRef(false);
  const mountedRef = useRef(true);

  const storeProjectsRef = useRef(storeProjects);
  storeProjectsRef.current = storeProjects;

  const refresh = useCallback(async () => {
    if (isRefreshingRef.current) return;
    isRefreshingRef.current = true;
    setIsLoading(true);

    try {
      if (hasDesktopDb() && window.db) {
        const data = await ipcCall(window.db.projects.findAll(), "projects.findAll");
        if (mountedRef.current) {
          setProjects(normalizeProjects(data) as Project[]);
        }
      } else {
        if (mountedRef.current) {
          setProjects(storeProjectsRef.current);
        }
      }
    } catch (error) {
      console.error("❌ Error in useProjects.refresh():", error);
    } finally {
      isRefreshingRef.current = false;
      if (mountedRef.current) {
        setIsLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    refresh();
    const unsubscribe = subscribeToDataChange("projects", refresh);
    return () => {
      mountedRef.current = false;
      unsubscribe();
    };
  }, [refresh]);

  const create = useCallback(
    async (data: CreateProjectDTO) => {
      if (hasDesktopDb() && window.db) {
        const project = await window.db.projects.create(data);
        emitDataChange("projects");
        return normalizeProject(
          project as unknown as Record<string, unknown>,
        ) as unknown as Project;
      } else {
        const newProject = addProject({
          ...data,
          defaultProviderId: data.defaultProviderId ?? null,
          gitRemoteUrl: data.gitRemoteUrl ?? null,
          lastOpenedAt: null,
        });
        emitDataChange("projects");
        return newProject;
      }
    },
    [addProject],
  );

  const update = useCallback(
    async (id: string, data: UpdateProjectDTO) => {
      if (hasDesktopDb() && window.db) {
        await window.db.projects.update(id, data);
        emitDataChange("projects");
      } else {
        updateProject(id, data);
        emitDataChange("projects");
      }
    },
    [updateProject],
  );

  const remove = useCallback(
    async (id: string) => {
      if (hasDesktopDb() && window.db) {
        await window.db.projects.delete(id);
        emitDataChange("projects");
      } else {
        deleteProject(id);
        emitDataChange("projects");
      }
    },
    [deleteProject],
  );

  const getById = useCallback(
    async (id: string) => {
      if (hasDesktopDb() && window.db) {
        const project = await window.db.projects.findById(id);
        return project
          ? (normalizeProject(
              project as unknown as Record<string, unknown>,
            ) as unknown as Project)
          : undefined;
      }
      return getProjectById(id);
    },
    [getProjectById],
  );

  const search = useCallback(
    async (query: string) => {
      if (hasDesktopDb() && window.db) {
        const data = await window.db.projects.search(query);
        return normalizeProjects(data) as Project[];
      } else {
        return storeProjects.filter(
          (p) =>
            p.name.toLowerCase().includes(query.toLowerCase()) ||
            p.description?.toLowerCase().includes(query.toLowerCase()),
        );
      }
    },
    [storeProjects],
  );

  return {
    projects: hasDesktopDb() ? projects : storeProjects,
    isLoading,
    refresh,
    create,
    update,
    remove,
    getById,
    search,
  };
}

// ============================================
// Hook para Providers
// ============================================

export function useProviders() {
  const storeProviders = useAppStore((s) => s.providers);
  const addProvider = useAppStore((s) => s.addProvider);
  const updateProvider = useAppStore((s) => s.updateProvider);
  const deleteProvider = useAppStore((s) => s.deleteProvider);
  const getProviderById = useAppStore((s) => s.getProviderById);

  const [providers, setProviders] = useState<Provider[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  const storeProvidersRef = useRef(storeProviders);
  storeProvidersRef.current = storeProviders;

  const refresh = useCallback(async () => {
    setIsLoading(true);
    try {
      if (hasDesktopDb() && window.db) {
        const data = await ipcCall(window.db.providers.findAll(), "providers.findAll");
        setProviders(normalizeProviders(data) as Provider[]);
      } else {
        setProviders(storeProvidersRef.current);
      }
    } catch (error) {
      console.error("[useProviders] refresh failed:", error);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
    const unsubscribe = subscribeToDataChange("providers", refresh);
    return unsubscribe;
  }, [refresh]);

  const create = useCallback(
    async (data: CreateProviderDTO) => {
      if (hasDesktopDb() && window.db) {
        const provider = await window.db.providers.create(data);
        emitDataChange("providers");
        return normalizeProvider(
          provider as unknown as Record<string, unknown>,
        ) as unknown as Provider;
      } else {
        const newProvider = addProvider({
          ...data,
          apiKey: data.apiKey ?? null,
          cliPath: data.cliPath ?? null,
          config: data.config ?? null,
          isActive: data.isActive ?? true,
        });
        emitDataChange("providers");
        return newProvider;
      }
    },
    [addProvider],
  );

  const update = useCallback(
    async (id: string, data: UpdateProviderDTO) => {
      if (hasDesktopDb() && window.db) {
        await window.db.providers.update(id, data);
        emitDataChange("providers");
      } else {
        updateProvider(id, data);
        emitDataChange("providers");
      }
    },
    [updateProvider],
  );

  const remove = useCallback(
    async (id: string) => {
      if (hasDesktopDb() && window.db) {
        await window.db.providers.delete(id);
        emitDataChange("providers");
      } else {
        deleteProvider(id);
        emitDataChange("providers");
      }
    },
    [deleteProvider],
  );

  const getById = useCallback(
    async (id: string) => {
      if (hasDesktopDb() && window.db) {
        const provider = await window.db.providers.findById(id);
        return provider
          ? (normalizeProvider(
              provider as unknown as Record<string, unknown>,
            ) as unknown as Provider)
          : undefined;
      }
      return getProviderById(id);
    },
    [getProviderById],
  );

  const getActive = useCallback(async () => {
    if (hasDesktopDb() && window.db) {
      const data = await window.db.providers.findActive();
      return normalizeProviders(data) as Provider[];
    }
    return storeProviders.filter((p) => p.isActive);
  }, [storeProviders]);

  const testConnection = useCallback(async (id: string) => {
    if (hasDesktopDb() && window.db) {
      return window.db.providers.testConnection(id);
    } else {
      await new Promise((r) => setTimeout(r, 1000));
      return { success: true, message: "Connection successful (mock)" };
    }
  }, []);

  return {
    providers: hasDesktopDb() ? providers : storeProviders,
    isLoading,
    refresh,
    create,
    update,
    remove,
    getById,
    getActive,
    testConnection,
  };
}

// ============================================
// Hook para Combs (Workspaces)
// ============================================

export function useCombs(projectId?: string) {
  const [combs, setCombs] = useState<Comb[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  const combsSeqRef = useRef(0);
  const combsInFlightRef = useRef(0);
  const refresh = useCallback(async (opts?: { silent?: boolean }) => {
    const silent = opts?.silent ?? false;
    if (!silent) {
      combsInFlightRef.current += 1;
      setIsLoading(true);
    }
    const mySeq = ++combsSeqRef.current;
    try {
      if (hasDesktopDb() && window.db?.combs && projectId) {
        const data = await ipcCall(window.db.combs.findByProject(projectId), "combs.findByProject");
        if (mySeq !== combsSeqRef.current) return;
        setCombs(normalizeCombs(data) as Comb[]);
      } else {
        if (mySeq !== combsSeqRef.current) return;
        setCombs([]);
      }
    } catch (error) {
      console.error("[useCombs] refresh failed:", error);
    } finally {
      if (!silent) {
        combsInFlightRef.current -= 1;
        if (combsInFlightRef.current === 0) setIsLoading(false);
      }
    }
  }, [projectId]);

  useEffect(() => {
    void refresh({ silent: false });
    const unsubscribe = subscribeToDataChange("combs", () => {
      void refresh({ silent: true });
    });
    return unsubscribe;
  }, [refresh]);

  const refreshSilent = useCallback(async () => {
    await refresh({ silent: true });
  }, [refresh]);

  const create = useCallback(
    async (data: CreateCombDTO) => {
      if (!hasDesktopDb() || !window.db?.combs) throw new Error("Combs not available");
      const comb = await window.db.combs.create(data);
      emitDataChange("combs");
      return normalizeComb(comb as unknown as Record<string, unknown>) as unknown as Comb;
    },
    [],
  );

  const update = useCallback(
    async (id: string, data: UpdateCombDTO) => {
      if (!hasDesktopDb() || !window.db?.combs) return;
      await window.db.combs.update(id, data);
      emitDataChange("combs");
    },
    [],
  );

  const remove = useCallback(
    async (id: string) => {
      if (!hasDesktopDb() || !window.db?.combs) return;
      await window.db.combs.delete(id);
      emitDataChange("combs");
      emitDataChange("panes");
    },
    [],
  );

  const togglePin = useCallback(
    async (id: string) => {
      if (!hasDesktopDb() || !window.db?.combs) return;
      await window.db.combs.togglePin(id);
      emitDataChange("combs");
    },
    [],
  );

  return {
    combs,
    isLoading,
    refresh: refreshSilent,
    create,
    update,
    remove,
    togglePin,
  };
}

// ============================================
// Hook para Panes
// ============================================

export function usePanes(combId?: string) {
  const [panes, setPanes] = useState<Pane[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    try {
      if (hasDesktopDb() && window.db?.panes && combId) {
        const data = await ipcCall(window.db.panes.findByComb(combId), "panes.findByComb");
        setPanes(normalizePanes(data) as Pane[]);
      } else {
        setPanes([]);
      }
    } catch (error) {
      console.error("[usePanes] refresh failed:", error);
    } finally {
      setIsLoading(false);
    }
  }, [combId]);

  useEffect(() => {
    refresh();
    const unsubscribe = subscribeToDataChange("panes", refresh);
    return unsubscribe;
  }, [refresh]);

  const create = useCallback(
    async (data: CreatePaneDTO) => {
      if (!hasDesktopDb() || !window.db?.panes) throw new Error("Panes not available");
      const pane = await window.db.panes.create(data);
      emitDataChange("panes");
      return normalizePane(pane as unknown as Record<string, unknown>) as unknown as Pane;
    },
    [],
  );

  const update = useCallback(
    async (id: string, data: UpdatePaneDTO) => {
      if (!hasDesktopDb() || !window.db?.panes) return;
      await window.db.panes.update(id, data);
      emitDataChange("panes");
    },
    [],
  );

  const remove = useCallback(
    async (id: string) => {
      if (!hasDesktopDb() || !window.db?.panes) return;
      await window.db.panes.delete(id);
      emitDataChange("panes");
    },
    [],
  );

  return {
    panes,
    isLoading,
    refresh,
    create,
    update,
    remove,
  };
}

export function useDesktopDbAvailable() {
  const [available, setAvailable] = useState(false);

  useEffect(() => {
    setAvailable(hasDesktopDb());
  }, []);

  return available;
}
