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
  MissionAgentSession,
  Comb,
  CombCreate,
  CombUpdate,
  Pane,
  PaneCreate,
  PaneUpdate,
  PaneSession,
} from "@/lib/database/types";
import type { TerminalAttentionPayload } from "@/lib/terminal/attention-types";

/** Plataforma do host (compatível com convenção Node-style). */
export type DesktopPlatform = "darwin" | "win32" | "linux" | "aix" | "freebsd" | "openbsd" | "sunos" | "web";

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
  appliedVia?: Array<{ path: string; via: "git-apply" | "file-write" }>;
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

export interface GitBranchState extends GitStatus {
  branch: string;
  upstreamBranch?: string | null;
  defaultBranch?: string | null;
  hasUpstream: boolean;
  aheadCount: number;
  behindCount: number;
  aheadOfDefaultCount: number;
  behindOfDefaultCount: number;
  changedFiles: string[];
  mergeReadiness:
    | "ready"
    | "dirty"
    | "behind_default"
    | "diverged"
    | "already_merged"
    | "not_applicable";
}

export interface GitCommit {
  hash: string;
  message: string;
  author: string;
  date: Date;
}

declare global {
  interface Window {
    /**
     * Ponte para o processo nativo (Tauri invoke / IPC).
     * Disponível após `installDesktopBridge()` no shell desktop.
     */
    desktopAPI?: {
      platform: DesktopPlatform;

      dialog: {
        selectDirectory: () => Promise<string | null>;
        showMessage: (options: {
          type: string;
          title: string;
          message: string;
          detail?: string;
          buttons?: string[];
          defaultId?: number;
          cancelId?: number;
        }) => Promise<number>;
        confirm: (message: string) => Promise<boolean>;
      };

      shell: {
        openExternal: (url: string) => Promise<void>;
        openPath: (path: string) => Promise<void>;
        showItemInFolder: (path: string) => Promise<void>;
        resolveCliPath: (command: string) => Promise<{ path: string | null }>;
        detectCliForProvider: (providerType: string) => Promise<{
          path: string | null;
        }>;
        validateCliPath: (
          providerType: string,
          cliPath: string
        ) => Promise<{ valid: boolean; message?: string }>;
        openTerminalAtPath: (
          dirPath: string,
          suggestedCommand?: string | { cliCommand: string; prompt: string }
        ) => Promise<{ success: boolean; error?: string }>;
      };

      terminal: {
        spawn: (options: {
          cwd: string;
          command?: string;
          args?: string[];
          cols?: number;
          rows?: number;
        }) => Promise<{ ptyId?: string; error?: string }>;
        getOrCreate: (
          missionId: string,
          options: {
            cwd: string;
            command?: string;
            args?: string[];
            cols?: number;
            rows?: number;
          }
        ) => Promise<{ ptyId?: string; error?: string; session?: MissionAgentSession | null }>;
        getSession: (missionId: string) => Promise<MissionAgentSession | null>;
        write: (ptyId: string, data: string) => Promise<{ ok: boolean }>;
        resize: (
          ptyId: string,
          cols: number,
          rows: number
        ) => Promise<{ ok: boolean }>;
        kill: (ptyId: string) => Promise<{ ok: boolean }>;
        killByMissionId: (missionId: string) => Promise<{ ok: boolean }>;
        getOrCreateForPane: (
          paneId: string,
          options: {
            cwd: string;
            command?: string;
            args?: string[];
            cols?: number;
            rows?: number;
            restart?: boolean;
          }
        ) => Promise<{ ptyId?: string; error?: string; session?: PaneSession | null }>;
        getPaneSession: (paneId: string) => Promise<PaneSession | null>;
        killByPaneId: (paneId: string) => Promise<{ ok: boolean }>;
        /** Últimas linhas de output (reidratação ao remontar o xterm). */
        getBacklog?: (ptyId: string) => Promise<{ lines: string[] }>;
        getProjectActivity: (projectId: string) => Promise<{
          totalRunningPanes: number;
          runningPanesByCombId: Record<string, number>;
        }>;
        onData: (
          callback: (ptyId: string, data: string) => void
        ) => () => void;
        onExit: (
          callback: (ptyId: string, code: number) => void
        ) => () => void;
        onAttention: (
          callback: (payload: TerminalAttentionPayload) => void
        ) => () => void;
      };

      worktree: {
        ensureForMission: (missionId: string) => Promise<{
          success: boolean;
          error?: string;
          worktreePath?: string;
          worktreeBranch?: string;
        }>;
        mergeIntoMain: (missionId: string) => Promise<{
          success: boolean;
          error?: string;
        }>;
        discard: (missionId: string) => Promise<{
          success: boolean;
          error?: string;
        }>;
        getDiffs: (missionId: string) => Promise<{
          success: boolean;
          error?: string;
          files: Array<{ path: string; status: string; diff: string }>;
          summary: {
            changedFiles: number;
            insertions: number;
            deletions: number;
          } | null;
        }>;
        applyMissionPatch: (
          missionId: string,
          targetBranch: string,
          options?: {
            includeFiles?: string[];
            commit?: boolean;
            message?: string;
          }
        ) => Promise<{
          success: boolean;
          error?: string;
          applyFailed?: boolean;
        }>;
      };

      comb: {
        ensureWorktree: (combId: string) => Promise<{
          success: boolean;
          error?: string;
          worktreePath?: string;
          branch?: string;
        }>;
        discard: (combId: string) => Promise<{
          success: boolean;
          error?: string;
        }>;
        mergeIntoMain: (
          combId: string,
          targetBranch?: string
        ) => Promise<{
          success: boolean;
          error?: string;
        }>;
        getDiffs: (combId: string) => Promise<{
          success: boolean;
          error?: string;
          files: Array<{ path: string; status: string; diff: string }>;
          summary: {
            changedFiles: number;
            insertions: number;
            deletions: number;
          } | null;
        }>;
        applyPatch: (
          combId: string,
          targetBranch: string,
          options?: {
            includeFiles?: string[];
            commit?: boolean;
            message?: string;
          }
        ) => Promise<{
          success: boolean;
          error?: string;
        }>;
      };

      window: {
        minimize: () => Promise<void>;
        maximize: () => Promise<void>;
        close: () => Promise<void>;
        isMaximized: () => Promise<boolean>;
      };

      license: {
        getStatus: () => Promise<{
          activated: boolean;
          email?: string;
          activatedAt?: string;
        }>;
        getMachineId: () => Promise<string>;
        activate: (email: string) => Promise<{
          success: boolean;
          message?: string;
        }>;
        skipActivation: () => Promise<{ success: boolean }>;
      };

      app: {
        getVersion: () => Promise<string>;
        checkForUpdates: () => Promise<void>;
        quitAndInstall: () => Promise<void>;
        showNotification: (payload: {
          title: string;
          body?: string;
        }) => Promise<void>;
        onUpdateStatus: (
          callback: (payload: {
            type: "available" | "not-available" | "downloaded" | "error";
            version?: string;
            message?: string;
          }) => void
        ) => () => void;
      };

      ai: {
        generatePlan: (
          missionId: string,
          options?: { planFeedback?: string }
        ) => Promise<AIResponse<MissionPlan>>;
        generateCode: (
          missionId: string,
          options?: { codeFeedback?: string }
        ) => Promise<AIResponse<GeneratedCode>>;
        applyChanges: (
          missionId: string,
          options?: {
            createBackup?: boolean;
            dryRun?: boolean;
            filePaths?: string[];
            editedContent?: Record<string, string>;
          }
        ) => Promise<ApplyChangesResult>;
        testConnection: (
          providerId: string
        ) => Promise<{ success: boolean; message: string }>;
        validateProvider: (provider: Provider) => Promise<ValidationResult>;
        invalidateAdapter: (providerId: string) => Promise<boolean>;
      };

      git: {
        getInfo: (projectPath: string) => Promise<GitInfo | null>;
        getStatus: (projectPath: string) => Promise<GitStatus>;
        getBranchState: (projectPath: string) => Promise<GitBranchState>;
        getFileDiffHead: (
          projectPath: string,
          filePath: string
        ) => Promise<string>;
        getFileDiffAgainstBase: (
          projectPath: string,
          filePath: string,
          baseRef: string
        ) => Promise<string>;
        isRepo: (projectPath: string) => Promise<boolean>;
        getCurrentBranch: (projectPath: string) => Promise<string>;
        getDefaultBranch: (projectPath: string) => Promise<string>;
        getLocalBranches: (projectPath: string) => Promise<string[]>;
        createBranch: (
          projectPath: string,
          branchName: string,
          fromBranch?: string
        ) => Promise<boolean>;
        listFiles: (
          projectPath: string,
          maxFiles?: number
        ) => Promise<string[]>;
        getRecentCommits: (
          projectPath: string,
          count?: number
        ) => Promise<GitCommit[]>;
        commit: (
          projectPath: string,
          message: string,
          files?: string[]
        ) => Promise<{ success: boolean; error?: string }>;
        push: (
          projectPath: string
        ) => Promise<{ success: boolean; error?: string }>;
        pull: (
          projectPath: string
        ) => Promise<{ success: boolean; error?: string }>;
        reset: (
          projectPath: string,
          ref?: "HEAD" | "HEAD~1"
        ) => Promise<{ success: boolean; error?: string }>;
        getWorktreeInfo: (
          projectPath: string
        ) => Promise<{ isWorktree: boolean; worktreeRoot?: string }>;
        getReviewDiffs: (worktreePath: string) => Promise<{
          success: boolean;
          error?: string;
          files: Array<{ path: string; status: string; diff: string }>;
          summary: {
            changedFiles: number;
            insertions: number;
            deletions: number;
          } | null;
        }>;
        applyWorktreePatch: (
          mainProjectPath: string,
          worktreePath: string,
          targetBranch: string,
          options?: {
            includeFiles?: string[];
            commit?: boolean;
            message?: string;
          }
        ) => Promise<{
          success: boolean;
          error?: string;
          applyFailed?: boolean;
        }>;
      };

      review: {
        getDiffsBundle: (worktreePaths: string[]) => Promise<
          Array<{
            worktreePath: string;
            success: boolean;
            error?: string;
            files: Array<{ path: string; status: string; diff: string }>;
            summary: {
              changedFiles: number;
              insertions: number;
              deletions: number;
            } | null;
          }>
        >;
      };
    };

    db?: {
      providers: {
        findAll: () => Promise<Provider[]>;
        findById: (id: string) => Promise<Provider | undefined>;
        findByType: (type: string) => Promise<Provider[]>;
        findActive: () => Promise<Provider[]>;
        create: (data: ProviderCreate) => Promise<Provider>;
        update: (
          id: string,
          data: ProviderUpdate
        ) => Promise<Provider | undefined>;
        delete: (id: string) => Promise<boolean>;
        setActive: (
          id: string,
          isActive: boolean
        ) => Promise<Provider | undefined>;
        testConnection: (
          id: string
        ) => Promise<{ success: boolean; error?: string }>;
        isEncryptionAvailable: () => Promise<boolean>;
      };

      projects: {
        findAll: () => Promise<Project[]>;
        findById: (id: string) => Promise<Project | undefined>;
        findByPath: (path: string) => Promise<Project | undefined>;
        search: (query: string) => Promise<Project[]>;
        create: (data: ProjectCreate) => Promise<Project>;
        update: (
          id: string,
          data: ProjectUpdate
        ) => Promise<Project | undefined>;
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
        update: (
          id: string,
          data: MissionUpdate
        ) => Promise<Mission | undefined>;
        delete: (id: string) => Promise<boolean>;
        updateStatus: (
          id: string,
          status: MissionStatus
        ) => Promise<Mission | undefined>;
        updatePlan: (id: string, plan: string) => Promise<Mission | undefined>;
        updateGeneratedCode: (
          id: string,
          code: string
        ) => Promise<Mission | undefined>;
        start: (id: string) => Promise<Mission | undefined>;
        complete: (
          id: string,
          summary?: string
        ) => Promise<Mission | undefined>;
        fail: (id: string, error: string) => Promise<Mission | undefined>;
        cancel: (id: string) => Promise<Mission | undefined>;
        getFullMission: (id: string) => Promise<MissionWithDetails | undefined>;
      };

      missionLogs: {
        findAll: () => Promise<MissionLog[]>;
        findById: (id: string) => Promise<MissionLog | undefined>;
        findByMission: (
          missionId: string,
          limit?: number,
          offset?: number
        ) => Promise<MissionLog[]>;
        findByLevel: (
          level: LogLevel,
          missionId?: string
        ) => Promise<MissionLog[]>;
        search: (query: string, missionId?: string) => Promise<MissionLog[]>;
        create: (data: MissionLogCreate) => Promise<MissionLog>;
        delete: (id: string) => Promise<boolean>;
        deleteByMission: (missionId: string) => Promise<number>;
        logInfo: (
          missionId: string,
          message: string,
          metadata?: Record<string, unknown>
        ) => Promise<MissionLog>;
        logWarning: (
          missionId: string,
          message: string,
          metadata?: Record<string, unknown>
        ) => Promise<MissionLog>;
        logError: (
          missionId: string,
          message: string,
          metadata?: Record<string, unknown>
        ) => Promise<MissionLog>;
        logDebug: (
          missionId: string,
          message: string,
          metadata?: Record<string, unknown>
        ) => Promise<MissionLog>;
        logAgentAction: (
          missionId: string,
          action: string,
          details?: Record<string, unknown>
        ) => Promise<MissionLog>;
        logUserInput: (missionId: string, input: string) => Promise<MissionLog>;
        getStats: (missionId: string) => Promise<MissionLogStats>;
        getUsageStats: (
          missionId: string
        ) => Promise<{ totalTokens: number; totalDurationMs: number }>;
        getLatest: (missionId: string, count?: number) => Promise<MissionLog[]>;
      };

      combs: {
        findByProject: (projectId: string) => Promise<Comb[]>;
        findById: (id: string) => Promise<Comb | undefined>;
        create: (data: CombCreate) => Promise<Comb>;
        update: (id: string, data: CombUpdate) => Promise<Comb | undefined>;
        delete: (id: string) => Promise<boolean>;
      };

      panes: {
        findByComb: (combId: string) => Promise<Pane[]>;
        findById: (id: string) => Promise<Pane | undefined>;
        create: (data: PaneCreate) => Promise<Pane>;
        update: (id: string, data: PaneUpdate) => Promise<Pane | undefined>;
        delete: (id: string) => Promise<boolean>;
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
