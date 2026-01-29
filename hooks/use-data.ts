"use client";

/**
 * Hook unificado para acesso a dados
 *
 * - No Electron: usa a API SQLite via IPC (window.db), com datas normalizadas
 * - No Browser: usa o Zustand store (estado em memória, sem mock)
 */

import { useCallback, useEffect, useState } from "react";
import { useAppStore } from "./use-app-store";
import {
  normalizeProject,
  normalizeProjects,
  normalizeProvider,
  normalizeProviders,
  normalizeMission,
  normalizeMissions,
  normalizeMissionLog,
  normalizeMissionLogs,
} from "@/lib/database/normalize";
import type {
  Project,
  CreateProjectDTO,
  UpdateProjectDTO,
  Provider,
  CreateProviderDTO,
  UpdateProviderDTO,
  Mission,
  CreateMissionDTO,
  UpdateMissionDTO,
  MissionLog,
  MissionStatus,
  MissionPlan,
  GeneratedCode,
} from "@/lib/database/types";

// Detecta ambiente Electron
const isElectron = () => typeof window !== "undefined" && !!window.db;

// ============================================
// Event Emitter para sincronização entre hooks
// ============================================
type DataEventType = "projects" | "providers" | "missions" | "missionLogs";
type DataEventListener = () => void;

