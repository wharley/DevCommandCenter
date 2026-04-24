// Dev Command Center - Application State Store
// Estado de UI (projeto/missão atuais) e cache em memória para uso no browser.
// No app desktop (Tauri), dados reais vêm do SQLite via use-data (window.db).

import { create } from "zustand";
import type {
  Project,
  Provider,
  Mission,
  MissionLog,
  MissionStatus,
  MissionPlan,
  GeneratedCode,
} from "@/lib/database/types";

// ============================================
// Estado da aplicação
// ============================================

interface AppState {
  // Dados
  projects: Project[];
  providers: Provider[];
  missions: Mission[];
  missionLogs: Record<string, MissionLog[]>;

  // UI State
  currentProjectId: string | null;
  currentMissionId: string | null;
  isLoading: boolean;
  sidebarCollapsed: boolean;
  setSidebarCollapsed: (collapsed: boolean) => void;

  // Actions - Projects
  addProject: (
    project: Omit<Project, "id" | "createdAt" | "updatedAt">,
  ) => Project;
  updateProject: (id: string, updates: Partial<Project>) => void;
  deleteProject: (id: string) => void;
  setCurrentProject: (id: string | null) => void;

  // Actions - Providers
  addProvider: (
    provider: Omit<Provider, "id" | "createdAt" | "updatedAt">,
  ) => Provider;
  updateProvider: (id: string, updates: Partial<Provider>) => void;
  deleteProvider: (id: string) => void;

  // Actions - Missions
  addMission: (
    mission: Omit<Mission, "id" | "createdAt" | "updatedAt" | "status">,
  ) => Mission;
  updateMission: (id: string, updates: Partial<Mission>) => void;
  deleteMission: (id: string) => void;
  setCurrentMission: (id: string | null) => void;
  updateMissionStatus: (id: string, status: MissionStatus) => void;
  setMissionPlan: (id: string, plan: MissionPlan) => void;
  setMissionCode: (id: string, code: GeneratedCode) => void;

  // Actions - Logs
  addMissionLog: (
    missionId: string,
    log: Omit<MissionLog, "id" | "missionId" | "createdAt">,
  ) => void;

  // Utils
  getProjectById: (id: string) => Project | undefined;
  getProviderById: (id: string) => Provider | undefined;
  getMissionById: (id: string) => Mission | undefined;
  getMissionsByProject: (projectId: string) => Mission[];
  getLogsByMission: (missionId: string) => MissionLog[];
}

// Gerador de IDs para uso no browser (estado em memória)
const generateId = () =>
  `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;

// ============================================
// Store Zustand
// ============================================

export const useAppStore = create<AppState>((set, get) => ({
  // Estado inicial vazio; no desktop os dados vêm do SQLite via use-data
  projects: [],
  providers: [],
  missions: [],
  missionLogs: {},

  currentProjectId: null,
  currentMissionId: null,
  isLoading: false,
  sidebarCollapsed: false,
  setSidebarCollapsed: (collapsed) => set({ sidebarCollapsed: collapsed }),

  // Actions - Projects
  addProject: (projectData) => {
    const newProject: Project = {
      ...projectData,
      repoConfig: projectData.repoConfig ?? null,
      id: generateId(),
      createdAt: new Date(),
      updatedAt: new Date(),
    };
    set((state) => ({ projects: [...state.projects, newProject] }));
    return newProject;
  },

  updateProject: (id, updates) => {
    set((state) => ({
      projects: state.projects.map((p) =>
        p.id === id ? { ...p, ...updates, updatedAt: new Date() } : p,
      ),
    }));
  },

  deleteProject: (id) => {
    set((state) => ({
      projects: state.projects.filter((p) => p.id !== id),
      missions: state.missions.filter((m) => m.projectId !== id),
    }));
  },

  setCurrentProject: (id) => set({ currentProjectId: id }),

  // Actions - Providers
  addProvider: (providerData) => {
    const newProvider: Provider = {
      ...providerData,
      id: generateId(),
      createdAt: new Date(),
      updatedAt: new Date(),
    };
    set((state) => ({ providers: [...state.providers, newProvider] }));
    return newProvider;
  },

  updateProvider: (id, updates) => {
    set((state) => ({
      providers: state.providers.map((p) =>
        p.id === id ? { ...p, ...updates, updatedAt: new Date() } : p,
      ),
    }));
  },

  deleteProvider: (id) => {
    set((state) => ({
      providers: state.providers.filter((p) => p.id !== id),
    }));
  },

  // Actions - Missions
  addMission: (missionData) => {
    const newMission: Mission = {
      ...missionData,
      id: generateId(),
      status: "created",
      createdAt: new Date(),
      updatedAt: new Date(),
    };
    set((state) => ({
      missions: [...state.missions, newMission],
      missionLogs: {
        ...state.missionLogs,
        [newMission.id]: [
          {
            id: generateId(),
            missionId: newMission.id,
            type: "info",
            content: "Missão criada",
            metadata: null,
            createdAt: new Date(),
          },
        ],
      },
    }));
    return newMission;
  },

  updateMission: (id, updates) => {
    set((state) => ({
      missions: state.missions.map((m) =>
        m.id === id ? { ...m, ...updates, updatedAt: new Date() } : m,
      ),
    }));
  },

  deleteMission: (id) => {
    set((state) => {
      const newLogs = { ...state.missionLogs };
      delete newLogs[id];
      return {
        missions: state.missions.filter((m) => m.id !== id),
        missionLogs: newLogs,
      };
    });
  },

  setCurrentMission: (id) => set({ currentMissionId: id }),

  updateMissionStatus: (id, status) => {
    const state = get();
    const mission = state.missions.find((m) => m.id === id);
    if (!mission) return;

    const updates: Partial<Mission> = { status };
    if (status === "planning" || status === "generating_code") {
      updates.startedAt = new Date();
    }
    if (status === "completed" || status === "failed") {
      updates.completedAt = new Date();
    }

    set((s) => ({
      missions: s.missions.map((m) =>
        m.id === id ? { ...m, ...updates, updatedAt: new Date() } : m,
      ),
    }));
  },

  setMissionPlan: (id, plan) => {
    set((state) => ({
      missions: state.missions.map((m) =>
        m.id === id
          ? { ...m, plan, status: "plan_generated", updatedAt: new Date() }
          : m,
      ),
    }));
  },

  setMissionCode: (id, code) => {
    set((state) => ({
      missions: state.missions.map((m) =>
        m.id === id
          ? {
              ...m,
              generatedCode: code,
              status: "code_ready",
              updatedAt: new Date(),
            }
          : m,
      ),
    }));
  },

  // Actions - Logs
  addMissionLog: (missionId, logData) => {
    const newLog: MissionLog = {
      ...logData,
      id: generateId(),
      missionId,
      createdAt: new Date(),
    };
    set((state) => ({
      missionLogs: {
        ...state.missionLogs,
        [missionId]: [...(state.missionLogs[missionId] || []), newLog],
      },
    }));
  },

  // Utils
  getProjectById: (id) => get().projects.find((p) => p.id === id),
  getProviderById: (id) => get().providers.find((p) => p.id === id),
  getMissionById: (id) => get().missions.find((m) => m.id === id),
  getMissionsByProject: (projectId) =>
    get().missions.filter((m) => m.projectId === projectId),
  getLogsByMission: (missionId) => get().missionLogs[missionId] || [],
}));
