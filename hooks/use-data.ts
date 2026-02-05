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
  // Use seletores específicos para evitar re-renders desnecessários
  const storeProjects = useAppStore((s) => s.projects);
  const addProject = useAppStore((s) => s.addProject);
  const updateProject = useAppStore((s) => s.updateProject);
  const deleteProject = useAppStore((s) => s.deleteProject);
  const getProjectById = useAppStore((s) => s.getProjectById);

  const [projects, setProjects] = useState<Project[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    try {
      if (isElectron() && window.db) {
        const data = await window.db.projects.findAll();
        setProjects(normalizeProjects(data) as Project[]);
      } else {
        setProjects(storeProjects);
      }
    } finally {
      setIsLoading(false);
    }
  }, [storeProjects]);

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
      if (isElectron() && window.db) {
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
      if (isElectron() && window.db) {
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
      if (isElectron() && window.db) {
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
      if (isElectron() && window.db) {
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
    projects: isElectron() ? projects : storeProjects,
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
  // Use seletores específicos para evitar re-renders desnecessários
  const storeProviders = useAppStore((s) => s.providers);
  const addProvider = useAppStore((s) => s.addProvider);
  const updateProvider = useAppStore((s) => s.updateProvider);
  const deleteProvider = useAppStore((s) => s.deleteProvider);
  const getProviderById = useAppStore((s) => s.getProviderById);

  const [providers, setProviders] = useState<Provider[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    try {
      if (isElectron() && window.db) {
        const data = await window.db.providers.findAll();
        setProviders(normalizeProviders(data) as Provider[]);
      } else {
        setProviders(storeProviders);
      }
    } finally {
      setIsLoading(false);
    }
  }, [storeProviders]);

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
      if (isElectron() && window.db) {
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
      if (isElectron() && window.db) {
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
      if (isElectron() && window.db) {
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
    if (isElectron() && window.db) {
      const data = await window.db.providers.findActive();
      return normalizeProviders(data) as Provider[];
    }
    return storeProviders.filter((p) => p.isActive);
  }, [storeProviders]);

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
    providers: isElectron() ? providers : storeProviders,
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
  // Use seletores específicos para evitar re-renders desnecessários
  const storeMissions = useAppStore((s) => s.missions);
  const getMissionsByProject = useAppStore((s) => s.getMissionsByProject);
  const getMissionById = useAppStore((s) => s.getMissionById);
  const addMission = useAppStore((s) => s.addMission);
  const updateMission = useAppStore((s) => s.updateMission);
  const deleteMission = useAppStore((s) => s.deleteMission);
  const updateMissionStatus = useAppStore((s) => s.updateMissionStatus);
  const setMissionPlan = useAppStore((s) => s.setMissionPlan);
  const setMissionCode = useAppStore((s) => s.setMissionCode);

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
          ? getMissionsByProject(projectId)
          : storeMissions;
        setMissions(data);
      }
    } finally {
      setIsLoading(false);
    }
  }, [projectId, getMissionsByProject, storeMissions]);

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
        const newMission = addMission({
          ...data,
          providerId: data.providerId ?? null,
          planProviderId: data.planProviderId ?? null,
          codeProviderId: data.codeProviderId ?? null,
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
    [addMission],
  );

  const update = useCallback(
    async (id: string, data: UpdateMissionDTO) => {
      if (isElectron() && window.db) {
        await window.db.missions.update(id, data);
        emitDataChange("missions");
      } else {
        updateMission(id, data);
        emitDataChange("missions");
      }
    },
    [updateMission],
  );

  const remove = useCallback(
    async (id: string) => {
      if (isElectron() && window.db) {
        await window.db.missions.delete(id);
        emitDataChange("missions");
      } else {
        deleteMission(id);
        emitDataChange("missions");
      }
    },
    [deleteMission],
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
      return getMissionById(id);
    },
    [getMissionById],
  );

  const updateStatusFn = useCallback(
    async (id: string, status: MissionStatus) => {
      if (isElectron() && window.db) {
        await window.db.missions.updateStatus(id, status);
        emitDataChange("missions");
      } else {
        updateMissionStatus(id, status);
        emitDataChange("missions");
      }
    },
    [updateMissionStatus],
  );

  const setPlan = useCallback(
    async (id: string, plan: MissionPlan) => {
      if (isElectron() && window.db) {
        await window.db.missions.updatePlan(id, JSON.stringify(plan));
        emitDataChange("missions");
      } else {
        setMissionPlan(id, plan);
        emitDataChange("missions");
      }
    },
    [setMissionPlan],
  );

  const setCode = useCallback(
    async (id: string, code: GeneratedCode) => {
      if (isElectron() && window.db) {
        await window.db.missions.updateGeneratedCode(id, JSON.stringify(code));
        emitDataChange("missions");
      } else {
        setMissionCode(id, code);
        emitDataChange("missions");
      }
    },
    [setMissionCode],
  );

  const start = useCallback(
    async (id: string) => {
      if (isElectron() && window.db) {
        await window.db.missions.start(id);
        emitDataChange("missions");
      } else {
        updateMissionStatus(id, "planning");
        emitDataChange("missions");
      }
    },
    [updateMissionStatus],
  );

  const complete = useCallback(
    async (id: string, summary?: string) => {
      if (isElectron() && window.db) {
        await window.db.missions.complete(id, summary);
        emitDataChange("missions");
      } else {
        updateMissionStatus(id, "completed");
        emitDataChange("missions");
      }
    },
    [updateMissionStatus],
  );

  const fail = useCallback(
    async (id: string, error: string) => {
      if (isElectron() && window.db) {
        await window.db.missions.fail(id, error);
        emitDataChange("missions");
      } else {
        updateMission(id, { errorMessage: error });
        updateMissionStatus(id, "failed");
        emitDataChange("missions");
      }
    },
    [updateMission, updateMissionStatus],
  );

  const cancel = useCallback(
    async (id: string) => {
      if (isElectron() && window.db) {
        await window.db.missions.cancel(id);
        emitDataChange("missions");
      } else {
        updateMissionStatus(id, "cancelled");
        emitDataChange("missions");
      }
    },
    [updateMissionStatus],
  );

  // Para o modo browser, retorna diretamente do store para ter dados atualizados
  const finalMissions = isElectron()
    ? missions
    : projectId
      ? getMissionsByProject(projectId)
      : storeMissions;

  return {
    missions: finalMissions,
    isLoading,
    refresh,
    create,
    update,
    remove,
    getById,
    updateStatus: updateStatusFn,
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
  // Use seletores específicos para evitar re-renders desnecessários
  const getLogsByMission = useAppStore((s) => s.getLogsByMission);
  const addMissionLog = useAppStore((s) => s.addMissionLog);

  const [logs, setLogs] = useState<MissionLog[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    try {
      if (isElectron() && window.db) {
        const data = await window.db.missionLogs.findByMission(missionId);
        setLogs(normalizeMissionLogs(data) as MissionLog[]);
      } else {
        setLogs(getLogsByMission(missionId));
      }
    } finally {
      setIsLoading(false);
    }
  }, [missionId, getLogsByMission]);

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
        addMissionLog(missionId, {
          type,
          content,
          metadata: metadata ?? null,
        });
        emitDataChange("missionLogs");
      }
    },
    [missionId, addMissionLog],
  );

  const logAgentAction = useCallback(
    async (action: string, details?: Record<string, unknown>) => {
      if (isElectron() && window.db) {
        await window.db.missionLogs.logAgentAction(missionId, action, details);
        emitDataChange("missionLogs");
      } else {
        addMissionLog(missionId, {
          type: "action",
          content: action,
          metadata: details ?? null,
        });
        emitDataChange("missionLogs");
      }
    },
    [missionId, addMissionLog],
  );

  const logUserInput = useCallback(
    async (input: string) => {
      if (isElectron() && window.db) {
        await window.db.missionLogs.logUserInput(missionId, input);
        emitDataChange("missionLogs");
      } else {
        addMissionLog(missionId, {
          type: "prompt",
          content: input,
          metadata: null,
        });
        emitDataChange("missionLogs");
      }
    },
    [missionId, addMissionLog],
  );

  return {
    logs: isElectron() ? logs : getLogsByMission(missionId),
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
