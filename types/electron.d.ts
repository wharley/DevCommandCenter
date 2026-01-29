import type {
  Provider,
  ProviderCreate,
  ProviderUpdate,
  Project,
  ProjectCreate,
  ProjectUpdate,
  ProjectStats,
  Mission,
  MissionCreate,
  MissionUpdate,
  MissionStatus,
  MissionWithDetails,
  MissionLog,
  MissionLogCreate,
  LogLevel,
  MissionLogStats,
  MissionPlan,
  GeneratedCode,
  CodeSuggestion,
} from '@/lib/database/types';

// ============================================
// AI Service Types
// ============================================

export interface AIResponse<T = MissionPlan | GeneratedCode> {
  success: boolean;
  data?: T;
  error?: string;
  metadata?: {
    tokensUsed?: number;
    durationMs?: number;
    model?: string;
    provider?: string;
  };
}

export interface ApplyChangesResult {
  success: boolean;
  appliedFiles: string[];
  failedFiles: Array<{ path: string; error: string }>;
  backupPath?: string;
}

export interface ValidationResult {
  valid: boolean;
  errors: string[];
  warnings?: string[];
}

// ============================================
// Git Service Types
// ============================================

export interface GitInfo {
  branch: string;
  remote?: string;
  status: GitStatus;
  recentCommits?: GitCommit[];
}

export interface GitStatus {
  isRepo: boolean;
  isDirty: boolean;
  staged: string[];
  unstaged: string[];
  untracked: string[];
}

export interface GitCommit {
  hash: string;
  message: string;
  author: string;
  date: Date;
}

declare global {
  interface Window {
    electronAPI?: {
      platform: NodeJS.Platform;
      
      dialog: {
        selectDirectory: () => Promise<string | null>;
        showMessage: (options: { type: string; title: string; message: string }) => Promise<number>;
        confirm: (message: string) => Promise<boolean>;
      };

      shell: {
        openExternal: (url: string) => Promise<void>;
        openPath: (path: string) => Promise<void>;
        showItemInFolder: (path: string) => Promise<void>;
        resolveCliPath: (command: string) => Promise<{ path: string | null }>;
      };

      window: {
        minimize: () => Promise<void>;
        maximize: () => Promise<void>;
        close: () => Promise<void>;
        isMaximized: () => Promise<boolean>;
      };

      // AI Service APIs
      ai: {
        generatePlan: (missionId: string) => Promise<AIResponse<MissionPlan>>;
        generateCode: (missionId: string) => Promise<AIResponse<GeneratedCode>>;
        applyChanges: (
          missionId: string,
          options?: { createBackup?: boolean; dryRun?: boolean }
        ) => Promise<ApplyChangesResult>;
        testConnection: (providerId: string) => Promise<{ success: boolean; message: string }>;
        validateProvider: (provider: Provider) => Promise<ValidationResult>;
        invalidateAdapter: (providerId: string) => Promise<boolean>;
      };

      // Git APIs
      git: {
        getInfo: (projectPath: string) => Promise<GitInfo | null>;
        getStatus: (projectPath: string) => Promise<GitStatus>;
        isRepo: (projectPath: string) => Promise<boolean>;
        getCurrentBranch: (projectPath: string) => Promise<string>;
        listFiles: (projectPath: string, maxFiles?: number) => Promise<string[]>;
        getRecentCommits: (projectPath: string, count?: number) => Promise<GitCommit[]>;
      };
    };

    db?: {
      providers: {
        findAll: () => Promise<Provider[]>;
        findById: (id: string) => Promise<Provider | undefined>;
        findByType: (type: string) => Promise<Provider[]>;
        findActive: () => Promise<Provider[]>;
        create: (data: ProviderCreate) => Promise<Provider>;
        update: (id: string, data: ProviderUpdate) => Promise<Provider | undefined>;
        delete: (id: string) => Promise<boolean>;
        setActive: (id: string, isActive: boolean) => Promise<Provider | undefined>;
        testConnection: (id: string) => Promise<{ success: boolean; message: string }>;
      };

      projects: {
        findAll: () => Promise<Project[]>;
        findById: (id: string) => Promise<Project | undefined>;
        findByPath: (path: string) => Promise<Project | undefined>;
        search: (query: string) => Promise<Project[]>;
        create: (data: ProjectCreate) => Promise<Project>;
        update: (id: string, data: ProjectUpdate) => Promise<Project | undefined>;
        delete: (id: string) => Promise<boolean>;
        getStats: (id: string) => Promise<ProjectStats>;
        updateLastOpened: (id: string) => Promise<Project | undefined>;
      };

      missions: {
        findAll: () => Promise<Mission[]>;
        findById: (id: string) => Promise<Mission | undefined>;
        findByProject: (projectId: string) => Promise<Mission[]>;
        findByStatus: (status: MissionStatus) => Promise<Mission[]>;
        findActive: () => Promise<Mission[]>;
        search: (query: string, projectId?: string) => Promise<Mission[]>;
        create: (data: MissionCreate) => Promise<Mission>;
        update: (id: string, data: MissionUpdate) => Promise<Mission | undefined>;
        delete: (id: string) => Promise<boolean>;
        updateStatus: (id: string, status: MissionStatus) => Promise<Mission | undefined>;
        updatePlan: (id: string, plan: string) => Promise<Mission | undefined>;
        updateGeneratedCode: (id: string, code: string) => Promise<Mission | undefined>;
        start: (id: string) => Promise<Mission | undefined>;
        complete: (id: string, summary?: string) => Promise<Mission | undefined>;
        fail: (id: string, error: string) => Promise<Mission | undefined>;
        cancel: (id: string) => Promise<Mission | undefined>;
        getFullMission: (id: string) => Promise<MissionWithDetails | undefined>;
      };

      missionLogs: {
        findAll: () => Promise<MissionLog[]>;
        findById: (id: string) => Promise<MissionLog | undefined>;
        findByMission: (missionId: string, limit?: number, offset?: number) => Promise<MissionLog[]>;
        findByLevel: (level: LogLevel, missionId?: string) => Promise<MissionLog[]>;
        search: (query: string, missionId?: string) => Promise<MissionLog[]>;
        create: (data: MissionLogCreate) => Promise<MissionLog>;
        delete: (id: string) => Promise<boolean>;
        deleteByMission: (missionId: string) => Promise<number>;
        logInfo: (missionId: string, message: string, metadata?: Record<string, unknown>) => Promise<MissionLog>;
        logWarning: (missionId: string, message: string, metadata?: Record<string, unknown>) => Promise<MissionLog>;
        logError: (missionId: string, message: string, metadata?: Record<string, unknown>) => Promise<MissionLog>;
        logDebug: (missionId: string, message: string, metadata?: Record<string, unknown>) => Promise<MissionLog>;
        logAgentAction: (missionId: string, action: string, details?: Record<string, unknown>) => Promise<MissionLog>;
        logUserInput: (missionId: string, input: string) => Promise<MissionLog>;
        getStats: (missionId: string) => Promise<MissionLogStats>;
        getLatest: (missionId: string, count?: number) => Promise<MissionLog[]>;
      };

      utils: {
        backup: (destPath: string) => Promise<boolean>;
        getPath: () => Promise<string | null>;
        getSize: () => Promise<number>;
      };
    };
  }
}

export {};