const dataEventListeners: Record<DataEventType, Set<DataEventListener>> = {
  projects: new Set(),
  providers: new Set(),
  missions: new Set(),
  missionLogs: new Set(),
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
  const store = useAppStore();
  const [projects, setProjects] = useState<Project[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    try {
      if (isElectron() && window.db) {
        const data = await window.db.projects.findAll();
        setProjects(normalizeProjects(data) as Project[]);
      } else {
        setProjects(store.projects);
      }
    } finally {
      setIsLoading(false);
    }
  }, [store.projects]);

  // Carrega dados inicialmente e escuta mudanças de outras instâncias
  useEffect(() => {
    refresh();
    // Subscreve para receber notificações de mudanças de outras instâncias do hook
    const unsubscribe = subscribeToDataChange("projects", refresh);
    return unsubscribe;
  }, [refresh]);

  const create = useCallback(
    async (data: CreateProjectDTO) => {
      if (isElectron() && window.db) {
        const project = await window.db.projects.create(data);
        // Notifica todas as instâncias do hook para atualizar
        emitDataChange("projects");
        return normalizeProject(
          project as unknown as Record<string, unknown>,
        ) as unknown as Project;
      } else {
        const newProject = store.addProject({
          ...data,
          defaultProviderId: data.defaultProviderId ?? null,
          gitRemoteUrl: data.gitRemoteUrl ?? null,
          lastOpenedAt: null,
        });
        emitDataChange("projects");
        return newProject;
      }
    },
    [store],
  );

  const update = useCallback(
    async (id: string, data: UpdateProjectDTO) => {
      if (isElectron() && window.db) {
        await window.db.projects.update(id, data);
        emitDataChange("projects");
      } else {
        store.updateProject(id, data);
        emitDataChange("projects");
      }
    },
    [store],
  );

  const remove = useCallback(
    async (id: string) => {
      if (isElectron() && window.db) {
        await window.db.projects.delete(id);
        emitDataChange("projects");
      } else {
        store.deleteProject(id);
        emitDataChange("projects");
      }
    },
    [store],
  );

  const getById = useCallback(
    async (id: string) => {
      if (isElectron() && window.db) {
        const project = await window.db.projects.findById(id);
        return project
          ? (normalizeProject(
              project as unknown as Record<string, unknown>,
            ) as unknown as Project)
          : undefined;
      }
      return store.getProjectById(id);
    },
    [store],
  );

  const search = useCallback(
    async (query: string) => {
      if (isElectron() && window.db) {
        const data = await window.db.projects.search(query);
        return normalizeProjects(data) as Project[];
      } else {
        return store.projects.filter(
          (p) =>
            p.name.toLowerCase().includes(query.toLowerCase()) ||
            p.description?.toLowerCase().includes(query.toLowerCase()),
        );
      }
    },
    [store.projects],
  );

  return {
    projects: isElectron() ? projects : store.projects,
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
  const store = useAppStore();
  const [providers, setProviders] = useState<Provider[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    try {
      if (isElectron() && window.db) {
        const data = await window.db.providers.findAll();
        setProviders(normalizeProviders(data) as Provider[]);
      } else {
        setProviders(store.providers);
      }
    } finally {
      setIsLoading(false);
    }
  }, [store.providers]);

  useEffect(() => {
    refresh();
    const unsubscribe = subscribeToDataChange("providers", refresh);
    return unsubscribe;
  }, [refresh]);

  const create = useCallback(
    async (data: CreateProviderDTO) => {
      if (isElectron() && window.db) {
        const provider = await window.db.providers.create(data);
        emitDataChange("providers");
        return normalizeProvider(
          provider as unknown as Record<string, unknown>,
        ) as unknown as Provider;
      } else {
        const newProvider = store.addProvider({
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
    [store],
  );

  const update = useCallback(
    async (id: string, data: UpdateProviderDTO) => {
      if (isElectron() && window.db) {
        await window.db.providers.update(id, data);
        emitDataChange("providers");
      } else {
        store.updateProvider(id, data);
        emitDataChange("providers");
      }
    },
    [store],
  );

  const remove = useCallback(
    async (id: string) => {
      if (isElectron() && window.db) {
        await window.db.providers.delete(id);
        emitDataChange("providers");
      } else {
        store.deleteProvider(id);
        emitDataChange("providers");
      }
    },
    [store],
  );

  const getById = useCallback(
    async (id: string) => {
      if (isElectron() && window.db) {
        const provider = await window.db.providers.findById(id);
        return provider
          ? (normalizeProvider(
              provider as unknown as Record<string, unknown>,
            ) as unknown as Provider)
          : undefined;
      }
      return store.getProviderById(id);
    },
    [store],
  );

  const getActive = useCallback(async () => {
    if (isElectron() && window.db) {
      const data = await window.db.providers.findActive();
      return normalizeProviders(data) as Provider[];
    }
    return store.providers.filter((p) => p.isActive);
  }, [store.providers]);

  const testConnection = useCallback(async (id: string) => {
    if (isElectron() && window.db) {
      return window.db.providers.testConnection(id);
    } else {
      // Mock test
      await new Promise((r) => setTimeout(r, 1000));
      return { success: true, message: "Connection successful (mock)" };
    }
  }, []);

  return {
    providers: isElectron() ? providers : store.providers,
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
// Hook para Missions
// ============================================

export function useMissions(projectId?: string) {
  const store = useAppStore();
  const [missions, setMissions] = useState<Mission[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    try {
      if (isElectron() && window.db) {
        const data = projectId
          ? await window.db.missions.findByProject(projectId)
          : await window.db.missions.findAll();
        setMissions(normalizeMissions(data) as Mission[]);
      } else {
        const data = projectId
          ? store.getMissionsByProject(projectId)
          : store.missions;
        setMissions(data);
      }
    } finally {
      setIsLoading(false);
    }
  }, [projectId, store]);

  useEffect(() => {
    refresh();
    const unsubscribe = subscribeToDataChange("missions", refresh);
    return unsubscribe;
  }, [refresh]);

  const create = useCallback(
    async (data: CreateMissionDTO) => {
      if (isElectron() && window.db) {
        const mission = await window.db.missions.create(data);
        emitDataChange("missions");
        return normalizeMission(
          mission as unknown as Record<string, unknown>,
        ) as unknown as Mission;
      } else {
        const newMission = store.addMission({
          ...data,
          providerId: data.providerId ?? null,
          plan: null,
          generatedCode: null,
          context: null,
          errorMessage: null,
          startedAt: null,
          completedAt: null,
        });
        emitDataChange("missions");
        return newMission;
      }
    },
    [store],
  );

  const update = useCallback(
    async (id: string, data: UpdateMissionDTO) => {
      if (isElectron() && window.db) {
        await window.db.missions.update(id, data);
        emitDataChange("missions");
      } else {
        store.updateMission(id, data);
        emitDataChange("missions");
      }
    },
    [store],
  );

  const remove = useCallback(
    async (id: string) => {
      if (isElectron() && window.db) {
        await window.db.missions.delete(id);
        emitDataChange("missions");
      } else {
        store.deleteMission(id);
        emitDataChange("missions");
      }
    },
    [store],
  );

  const getById = useCallback(
    async (id: string) => {
      if (isElectron() && window.db) {
        const mission = await window.db.missions.findById(id);
        return mission
          ? (normalizeMission(
              mission as unknown as Record<string, unknown>,
            ) as unknown as Mission)
          : undefined;
      }
      return store.getMissionById(id);
    },
    [store],
  );

  const updateStatus = useCallback(
    async (id: string, status: MissionStatus) => {
      if (isElectron() && window.db) {
        await window.db.missions.updateStatus(id, status);
        emitDataChange("missions");
      } else {
        store.updateMissionStatus(id, status);
        emitDataChange("missions");
      }
    },
    [store],
  );

  const setPlan = useCallback(
    async (id: string, plan: MissionPlan) => {
      if (isElectron() && window.db) {
        await window.db.missions.updatePlan(id, JSON.stringify(plan));
        emitDataChange("missions");
      } else {
        store.setMissionPlan(id, plan);
        emitDataChange("missions");
      }
    },
    [store],
  );

  const setCode = useCallback(
    async (id: string, code: GeneratedCode) => {
      if (isElectron() && window.db) {
        await window.db.missions.updateGeneratedCode(id, JSON.stringify(code));
        emitDataChange("missions");
      } else {
        store.setMissionCode(id, code);
        emitDataChange("missions");
      }
    },
    [store],
  );

  const start = useCallback(
    async (id: string) => {
      if (isElectron() && window.db) {
        await window.db.missions.start(id);
        emitDataChange("missions");
      } else {
        store.updateMissionStatus(id, "planning");
        emitDataChange("missions");
      }
    },
    [store],
  );

  const complete = useCallback(
    async (id: string, summary?: string) => {
      if (isElectron() && window.db) {
        await window.db.missions.complete(id, summary);
        emitDataChange("missions");
      } else {
        store.updateMissionStatus(id, "completed");
        emitDataChange("missions");
      }
    },
    [store],
  );

  const fail = useCallback(
    async (id: string, error: string) => {
      if (isElectron() && window.db) {
        await window.db.missions.fail(id, error);
        emitDataChange("missions");
      } else {
        store.updateMission(id, { errorMessage: error });
        store.updateMissionStatus(id, "failed");
        emitDataChange("missions");
      }
    },
    [store],
  );

  const cancel = useCallback(
    async (id: string) => {
      if (isElectron() && window.db) {
        await window.db.missions.cancel(id);
        emitDataChange("missions");
      } else {
        store.updateMissionStatus(id, "cancelled");
        emitDataChange("missions");
      }
    },
    [store],
  );

  return {
    missions: isElectron()
      ? missions
      : projectId
        ? store.getMissionsByProject(projectId)
        : store.missions,
    isLoading,
    refresh,
    create,
    update,
    remove,
    getById,
    updateStatus,
    setPlan,
    setCode,
    start,
    complete,
    fail,
    cancel,
  };
}

// ============================================
// Hook para Mission Logs
// ============================================

export function useMissionLogs(missionId: string) {
  const store = useAppStore();
  const [logs, setLogs] = useState<MissionLog[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    try {
      if (isElectron() && window.db) {
        const data = await window.db.missionLogs.findByMission(missionId);
        setLogs(normalizeMissionLogs(data) as MissionLog[]);
      } else {
        setLogs(store.getLogsByMission(missionId));
      }
    } finally {
      setIsLoading(false);
    }
  }, [missionId, store]);

  useEffect(() => {
    refresh();
    const unsubscribe = subscribeToDataChange("missionLogs", refresh);
    return unsubscribe;
  }, [refresh]);

  const addLog = useCallback(
    async (
      type: MissionLog["type"],
      content: string,
      metadata?: Record<string, unknown>,
    ) => {
      if (isElectron() && window.db) {
        switch (type) {
          case "info":
            await window.db.missionLogs.logInfo(missionId, content, metadata);
            break;
          case "error":
            await window.db.missionLogs.logError(missionId, content, metadata);
            break;
          case "warning":
            await window.db.missionLogs.logWarning(
              missionId,
              content,
              metadata,
            );
            break;
          case "debug":
            await window.db.missionLogs.logDebug(missionId, content, metadata);
            break;
          default:
            await window.db.missionLogs.create({
              missionId,
              type,
              content,
              metadata: metadata ?? undefined,
            });
        }
        emitDataChange("missionLogs");
      } else {
        store.addMissionLog(missionId, {
          type,
          content,
          metadata: metadata ?? null,
        });
        emitDataChange("missionLogs");
      }
    },
    [missionId, store],
  );

  const logAgentAction = useCallback(
    async (action: string, details?: Record<string, unknown>) => {
      if (isElectron() && window.db) {
        await window.db.missionLogs.logAgentAction(missionId, action, details);
        emitDataChange("missionLogs");
      } else {
        store.addMissionLog(missionId, {
          type: "action",
          content: action,
          metadata: details ?? null,
        });
        emitDataChange("missionLogs");
      }
    },
    [missionId, store],
  );

  const logUserInput = useCallback(
    async (input: string) => {
      if (isElectron() && window.db) {
        await window.db.missionLogs.logUserInput(missionId, input);
        emitDataChange("missionLogs");
      } else {
        store.addMissionLog(missionId, {
          type: "prompt",
          content: input,
          metadata: null,
        });
        emitDataChange("missionLogs");
      }
    },
    [missionId, store],
  );

  return {
    logs: isElectron() ? logs : store.getLogsByMission(missionId),
    isLoading,
    refresh,
    addLog,
    logAgentAction,
    logUserInput,
  };
}

// ============================================
// Hook para verificar ambiente
// ============================================

export function useIsElectron() {
  const [isElectronEnv, setIsElectronEnv] = useState(false);

  useEffect(() => {
    setIsElectronEnv(isElectron());
  }, []);

  return isElectronEnv;
}
